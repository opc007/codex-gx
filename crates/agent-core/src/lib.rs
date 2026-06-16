//! AgentShell 核心：会话、消息、工具注册表
//!
//! 设计参考：docs/开发文档.md §3 / §5

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod error;
pub mod message;
pub mod session;
pub mod tool;
pub mod config;

pub use error::{Error, Result};
pub use message::{Message, MessageRole, ContentBlock, ToolCall, ToolResult};
pub use session::{Session, SessionId, SessionManager};
pub use tool::{Tool, ToolRegistry, ToolSchema};
pub use config::AgentConfig;
