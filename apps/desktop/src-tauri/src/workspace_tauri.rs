//! v1.3：多 Workspace 桥接命令
//!
//! 设计：
//! - 工作区是前端逻辑层概念（数据存 localStorage）
//! - session 仍存 plugin-store（按 workspaceId 字段过滤）
//! - 切换 workspace 时发 `workspace:changed` 事件，session store 监听并 reload
//! - 这些 command 主要做日志 / 广播，保留接口以备后端持久化扩展

use tauri::{AppHandle, Emitter};

/// 通知后端 workspace 已切换
/// （目前仅做日志 + 广播事件，前端 session store 监听这个事件 reload）
#[tauri::command]
pub fn workspace_changed_broadcast(app: AppHandle, workspace_id: String) {
    eprintln!("[v1.3] workspace changed -> {workspace_id}");
    let _ = app.emit("workspace:changed", workspace_id);
}
