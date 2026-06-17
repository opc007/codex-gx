//! v1.8：Background Terminal Tauri 命令
//!
//! ## 注册的命令
//! - `bg_list`         — 列出所有
//! - `bg_list_running` — 列出 running
//! - `bg_get`          — 按 id
//! - `bg_spawn`        — 启动一个后台进程
//! - `bg_stop`         — 停一个
//! - `bg_stop_all`     — 停全部
//! - `bg_tail`         — 看输出 tail

use background::{BackgroundManager, BackgroundTerminal, BgStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

pub type BackgroundState = Arc<Mutex<BackgroundManager>>;

pub fn build_state() -> BackgroundState {
    Arc::new(Mutex::new(BackgroundManager::new()))
}

#[derive(Serialize)]
pub struct BgInfo {
    pub id: String,
    pub label: String,
    pub command: String,
    pub started_at: i64,
    pub pid: u32,
    pub status: String,
    pub status_label: String,
    pub log_path: Option<String>,
    pub exit_code: Option<i32>,
    pub tail: String,
}

impl From<&BackgroundTerminal> for BgInfo {
    fn from(t: &BackgroundTerminal) -> Self {
        BgInfo {
            id: t.id.clone(),
            label: t.label.clone(),
            command: t.command.clone(),
            started_at: t.started_at,
            pid: t.pid,
            status: format!("{:?}", t.status).to_lowercase(),
            status_label: t.status.label().to_string(),
            log_path: t.log_path.as_ref().map(|p| p.display().to_string()),
            exit_code: t.exit_code,
            tail: t.tail_text(),
        }
    }
}

#[tauri::command]
pub async fn bg_list(state: State<'_, BackgroundState>) -> Result<Vec<BgInfo>, String> {
    let mgr = state.lock().await;
    Ok(mgr.list().iter().map(BgInfo::from).collect())
}

#[tauri::command]
pub async fn bg_list_running(state: State<'_, BackgroundState>) -> Result<Vec<BgInfo>, String> {
    let mgr = state.lock().await;
    Ok(mgr.list_running().iter().map(BgInfo::from).collect())
}

#[tauri::command]
pub async fn bg_get(id: String, state: State<'_, BackgroundState>) -> Result<Option<BgInfo>, String> {
    let mgr = state.lock().await;
    Ok(mgr.get(&id).as_ref().map(BgInfo::from))
}

#[derive(Deserialize)]
pub struct SpawnArgs {
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[tauri::command]
pub async fn bg_spawn(
    args: SpawnArgs,
    state: State<'_, BackgroundState>,
) -> Result<BgInfo, String> {
    let mgr = state.lock().await;
    let arg_refs: Vec<&str> = args.args.iter().map(|s| s.as_str()).collect();
    let t = mgr
        .spawn(&args.label, &args.command, &arg_refs)
        .await
        .map_err(|e| e.to_string())?;
    Ok(BgInfo::from(&t))
}

#[tauri::command]
pub async fn bg_stop(id: String, state: State<'_, BackgroundState>) -> Result<bool, String> {
    let mgr = state.lock().await;
    mgr.stop(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn bg_stop_all(state: State<'_, BackgroundState>) -> Result<usize, String> {
    let mgr = state.lock().await;
    Ok(mgr.stop_all())
}

#[tauri::command]
pub async fn bg_tail(
    id: String,
    state: State<'_, BackgroundState>,
) -> Result<String, String> {
    let mgr = state.lock().await;
    Ok(mgr.get(&id).map(|t| t.tail_text()).unwrap_or_default())
}
