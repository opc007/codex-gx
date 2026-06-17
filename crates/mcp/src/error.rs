//! MCP 错误

use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("jsonrpc error: {0}")]
    JsonRpc(i32, String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("transport closed")]
    TransportClosed,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("internal: {0}")]
    Internal(String),

    #[error("transport error: {0}")]
    Transport(String),
}

pub type Result<T> = std::result::Result<T, McpError>;
