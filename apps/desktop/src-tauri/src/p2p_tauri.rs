//! v1.4：P2P 设备协同 tauri 命令

use p2p::{DeviceInfo, Message, P2pEvent, P2pHost, P2pClient, PeerDevice, SessionSummary};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

pub struct P2pState {
    pub host: RwLock<Option<Arc<P2pHost>>>,
    pub client: Arc<P2pClient>,
}

impl P2pState {
    pub fn new() -> Self {
        let info = device_info();
        let (client, _rx) = P2pClient::new(info);
        P2pState {
            host: RwLock::new(None),
            client,
        }
    }
}

fn device_info() -> DeviceInfo {
    let device_id = {
        // 持久化 / 一次性：放用户 home
        let path = dirs_home().join(".agentshell").join("device_id");
        if let Ok(s) = std::fs::read_to_string(&path) {
            s.trim().to_string()
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let _ = std::fs::write(&path, &id);
            id
        }
    };
    let name = hostname();
    let version = env!("CARGO_PKG_VERSION").to_string();
    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
    .to_string();
    DeviceInfo {
        device_id,
        name,
        version,
        platform,
    }
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| {
            format!(
                "{}-device",
                if cfg!(target_os = "macos") {
                    "mac"
                } else if cfg!(target_os = "windows") {
                    "win"
                } else {
                    "linux"
                }
            )
        })
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

#[tauri::command]
pub async fn p2p_start_host(
    port: u16,
    app: AppHandle,
    state: tauri::State<'_, P2pState>,
) -> Result<String, String> {
    {
        let h = state.host.read().await;
        if h.is_some() {
            return Err("host already running".to_string());
        }
    }
    // 用一个空 SessionProvider（实际场景由前端提供）
    struct EmptyProvider;
    #[async_trait::async_trait]
    impl p2p::SessionProvider for EmptyProvider {
        async fn list_sessions(&self) -> Vec<SessionSummary> {
            vec![]
        }
        async fn get_session_messages(
            &self,
            _id: &str,
        ) -> Vec<p2p::SharedMessage> {
            vec![]
        }
    }
    let provider = Arc::new(EmptyProvider);
    let (host, mut rx) = P2pHost::new(device_info(), provider, port);
    host.clone().start().await?;
    {
        let mut h = state.host.write().await;
        *h = Some(host.clone());
    }
    // 启动事件 forwarder
    let app_handle = app.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let _ = app_handle.emit("p2p:event", &ev);
        }
    });
    Ok(format!("host started on port {port}"))
}

#[tauri::command]
pub async fn p2p_stop_host(state: tauri::State<'_, P2pState>) -> Result<(), String> {
    let mut h = state.host.write().await;
    *h = None;
    Ok(())
}

#[tauri::command]
pub async fn p2p_generate_pairing(
    state: tauri::State<'_, P2pState>,
) -> Result<String, String> {
    let h = state.host.read().await;
    let host = h.as_ref().ok_or("host not running")?;
    Ok(host.generate_pairing_code().await)
}

#[tauri::command]
pub async fn p2p_accept_pairing(
    device_id: String,
    state: tauri::State<'_, P2pState>,
) -> Result<String, String> {
    let h = state.host.read().await;
    let host = h.as_ref().ok_or("host not running")?;
    host.accept_pairing(&device_id).await
}

#[tauri::command]
pub async fn p2p_reject_pairing(
    device_id: String,
    state: tauri::State<'_, P2pState>,
) -> Result<(), String> {
    let h = state.host.read().await;
    let host = h.as_ref().ok_or("host not running")?;
    host.reject_pairing(&device_id).await;
    Ok(())
}

#[tauri::command]
pub async fn p2p_list_peers(
    state: tauri::State<'_, P2pState>,
) -> Result<Vec<PeerDevice>, String> {
    let h = state.host.read().await;
    if let Some(host) = h.as_ref() {
        Ok(host.list_peers().await)
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
pub async fn p2p_connect(
    address: String,
    code: String,
    state: tauri::State<'_, P2pState>,
) -> Result<String, String> {
    let c = state.client.clone();
    c.connect(&address, &code).await
}
