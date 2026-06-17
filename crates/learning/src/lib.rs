//! v1.4：Agent 学习 / 个性化
//!
//! 跟踪用户行为：使用的模型 / 工具 / 命令 / 提示词长度 / 时间分布
//! 推断偏好：默认模型 / 偏好工具 / 偏好语言 / 平均提问长度
//!
//! 数据存在 ~/.agentshell/learning.json

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Learning {
    pub signals: Signals,
    pub preferences: Preferences,
    pub updated_at: u64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Signals {
    /// 每次 chat 一次
    pub total_chats: u32,
    pub total_tool_calls: u32,
    pub total_messages: u32,
    /// 模型使用次数
    pub model_usage: HashMap<String, u32>,
    /// 工具使用次数
    pub tool_usage: HashMap<String, u32>,
    /// slash 命令使用次数
    pub slash_usage: HashMap<String, u32>,
    /// 提示词长度分布（bucket: 0-50, 50-200, 200-1000, 1000+）
    pub prompt_length_buckets: HashMap<String, u32>,
    /// 用户反馈：点赞 / 点踩
    pub positive_feedback: u32,
    pub negative_feedback: u32,
    /// 写作语言（基于最近 N 条 user 消息）
    pub languages: HashMap<String, u32>,
    /// 写作风格（基于 TF-IDF 的 top tokens）
    pub frequent_tokens: HashMap<String, u32>,
    /// 工作时间分布
    pub hours: HashMap<u32, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Preferences {
    pub default_model: Option<String>,
    pub favorite_tools: Vec<String>,
    pub favorite_slash_commands: Vec<String>,
    pub preferred_language: Option<String>,
    pub typical_prompt_length: Option<String>,
    pub active_hours: Vec<u32>,
    pub confidence: f32, // 0.0 - 1.0
}

impl Learning {
    pub fn new() -> Self {
        let now = now_ms();
        Learning {
            signals: Signals::default(),
            preferences: Preferences::default(),
            updated_at: now,
            created_at: now,
        }
    }

    /// 从文件加载
    pub async fn load() -> Self {
        let path = Self::default_path();
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| Self::new()),
            Err(_) => Self::new(),
        }
    }

    pub async fn save(&self) -> Result<(), String> {
        let path = Self::default_path();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        tokio::fs::write(&path, text)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".agentshell").join("learning.json")
    }

    /// 记录一次 chat
    pub fn record_chat(&mut self, model: &str, user_msg: &str) {
        self.signals.total_chats += 1;
        self.signals.total_messages += 1;
        *self.signals.model_usage.entry(model.to_string()).or_insert(0) += 1;
        // 长度分桶
        let len = user_msg.chars().count();
        let bucket = if len < 50 {
            "0-50"
        } else if len < 200 {
            "50-200"
        } else if len < 1000 {
            "200-1000"
        } else {
            "1000+"
        };
        *self.signals.prompt_length_buckets.entry(bucket.to_string()).or_insert(0) += 1;
        // 简单 token 提取（中文按 char，英文按 word）
        record_tokens(&mut self.signals.frequent_tokens, user_msg);
        // 语言检测（看是否含中文）
        let lang = if user_msg.chars().any(|c| c as u32 >= 0x4E00 && c as u32 <= 0x9FFF) {
            "zh"
        } else {
            "en"
        };
        *self.signals.languages.entry(lang.to_string()).or_insert(0) += 1;
        // 工作时间
        let hour = current_hour();
        *self.signals.hours.entry(hour).or_insert(0) += 1;
        self.updated_at = now_ms();
    }

    /// 记录一次工具调用
    pub fn record_tool_call(&mut self, tool_name: &str) {
        self.signals.total_tool_calls += 1;
        *self.signals.tool_usage.entry(tool_name.to_string()).or_insert(0) += 1;
        self.updated_at = now_ms();
    }

    /// 记录一次 slash 命令
    pub fn record_slash_command(&mut self, cmd: &str) {
        *self.signals.slash_usage.entry(cmd.to_string()).or_insert(0) += 1;
        self.updated_at = now_ms();
    }

    /// 记录用户反馈
    pub fn record_feedback(&mut self, positive: bool) {
        if positive {
            self.signals.positive_feedback += 1;
        } else {
            self.signals.negative_feedback += 1;
        }
        self.updated_at = now_ms();
    }

    /// 重新计算偏好
    pub fn compute_preferences(&mut self) {
        // 默认模型：使用次数最多
        self.preferences.default_model = self
            .signals
            .model_usage
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(m, _)| m.clone());
        // 偏好工具：top 5
        self.preferences.favorite_tools = top_n_keys(&self.signals.tool_usage, 5);
        // 偏好 slash 命令：top 5
        self.preferences.favorite_slash_commands = top_n_keys(&self.signals.slash_usage, 5);
        // 偏好语言
        self.preferences.preferred_language = self
            .signals
            .languages
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(l, _)| l.clone());
        // 提示长度
        self.preferences.typical_prompt_length = self
            .signals
            .prompt_length_buckets
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(b, _)| b.clone());
        // 活跃小时：top 5
        self.preferences.active_hours = top_n_keys_u32(&self.signals.hours, 5);
        // 置信度：总信号数（cap 100）
        let total = self.signals.total_chats + self.signals.total_tool_calls;
        self.preferences.confidence = (total as f32 / 100.0).min(1.0);
    }

    /// 生成个性化 system prompt 注入文本
    pub fn inject_text(&self) -> String {
        let mut s = String::new();
        if self.preferences.confidence < 0.1 {
            return s;
        }
        s.push_str("\n[用户偏好 — v1.4 自动学习]\n");
        if let Some(m) = &self.preferences.default_model {
            s.push_str(&format!("- 默认模型: {m}\n"));
        }
        if let Some(l) = &self.preferences.preferred_language {
            s.push_str(&format!("- 偏好语言: {l}\n"));
        }
        if let Some(b) = &self.preferences.typical_prompt_length {
            s.push_str(&format!("- 典型提示长度: {b} 字符\n"));
        }
        if !self.preferences.favorite_tools.is_empty() {
            s.push_str(&format!(
                "- 常用工具: {}\n",
                self.preferences.favorite_tools.join(", ")
            ));
        }
        if !self.preferences.favorite_slash_commands.is_empty() {
            s.push_str(&format!(
                "- 常用命令: {}\n",
                self.preferences.favorite_slash_commands.join(", ")
            ));
        }
        if !self.preferences.active_hours.is_empty() {
            let hours: Vec<String> = self
                .preferences
                .active_hours
                .iter()
                .map(|h| format!("{h}:00"))
                .collect();
            s.push_str(&format!("- 活跃时段: {}\n", hours.join(", ")));
        }
        s.push_str(&format!(
            "- 反馈: 👍 {} 👎 {} (置信度 {:.0}%)\n",
            self.signals.positive_feedback,
            self.signals.negative_feedback,
            self.preferences.confidence * 100.0
        ));
        s
    }

    /// 重置所有学习数据
    pub fn reset(&mut self) {
        self.signals = Signals::default();
        self.preferences = Preferences::default();
        self.created_at = now_ms();
        self.updated_at = now_ms();
    }
}

fn record_tokens(map: &mut HashMap<String, u32>, text: &str) {
    let mut seen: HashSet<String> = HashSet::new();
    // 简单分词：英文按 word，中文按 2-gram
    let mut word = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            if !word.is_empty() {
                let w = word.to_lowercase();
                if w.len() >= 3 && !is_stopword(&w) {
                    seen.insert(w);
                }
                word.clear();
            }
        } else if (c as u32) < 128 {
            word.push(c);
        } else {
            // 中文 / 其他 unicode：flush current word
            if !word.is_empty() {
                let w = word.to_lowercase();
                if w.len() >= 3 && !is_stopword(&w) {
                    seen.insert(w);
                }
                word.clear();
            }
        }
    }
    if !word.is_empty() {
        let w = word.to_lowercase();
        if w.len() >= 3 && !is_stopword(&w) {
            seen.insert(w);
        }
    }
    for t in seen {
        *map.entry(t).or_insert(0) += 1;
    }
}

fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        "the" | "and" | "for" | "are" | "but" | "not" | "you" | "all"
            | "can" | "her" | "was" | "one" | "our" | "had" | "has"
            | "this" | "that" | "with" | "from" | "they" | "have"
            | "what" | "when" | "where" | "which" | "who" | "how"
    )
}

fn top_n_keys(map: &HashMap<String, u32>, n: usize) -> Vec<String> {
    let mut v: Vec<(&String, &u32)> = map.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    v.into_iter()
        .take(n)
        .map(|(k, _)| k.clone())
        .collect()
}

fn top_n_keys_u32(map: &HashMap<u32, u32>, n: usize) -> Vec<u32> {
    let mut v: Vec<(&u32, &u32)> = map.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    v.into_iter().take(n).map(|(k, _)| *k).collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn current_hour() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 简化：以 UTC 小时为代理（实际应用按本地时区解析）
    let hour = (secs / 3600) % 24;
    // 把 UTC 转到东八区近似
    ((hour + 8) % 24) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_chat_buckets_short() {
        let mut l = Learning::new();
        l.record_chat("MiniMax-M3", "hi");
        l.record_chat("MiniMax-M3", "hello world");
        assert_eq!(l.signals.prompt_length_buckets.get("0-50").copied(), Some(2));
    }

    #[test]
    fn record_chat_buckets_long() {
        let mut l = Learning::new();
        l.record_chat("m", &"a".repeat(500));
        assert_eq!(l.signals.prompt_length_buckets.get("200-1000").copied(), Some(1));
    }

    #[test]
    fn record_chat_language() {
        let mut l = Learning::new();
        l.record_chat("m", "hello world");
        l.record_chat("m", "你好世界");
        assert_eq!(l.signals.languages.get("en").copied(), Some(1));
        assert_eq!(l.signals.languages.get("zh").copied(), Some(1));
    }

    #[test]
    fn record_tool_call() {
        let mut l = Learning::new();
        l.record_tool_call("read_file");
        l.record_tool_call("read_file");
        l.record_tool_call("write_file");
        assert_eq!(l.signals.tool_usage.get("read_file").copied(), Some(2));
        assert_eq!(l.signals.tool_usage.get("write_file").copied(), Some(1));
    }

    #[test]
    fn record_slash_command() {
        let mut l = Learning::new();
        l.record_slash_command("/lint");
        l.record_slash_command("/lint");
        l.record_slash_command("/queue");
        assert_eq!(l.signals.slash_usage.get("/lint").copied(), Some(2));
    }

    #[test]
    fn compute_preferences_picks_most_used_model() {
        let mut l = Learning::new();
        l.record_chat("m1", "x");
        l.record_chat("m1", "x");
        l.record_chat("m2", "x");
        l.compute_preferences();
        assert_eq!(l.preferences.default_model.as_deref(), Some("m1"));
    }

    #[test]
    fn compute_preferences_favorite_tools() {
        let mut l = Learning::new();
        for _ in 0..3 {
            l.record_tool_call("read_file");
        }
        for _ in 0..2 {
            l.record_tool_call("bash");
        }
        l.compute_preferences();
        assert!(l.preferences.favorite_tools.contains(&"read_file".to_string()));
    }

    #[test]
    fn inject_text_empty_when_low_confidence() {
        let l = Learning::new();
        assert_eq!(l.inject_text(), "");
    }

    #[test]
    fn inject_text_with_data() {
        let mut l = Learning::new();
        for _ in 0..10 {
            l.record_chat("MiniMax-M3", "fix this bug in my code");
            l.record_tool_call("read_file");
        }
        l.compute_preferences();
        let text = l.inject_text();
        assert!(text.contains("MiniMax-M3"));
        assert!(text.contains("read_file"));
    }

    #[test]
    fn reset_clears_everything() {
        let mut l = Learning::new();
        l.record_chat("m", "test");
        l.record_tool_call("bash");
        l.reset();
        assert_eq!(l.signals.total_chats, 0);
        assert_eq!(l.signals.total_tool_calls, 0);
        assert!(l.signals.model_usage.is_empty());
    }

    #[test]
    fn stopwords_filtered() {
        let mut l = Learning::new();
        l.record_chat("m", "the quick brown fox jumps over the lazy dog");
        // "the" 是 stopword，不应出现
        assert!(!l.signals.frequent_tokens.contains_key("the"));
        // "quick" 应该出现
        assert!(l.signals.frequent_tokens.contains_key("quick"));
    }
}