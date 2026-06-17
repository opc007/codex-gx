//! AgentShell 跨会话长期记忆
//!
//! 设计：
//! - 简单 TF-IDF + 关键词匹配（不依赖 embedding API）
//! - 持久化到 ~/.agentshell/memory.json
//! - 每条记忆包含：id, content, tags, importance (1-5), created_at, accessed_count
//! - retrieve(query, k=5) 返回最相关的 k 条记忆

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

/// 单条记忆
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    /// 标签（手动或自动提取的关键词）
    #[serde(default)]
    pub tags: Vec<String>,
    /// 重要性 1-5（手动或自动）
    #[serde(default = "default_importance")]
    pub importance: u8,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 上次访问时间
    #[serde(default)]
    pub last_accessed_at: Option<DateTime<Utc>>,
    /// 访问次数
    #[serde(default)]
    pub accessed_count: u32,
    /// 来源会话 ID（可选）
    #[serde(default)]
    pub session_id: Option<String>,
}

fn default_importance() -> u8 {
    3
}

/// 内存存储
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MemoryStore {
    pub memories: Vec<Memory>,
    /// IDF 缓存（避免每次重建）
    #[serde(skip)]
    pub idf_cache: HashMap<String, f32>,
}

/// 跨会话记忆管理器
#[derive(Debug, Clone)]
pub struct MemoryManager {
    inner: Arc<Mutex<MemoryStore>>,
    storage_path: PathBuf,
}

impl MemoryManager {
    /// 创建新 manager（内存）
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MemoryStore::default())),
            storage_path: PathBuf::from(""),
        }
    }

    /// 从文件加载（或新建）
    pub async fn load_or_new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let store = if path.exists() {
            let data = tokio::fs::read(&path).await?;
            if data.is_empty() {
                MemoryStore::default()
            } else {
                serde_json::from_slice::<MemoryStore>(&data).unwrap_or_else(|e| {
                    warn!("记忆文件解析失败，使用空 store: {}", e);
                    MemoryStore::default()
                })
            }
        } else {
            MemoryStore::default()
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(store)),
            storage_path: path,
        })
    }

    /// 默认路径 ~/.agentshell/memory.json
    pub async fn default_path() -> Result<Self> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let path = PathBuf::from(home).join(".agentshell").join("memory.json");
        Self::load_or_new(path).await
    }

    /// 添加一条记忆
    pub async fn add(&self, content: String, tags: Vec<String>, importance: u8) -> Result<Memory> {
        self.add_with_session(content, tags, importance, None).await
    }

    pub async fn add_with_session(
        &self,
        content: String,
        tags: Vec<String>,
        importance: u8,
        session_id: Option<String>,
    ) -> Result<Memory> {
        let memory = Memory {
            id: uuid::Uuid::new_v4().to_string(),
            content,
            tags,
            importance: importance.clamp(1, 5),
            created_at: Utc::now(),
            last_accessed_at: None,
            accessed_count: 0,
            session_id,
        };
        {
            let mut store = self.inner.lock().await;
            store.memories.push(memory.clone());
        }
        self.save().await?;
        Ok(memory)
    }

    /// 列出所有记忆
    pub async fn list(&self) -> Vec<Memory> {
        let store = self.inner.lock().await;
        store.memories.clone()
    }

    /// 删除一条
    pub async fn forget(&self, id: &str) -> Result<bool> {
        let removed;
        {
            let mut store = self.inner.lock().await;
            let before = store.memories.len();
            store.memories.retain(|m| m.id != id);
            removed = store.memories.len() < before;
        }
        if removed {
            self.save().await?;
        }
        Ok(removed)
    }

    /// 清空所有
    pub async fn clear(&self) -> Result<()> {
        {
            let mut store = self.inner.lock().await;
            store.memories.clear();
        }
        self.save().await
    }

    /// 检索：返回与 query 最相关的 k 条记忆
    pub async fn retrieve(&self, query: &str, k: usize) -> Vec<Memory> {
        let mut store = self.inner.lock().await;
        if store.memories.is_empty() {
            return Vec::new();
        }
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // 重建 IDF（如未构建）
        if store.idf_cache.is_empty() {
            store.idf_cache = compute_idf(&store.memories);
        }

        let now = Utc::now();
        let mut scored: Vec<(f32, usize)> = store
            .memories
            .iter()
            .enumerate()
            .map(|(idx, mem)| {
                let score = score_memory(mem, &query_tokens, &store.idf_cache);
                (score, idx)
            })
            .collect();
        // 按分数倒序
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // 取 top-k，更新访问计数
        let top: Vec<Memory> = scored
            .into_iter()
            .take(k)
            .filter(|(s, _)| *s > 0.0)
            .map(|(_, idx)| {
                let mem = &mut store.memories[idx];
                mem.accessed_count += 1;
                mem.last_accessed_at = Some(now);
                mem.clone()
            })
            .collect();

        top
    }

    /// 把检索结果格式化为可注入 system prompt 的字符串
    pub async fn recall_context(&self, query: &str, k: usize) -> String {
        let memories = self.retrieve(query, k).await;
        if memories.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n[相关历史记忆]\n");
        for (i, m) in memories.iter().enumerate() {
            let tag_str = if m.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", m.tags.join(", "))
            };
            out.push_str(&format!(
                "{}. (重要度 {}/5){tag_str} {}\n",
                i + 1,
                m.importance,
                m.content
            ));
        }
        out
    }

    /// 持久化到文件
    pub async fn save(&self) -> Result<()> {
        if self.storage_path.as_os_str().is_empty() {
            return Ok(()); // 内存模式不保存
        }
        let store = self.inner.lock().await;
        if let Some(parent) = self.storage_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(&*store)?;
        tokio::fs::write(&self.storage_path, json).await?;
        Ok(())
    }
}

// ============================================================
// 文本相似度（简单 TF-IDF）
// ============================================================

fn tokenize(s: &str) -> HashSet<String> {
    // 中文按字符切分，英文按 word 切分
    let mut out = HashSet::new();
    let mut buf = String::new();
    let flush_buf = |buf: &mut String, out: &mut HashSet<String>| {
        if buf.chars().count() >= 2 {
            out.insert(buf.clone());
        }
        buf.clear();
    };
    for c in s.chars() {
        let is_cjk = (c as u32) >= 0x4E00 && (c as u32) <= 0x9FFF;
        if is_cjk {
            // CJK：先 flush 英文 buffer，再把单字符加入
            flush_buf(&mut buf, &mut out);
            out.insert(c.to_string());
        } else if c.is_alphanumeric() {
            buf.push(c.to_ascii_lowercase());
        } else {
            flush_buf(&mut buf, &mut out);
        }
    }
    flush_buf(&mut buf, &mut out);
    out
}

fn compute_idf(memories: &[Memory]) -> HashMap<String, f32> {
    let n = memories.len() as f32;
    let mut df: HashMap<String, u32> = HashMap::new();
    for m in memories {
        let tokens = tokenize(&m.content);
        let combined: HashSet<String> = tokens.into_iter().chain(m.tags.iter().cloned()).collect();
        for t in combined {
            *df.entry(t).or_insert(0) += 1;
        }
    }
    df.into_iter()
        .map(|(term, count)| {
            let idf = (n / (count as f32 + 1.0)).ln() + 1.0;
            (term, idf)
        })
        .collect()
}

fn tfidf(tokens: &HashSet<String>, idf: &HashMap<String, f32>) -> f32 {
    tokens
        .iter()
        .map(|t| idf.get(t).copied().unwrap_or(0.5))
        .sum()
}

fn score_memory(mem: &Memory, query_tokens: &HashSet<String>, idf: &HashMap<String, f32>) -> f32 {
    let mem_tokens = tokenize(&mem.content);
    // 重叠
    let overlap: usize = query_tokens.intersection(&mem_tokens).count();
    if overlap == 0 {
        // 检查 tags
        let tag_set: HashSet<String> = mem.tags.iter().cloned().collect();
        let tag_overlap = query_tokens.intersection(&tag_set).count();
        if tag_overlap == 0 {
            return 0.0;
        }
        return tag_overlap as f32 * 0.5 * mem.importance as f32;
    }
    let q_score = tfidf(query_tokens, idf);
    let m_score = tfidf(&mem_tokens, idf);
    // cosine-like 近似
    let score = (overlap as f32).sqrt() * m_score / (q_score + 1.0);
    score * (mem.importance as f32 / 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_add_and_list() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.json");
        let mgr = MemoryManager::load_or_new(&path).await.unwrap();
        mgr.add("用户偏好 Rust 编程".into(), vec!["偏好".into()], 5)
            .await
            .unwrap();
        let all = mgr.list().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].importance, 5);
    }

    #[tokio::test]
    async fn test_retrieve() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.json");
        let mgr = MemoryManager::load_or_new(&path).await.unwrap();
        mgr.add("用户喜欢用 Rust 写代码".into(), vec!["rust".into()], 4)
            .await
            .unwrap();
        mgr.add("用户喜欢喝咖啡".into(), vec!["习惯".into()], 2)
            .await
            .unwrap();
        let top = mgr.retrieve("帮我写个 Rust 函数", 1).await;
        assert_eq!(top.len(), 1);
        assert!(top[0].content.contains("Rust"));
    }

    #[tokio::test]
    async fn test_forget() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.json");
        let mgr = MemoryManager::load_or_new(&path).await.unwrap();
        let mem = mgr.add("临时记忆".into(), vec![], 1).await.unwrap();
        assert_eq!(mgr.list().await.len(), 1);
        assert!(mgr.forget(&mem.id).await.unwrap());
        assert_eq!(mgr.list().await.len(), 0);
    }

    #[test]
    fn test_tokenize_chinese_english() {
        let toks = tokenize("Rust 编程");
        assert!(toks.contains("rust"));
        // 中文单字符：基础 CJK 区 (U+4E00-U+9FFF) 如 "中文"
        let toks2 = tokenize("中文编程");
        assert!(toks2.contains("中"));
        assert!(toks2.contains("文"));
    }
}
