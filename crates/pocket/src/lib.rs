//! v1.9.2：Pocket — 消息 App 触发任务
//!
//! 设计参考：docs/开发文档.md §5.29
//!
//! ## 简化的 v1.9.2 实现
//! - 支持 4 种 source：feishu / wecom / dingtalk / slack
//! - HMAC-SHA256 签名验证（X-Pocket-Signature: sha256=...）
//! - 配对（pairings.json 持久化）
//! - 消息路由（按 user_id / chat_id）
//! - 真实 HTTP server 实现留 v1.9.3
//!
//! ## DoD
//! - 4 种 source 配对管理
//! - 签名验证函数（生产可用）
//! - 消息接收 + 路由到 thread
//! - 状态查询

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 消息源
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PocketSource {
    Feishu,
    WeCom,
    DingTalk,
    Slack,
    WeChat,
}

impl PocketSource {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "feishu" | "lark" => Some(Self::Feishu),
            "wecom" | "wework" => Some(Self::WeCom),
            "dingtalk" | "ding" => Some(Self::DingTalk),
            "slack" => Some(Self::Slack),
            "wechat" | "wx" => Some(Self::WeChat),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Feishu => "feishu",
            Self::WeCom => "wecom",
            Self::DingTalk => "dingtalk",
            Self::Slack => "slack",
            Self::WeChat => "wechat",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::Feishu, Self::WeCom, Self::DingTalk, Self::Slack, Self::WeChat]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Feishu => "🪶 Feishu (飞书)",
            Self::WeCom => "🏢 WeCom (企微)",
            Self::DingTalk => "📌 DingTalk (钉钉)",
            Self::Slack => "💼 Slack",
            Self::WeChat => "💬 WeChat (微信)",
        }
    }
}

/// 配对配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pairing {
    pub id: String,
    pub source: PocketSource,
    pub user_id: String,
    pub user_name: String,
    pub chat_id: String,
    pub chat_type: String, // direct / group
    pub signature_key: String, // HMAC 共享密钥
    pub paired_at: i64,
    pub enabled: bool,
}

/// Pocket 入站请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PocketRequest {
    pub source: String,
    pub user_id: String,
    pub user_name: String,
    pub chat_id: String,
    pub chat_type: String, // direct | group
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<PocketAttachment>,
    /// 签名（`sha256=...` 格式）
    #[serde(default)]
    pub signature: Option<String>,
    /// 时间戳
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PocketAttachment {
    pub kind: String, // image / file / audio
    pub url: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// Pocket 出站响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PocketResponse {
    pub status: String, // accepted | error
    pub thread_id: String,
    pub message: String,
    pub timestamp: i64,
}

impl PocketResponse {
    pub fn accepted(thread_id: &str) -> Self {
        Self {
            status: "accepted".into(),
            thread_id: thread_id.to_string(),
            message: "AgentShell 收到任务，正在处理…".into(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
    pub fn error(msg: &str) -> Self {
        Self {
            status: "error".into(),
            thread_id: String::new(),
            message: msg.into(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// 持久化
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PocketConfig {
    pub pairings: Vec<Pairing>,
    /// 全局 webhook 端点（演示）
    #[serde(default)]
    pub webhook_url: Option<String>,
    /// 全局 HMAC 密钥（演示用）
    #[serde(default)]
    pub global_hmac_key: Option<String>,
}

impl PocketConfig {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), PocketError> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(PocketError::Io)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(PocketError::Json)?;
        std::fs::write(&path, text).map_err(PocketError::Io)?;
        Ok(())
    }

    /// 找匹配的配对
    pub fn find_pairing(&self, source: PocketSource, user_id: &str, chat_id: &str) -> Option<&Pairing> {
        self.pairings.iter().find(|p| {
            p.source == source && p.user_id == user_id && (p.chat_id == chat_id || p.chat_type == "direct") && p.enabled
        })
    }

    pub fn add_pairing(&mut self, p: Pairing) {
        // 替换已存在的
        if let Some(idx) = self.pairings.iter().position(|x| x.id == p.id) {
            self.pairings[idx] = p;
        } else {
            self.pairings.push(p);
        }
    }

    pub fn remove_pairing(&mut self, id: &str) -> bool {
        let before = self.pairings.len();
        self.pairings.retain(|p| p.id != id);
        before != self.pairings.len()
    }
}

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".agentshell").join("pocket.json")
}

pub mod http_server;

pub use http_server::{
    start as server_start, stop as server_stop, default_bind, InboundLogEntry,
    ServerHandle, ServerInfo, ServerState, ServerStatus, read_inbound_log,
};

/// HMAC-SHA256 签名
pub fn sign_hmac(key: &str, body: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key");
    mac.update(body.as_bytes());
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// 验签
pub fn verify_hmac(key: &str, body: &str, signature: &str) -> bool {
    let expected = sign_hmac(key, body);
    // constant-time compare (防时序攻击)
    if expected.len() != signature.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(signature.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// 处理入站请求
pub fn handle_request(req: PocketRequest, config: &PocketConfig) -> PocketResponse {
    // 1. 解析 source
    let source = match PocketSource::parse(&req.source) {
        Some(s) => s,
        None => return PocketResponse::error(&format!("unknown source: {}", req.source)),
    };

    // 2. 找配对
    let pairing = match config.find_pairing(source, &req.user_id, &req.chat_id) {
        Some(p) => p,
        None => {
            return PocketResponse::error(&format!(
                "no pairing for {} user={} chat={}",
                source.as_str(),
                req.user_id,
                req.chat_id
            ))
        }
    };

    // 3. 验签
    if let Some(sig) = &req.signature {
        let body = format!(
            r#"{{"user_id":"{}","chat_id":"{}","text":"{}"}}"#,
            req.user_id, req.chat_id, req.text
        );
        if !verify_hmac(&pairing.signature_key, &body, sig) {
            return PocketResponse::error("signature verification failed");
        }
    }

    // 4. 路由到 thread（演示：thread_id 来自 user_id + chat_id hash）
    let thread_id = format!("pocket-{}-{}", source.as_str(), uuid::Uuid::new_v4().simple());

    PocketResponse::accepted(&thread_id)
}

#[derive(Debug, thiserror::Error)]
pub enum PocketError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_parse() {
        assert_eq!(PocketSource::parse("feishu"), Some(PocketSource::Feishu));
        assert_eq!(PocketSource::parse("FEISHU"), Some(PocketSource::Feishu));
        assert_eq!(PocketSource::parse("lark"), Some(PocketSource::Feishu));
        assert_eq!(PocketSource::parse("wecom"), Some(PocketSource::WeCom));
        assert_eq!(PocketSource::parse("dingtalk"), Some(PocketSource::DingTalk));
        assert_eq!(PocketSource::parse("slack"), Some(PocketSource::Slack));
        assert_eq!(PocketSource::parse("unknown"), None);
    }

    #[test]
    fn test_hmac_sign_verify() {
        let key = "test-secret-123";
        let body = r#"{"user_id":"u1","text":"hello"}"#;
        let sig = sign_hmac(key, body);
        assert!(sig.starts_with("sha256="));
        assert!(verify_hmac(key, body, &sig));
        assert!(!verify_hmac("wrong-key", body, &sig));
        assert!(!verify_hmac(key, body, "sha256=bad"));
    }

    #[test]
    fn test_handle_request_no_pairing() {
        let config = PocketConfig::default();
        let req = PocketRequest {
            source: "feishu".into(),
            user_id: "u1".into(),
            user_name: "User".into(),
            chat_id: "c1".into(),
            chat_type: "direct".into(),
            text: "hi".into(),
            attachments: vec![],
            signature: None,
            timestamp: None,
        };
        let r = handle_request(req, &config);
        assert_eq!(r.status, "error");
        assert!(r.message.contains("no pairing"));
    }

    #[test]
    fn test_handle_request_success() {
        let mut config = PocketConfig::default();
        let key = "test-key".to_string();
        config.add_pairing(Pairing {
            id: "p1".into(),
            source: PocketSource::Feishu,
            user_id: "u1".into(),
            user_name: "User".into(),
            chat_id: "c1".into(),
            chat_type: "direct".into(),
            signature_key: key.clone(),
            paired_at: 0,
            enabled: true,
        });
        let body = r#"{"user_id":"u1","chat_id":"c1","text":"hi"}"#;
        let sig = sign_hmac(&key, body);
        let req = PocketRequest {
            source: "feishu".into(),
            user_id: "u1".into(),
            user_name: "User".into(),
            chat_id: "c1".into(),
            chat_type: "direct".into(),
            text: "hi".into(),
            attachments: vec![],
            signature: Some(sig),
            timestamp: None,
        };
        let r = handle_request(req, &config);
        assert_eq!(r.status, "accepted");
        assert!(r.thread_id.starts_with("pocket-feishu-"));
    }

    #[test]
    fn test_handle_request_bad_signature() {
        let mut config = PocketConfig::default();
        config.add_pairing(Pairing {
            id: "p1".into(),
            source: PocketSource::Slack,
            user_id: "u1".into(),
            user_name: "User".into(),
            chat_id: "c1".into(),
            chat_type: "direct".into(),
            signature_key: "real-key".into(),
            paired_at: 0,
            enabled: true,
        });
        let req = PocketRequest {
            source: "slack".into(),
            user_id: "u1".into(),
            user_name: "User".into(),
            chat_id: "c1".into(),
            chat_type: "direct".into(),
            text: "hi".into(),
            attachments: vec![],
            signature: Some("sha256=deadbeef".into()),
            timestamp: None,
        };
        let r = handle_request(req, &config);
        assert_eq!(r.status, "error");
        assert!(r.message.contains("signature"));
    }

    #[test]
    fn test_pairing_remove() {
        let mut config = PocketConfig::default();
        config.add_pairing(Pairing {
            id: "p1".into(),
            source: PocketSource::Feishu,
            user_id: "u1".into(),
            user_name: "User".into(),
            chat_id: "c1".into(),
            chat_type: "direct".into(),
            signature_key: "k".into(),
            paired_at: 0,
            enabled: true,
        });
        assert_eq!(config.pairings.len(), 1);
        assert!(config.remove_pairing("p1"));
        assert_eq!(config.pairings.len(), 0);
    }

    #[test]
    fn test_source_all() {
        let all = PocketSource::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&PocketSource::Feishu));
    }
}
