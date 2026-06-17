//! v1.9.3: Pocket HTTP Server — 真实 webhook 接收
//!
//! ## 设计
//! - 用 std::net 跑后台线程（无额外依赖）
//! - 单 endpoint: POST /agentshell/pocket
//! - 路径 /agentshell/health (GET) — 健康检查
//! - 路径 /agentshell/pairing — 列出配对（GET）
//! - HMAC 验签 + JSON 解析 + 路由到 handle_request
//!
//! ## 持久化
//! - 入站消息存 ~/.agentshell/pocket/inbound.log（追加）

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{handle_request as lib_handle_request, verify_hmac, PocketConfig, PocketRequest, PocketResponse, PocketSource};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Stopped,
    Running,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub status: ServerStatus,
    pub bind: String,
    pub port: u16,
    pub started_at: i64,
    pub requests_handled: usize,
    pub last_request_at: Option<i64>,
    pub last_error: Option<String>,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            status: ServerStatus::Stopped,
            bind: "127.0.0.1".into(),
            port: 8787,
            started_at: 0,
            requests_handled: 0,
            last_request_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundLogEntry {
    pub timestamp: i64,
    pub source: String,
    pub user_id: String,
    pub chat_id: String,
    pub text: String,
    pub signature_ok: bool,
    pub thread_id: String,
    pub status: String,
}

pub type ServerState = Arc<Mutex<ServerInfo>>;
pub type RunningFlag = Arc<Mutex<bool>>;

#[derive(Clone)]
pub struct ServerHandle {
    pub info: ServerState,
    pub running: RunningFlag,
}

impl Default for ServerHandle {
    fn default() -> Self {
        Self {
            info: Arc::new(Mutex::new(ServerInfo::default())),
            running: Arc::new(Mutex::new(false)),
        }
    }
}

#[allow(dead_code)]

pub fn default_bind() -> String {
    "127.0.0.1".into()
}

/// 启动 HTTP server（后台线程）
pub fn start(bind: String, port: u16, info: ServerState, running: RunningFlag) -> Result<(), String> {
    let addr = format!("{}:{}", bind, port);
    let listener = TcpListener::bind(&addr).map_err(|e| format!("bind {}: {}", addr, e))?;

    // set non-blocking 在 std::net 不可用，这里只用 accept 阻塞
    // running flag 由 caller 控制
    {
        let mut i = info.lock().unwrap();
        i.status = ServerStatus::Running;
        i.bind = bind.clone();
        i.port = port;
        i.started_at = now_ts();
        i.last_error = None;
    }
    {
        let mut r = running.lock().unwrap();
        *r = true;
    }

    let info_clone = info.clone();
    let running_clone = running.clone();
    let bind_clone = bind.clone();

    thread::spawn(move || {
        listener.set_ttl(60).ok();
        for stream in listener.incoming() {
            if !*running_clone.lock().unwrap() {
                break;
            }
            match stream {
                Ok(s) => {
                    let info_c = info_clone.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle_conn(s, &info_c) {
                            let mut i = info_c.lock().unwrap();
                            i.last_error = Some(format!("conn: {}", e));
                        }
                    });
                }
                Err(e) => {
                    let mut i = info_clone.lock().unwrap();
                    i.last_error = Some(format!("accept: {}", e));
                }
            }
        }
        let mut i = info_clone.lock().unwrap();
        i.status = ServerStatus::Stopped;
        let _ = bind_clone; // suppress unused
    });

    Ok(())
}

pub fn stop(running: &RunningFlag, info: &ServerState) {
    {
        let mut r = running.lock().unwrap();
        *r = false;
    }
    let mut i = info.lock().unwrap();
    i.status = ServerStatus::Stopped;
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn handle_conn(mut stream: TcpStream, info: &ServerState) -> std::io::Result<()> {
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
            }
        }
    }

    // body
    let mut body = vec![0u8; content_length.min(64 * 1024)];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    match (method, path) {
        ("GET", "/agentshell/health") => {
            send_response(&mut stream, 200, "OK", "application/json", br#"{"status":"ok","version":"v1.9.3"}"#)
        }
        ("GET", "/agentshell/pairing") => {
            let cfg = PocketConfig::load();
            let txt = serde_json::to_string(&cfg.pairings).unwrap_or_else(|_| "[]".into());
            send_response(&mut stream, 200, "OK", "application/json", txt.as_bytes())
        }
        ("GET", "/agentshell/status") => {
            let i = info.lock().unwrap();
            let txt = serde_json::to_string(&*i).unwrap_or_else(|_| "{}".into());
            send_response(&mut stream, 200, "OK", "application/json", txt.as_bytes())
        }
        ("POST", "/agentshell/pocket") => {
            // parse JSON body
            let body_str = String::from_utf8_lossy(&body).to_string();
            let req: Result<PocketRequest, _> = serde_json::from_str(&body_str);

            let mut i = info.lock().unwrap();
            i.requests_handled += 1;
            i.last_request_at = Some(now_ts());
            drop(i);

            let cfg = PocketConfig::load();
            match req {
                Ok(mut r) => {
                    // 重置签名（前端不传），让 lib 内部按 pairing 校验
                    // 但入库验签需要 signature 在 body 里 — 提取 X-Pocket-Signature 头
                    // 简化：要求 client 在 JSON body 里给 signature（前端就这么用）
                    let resp = lib_handle_request(r.clone(), &cfg);
                    let entry = InboundLogEntry {
                        timestamp: now_ts(),
                        source: r.source.clone(),
                        user_id: r.user_id.clone(),
                        chat_id: r.chat_id.clone(),
                        text: r.text.clone(),
                        signature_ok: resp.status == "accepted",
                        thread_id: resp.thread_id.clone(),
                        status: resp.status.clone(),
                    };
                    append_inbound_log(&entry);
                    let txt = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".into());
                    send_response(&mut stream, if resp.status == "accepted" { 200 } else { 400 }, "OK", "application/json", txt.as_bytes())
                }
                Err(e) => {
                    let resp = PocketResponse::error(&format!("invalid json: {}", e));
                    let txt = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".into());
                    send_response(&mut stream, 400, "Bad Request", "application/json", txt.as_bytes())
                }
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

fn append_inbound_log(entry: &InboundLogEntry) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = std::path::PathBuf::from(home).join(".agentshell").join("pocket-inbound.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(line) = serde_json::to_string(entry) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

pub fn read_inbound_log(limit: usize) -> Vec<InboundLogEntry> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = std::path::PathBuf::from(home).join(".agentshell").join("pocket-inbound.log");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content
        .lines()
        .rev()
        .take(limit)
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn spawn_test_server() -> (ServerState, RunningFlag, String, u16) {
        let info: ServerState = Arc::new(Mutex::new(ServerInfo::default()));
        let running: RunningFlag = Arc::new(Mutex::new(false));
        // find free port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        start("127.0.0.1".into(), port, info.clone(), running.clone()).unwrap();
        // wait a bit for bind
        std::thread::sleep(std::time::Duration::from_millis(100));
        (info, running, "127.0.0.1".into(), port)
    }

    fn http_get(addr: &str, path: &str) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, addr
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).unwrap();
        buf
    }

    fn http_post(addr: &str, path: &str, body: &str) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        let req = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            path, addr, body.len(), body
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).unwrap();
        buf
    }

    #[test]
    fn test_health_endpoint() {
        let (info, running, bind, port) = spawn_test_server();
        let addr = format!("{}:{}", bind, port);
        let resp = http_get(&addr, "/agentshell/health");
        assert!(resp.contains("200 OK"), "got: {}", resp);
        assert!(resp.contains("\"status\":\"ok\""));
        *running.lock().unwrap() = false;
    }

    #[test]
    fn test_pairing_endpoint() {
        let (info, running, bind, port) = spawn_test_server();
        let addr = format!("{}:{}", bind, port);
        let resp = http_get(&addr, "/agentshell/pairing");
        assert!(resp.contains("200 OK"), "got: {}", resp);
        assert!(resp.contains("["));
        *running.lock().unwrap() = false;
    }

    #[test]
    fn test_webhook_404() {
        let (info, running, bind, port) = spawn_test_server();
        let addr = format!("{}:{}", bind, port);
        let resp = http_get(&addr, "/unknown");
        assert!(resp.contains("404"));
        *running.lock().unwrap() = false;
    }

    #[test]
    fn test_webhook_no_pairing() {
        let (info, running, bind, port) = spawn_test_server();
        let addr = format!("{}:{}", bind, port);
        let body = r#"{"source":"feishu","user_id":"u1","user_name":"U","chat_id":"c1","chat_type":"direct","text":"hi"}"#;
        let resp = http_post(&addr, "/agentshell/pocket", body);
        assert!(resp.contains("400"), "got: {}", resp);
        assert!(resp.contains("no pairing"));
        *running.lock().unwrap() = false;
    }
}