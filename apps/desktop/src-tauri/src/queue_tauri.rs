//! v1.4：任务队列 tauri 命令 + 全局 state

use queue::{Queue, Task, TaskEvent, TaskKind};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub type QueueState = Arc<Queue>;

pub fn build_state() -> QueueState {
    Queue::new(2)
}

/// 在 setup 内调（确保在 tokio context 中启动调度器）
pub fn start_scheduler(state: QueueState) {
    // 走 tauri::async_runtime::spawn，让 tauri 来保证 tokio runtime 存在
    state.start_with(|fut| {
        tauri::async_runtime::spawn(fut);
    });
}

#[tauri::command]
pub async fn queue_list(state: tauri::State<'_, QueueState>) -> Result<Vec<Task>, String> {
    Ok(state.list().await)
}

#[tauri::command]
pub async fn queue_get(id: String, state: tauri::State<'_, QueueState>) -> Result<Option<Task>, String> {
    Ok(state.get(&id).await)
}

#[derive(serde::Deserialize)]
pub struct EnqueueArgs {
    pub kind: String,
    pub title: String,
    pub input: serde_json::Value,
    pub session_id: Option<String>,
    pub description: Option<String>,
}

#[tauri::command]
pub async fn queue_enqueue(
    args: EnqueueArgs,
    state: tauri::State<'_, QueueState>,
) -> Result<String, String> {
    let kind = match args.kind.as_str() {
        "agent" => TaskKind::Agent,
        "command" => TaskKind::Command,
        "lint" => TaskKind::Lint,
        _ => TaskKind::Custom,
    };
    let mut task = Task::new(kind, args.title, args.input);
    if let Some(sid) = args.session_id {
        task = task.with_session(sid);
    }
    if let Some(d) = args.description {
        task = task.with_description(d);
    }
    Ok(state.enqueue(task).await)
}

#[tauri::command]
pub async fn queue_cancel(id: String, state: tauri::State<'_, QueueState>) -> Result<bool, String> {
    Ok(state.cancel(&id).await)
}

#[tauri::command]
pub async fn queue_clear_finished(state: tauri::State<'_, QueueState>) -> Result<usize, String> {
    Ok(state.clear_finished().await)
}

/// 启动一个后台任务，把队列事件以 `queue:event` Tauri event 推给前端
pub fn spawn_event_forwarder(app: AppHandle, queue: QueueState) {
    let mut rx = queue.subscribe();
    tauri::async_runtime::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            let (kind, payload) = match ev {
                TaskEvent::Added(t) => ("added", serde_json::to_value(t).unwrap_or_default()),
                TaskEvent::Started(t) => ("started", serde_json::to_value(t).unwrap_or_default()),
                TaskEvent::Progress { id, progress, log } => (
                    "progress",
                    serde_json::json!({ "id": id, "progress": progress, "log": log }),
                ),
                TaskEvent::Completed(t) => ("completed", serde_json::to_value(t).unwrap_or_default()),
                TaskEvent::Failed(t) => ("failed", serde_json::to_value(t).unwrap_or_default()),
                TaskEvent::Cancelled(t) => ("cancelled", serde_json::to_value(t).unwrap_or_default()),
            };
            let _ = app.emit(
                "queue:event",
                serde_json::json!({ "kind": kind, "payload": payload }),
            );
        }
    });
}
