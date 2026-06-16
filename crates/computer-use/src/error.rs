//! Computer Use 错误

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ComputerUseError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("spawn failed: {0}")]
    Spawn(String),

    #[error("script error: {0}")]
    Script(String),

    #[error("timeout after {0}ms")]
    Timeout(u32),

    #[error("browser not initialized")]
    NotInitialized,

    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, ComputerUseError>;