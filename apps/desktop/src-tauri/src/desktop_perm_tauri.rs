//! v1.9：App 白名单 + 权限系统 Tauri 命令
//!
//! ## 注册的命令
//! - `perm_get_list`           — 当前白名单
//! - `perm_add_allow`          — 加到 always_allow
//! - `perm_add_deny`           — 加到 denied
//! - `perm_clear_allow`        — 清空 always_allow
//! - `perm_decide`             — 决策：allow / ask / deny
//! - `perm_is_blacklisted`     — 是否强制黑名单

use desktop_perm::{AppMeta, PermissionDecision, PermissionList};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub type PermListState = Arc<Mutex<PermissionList>>;

pub fn build_state() -> PermListState {
    Arc::new(Mutex::new(PermissionList::load()))
}

#[derive(Serialize)]
pub struct PermListInfo {
    pub always_allow: Vec<String>,
    pub always_ask: Vec<String>,
    pub denied: Vec<String>,
    pub usage_count: std::collections::HashMap<String, u32>,
}

impl From<&PermissionList> for PermListInfo {
    fn from(l: &PermissionList) -> Self {
        PermListInfo {
            always_allow: l.always_allow.clone(),
            always_ask: l.always_ask.clone(),
            denied: l.denied.clone(),
            usage_count: l.usage_count.clone(),
        }
    }
}

#[tauri::command]
pub async fn perm_get_list(state: tauri::State<'_, PermListState>) -> Result<PermListInfo, String> {
    let l = state.lock().map_err(|e| e.to_string())?;
    Ok(PermListInfo::from(&*l))
}

#[tauri::command]
pub async fn perm_add_allow(
    key: String,
    state: tauri::State<'_, PermListState>,
) -> Result<PermListInfo, String> {
    let mut l = state.lock().map_err(|e| e.to_string())?;
    l.add_allow(key);
    l.save().map_err(|e| e.to_string())?;
    Ok(PermListInfo::from(&*l))
}

#[tauri::command]
pub async fn perm_add_deny(
    key: String,
    state: tauri::State<'_, PermListState>,
) -> Result<PermListInfo, String> {
    let mut l = state.lock().map_err(|e| e.to_string())?;
    l.add_deny(key);
    l.save().map_err(|e| e.to_string())?;
    Ok(PermListInfo::from(&*l))
}

#[tauri::command]
pub async fn perm_clear_allow(
    state: tauri::State<'_, PermListState>,
) -> Result<PermListInfo, String> {
    let mut l = state.lock().map_err(|e| e.to_string())?;
    l.clear_allow();
    l.save().map_err(|e| e.to_string())?;
    Ok(PermListInfo::from(&*l))
}

#[tauri::command]
pub async fn perm_decide(
    app: AppMeta,
    state: tauri::State<'_, PermListState>,
) -> Result<String, String> {
    let l = state.lock().map_err(|e| e.to_string())?;
    let d = l.decide(&app);
    Ok(format!("{:?}", d).to_lowercase())
}

#[tauri::command]
pub fn perm_is_blacklisted(app: AppMeta) -> bool {
    desktop_perm::is_blacklisted(&app)
}

#[derive(Deserialize)]
pub struct DecideRequest {
    pub bundle_id: Option<String>,
    pub display_name: String,
    pub process_name: String,
    #[serde(default = "default_platform")]
    pub platform: String,
}

fn default_platform() -> String {
    "macos".to_string()
}

#[tauri::command]
pub async fn perm_decide_request(
    req: DecideRequest,
    state: tauri::State<'_, PermListState>,
) -> Result<PermDecision, String> {
    let app = AppMeta {
        bundle_id: req.bundle_id,
        display_name: req.display_name,
        process_name: req.process_name,
        platform: req.platform,
    };
    let l = state.lock().map_err(|e| e.to_string())?;
    let d = l.decide(&app);
    Ok(PermDecision {
        decision: format!("{:?}", d).to_lowercase(),
        blacklisted: desktop_perm::is_blacklisted(&app),
    })
}

#[derive(Serialize)]
pub struct PermDecision {
    pub decision: String,
    pub blacklisted: bool,
}
