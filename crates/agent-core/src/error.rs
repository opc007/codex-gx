//! 错误类型

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("tool not found: {0}")]
    ToolNotFound(String),

    #[error("tool execution failed: {0}")]
    ToolExecution(String),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;
