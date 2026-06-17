//! v1.9.1：Mobile Remote Tauri 命令
//!
//! ## 注册的命令
//! - `mobile_get_token`     — 取 token 信息
//! - `mobile_regen_token`   — 重新生成 token
//! - `mobile_pair_device`   — 配对新设备
//! - `mobile_unpair_device` — 解除配对
//! - `mobile_list_devices`  — 列出已配对设备
//! - `mobile_verify`        — 验证 token
//! - `mobile_call`          — 模拟一次 API 调用

use mobile_remote::{MobileToken, verify_token};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub type MobileState = Arc<Mutex<MobileToken>>;

pub fn build_state() -> MobileState {
    Arc::new(Mutex::new(MobileToken::load()))
}

#[derive(Serialize)]
pub struct TokenInfo {
    pub token: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub description: String,
    pub device_count: usize,
    pub token_path: String,
}

impl From<&MobileToken> for TokenInfo {
    fn from(t: &MobileToken) -> Self {
        TokenInfo {
            token: t.token.clone(),
            created_at: t.created_at,
            last_used_at: t.last_used_at,
            description: t.description.clone(),
            device_count: t.paired_devices.len(),
            token_path: mobile_remote::token_path().display().to_string(),
        }
    }
}

#[tauri::command]
pub async fn mobile_get_token(state: tauri::State<'_, MobileState>) -> Result<TokenInfo, String> {
    let t = state.lock().map_err(|e| e.to_string())?;
    Ok(TokenInfo::from(&*t))
}

#[tauri::command]
pub async fn mobile_regen_token(state: tauri::State<'_, MobileState>) -> Result<TokenInfo, String> {
    let mut t = state.lock().map_err(|e| e.to_string())?;
    let new_token = t.regenerate();
    t.save().map_err(|e| e.to_string())?;
    let _ = new_token;
    Ok(TokenInfo::from(&*t))
}

#[derive(Deserialize)]
pub struct PairArgs {
    pub name: String,
    pub platform: String,
}

#[tauri::command]
pub async fn mobile_pair_device(
    args: PairArgs,
    state: tauri::State<'_, MobileState>,
) -> Result<TokenInfo, String> {
    let mut t = state.lock().map_err(|e| e.to_string())?;
    t.pair_device(&args.name, &args.platform);
    t.save().map_err(|e| e.to_string())?;
    Ok(TokenInfo::from(&*t))
}

#[tauri::command]
pub async fn mobile_unpair_device(
    id: String,
    state: tauri::State<'_, MobileState>,
) -> Result<bool, String> {
    let mut t = state.lock().map_err(|e| e.to_string())?;
    let r = t.unpair_device(&id);
    t.save().map_err(|e| e.to_string())?;
    Ok(r)
}

#[tauri::command]
pub async fn mobile_list_devices(
    state: tauri::State<'_, MobileState>,
) -> Result<Vec<mobile_remote::PairedDevice>, String> {
    let t = state.lock().map_err(|e| e.to_string())?;
    Ok(t.paired_devices.clone())
}

#[tauri::command]
pub async fn mobile_verify(
    token: String,
    state: tauri::State<'_, MobileState>,
) -> Result<bool, String> {
    let t = state.lock().map_err(|e| e.to_string())?;
    Ok(verify_token(&t, &token))
}

#[tauri::command]
pub async fn mobile_call(
    req: mobile_remote::MobileRequest,
    state: tauri::State<'_, MobileState>,
) -> Result<mobile_remote::MobileResponse, String> {
    let mut t = state.lock().map_err(|e| e.to_string())?;
    if !verify_token(&t, &req.token) {
        return Ok(mobile_remote::MobileResponse::err("invalid token"));
    }
    t.touch();
    t.save().map_err(|e| e.to_string())?;
    // 演示：根据 action 返回 mock 数据
    let data = match req.action.as_str() {
        "list_sessions" => serde_json::json!({
            "sessions": [
                { "id": "demo-1", "title": "Demo session 1", "updated_at": 0 },
                { "id": "demo-2", "title": "Demo session 2", "updated_at": 0 },
            ]
        }),
        "get_session" => serde_json::json!({
            "id": req.session_id,
            "title": "Demo",
            "messages": [],
        }),
        "send_message" => serde_json::json!({
            "accepted": true,
            "message": req.message,
        }),
        _ => serde_json::json!({ "unknown_action": req.action }),
    };
    Ok(mobile_remote::MobileResponse::ok(data))
}
