//! v1.4：Agent 任务队列
//!
//! 后台并发执行任务，不阻塞当前 chat session。
//! 任务状态机：pending → running → completed | failed | cancelled
//!
//! 设计：
//! - 每个任务有自己的 tokio task
//! - 状态变更通过 broadcast channel 通知订阅者
//! - 可取消：发送 oneshot 信号
//! - 并发度可配（默认 2）

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// 跑 agent loop（chat）
    Agent,
    /// 跑 shell 命令
    Command,
    /// 跑 lint
    Lint,
    /// 自由任务（任意 async closure）
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub kind: TaskKind,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub progress: f32, // 0.0 - 1.0
    pub log: Vec<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    /// 关联 chat session id
    pub session_id: Option<String>,
    /// 任务入参（kind-specific）
    pub input: serde_json::Value,
}

impl Task {
    pub fn new(kind: TaskKind, title: impl Into<String>, input: serde_json::Value) -> Self {
        let now = now_ms();
        Task {
            id: Uuid::new_v4().to_string(),
            kind,
            title: title.into(),
            description: None,
            status: TaskStatus::Pending,
            progress: 0.0,
            log: Vec::new(),
            result: None,
            error: None,
            created_at: now,
            started_at: None,
            completed_at: None,
            session_id: None,
            input,
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn duration_ms(&self) -> u64 {
        match (self.started_at, self.completed_at) {
            (Some(s), Some(e)) => e.saturating_sub(s),
            (Some(s), None) => now_ms().saturating_sub(s),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TaskEvent {
    Added(Task),
    Started(Task),
    Progress { id: String, progress: f32, log: Option<String> },
    Completed(Task),
    Failed(Task),
    Cancelled(Task),
}

/// 任务函数 trait — 用户实现
#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(
        &self,
        task: Task,
        progress: Box<dyn Fn(f32, Option<String>) + Send + Sync>,
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<String, String>;
}

pub struct Queue {
    tasks: Arc<RwLock<HashMap<String, Arc<Mutex<Task>>>>>,
    order: Arc<RwLock<Vec<String>>>,
    sender: mpsc::UnboundedSender<TaskCommand>,
    pub event_tx: broadcast::Sender<TaskEvent>,
    cancel_txs: Arc<RwLock<HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    concurrency: usize,
}

#[derive(Debug)]
enum TaskCommand {
    Schedule(String), // task id
    Cancel(String),
}

impl Queue {
    pub fn new(concurrency: usize) -> Arc<Self> {
        let (sender, receiver) = mpsc::unbounded_channel::<TaskCommand>();
        let (event_tx, _) = broadcast::channel(256);
        let q = Arc::new(Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            order: Arc::new(RwLock::new(Vec::new())),
            sender,
            event_tx: event_tx.clone(),
            cancel_txs: Arc::new(RwLock::new(HashMap::new())),
            concurrency: concurrency.max(1),
        });
        // 启动调度器
        let q_clone = q.clone();
        tokio::spawn(async move {
            Self::scheduler_loop(receiver, q_clone, concurrency).await;
        });
        q
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.event_tx.subscribe()
    }

    /// 添加任务到队列（不立即执行）
    pub async fn enqueue(&self, task: Task) -> String {
        let id = task.id.clone();
        let arc = Arc::new(Mutex::new(task.clone()));
        self.tasks.write().await.insert(id.clone(), arc);
        self.order.write().await.push(id.clone());
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        self.cancel_txs.write().await.insert(id.clone(), cancel_tx);
        let _ = self.event_tx.send(TaskEvent::Added(task));
        let _ = self.sender.send(TaskCommand::Schedule(id.clone()));
        id
    }

    pub async fn cancel(&self, id: &str) -> bool {
        if let Some(tx) = self.cancel_txs.write().await.get(id) {
            let _ = tx.send(true);
            true
        } else {
            false
        }
    }

    pub async fn list(&self) -> Vec<Task> {
        let order = self.order.read().await;
        let tasks = self.tasks.read().await;
        let mut out = Vec::new();
        for id in order.iter() {
            if let Some(t) = tasks.get(id) {
                out.push(t.lock().await.clone());
            }
        }
        out
    }

    pub async fn get(&self, id: &str) -> Option<Task> {
        let tasks = self.tasks.read().await;
        if let Some(t) = tasks.get(id) {
            Some(t.lock().await.clone())
        } else {
            None
        }
    }

    /// 移除已完成 / 失败的任务（清理）
    pub async fn clear_finished(&self) -> usize {
        let mut order = self.order.write().await;
        let mut tasks = self.tasks.write().await;
        let mut cancel_txs = self.cancel_txs.write().await;
        let original = order.len();
        order.retain(|id| {
            let keep = tasks
                .get(id)
                .map(|t| {
                    let s = t.try_lock().map(|t| t.status).unwrap_or(TaskStatus::Running);
                    matches!(s, TaskStatus::Pending | TaskStatus::Running)
                })
                .unwrap_or(false);
            if !keep {
                tasks.remove(id);
                cancel_txs.remove(id);
            }
            keep
        });
        original - order.len()
    }

    async fn scheduler_loop(
        mut receiver: mpsc::UnboundedReceiver<TaskCommand>,
        q: Arc<Self>,
        concurrency: usize,
    ) {
        loop {
            match receiver.recv().await {
                Some(TaskCommand::Schedule(_)) => {
                    Self::try_dispatch(&q, concurrency).await;
                }
                Some(TaskCommand::Cancel(id)) => {
                    q.cancel(&id).await;
                }
                None => break,
            }
        }
    }

    async fn try_dispatch(q: &Arc<Self>, concurrency: usize) {
        let running = {
            let tasks = q.tasks.read().await;
            tasks
                .values()
                .filter(|t| {
                    t.try_lock()
                        .map(|t| t.status == TaskStatus::Running)
                        .unwrap_or(false)
                })
                .count()
        };
        if running >= concurrency {
            return;
        }
        let order = q.order.read().await.clone();
        for id in order {
            let task_arc = {
                let tasks = q.tasks.read().await;
                tasks.get(&id).cloned()
            };
            if let Some(task_arc) = task_arc {
                let should_start = {
                    let t = task_arc.lock().await;
                    t.status == TaskStatus::Pending
                };
                if should_start {
                    Self::start_task(q.clone(), task_arc).await;
                    if running + 1 >= concurrency {
                        break;
                    }
                }
            }
        }
    }

    async fn start_task(q: Arc<Self>, task_arc: Arc<Mutex<Task>>) {
        let (id, kind, cancel_rx) = {
            let mut t = task_arc.lock().await;
            t.status = TaskStatus::Running;
            t.started_at = Some(now_ms());
            let id = t.id.clone();
            let kind = t.kind;
            let cancel_rx = q
                .cancel_txs
                .read()
                .await
                .get(&id)
                .map(|tx| tx.subscribe())
                .unwrap_or_else(|| {
                    let (tx, rx) = tokio::sync::watch::channel(false);
                    drop(tx);
                    rx
                });
            (id, kind, cancel_rx)
        };
        let task_snapshot = task_arc.lock().await.clone();
        let _ = q.event_tx.send(TaskEvent::Started(task_snapshot.clone()));

        let q_clone = q.clone();
        let task_arc_clone = task_arc.clone();
        let id_for_progress = id.clone();
        tokio::spawn(async move {
            let progress = Box::new(move |p: f32, log: Option<String>| {
                let q2 = q_clone.clone();
                let task_arc2 = task_arc_clone.clone();
                let id2 = id_for_progress.clone();
                tokio::spawn(async move {
                    let mut t = task_arc2.lock().await;
                    t.progress = p;
                    if let Some(l) = log {
                        t.log.push(l);
                    }
                    let snap = t.clone();
                    drop(t);
                    let _ = q2.event_tx.send(TaskEvent::Progress {
                        id: id2,
                        progress: snap.progress,
                        log: snap.log.last().cloned(),
                    });
                });
            });
            let res = match kind {
                TaskKind::Custom => Err("no custom executor registered".to_string()),
                TaskKind::Command => run_command(task_snapshot.clone(), progress, cancel_rx).await,
                TaskKind::Lint => run_lint(task_snapshot.clone(), progress, cancel_rx).await,
                TaskKind::Agent => Err("Agent kind not yet wired in queue executor".to_string()),
            };
            let mut t = task_arc.lock().await;
            t.completed_at = Some(now_ms());
            match res {
                Ok(output) => {
                    t.status = TaskStatus::Completed;
                    t.result = Some(output);
                    t.progress = 1.0;
                    let snap = t.clone();
                    drop(t);
                    let _ = q.event_tx.send(TaskEvent::Completed(snap));
                }
                Err(e) => {
                    if e == "__cancelled__" {
                        t.status = TaskStatus::Cancelled;
                        let snap = t.clone();
                        drop(t);
                        let _ = q.event_tx.send(TaskEvent::Cancelled(snap));
                    } else {
                        t.status = TaskStatus::Failed;
                        t.error = Some(e.clone());
                        t.log.push(format!("❌ {e}"));
                        let snap = t.clone();
                        drop(t);
                        let _ = q.event_tx.send(TaskEvent::Failed(snap));
                    }
                }
            }
        });
    }
}

// =============================================================================
// 内置 executor：shell 命令
// =============================================================================

async fn run_command(
    task: Task,
    progress: Box<dyn Fn(f32, Option<String>) + Send + Sync>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<String, String> {
    use tokio::process::Command;
    let cmd_str = task
        .input
        .get("cmd")
        .and_then(|v| v.as_str())
        .ok_or("missing 'cmd' in input")?;
    let cwd = task.input.get("cwd").and_then(|v| v.as_str());
    progress(0.1, Some(format!("$ {cmd_str}")));
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(cmd_str);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd_str);
        c
    };
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let child = tokio::spawn(async move { cmd.output().await });
    tokio::select! {
        res = child => {
            let out = res.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            progress(1.0, Some(format!("exit code {}", out.status.code().unwrap_or(-1))));
            Ok(format!("{}{}", stdout, if stderr.is_empty() { String::new() } else { format!("\n[stderr]\n{stderr}") }))
        }
        _ = cancel.changed() => {
            Err("__cancelled__".to_string())
        }
    }
}

// =============================================================================
// 内置 executor：lint 任务
// =============================================================================

async fn run_lint(
    task: Task,
    progress: Box<dyn Fn(f32, Option<String>) + Send + Sync>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<String, String> {
    let path = task
        .input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    progress(0.2, Some(format!("scanning {path}…")));
    let linter = lint::Linter::new();
    let root = std::path::PathBuf::from(path);
    let reports = tokio::select! {
        r = linter.run_all(&root) => r,
        _ = cancel.changed() => return Err("__cancelled__".to_string()),
    };
    let total_errors: u32 = reports
        .iter()
        .flat_map(|r| r.issues.iter())
        .filter(|i| i.severity == lint::Severity::Error)
        .count() as u32;
    let total_warnings: u32 = reports
        .iter()
        .flat_map(|r| r.issues.iter())
        .filter(|i| i.severity == lint::Severity::Warning)
        .count() as u32;
    let total_infos: u32 = reports
        .iter()
        .flat_map(|r| r.issues.iter())
        .filter(|i| i.severity == lint::Severity::Info)
        .count() as u32;
    progress(1.0, Some(format!("done")));
    Ok(serde_json::json!({
        "path": path,
        "errors": total_errors,
        "warnings": total_warnings,
        "infos": total_infos,
        "reports": reports,
    })
    .to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_new_fields() {
        let t = Task::new(TaskKind::Command, "echo hi", serde_json::json!({"cmd": "echo hi"}));
        assert_eq!(t.title, "echo hi");
        assert_eq!(t.kind, TaskKind::Command);
        assert_eq!(t.status, TaskStatus::Pending);
        assert_eq!(t.progress, 0.0);
        assert!(t.result.is_none());
    }

    #[test]
    fn task_with_session() {
        let t = Task::new(TaskKind::Lint, "scan", serde_json::json!({}))
            .with_session("sess-1")
            .with_description("scan all");
        assert_eq!(t.session_id.as_deref(), Some("sess-1"));
        assert_eq!(t.description.as_deref(), Some("scan all"));
    }

    #[tokio::test]
    async fn queue_enqueue_and_list() {
        let q = Queue::new(2);
        let id = q
            .enqueue(
                Task::new(TaskKind::Command, "noop", serde_json::json!({"cmd": "true"})),
            )
            .await;
        let list = q.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
    }

    #[tokio::test]
    async fn queue_cancel_existing() {
        let q = Queue::new(1);
        let id = q
            .enqueue(
                Task::new(TaskKind::Command, "long", serde_json::json!({"cmd": "sleep 5"})),
            )
            .await;
        // 给调度器一点时间启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let cancelled = q.cancel(&id).await;
        assert!(cancelled);
    }

    #[tokio::test]
    async fn queue_cancel_nonexistent() {
        let q = Queue::new(1);
        assert!(!q.cancel("nope").await);
    }

    #[tokio::test]
    async fn queue_clear_finished() {
        let q = Queue::new(1);
        let _id = q
            .enqueue(
                Task::new(TaskKind::Command, "echo done", serde_json::json!({"cmd": "echo done"})),
            )
            .await;
        // 等完成
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        let n = q.clear_finished().await;
        assert!(n >= 1);
    }

    #[test]
    fn task_kind_serialize() {
        let t = Task::new(TaskKind::Lint, "x", serde_json::json!({}));
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"kind\":\"lint\""));
        assert!(s.contains("\"status\":\"pending\""));
    }
}