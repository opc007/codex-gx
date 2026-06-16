//! Provider 错误

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("API returned error: status={status}, message={message}")]
    Api {
        status: u16,
        message: String,
    },

    #[error("invalid API key")]
    InvalidApiKey,

    #[error("rate limited")]
    RateLimited,

    #[error("context length exceeded (max: {0})")]
    ContextTooLong(u32),

    #[error("model not supported: {0}")]
    ModelNotSupported(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("stream closed unexpectedly")]
    StreamClosed,

    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, ProviderError>;