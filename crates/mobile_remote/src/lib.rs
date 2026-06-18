//! v1.9.1：Mobile Remote — 移动 App 远程监控/遥控
//!
//! 设计参考：docs/开发文档.md §5.30
//!
//! ## 目标
//! - 移动 App 通过 HTTP API 连接本地 AgentShell
//! - 认证：Bearer token（持久化到 `~/.agentshell/mobile-token`）
//! - 命令：列出 sessions / 看 session 详情 / 发送消息
//!
//! ## v1.9.1 简化版
//! - 只做 token 生成 + 状态机 + 数据结构
//! - HTTP server 实现留 v1.9.2（需要 axum + 端口冲突处理）
//!
//! ## 安全
//! - Token 32 字节随机（base64 编码）
//! - Token 仅存本地
//! - 任何请求必须 Bearer token
//! - 速率限制（待 v1.9.2）

use std::path::PathBuf;

pub mod http;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileToken {
    pub token: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub description: String,
    /// 配对的移动设备
    pub paired_devices: Vec<PairedDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub platform: String, // ios / android
    pub paired_at: i64,
    pub last_seen_at: Option<i64>,
}

impl Default for MobileToken {
    fn default() -> Self {
        Self {
            token: generate_token(),
            created_at: chrono::Utc::now().timestamp(),
            last_used_at: None,
            description: "Default mobile remote token".to_string(),
            paired_devices: Vec::new(),
        }
    }
}

impl MobileToken {
    /// 从持久化文件加载
    pub fn load() -> Self {
        let path = token_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            serde_json::from_str(&text).unwrap_or_else(|_| Self::default())
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), MobileError> {
        let path = token_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(MobileError::Io)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(MobileError::Json)?;
        std::fs::write(&path, text).map_err(MobileError::Io)?;
        Ok(())
    }

    pub fn regenerate(&mut self) -> String {
        self.token = generate_token();
        self.created_at = chrono::Utc::now().timestamp();
        self.last_used_at = None;
        self.token.clone()
    }

    pub fn touch(&mut self) {
        self.last_used_at = Some(chrono::Utc::now().timestamp());
    }

    pub fn pair_device(&mut self, name: &str, platform: &str) -> PairedDevice {
        let device = PairedDevice {
            id: format!("dev-{}", Uuid::new_v4()),
            name: name.to_string(),
            platform: platform.to_string(),
            paired_at: chrono::Utc::now().timestamp(),
            last_seen_at: None,
        };
        self.paired_devices.push(device.clone());
        device
    }

    pub fn unpair_device(&mut self, id: &str) -> bool {
        let before = self.paired_devices.len();
        self.paired_devices.retain(|d| d.id != id);
        before != self.paired_devices.len()
    }
}

pub fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    format!("mr_{}", base64_encode(&bytes))
}

fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    // 简化 base64 — 实际项目会用 base64 crate
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk.len();
        let b0 = chunk[0];
        let b1 = if n > 1 { chunk[1] } else { 0 };
        let b2 = if n > 2 { chunk[2] } else { 0 };
        out.push(ALPHA[(b0 >> 2) as usize] as char);
        out.push(ALPHA[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if n > 1 {
            out.push(ALPHA[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if n > 2 {
            out.push(ALPHA[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Mobile API 请求（演示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileRequest {
    pub action: String, // list_sessions / get_session / send_message
    pub token: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Mobile API 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileResponse {
    pub status: String, // ok / error
    pub data: serde_json::Value,
    pub timestamp: i64,
}

impl MobileResponse {
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            status: "ok".into(),
            data,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
    pub fn err(msg: &str) -> Self {
        Self {
            status: "error".into(),
            data: serde_json::json!({ "message": msg }),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// 验证 token
pub fn verify_token(stored: &MobileToken, provided: &str) -> bool {
    stored.token == provided
}

pub fn token_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".agentshell")
        .join("mobile-token.json")
}

#[derive(Debug, thiserror::Error)]
pub enum MobileError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("auth failed")]
    Auth,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_format() {
        let t = generate_token();
        assert!(t.starts_with("mr_"));
        assert!(t.len() > 30);
    }

    #[test]
    fn test_base64_encode() {
        let b = base64_encode(b"hello");
        assert_eq!(b, "aGVsbG8=");
    }

    #[test]
    fn test_pair_device() {
        let mut tok = MobileToken::default();
        assert_eq!(tok.paired_devices.len(), 0);
        let dev = tok.pair_device("iPhone 15", "ios");
        assert_eq!(tok.paired_devices.len(), 1);
        assert_eq!(dev.name, "iPhone 15");
    }

    #[test]
    fn test_unpair() {
        let mut tok = MobileToken::default();
        let dev = tok.pair_device("Test", "android");
        assert!(tok.unpair_device(&dev.id));
        assert!(!tok.unpair_device(&dev.id));
    }

    #[test]
    fn test_regenerate() {
        let mut tok = MobileToken::default();
        let old = tok.token.clone();
        let new = tok.regenerate();
        assert_ne!(old, new);
        assert!(new.starts_with("mr_"));
    }

    #[test]
    fn test_verify() {
        let tok = MobileToken::default();
        assert!(verify_token(&tok, &tok.token));
        assert!(!verify_token(&tok, "wrong"));
    }

    #[test]
    fn test_response() {
        let r = MobileResponse::ok(serde_json::json!({"a": 1}));
        assert_eq!(r.status, "ok");
        let e = MobileResponse::err("bad");
        assert_eq!(e.status, "error");
    }

    #[test]
    fn test_touch() {
        let mut tok = MobileToken::default();
        assert!(tok.last_used_at.is_none());
        tok.touch();
        assert!(tok.last_used_at.is_some());
    }
}

pub mod tunnel;

pub use tunnel::{
    start as tunnel_start, stop as tunnel_stop, short_id, simulate_request as tunnel_simulate,
    read_log as tunnel_read_log, MobileSession, TunnelHandle, TunnelInfo, TunnelLogEntry,
    TunnelState, TunnelStatus,
};
