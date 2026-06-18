//! v1.9.11：任务队列持久化（重启可恢复）
//!
//! 设计：
//! - 持久化任务列表到 `~/.agentshell/queue/persist.json`
//! - **每次**任务状态变化后增量写盘（防失风险）
//! - 加载时把 Pending/Running 重置为 Pending（中断的 Running 重跑）
//! - 完成 / 失败 / 取消 状态保留
//!
//! 持久化的内容：
//! - Task 全部字段（id / kind / title / status / progress / input / ...）
//! - 关联 session id（用于在重启后查询 chat 历史）
//!
//! 路径：
//! - `~/.agentshell/queue/persist.json`

use crate::{Task, TaskStatus};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, PersistError>;

/// 持久化的根结构
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedQueue {
    /// schema 版本（用于未来兼容）
    #[serde(default = "default_version")]
    pub version: String,
    /// 最后一次写入时间（rfc3339）
    #[serde(default)]
    pub updated_at: String,
    /// 任务列表（按时间顺序）
    pub tasks: Vec<Task>,
}

fn default_version() -> String {
    "1".to_string()
}

/// 默认目录
pub fn default_persist_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    home.join(".agentshell").join("queue").join("persist.json")
}

/// 持久化器
pub struct QueuePersister {
    path: PathBuf,
}

impl QueuePersister {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_default() -> Self {
        Self::new(default_persist_path())
    }

    fn ensure_dir(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    /// 保存任务列表
    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        self.ensure_dir()?;
        let persisted = PersistedQueue {
            version: default_version(),
            updated_at: chrono_now_iso(),
            tasks: tasks.to_vec(),
        };
        let text = serde_json::to_string_pretty(&persisted)?;
        std::fs::write(&self.path, text)?;
        Ok(())
    }

    /// 读取任务列表
    pub fn load(&self) -> Result<PersistedQueue> {
        if !self.path.exists() {
            return Ok(PersistedQueue::default());
        }
        let text = std::fs::read_to_string(&self.path)?;
        let p: PersistedQueue = serde_json::from_str(&text).unwrap_or_default();
        Ok(p)
    }

    /// 加载并把 Running 重置为 Pending（重启后这些任务应重跑）
    pub fn load_recoverable(&self) -> Result<Vec<Task>> {
        let mut p = self.load()?;
        for t in p.tasks.iter_mut() {
            if matches!(t.status, TaskStatus::Running) {
                t.status = TaskStatus::Pending;
                t.started_at = None;
                t.progress = 0.0;
                t.log.push("[recover] 重启后重置为 Pending".to_string());
            }
        }
        Ok(p.tasks)
    }

    /// 路径
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn chrono_now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Task, TaskKind};
    use std::env;

    fn temp_path(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("agentshell_queue_persist_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("persist.json")
    }

    #[test]
    fn default_persist_path_under_home() {
        let p = default_persist_path();
        assert!(p.to_string_lossy().contains(".agentshell"));
        assert!(p.to_string_lossy().ends_with("persist.json"));
    }

    #[test]
    fn save_load_roundtrip() {
        let p = temp_path("roundtrip");
        let persister = QueuePersister::new(p.clone());
        let t1 = Task::new(TaskKind::Command, "echo hi", serde_json::json!({"cmd": "echo hi"}));
        let t2 = Task::new(TaskKind::Lint, "scan", serde_json::json!({})).with_session("s1");
        persister.save(&[t1, t2]).expect("save");
        let loaded = persister.load().expect("load");
        assert_eq!(loaded.tasks.len(), 2);
        assert_eq!(loaded.tasks[0].title, "echo hi");
        assert_eq!(loaded.tasks[1].session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn load_missing_returns_empty() {
        let p = temp_path("missing");
        let persister = QueuePersister::new(p);
        let loaded = persister.load().expect("load");
        assert_eq!(loaded.tasks.len(), 0);
    }

    #[test]
    fn recoverable_resets_running_to_pending() {
        let p = temp_path("recover");
        let persister = QueuePersister::new(p);
        let mut t1 = Task::new(TaskKind::Command, "x", serde_json::json!({}));
        t1.status = TaskStatus::Running;
        t1.progress = 0.5;
        t1.started_at = Some(12345);
        let mut t2 = Task::new(TaskKind::Command, "y", serde_json::json!({}));
        t2.status = TaskStatus::Completed;
        t2.progress = 1.0;
        persister.save(&[t1, t2]).expect("save");

        let recovered = persister.load_recoverable().expect("load");
        assert_eq!(recovered.len(), 2);
        // 第一个被重置
        let r0 = &recovered[0];
        assert_eq!(r0.status, TaskStatus::Pending);
        assert_eq!(r0.progress, 0.0);
        assert!(r0.started_at.is_none());
        assert!(r0.log.iter().any(|l| l.contains("recover")));
        // 第二个保留 Completed
        assert_eq!(recovered[1].status, TaskStatus::Completed);
    }

    #[test]
    fn ensure_dir_creates_parents() {
        let p = temp_path("ensure").join("sub/dir/persist.json");
        let persister = QueuePersister::new(p.clone());
        persister.save(&[]).expect("save");
        assert!(p.exists());
    }
}