//! MCP 错误

use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("jsonrpc error: {0}")]
    JsonRpc(i32, String),

    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("transport closed")]
    TransportClosed,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, McpError>;