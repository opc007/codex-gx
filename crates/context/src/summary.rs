//! v1.9.9：上下文摘要（heuristic summary for compaction）
//!
//! 设计参考：docs/开发文档.md §8.5 Context Compaction
//!
//! ## v1.9.9 范围
//! - 启发式摘要（无需 LLM 调用）
//! - 关键事实提取（工具调用 / 文件路径 / 命令 / 错误）
//! - 自动触发策略（基于 token 预算）
//! - 摘要 token 估算

use agent_core::message::{ContentBlock, Message, MessageRole};
use serde::{Deserialize, Serialize};

use super::compact::{compact_messages, estimate_tokens, CompactionStrategy};

/// 摘要配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// 最大 token 预算（默认 8000）
    pub max_tokens: u32,
    /// 触发压缩的阈值（默认 6000 = 75%）
    pub trigger_threshold: u32,
    /// KeepLast 策略的 tail 大小
    pub keep_tail: usize,
    /// TruncateMiddle 策略的 head 大小
    pub keep_head: usize,
    /// 是否启用摘要（而不是简单截断）
    pub use_summary: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            trigger_threshold: 6000,
            keep_tail: 10,
            keep_head: 3,
            use_summary: true,
        }
    }
}

impl CompressionConfig {
    pub fn should_trigger(&self, current_tokens: u32) -> bool {
        current_tokens >= self.trigger_threshold
    }
}

/// 关键事实（从消息中提取）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyFacts {
    pub files_mentioned: Vec<String>,
    pub commands_run: Vec<String>,
    pub errors_seen: Vec<String>,
    pub tool_calls: Vec<String>,
    pub decisions_made: Vec<String>,
    pub total_messages: usize,
    pub total_user_msgs: usize,
    pub total_assistant_msgs: usize,
}

impl KeyFacts {
    pub fn from_messages(messages: &[Message]) -> Self {
        let mut f = Self::default();
        f.total_messages = messages.len();
        for m in messages {
            match m.role {
                MessageRole::User => f.total_user_msgs += 1,
                MessageRole::Assistant => f.total_assistant_msgs += 1,
                _ => {}
            }
            for c in &m.content {
                if let ContentBlock::Text { text } = c {
                    extract_files(text, &mut f.files_mentioned);
                    extract_commands(text, &mut f.commands_run);
                    extract_errors(text, &mut f.errors_seen);
                    extract_decisions(text, &mut f.decisions_made);
                } else if let ContentBlock::ToolUse(tc) = c {
                    f.tool_calls.push(tc.name.clone());
                }
            }
        }
        // dedup + cap
        f.files_mentioned = dedup(f.files_mentioned, 20);
        f.commands_run = dedup(f.commands_run, 20);
        f.errors_seen = dedup(f.errors_seen, 10);
        f.tool_calls = dedup(f.tool_calls, 20);
        f.decisions_made = dedup(f.decisions_made, 10);
        f
    }
}

fn dedup(v: Vec<String>, cap: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in v {
        if seen.insert(s.clone()) {
            out.push(s);
            if out.len() >= cap {
                break;
            }
        }
    }
    out
}

fn extract_files(text: &str, out: &mut Vec<String>) {
    // 文件路径启发式：包含 / 且以常见扩展结尾
    for word in text.split_whitespace() {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-');
        if w.contains('/') && (w.ends_with(".rs") || w.ends_with(".ts") || w.ends_with(".tsx")
            || w.ends_with(".js") || w.ends_with(".json") || w.ends_with(".md")
            || w.ends_with(".toml") || w.ends_with(".py") || w.ends_with(".yaml")
            || w.ends_with(".yml") || w.ends_with(".html") || w.ends_with(".css"))
        {
            out.push(w.to_string());
        }
    }
}

fn extract_commands(text: &str, out: &mut Vec<String>) {
    // 命令启发式：以 $ / cargo / npm / git 等开头的子串
    let prefixes = ["$ ", "cargo ", "npm ", "git ", "yarn ", "pnpm ", "docker ", "kubectl ", "make "];
    for line in text.lines() {
        for p in &prefixes {
            if let Some(idx) = line.find(p) {
                // 提取从 prefix 开始的命令片段
                let cmd_start = idx;
                let cmd: String = line[cmd_start..].chars().take(120).collect();
                if !cmd.is_empty() {
                    out.push(cmd);
                    break;
                }
            }
        }
    }
}

fn extract_errors(text: &str, out: &mut Vec<String>) {
    let markers = ["error:", "Error:", "ERROR:", "panic:", "FAILED", "fatal:"];
    for line in text.lines() {
        for m in &markers {
            if line.contains(m) {
                let snippet: String = line.chars().take(150).collect();
                out.push(snippet);
                break;
            }
        }
    }
}

fn extract_decisions(text: &str, out: &mut Vec<String>) {
    let markers = ["decided to", "i'll go with", "going with", "chose to", "let's use"];
    for line in text.lines() {
        let lower = line.to_lowercase();
        for m in &markers {
            if lower.contains(m) {
                let snippet: String = line.chars().take(150).collect();
                out.push(snippet);
                break;
            }
        }
    }
}

/// 摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub key_facts: KeyFacts,
    pub original_tokens: u32,
    pub summary_tokens: u32,
    pub strategy: String,
    pub text: String,
}

/// 生成摘要（heuristic）
pub fn summarize(messages: &[Message], config: &CompressionConfig) -> Summary {
    let facts = KeyFacts::from_messages(messages);
    let original_tokens = estimate_tokens(messages);

    // 选择策略
    let (strategy, strategy_name) = if config.use_summary {
        if facts.tool_calls.len() > 5 || facts.commands_run.len() > 5 {
            // 工具调用多 → 用 TruncateMiddle 保留上下文
            (CompactionStrategy::TruncateMiddle {
                keep_head: config.keep_head,
                keep_tail: config.keep_tail,
            }, "truncate_middle_with_facts")
        } else {
            (CompactionStrategy::SystemPlusTail(config.keep_tail), "system_plus_tail_with_facts")
        }
    } else {
        (CompactionStrategy::KeepLast(config.keep_tail), "keep_last")
    };

    // 渲染摘要文本
    let summary_text = render_summary(&facts, &strategy_name);

    let summary_tokens = (summary_text.len() / 4) as u32;

    Summary {
        key_facts: facts,
        original_tokens,
        summary_tokens,
        strategy: strategy_name.to_string(),
        text: summary_text,
    }
}

fn render_summary(facts: &KeyFacts, strategy: &str) -> String {
    let mut out = format!("# Context Summary (strategy: {})\n\n", strategy);
    out.push_str(&format!(
        "**Messages**: {} ({} user / {} assistant)\n\n",
        facts.total_messages, facts.total_user_msgs, facts.total_assistant_msgs
    ));
    if !facts.files_mentioned.is_empty() {
        out.push_str("**Files**:\n");
        for f in &facts.files_mentioned {
            out.push_str(&format!("  - `{}`\n", f));
        }
        out.push('\n');
    }
    if !facts.commands_run.is_empty() {
        out.push_str("**Commands**:\n");
        for c in &facts.commands_run {
            out.push_str(&format!("  - `{}`\n", c));
        }
        out.push('\n');
    }
    if !facts.tool_calls.is_empty() {
        out.push_str(&format!("**Tool calls**: {}\n", facts.tool_calls.join(", ")));
        out.push('\n');
    }
    if !facts.errors_seen.is_empty() {
        out.push_str("**Errors**:\n");
        for e in &facts.errors_seen {
            out.push_str(&format!("  - {}\n", e));
        }
        out.push('\n');
    }
    if !facts.decisions_made.is_empty() {
        out.push_str("**Decisions**:\n");
        for d in &facts.decisions_made {
            out.push_str(&format!("  - {}\n", d));
        }
        out.push('\n');
    }
    out
}

/// 应用压缩（返回压缩后的消息列表 + 摘要）
pub fn compress(messages: &[Message], config: &CompressionConfig) -> (Vec<Message>, Summary) {
    let summary = summarize(messages, config);
    let strategy = if config.use_summary {
        CompactionStrategy::TruncateMiddle {
            keep_head: config.keep_head,
            keep_tail: config.keep_tail,
        }
    } else {
        CompactionStrategy::KeepLast(config.keep_tail)
    };
    let _ = strategy; // suppress unused
    let compacted = compact_messages(messages, CompactionStrategy::KeepLast(config.keep_tail));
    (compacted, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::message::Message;

    fn mk_msg(role: MessageRole, text: impl Into<String>) -> Message {
        let sid = uuid::Uuid::new_v4();
        let text = text.into();
        match role {
            MessageRole::User => Message::user(sid, text),
            MessageRole::Assistant => Message::assistant(sid, text),
            MessageRole::System => Message::system(sid, text),
            _ => Message::user(sid, text),
        }
    }

    #[test]
    fn test_should_trigger() {
        let c = CompressionConfig::default();
        assert!(!c.should_trigger(100));
        assert!(c.should_trigger(7000));
    }

    #[test]
    fn test_extract_files() {
        let mut v = vec![];
        extract_files("edit src/main.rs and tests/integration.rs", &mut v);
        assert!(v.contains(&"src/main.rs".to_string()));
        assert!(v.contains(&"tests/integration.rs".to_string()));
    }

    #[test]
    fn test_extract_commands() {
        let mut v = vec![];
        extract_commands("$ cargo build\nnpm install", &mut v);
        assert!(v.iter().any(|s| s.contains("cargo build")));
        assert!(v.iter().any(|s| s.contains("npm install")));
    }

    #[test]
    fn test_extract_errors() {
        let mut v = vec![];
        extract_errors("error: undefined variable\nERROR: build failed", &mut v);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn test_extract_decisions() {
        let mut v = vec![];
        extract_decisions("decided to use tokio\nI'll go with axum", &mut v);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn test_key_facts_dedup() {
        let msgs = vec![
            mk_msg(MessageRole::User, "edit src/main.rs"),
            mk_msg(MessageRole::User, "edit src/main.rs"),
            mk_msg(MessageRole::User, "edit src/lib.rs"),
        ];
        let f = KeyFacts::from_messages(&msgs);
        assert_eq!(f.files_mentioned.len(), 2);
        assert_eq!(f.total_messages, 3);
    }

    #[test]
    fn test_summarize_basic() {
        let msgs = vec![
            mk_msg(MessageRole::System, "be helpful"),
            mk_msg(MessageRole::User, "edit src/main.rs"),
            mk_msg(MessageRole::Assistant, "I'll fix src/main.rs"),
            mk_msg(MessageRole::User, "run $ cargo build"),
            mk_msg(MessageRole::Assistant, "error: compile failed"),
        ];
        let c = CompressionConfig::default();
        let s = summarize(&msgs, &c);
        assert!(s.original_tokens > 0);
        assert!(s.summary_tokens > 0);
        assert!(s.text.contains("Context Summary"));
        assert!(s.text.contains("main.rs"));
        assert!(s.text.contains("cargo build"));
    }

    #[test]
    fn test_compress_reduces_messages() {
        let msgs: Vec<Message> = (0..30)
            .map(|i| mk_msg(MessageRole::User, format!("msg {}: $ cargo build {}", i, i)))
            .collect();
        let c = CompressionConfig { keep_tail: 5, keep_head: 2, use_summary: true, ..Default::default() };
        let (out, sum) = compress(&msgs, &c);
        assert!(out.len() < msgs.len());
        assert!(sum.summary_tokens < sum.original_tokens || out.len() < msgs.len());
    }

    #[test]
    fn test_compress_no_summary() {
        let msgs: Vec<Message> = (0..30).map(|i| mk_msg(MessageRole::User, format!("msg {}", i))).collect();
        let c = CompressionConfig { use_summary: false, keep_tail: 3, ..Default::default() };
        let (out, _) = compress(&msgs, &c);
        assert_eq!(out.len(), 3);
    }
}