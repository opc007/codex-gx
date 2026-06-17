//! v1.4：P2P 设备协同（同局域网）
//!
//! 协议：
//! - **mDNS 广播**：用 mdns crate（macOS / Linux 内置 Bonjour / Avahi）
//!   持续在 5353 端口广播 `_codex-gx._tcp.local` 服务
//! - **TCP 长连接**：发现对方后建立 TCP 连接，按行传 JSON 消息
//! - **配对**：主机生成 6 位 pairing code，客户端手动输入
//! - **消息类型**：
//!   - `Hello { device_id, name, version }` — 连接握手
//!   - `Pair { code }` — 客户端提交 pairing code
//!   - `PairOk { token }` — 主机接受，给 token
//!   - `PairDenied` — 主机拒绝
//!   - `SessionList { sessions: Vec<SessionSummary> }` — 同步 session 列表
//!   - `SessionPull { session_id }` — 请求完整 session
//!   - `SessionData { session }` — 完整 session
//!   - `Ping / Pong` — 保活
//!   - `Bye { reason }` — 优雅断开

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub name: String,
    pub version: String,
    pub platform: String, // "macos" / "windows" / "linux"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DeviceStatus {
    Discovered,
    Pairing,
    Paired,
    Connecting,
    Connected,
    Rejected,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDevice {
    pub info: DeviceInfo,
    pub address: String, // ip:port
    pub status: DeviceStatus,
    pub last_seen: u64,
    pub paired_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello {
        from: DeviceInfo,
    },
    Pair {
        code: String,
    },
    PairOk {
        token: String,
    },
    PairDenied {
        reason: String,
    },
    SessionList {
        sessions: Vec<SessionSummary>,
    },
    SessionPull {
        session_id: String,
    },
    SessionData {
        session_id: String,
        messages: Vec<SharedMessage>,
    },
    Ping,
    Pong,
    Bye {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub message_count: u32,
    pub updated_at: u64,
    pub owner_id: Option<String>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedMessage {
    pub role: String,
    pub content: String,
    pub created_at: u64,
    pub tool_name: Option<String>,
}

#[derive(Clone)]
pub struct PairingCode(pub String);

impl PairingCode {
    pub fn generate() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let code: String = (0..6)
            .map(|_| {
                let n: u32 = rng.gen_range(0..36);
                if n < 10 {
                    (b'0' + n as u8) as char
                } else {
                    (b'A' + (n - 10) as u8) as char
                }
            })
            .collect();
        PairingCode(code)
    }
}

pub struct P2pHost {
    pub device_info: DeviceInfo,
    pub pairing_code: Arc<RwLock<Option<PairingCode>>>,
    pub listener_addr: Arc<RwLock<Option<SocketAddr>>>,
    pub peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    pub event_tx: mpsc::UnboundedSender<P2pEvent>,
    pub sessions_provider: Arc<dyn SessionProvider>,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum P2pEvent {
    ListenerReady {
        port: u16,
        addr: String,
    },
    Discovered(PeerDevice),
    PairingRequested {
        device_id: String,
        code: String,
        name: String,
    },
    PeerConnected(PeerDevice),
    PeerDisconnected {
        device_id: String,
        reason: String,
    },
    PeerRejected {
        device_id: String,
        reason: String,
    },
    MessageReceived {
        device_id: String,
        msg: Message,
    },
    Error(String),
}

#[async_trait::async_trait]
pub trait SessionProvider: Send + Sync {
    async fn list_sessions(&self) -> Vec<SessionSummary>;
    async fn get_session_messages(&self, session_id: &str) -> Vec<SharedMessage>;
}

impl P2pHost {
    pub fn new(
        device_info: DeviceInfo,
        sessions_provider: Arc<dyn SessionProvider>,
        port: u16,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<P2pEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let host = Arc::new(Self {
            device_info,
            pairing_code: Arc::new(RwLock::new(None)),
            listener_addr: Arc::new(RwLock::new(None)),
            peers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            sessions_provider,
            port,
        });
        (host, event_rx)
    }

    /// 启动 mDNS 广播 + TCP listener
    pub async fn start(self: Arc<Self>) -> Result<(), String> {
        // 1. 启动 TCP listener
        let host = self.clone();
        let host_for_err = self.clone();
        tokio::spawn(async move {
            if let Err(e) = host.run_listener().await {
                let _ = host_for_err
                    .event_tx
                    .send(P2pEvent::Error(format!("listener: {e}")));
            }
        });

        // 2. 启动 mDNS broadcast
        let host = self.clone();
        let host_for_err2 = self.clone();
        tokio::spawn(async move {
            if let Err(e) = host.run_mdns().await {
                let _ = host_for_err2
                    .event_tx
                    .send(P2pEvent::Error(format!("mdns: {e}")));
            }
        });

        Ok(())
    }

    pub async fn generate_pairing_code(&self) -> String {
        let code = PairingCode::generate();
        let s = code.0.clone();
        *self.pairing_code.write().await = Some(code);
        s
    }

    pub async fn accept_pairing(&self, device_id: &str) -> Result<String, String> {
        let mut peers = self.peers.write().await;
        let peer = peers
            .get_mut(device_id)
            .ok_or_else(|| "unknown device".to_string())?;
        let token = Uuid::new_v4().to_string();
        peer.status = DeviceStatus::Paired;
        peer.paired_token = Some(token.clone());
        Ok(token)
    }

    pub async fn reject_pairing(&self, device_id: &str) {
        let mut peers = self.peers.write().await;
        if let Some(p) = peers.get_mut(device_id) {
            p.status = DeviceStatus::Rejected;
        }
    }

    pub async fn list_peers(&self) -> Vec<PeerDevice> {
        self.peers.read().await.values().cloned().collect()
    }

    async fn run_listener(self: Arc<Self>) -> Result<(), String> {
        let listener = TcpListener::bind(("0.0.0.0", self.port))
            .await
            .map_err(|e| e.to_string())?;
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        *self.listener_addr.write().await = Some(addr);
        let _ = self.event_tx.send(P2pEvent::ListenerReady {
            port: addr.port(),
            addr: addr.to_string(),
        });
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    let host = self.clone();
                    let host2 = host.clone();
                    tokio::spawn(async move {
                        if let Err(e) = host.handle_connection(stream, peer).await {
                            let _ = host2
                                .event_tx
                                .send(P2pEvent::Error(format!("conn {peer}: {e}")));
                        }
                    });
                }
                Err(e) => {
                    let _ = self.event_tx.send(P2pEvent::Error(format!("accept: {e}")));
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    async fn handle_connection(
        self: Arc<Self>,
        stream: TcpStream,
        peer: SocketAddr,
    ) -> Result<(), String> {
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read).lines();
        let mut current_peer_id: Option<String> = None;
        loop {
            let line = match reader.next_line().await {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => return Err(e.to_string()),
            };
            let msg: Message = match serde_json::from_str(&line) {
                Ok(m) => m,
                Err(e) => {
                    let _ = write
                        .write_all(
                            format!(
                                "{}\n",
                                serde_json::to_string(&Message::Bye {
                                    reason: format!("bad json: {e}"),
                                })
                                .unwrap()
                            )
                            .as_bytes(),
                        )
                        .await;
                    break;
                }
            };
            match &msg {
                Message::Hello { from } => {
                    let mut peers = self.peers.write().await;
                    let pid = from.device_id.clone();
                    peers.insert(
                        pid.clone(),
                        PeerDevice {
                            info: from.clone(),
                            address: peer.to_string(),
                            status: DeviceStatus::Discovered,
                            last_seen: now_ms(),
                            paired_token: None,
                        },
                    );
                    current_peer_id = Some(pid.clone());
                    let _ = self
                        .event_tx
                        .send(P2pEvent::Discovered(peers.get(&pid).cloned().unwrap()));
                    let resp = serde_json::to_string(&Message::Hello {
                        from: self.device_info.clone(),
                    })
                    .unwrap();
                    let _ = write.write_all(format!("{resp}\n").as_bytes()).await;
                }
                Message::Pair { code } => {
                    if let Some(pid) = &current_peer_id {
                        let _ = self.event_tx.send(P2pEvent::PairingRequested {
                            device_id: pid.clone(),
                            code: code.clone(),
                            name: self
                                .peers
                                .read()
                                .await
                                .get(pid)
                                .map(|p| p.info.name.clone())
                                .unwrap_or_default(),
                        });
                        // 等待人工决定
                        // 简单实现：等待 30s，期间任何决定都生效
                        let token = {
                            let mut waited = 0u32;
                            let step_ms = 200u32;
                            let mut accepted_token: Option<String> = None;
                            while waited < 30_000 {
                                tokio::time::sleep(Duration::from_millis(step_ms as u64)).await;
                                waited += step_ms;
                                let st = self
                                    .peers
                                    .read()
                                    .await
                                    .get(pid)
                                    .map(|p| p.status.clone())
                                    .unwrap_or(DeviceStatus::Discovered);
                                match st {
                                    DeviceStatus::Paired => {
                                        accepted_token = self
                                            .peers
                                            .read()
                                            .await
                                            .get(pid)
                                            .and_then(|p| p.paired_token.clone());
                                        break;
                                    }
                                    DeviceStatus::Rejected => break,
                                    _ => {}
                                }
                            }
                            accepted_token
                        };
                        if let Some(token) = token {
                            let resp = serde_json::to_string(&Message::PairOk { token }).unwrap();
                            let _ = write.write_all(format!("{resp}\n").as_bytes()).await;
                        } else {
                            let resp = serde_json::to_string(&Message::PairDenied {
                                reason: "timeout or rejected".to_string(),
                            })
                            .unwrap();
                            let _ = write.write_all(format!("{resp}\n").as_bytes()).await;
                        }
                    }
                }
                Message::SessionPull { session_id } => {
                    let msgs = self
                        .sessions_provider
                        .get_session_messages(session_id)
                        .await;
                    let resp = serde_json::to_string(&Message::SessionData {
                        session_id: session_id.clone(),
                        messages: msgs,
                    })
                    .unwrap();
                    let _ = write.write_all(format!("{resp}\n").as_bytes()).await;
                }
                Message::Ping => {
                    let resp = serde_json::to_string(&Message::Pong).unwrap();
                    let _ = write.write_all(format!("{resp}\n").as_bytes()).await;
                }
                Message::Bye { reason } => {
                    if let Some(pid) = &current_peer_id {
                        let _ = self.event_tx.send(P2pEvent::PeerDisconnected {
                            device_id: pid.clone(),
                            reason: reason.clone(),
                        });
                    }
                    break;
                }
                _ => {}
            }
            if let Some(pid) = &current_peer_id {
                let _ = self.event_tx.send(P2pEvent::MessageReceived {
                    device_id: pid.clone(),
                    msg: msg.clone(),
                });
            }
        }
        Ok(())
    }

    async fn run_mdns(self: Arc<Self>) -> Result<(), String> {
        // mDNS 广播（macOS / Linux 内置）
        // 用 `dns-sd` / `avahi-publish` 命令行作为 fallback
        // 简单实现：开启一个 daemon 进程
        let name = format!("codex-gx-{}", &self.device_info.device_id[..6]);
        let port = self.port;
        if cfg!(target_os = "macos") {
            let mut child = tokio::process::Command::new("dns-sd")
                .args(&["-R", &name, "_codex-gx._tcp", "local", &port.to_string()])
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("dns-sd 启动失败: {e}"))?;
            // 后台 kill on drop
            let _ = child.wait().await;
        } else if cfg!(target_os = "linux") {
            // 尝试 avahi-publish
            let _ = tokio::process::Command::new("avahi-publish-service")
                .args(&[&name, "_codex-gx._tcp", &port.to_string()])
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("avahi-publish-service 启动失败: {e}"))?;
        } else {
            // Windows: 跳过 mDNS（NSBonjour API 复杂）
            // 仍然报告一个虚拟事件
        }
        Ok(())
    }
}

// =============================================================================
// P2pClient（主动连接）
// =============================================================================

pub struct P2pClient {
    pub device_info: DeviceInfo,
    pub peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    pub event_tx: mpsc::UnboundedSender<P2pEvent>,
}

impl P2pClient {
    pub fn new(device_info: DeviceInfo) -> (Arc<Self>, mpsc::UnboundedReceiver<P2pEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let c = Arc::new(Self {
            device_info,
            peers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        });
        (c, event_rx)
    }

    pub async fn connect(self: Arc<Self>, addr: &str, code: &str) -> Result<String, String> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| format!("connect: {e}"))?;
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read).lines();
        // 1. 发送 Hello
        let hello = Message::Hello {
            from: self.device_info.clone(),
        };
        let _ = write
            .write_all(format!("{}\n", serde_json::to_string(&hello).unwrap()).as_bytes())
            .await;
        // 2. 接收 Hello
        let line = match reader.next_line().await {
            Ok(Some(l)) => l,
            _ => return Err("no hello back".to_string()),
        };
        let host_info: DeviceInfo =
            match serde_json::from_str::<Message>(&line).map_err(|e| e.to_string())? {
                Message::Hello { from } => from,
                _ => return Err("expected hello back".to_string()),
            };
        // 3. 发送 Pair code
        let pair = Message::Pair {
            code: code.to_string(),
        };
        let _ = write
            .write_all(format!("{}\n", serde_json::to_string(&pair).unwrap()).as_bytes())
            .await;
        // 4. 接收 PairOk / PairDenied
        let line = match reader.next_line().await {
            Ok(Some(l)) => l,
            _ => return Err("no pair response".to_string()),
        };
        match serde_json::from_str::<Message>(&line).map_err(|e| e.to_string())? {
            Message::PairOk { token } => {
                let mut peers = self.peers.write().await;
                peers.insert(
                    host_info.device_id.clone(),
                    PeerDevice {
                        info: host_info.clone(),
                        address: addr.to_string(),
                        status: DeviceStatus::Connected,
                        last_seen: now_ms(),
                        paired_token: Some(token.clone()),
                    },
                );
                let _ = self.event_tx.send(P2pEvent::PeerConnected(
                    peers.get(&host_info.device_id).cloned().unwrap(),
                ));
                Ok(token)
            }
            Message::PairDenied { reason } => {
                let _ = self.event_tx.send(P2pEvent::PeerRejected {
                    device_id: host_info.device_id.clone(),
                    reason: reason.clone(),
                });
                Err(reason)
            }
            _ => Err("unexpected response".to_string()),
        }
    }

    pub async fn list_peers(&self) -> Vec<PeerDevice> {
        self.peers.read().await.values().cloned().collect()
    }
}

fn _serde_err_to_string(e: serde_json::Error) -> String {
    e.to_string()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_code_6_chars() {
        let c = PairingCode::generate();
        assert_eq!(c.0.len(), 6);
        assert!(c.0.chars().all(|ch| ch.is_ascii_alphanumeric()));
    }

    #[test]
    fn device_info_serde() {
        let info = DeviceInfo {
            device_id: "abc".into(),
            name: "test".into(),
            version: "1.4".into(),
            platform: "macos".into(),
        };
        let s = serde_json::to_string(&info).unwrap();
        let d: DeviceInfo = serde_json::from_str(&s).unwrap();
        assert_eq!(d.name, "test");
    }

    #[test]
    fn message_hello_serde() {
        let m = Message::Hello {
            from: DeviceInfo {
                device_id: "x".into(),
                name: "n".into(),
                version: "1".into(),
                platform: "macos".into(),
            },
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"type\":\"hello\""));
    }

    #[test]
    fn message_session_list_serde() {
        let m = Message::SessionList {
            sessions: vec![SessionSummary {
                id: "s1".into(),
                title: "test".into(),
                message_count: 3,
                updated_at: 100,
                owner_id: None,
                workspace_id: None,
            }],
        };
        let s = serde_json::to_string(&m).unwrap();
        let d: Message = serde_json::from_str(&s).unwrap();
        match d {
            Message::SessionList { sessions } => assert_eq!(sessions[0].id, "s1"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn peer_device_status_default() {
        let p = PeerDevice {
            info: DeviceInfo {
                device_id: "x".into(),
                name: "n".into(),
                version: "1".into(),
                platform: "linux".into(),
            },
            address: "127.0.0.1:1234".into(),
            status: DeviceStatus::Discovered,
            last_seen: 0,
            paired_token: None,
        };
        assert_eq!(p.status, DeviceStatus::Discovered);
    }
}
