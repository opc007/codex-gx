//! v1.9.2：Pocket 消息 App 触发 Tauri 命令
//!
//! ## 注册的命令
//! - `pocket_list_sources`     — 列出支持的 source
//! - `pocket_list_pairings`    — 列出配对
//! - `pocket_add_pairing`      — 新增配对（生成 HMAC key）
//! - `pocket_remove_pairing`   — 删除配对
//! - `pocket_handle_request`   — 处理入站请求
//! - `pocket_sign`             — 签名（调试用）
//! - `pocket_webhook_url`      — 显示 webhook URL（演示）

use pocket::{
    handle_request as lib_handle_request, read_inbound_log, server_start, server_stop, sign_hmac,
    default_bind, InboundLogEntry, Pairing, PocketConfig, PocketRequest, PocketResponse, PocketSource,
    ServerHandle, ServerInfo, ServerStatus,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub type PocketState = Arc<Mutex<PocketConfig>>;
pub type PocketServerState = Arc<Mutex<ServerHandle>>;

pub fn build_state() -> PocketState {
    Arc::new(Mutex::new(PocketConfig::load()))
}

pub fn build_server_state() -> PocketServerState {
    Arc::new(Mutex::new(ServerHandle::default()))
}

#[derive(Serialize)]
pub struct SourceInfo {
    pub name: String,
    pub label: String,
    pub paired: bool,
}

#[tauri::command]
pub fn pocket_list_sources(state: tauri::State<'_, PocketState>) -> Vec<SourceInfo> {
    let config = state.lock().unwrap();
    PocketSource::all()
        .into_iter()
        .map(|s| {
            let paired = config.pairings.iter().any(|p| p.source == s);
            SourceInfo {
                name: s.as_str().to_string(),
                label: s.label().to_string(),
                paired,
            }
        })
        .collect()
}

#[tauri::command]
pub async fn pocket_list_pairings(
    state: tauri::State<'_, PocketState>,
) -> Result<Vec<Pairing>, String> {
    let c = state.lock().map_err(|e| e.to_string())?;
    Ok(c.pairings.clone())
}

#[derive(Deserialize)]
pub struct AddPairingArgs {
    pub source: String,
    pub user_id: String,
    pub user_name: String,
    pub chat_id: String,
    #[serde(default = "default_chat_type")]
    pub chat_type: String,
}

fn default_chat_type() -> String {
    "direct".into()
}

#[tauri::command]
pub async fn pocket_add_pairing(
    args: AddPairingArgs,
    state: tauri::State<'_, PocketState>,
) -> Result<Pairing, String> {
    let source = PocketSource::parse(&args.source)
        .ok_or_else(|| format!("unknown source: {}", args.source))?;
    let key = format!("pk_{}", Uuid::new_v4().simple());
    let p = Pairing {
        id: format!("pair-{}", Uuid::new_v4().simple()),
        source,
        user_id: args.user_id,
        user_name: args.user_name,
        chat_id: args.chat_id,
        chat_type: args.chat_type,
        signature_key: key,
        paired_at: chrono::Utc::now().timestamp(),
        enabled: true,
    };
    let mut c = state.lock().map_err(|e| e.to_string())?;
    c.add_pairing(p.clone());
    c.save().map_err(|e| e.to_string())?;
    Ok(p)
}

#[tauri::command]
pub async fn pocket_remove_pairing(
    id: String,
    state: tauri::State<'_, PocketState>,
) -> Result<bool, String> {
    let mut c = state.lock().map_err(|e| e.to_string())?;
    let r = c.remove_pairing(&id);
    c.save().map_err(|e| e.to_string())?;
    Ok(r)
}

#[tauri::command]
pub fn pocket_handle_request(req: PocketRequest) -> PocketResponse {
    let config = PocketConfig::load();
    lib_handle_request(req, &config)
}

#[derive(Deserialize)]
pub struct SignArgs {
    pub key: String,
    pub body: String,
}

#[tauri::command]
pub fn pocket_sign(args: SignArgs) -> String {
    sign_hmac(&args.key, &args.body)
}

#[tauri::command]
pub fn pocket_webhook_url() -> String {
    // 演示：本地端点
    "http://localhost:8787/agentshell/pocket".into()
}

#[derive(Serialize)]
pub struct PocketStatus {
    pub source_count: usize,
    pub pairing_count: usize,
    pub enabled_pairings: usize,
    pub sources: Vec<SourceInfo>,
    pub config_path: String,
}

#[tauri::command]
pub async fn pocket_status(state: tauri::State<'_, PocketState>) -> Result<PocketStatus, String> {
    let c = state.lock().map_err(|e| e.to_string())?;
    let sources = PocketSource::all()
        .into_iter()
        .map(|s| {
            let paired = c.pairings.iter().any(|p| p.source == s);
            SourceInfo {
                name: s.as_str().to_string(),
                label: s.label().to_string(),
                paired,
            }
        })
        .collect();
    Ok(PocketStatus {
        source_count: PocketSource::all().len(),
        pairing_count: c.pairings.len(),
        enabled_pairings: c.pairings.iter().filter(|p| p.enabled).count(),
        sources,
        config_path: pocket::config_path().display().to_string(),
    })
}

#[derive(Deserialize)]
pub struct ServerStartArgs {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_bind_str")]
    pub bind: String,
}

fn default_port() -> u16 {
    8787
}

fn default_bind_str() -> String {
    default_bind()
}

#[tauri::command]
pub async fn pocket_server_start(
    args: ServerStartArgs,
    state: tauri::State<'_, PocketServerState>,
) -> Result<ServerInfo, String> {
    let handle = {
        let guard = state.inner().lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    {
        let i = handle.info.lock().map_err(|e| e.to_string())?;
        if i.status == ServerStatus::Running {
            return Err(format!("server already running at {}:{}", i.bind, i.port));
        }
    }
    server_start(
        args.bind.clone(),
        args.port,
        handle.info.clone(),
        handle.running.clone(),
    )
    .map_err(|e| e.to_string())?;
    let i = handle.info.lock().map_err(|e| e.to_string())?;
    Ok(i.clone())
}

#[tauri::command]
pub async fn pocket_server_stop(state: tauri::State<'_, PocketServerState>) -> Result<ServerInfo, String> {
    let handle = {
        let guard = state.inner().lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    server_stop(&handle.running, &handle.info);
    let i = handle.info.lock().map_err(|e| e.to_string())?;
    Ok(i.clone())
}

#[tauri::command]
pub async fn pocket_server_status(state: tauri::State<'_, PocketServerState>) -> Result<ServerInfo, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    let i = guard.info.lock().map_err(|e| e.to_string())?;
    Ok(i.clone())
}

#[derive(Deserialize)]
pub struct InboundArgs {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

#[tauri::command]
pub fn pocket_inbound_log(args: InboundArgs) -> Vec<InboundLogEntry> {
    read_inbound_log(args.limit)
}
