//! v1.9.5: Mobile Remote HTTP Server — 公网接入
//!
//! ## 设计
//! - 真实 HTTP server（std::net + std::thread）
//! - 4 个 endpoint：
//!   - GET  /health
//!   - GET  /devices          — 列出配对设备
//!   - POST /send             — 发送命令到设备（要 Bearer token）
//!   - POST /register         — 设备注册（要 Bearer token）
//! - 公网 tunnel stub（生成 fake 公网 URL）
//! - 多设备路由（按 device_id 路由命令）
//! - 命令队列 + 通知历史（最近 50 条）

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Stopped,
    Running,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatus {
    Disabled,
    Active,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRoute {
    pub device_id: String,
    pub last_seen: i64,
    pub pending_commands: usize,
    pub status: String, // online / offline
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCommand {
    pub id: String,
    pub device_id: String,
    pub command: String, // ping / screenshot / open / shell / notify
    pub payload: serde_json::Value,
    pub timestamp: i64,
    pub status: String, // queued / sent / ack / failed
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationLog {
    pub timestamp: i64,
    pub level: String, // info / warn / error
    pub message: String,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub status: ServerStatus,
    pub bind: String,
    pub port: u16,
    pub tunnel_status: TunnelStatus,
    pub public_url: Option<String>,
    pub started_at: i64,
    pub requests_handled: usize,
    pub last_request_at: Option<i64>,
    pub last_error: Option<String>,
    pub devices: Vec<DeviceRoute>,
    pub pending_commands: usize,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            status: ServerStatus::Stopped,
            bind: "0.0.0.0".into(),
            port: 8788,
            tunnel_status: TunnelStatus::Disabled,
            public_url: None,
            started_at: 0,
            requests_handled: 0,
            last_request_at: None,
            last_error: None,
            devices: vec![],
            pending_commands: 0,
        }
    }
}

#[derive(Default)]
pub struct ServerStateInner {
    pub info: ServerInfo,
    pub commands: Vec<RemoteCommand>, // 队列
    pub notifications: Vec<NotificationLog>, // 最近 50 条
    pub running: bool,
}

pub type ServerState = Arc<Mutex<ServerStateInner>>;
pub type RunningFlag = Arc<Mutex<bool>>;

pub fn default_bind() -> String {
    "0.0.0.0".into()
}

/// 生成 fake 公网 URL（演示 tunnel）
pub fn generate_public_url(port: u16) -> String {
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let ts = now_ts() as usize;
    let token: String = (0..8)
        .map(|i| chars[(ts.wrapping_mul(2654435761).wrapping_add(i)) % chars.len()])
        .collect();
    format!("https://{}.tunnel.agentshell.dev:{}", token, port)
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// 启动 HTTP server
pub fn start(bind: String, port: u16, enable_tunnel: bool, state: ServerState, running: RunningFlag) -> Result<(), String> {
    let addr = format!("{}:{}", bind, port);
    let listener = TcpListener::bind(&addr).map_err(|e| format!("bind {}: {}", addr, e))?;

    {
        let mut s = state.lock().unwrap();
        s.info.status = ServerStatus::Running;
        s.info.bind = bind.clone();
        s.info.port = port;
        s.info.started_at = now_ts();
        s.info.last_error = None;
        if enable_tunnel {
            s.info.tunnel_status = TunnelStatus::Active;
            s.info.public_url = Some(generate_public_url(port));
        }
    }
    {
        let mut r = running.lock().unwrap();
        *r = true;
    }

    let state_clone = state.clone();
    let running_clone = running.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            if !*running_clone.lock().unwrap() {
                break;
            }
            match stream {
                Ok(s) => {
                    let st = state_clone.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle_conn(s, &st) {
                            let mut s = st.lock().unwrap();
                            s.info.last_error = Some(format!("conn: {}", e));
                        }
                    });
                }
                Err(e) => {
                    let mut s = state_clone.lock().unwrap();
                    s.info.last_error = Some(format!("accept: {}", e));
                }
            }
        }
        let mut s = state_clone.lock().unwrap();
        s.info.status = ServerStatus::Stopped;
        s.running = false;
    });

    Ok(())
}

pub fn stop(running: &RunningFlag, state: &ServerState) {
    {
        let mut r = running.lock().unwrap();
        *r = false;
    }
    let mut s = state.lock().unwrap();
    s.info.status = ServerStatus::Stopped;
    s.running = false;
}

fn handle_conn(mut stream: TcpStream, state: &ServerState) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return send_response(&mut stream, 400, "Bad Request", "text/plain", b"bad request");
    }
    let method = parts[0];
    let path = parts[1];

    // headers
    let mut content_length = 0usize;
    let mut auth_token: Option<String> = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some(idx) = line.find(':') {
            let (k, v) = line.split_at(idx);
            let v = v[1..].trim();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            } else if k.eq_ignore_ascii_case("authorization") {
                if let Some(stripped) = v.strip_prefix("Bearer ") {
                    auth_token = Some(stripped.to_string());
                }
            }
        }
    }

    let mut body = vec![0u8; content_length.min(64 * 1024)];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body_str = String::from_utf8_lossy(&body).to_string();

    // 统计
    {
        let mut s = state.lock().unwrap();
        s.info.requests_handled += 1;
        s.info.last_request_at = Some(now_ts());
    }

    match (method, path) {
        ("GET", "/health") => {
            let s = state.lock().unwrap();
            let txt = serde_json::json!({
                "status": "ok",
                "version": "v1.9.5",
                "uptime_secs": now_ts() - s.info.started_at,
                "devices": s.info.devices.len(),
                "requests_handled": s.info.requests_handled,
            }).to_string();
            send_response(&mut stream, 200, "OK", "application/json", txt.as_bytes())
        }
        ("GET", "/devices") => {
            let s = state.lock().unwrap();
            let txt = serde_json::to_string(&s.info.devices).unwrap_or_else(|_| "[]".into());
            send_response(&mut stream, 200, "OK", "application/json", txt.as_bytes())
        }
        ("POST", "/send") => {
            // 需要 Bearer token
            let token_str = auth_token.unwrap_or_default();
            let stored = crate::MobileToken::load();
            let token_ok = crate::verify_token(&stored, &token_str);
            if !token_ok {
                return send_response(&mut stream, 401, "Unauthorized", "application/json", br#"{"error":"invalid or missing bearer token"}"#);
            }
            // 解析 { device_id, command, payload }
            let req: Result<serde_json::Value, _> = serde_json::from_str(&body_str);
            match req {
                Ok(v) => {
                    let device_id = v.get("device_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let command = v.get("command").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let payload = v.get("payload").cloned().unwrap_or(serde_json::json!({}));
                    let cmd = RemoteCommand {
                        id: format!("cmd-{}", now_ts()),
                        device_id: device_id.clone(),
                        command,
                        payload,
                        timestamp: now_ts(),
                        status: "queued".into(),
                        result: None,
                    };
                    let response_txt;
                    {
                        let mut s = state.lock().unwrap();
                        // 更新设备状态
                        let dev = s.info.devices.iter_mut().find(|d| d.device_id == device_id);
                        match dev {
                            Some(d) => {
                                d.last_seen = now_ts();
                                d.pending_commands += 1;
                                d.status = "online".into();
                            }
                            None => {
                                s.info.devices.push(DeviceRoute {
                                    device_id: device_id.clone(),
                                    last_seen: now_ts(),
                                    pending_commands: 1,
                                    status: "online".into(),
                                });
                            }
                        }
                        s.commands.push(cmd.clone());
                        s.info.pending_commands = s.commands.iter().filter(|c| c.status == "queued").count();
                        response_txt = serde_json::to_string(&cmd).unwrap_or_else(|_| "{}".into());
                        s.notifications.push(NotificationLog {
                            timestamp: now_ts(),
                            level: "info".into(),
                            message: format!("cmd queued: {} for {}", cmd.command, device_id),
                            device_id: Some(device_id.clone()),
                        });
                        if s.notifications.len() > 50 {
                            let drop_n = s.notifications.len() - 50;
                            s.notifications.drain(0..drop_n);
                        }
                    }
                    send_response(&mut stream, 200, "OK", "application/json", response_txt.as_bytes())
                }
                Err(e) => {
                    send_response(&mut stream, 400, "Bad Request", "application/json", format!(r#"{{"error":"invalid json: {}"}}"#, e).as_bytes())
                }
            }
        }
        ("POST", "/register") => {
            let token_str = auth_token.unwrap_or_default();
            let stored = crate::MobileToken::load();
            if !crate::verify_token(&stored, &token_str) {
                return send_response(&mut stream, 401, "Unauthorized", "application/json", br#"{"error":"invalid token"}"#);
            }
            let req: Result<serde_json::Value, _> = serde_json::from_str(&body_str);
            match req {
                Ok(v) => {
                    let device_id = v.get("device_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let platform = v.get("platform").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
                    let mut s = state.lock().unwrap();
                    if !s.info.devices.iter().any(|d| d.device_id == device_id) {
                        s.info.devices.push(DeviceRoute {
                            device_id: device_id.clone(),
                            last_seen: now_ts(),
                            pending_commands: 0,
                            status: "online".into(),
                        });
                    }
                    s.notifications.push(NotificationLog {
                        timestamp: now_ts(),
                        level: "info".into(),
                        message: format!("device registered: {} ({})", device_id, platform),
                        device_id: Some(device_id.clone()),
                    });
                    let txt = serde_json::json!({"registered": device_id, "platform": platform}).to_string();
                    send_response(&mut stream, 200, "OK", "application/json", txt.as_bytes())
                }
                Err(e) => send_response(&mut stream, 400, "Bad Request", "application/json", format!(r#"{{"error":"{}"}}"#, e).as_bytes()),
            }
        }
        _ => send_response(&mut stream, 404, "Not Found", "text/plain", b"not found"),
    }
}

fn send_response(stream: &mut TcpStream, code: u16, reason: &str, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        code, reason, content_type, body.len()
    );
    stream.write_all(resp.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_token() -> crate::MobileToken {
        static TOKEN: std::sync::OnceLock<crate::MobileToken> = std::sync::OnceLock::new();
        TOKEN.get_or_init(|| {
            let t = crate::MobileToken::load();
            t.save().ok();
            t
        }).clone()
    }

    fn spawn_test_server() -> (ServerState, RunningFlag, u16) {
        let state: ServerState = Arc::new(Mutex::new(ServerStateInner::default()));
        let running: RunningFlag = Arc::new(Mutex::new(false));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        start("127.0.0.1".into(), port, false, state.clone(), running.clone()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        (state, running, port)
    }

    fn http_get(addr: &str, path: &str, token: Option<&str>) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        let auth = token.map(|t| format!("Authorization: Bearer {}\r\n", t)).unwrap_or_default();
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\n{}\r\nConnection: close\r\n\r\n",
            path, addr, auth
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut s, &mut buf).unwrap();
        buf
    }

    fn http_post(addr: &str, path: &str, body: &str, token: Option<&str>) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        let auth = token.map(|t| format!("Authorization: Bearer {}\r\n", t)).unwrap_or_default();
        let req = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\n{}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            path, addr, auth, body.len(), body
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut s, &mut buf).unwrap();
        buf
    }

    #[test]
    fn test_health() {
        let (state, running, port) = spawn_test_server();
        let addr = format!("127.0.0.1:{}", port);
        let resp = http_get(&addr, "/health", None);
        assert!(resp.contains("200 OK"));
        assert!(resp.contains("\"version\":\"v1.9.5\""));
        *running.lock().unwrap() = false;
    }

    #[test]
    fn test_send_no_auth() {
        let (state, running, port) = spawn_test_server();
        let addr = format!("127.0.0.1:{}", port);
        let body = r#"{"device_id":"dev1","command":"ping","payload":{}}"#;
        let resp = http_post(&addr, "/send", body, None);
        assert!(resp.contains("401"));
        *running.lock().unwrap() = false;
    }

    #[test]
    fn test_send_with_auth() {
        let stored = ensure_token();
        let (_state, running, port) = spawn_test_server();
        let addr = format!("127.0.0.1:{}", port);
        let body = r#"{"device_id":"dev1","command":"ping","payload":{}}"#;
        let resp = http_post(&addr, "/send", body, Some(&stored.token));
        assert!(resp.contains("200 OK"), "got: {}", resp);
        assert!(resp.contains("\"status\":\"queued\""));
        *running.lock().unwrap() = false;
    }

    #[test]
    fn test_devices_listing() {
        let stored = ensure_token();
        let (_state, running, port) = spawn_test_server();
        let addr = format!("127.0.0.1:{}", port);
        let body = r#"{"device_id":"dev1","command":"ping","payload":{}}"#;
        http_post(&addr, "/send", body, Some(&stored.token));
        let resp = http_get(&addr, "/devices", None);
        assert!(resp.contains("200 OK"));
        assert!(resp.contains("dev1"));
        *running.lock().unwrap() = false;
    }

    #[test]
    fn test_public_url_format() {
        let url = generate_public_url(8788);
        assert!(url.starts_with("https://"));
        assert!(url.contains(".tunnel.agentshell.dev"));
    }
}