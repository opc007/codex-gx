//! v1.9.5：Mobile Remote 完整版 Tauri 命令 (HTTP server + tunnel + 多设备)
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

// v1.9.5: HTTP server + tunnel commands
pub use mobile_remote::http::{
    DeviceRoute, NotificationLog, RemoteCommand, RunningFlag, ServerInfo, ServerStateInner,
    ServerStatus, TunnelStatus,
};

pub type MobileHttpServerState = std::sync::Arc<std::sync::Mutex<ServerStateInner>>;
pub type MobileHttpRunningFlag = std::sync::Arc<std::sync::Mutex<bool>>;

pub fn build_http_state() -> MobileHttpServerState {
    std::sync::Arc::new(std::sync::Mutex::new(ServerStateInner::default()))
}

fn http_running_flag() -> MobileHttpRunningFlag {
    static FLAG: std::sync::OnceLock<MobileHttpRunningFlag> = std::sync::OnceLock::new();
    FLAG.get_or_init(|| std::sync::Arc::new(std::sync::Mutex::new(false))).clone()
}

#[derive(serde::Deserialize)]
pub struct ServerStartArgs {
    #[serde(default = "default_port_v")]
    pub port: u16,
    #[serde(default)]
    pub enable_tunnel: bool,
    #[serde(default = "default_bind_v")]
    pub bind: String,
}

fn default_port_v() -> u16 {
    8788
}
fn default_bind_v() -> String {
    mobile_remote::http::default_bind()
}

#[tauri::command]
pub async fn mobile_server_start(
    args: ServerStartArgs,
    state: tauri::State<'_, MobileHttpServerState>,
) -> Result<ServerInfo, String> {
    let st = state.inner().clone();
    let flag = http_running_flag();
    {
        let s = st.lock().map_err(|e| e.to_string())?;
        if s.info.status == ServerStatus::Running {
            return Err(format!("already running at {}:{}", s.info.bind, s.info.port));
        }
    }
    mobile_remote::http::start(args.bind.clone(), args.port, args.enable_tunnel, st.clone(), flag)
        .map_err(|e| e.to_string())?;
    let s = st.lock().map_err(|e| e.to_string())?;
    Ok(s.info.clone())
}

#[tauri::command]
pub async fn mobile_server_stop(state: tauri::State<'_, MobileHttpServerState>) -> Result<ServerInfo, String> {
    let st = state.inner().clone();
    let flag = http_running_flag();
    mobile_remote::http::stop(&flag, &st);
    let s = st.lock().map_err(|e| e.to_string())?;
    Ok(s.info.clone())
}

#[tauri::command]
pub async fn mobile_server_status(state: tauri::State<'_, MobileHttpServerState>) -> Result<ServerInfo, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(s.info.clone())
}

#[tauri::command]
pub async fn mobile_server_devices(state: tauri::State<'_, MobileHttpServerState>) -> Result<Vec<DeviceRoute>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(s.info.devices.clone())
}

#[tauri::command]
pub async fn mobile_server_notifications(
    state: tauri::State<'_, MobileHttpServerState>,
) -> Result<Vec<NotificationLog>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(s.notifications.clone())
}

#[tauri::command]
pub async fn mobile_server_commands(state: tauri::State<'_, MobileHttpServerState>) -> Result<Vec<RemoteCommand>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(s.commands.clone())
}

#[derive(serde::Serialize)]
pub struct MobileFullStatus {
    pub token: mobile_remote::MobileToken,
    pub server: ServerInfo,
}

#[tauri::command]
pub async fn mobile_full_status(
    token_state: tauri::State<'_, MobileState>,
    server_state: tauri::State<'_, MobileHttpServerState>,
) -> Result<MobileFullStatus, String> {
    let t = token_state.lock().map_err(|e| e.to_string())?.clone();
    let s = server_state.lock().map_err(|e| e.to_string())?;
    Ok(MobileFullStatus { token: t, server: s.info.clone() })
}

#[tauri::command]
pub fn mobile_qr_payload(token: String, public_url: Option<String>) -> String {
    // 简单把 (token, url) 序列化成 base64 当作 QR 内容（演示）
    let raw = format!("agentshell://mobile?token={}&url={}", token, public_url.unwrap_or_default());
    base64_encode(&raw)
}

fn base64_encode(s: &str) -> String {
    use std::io::Write;
    // 简单 base64
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let b = &bytes[i..i + 3];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
        out.push(CHARSET[((n >> 6) & 0x3F) as usize] as char);
        out.push(CHARSET[(n & 0x3F) as usize] as char);
        i += 3;
    }
    if i < bytes.len() {
        let remaining = &bytes[i..];
        let n = match remaining.len() {
            1 => (remaining[0] as u32) << 16,
            2 => ((remaining[0] as u32) << 16) | ((remaining[1] as u32) << 8),
            _ => 0,
        };
        out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
        if remaining.len() == 2 {
            out.push(CHARSET[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}
