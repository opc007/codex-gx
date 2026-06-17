//! AgentShell 核心：会话、消息、工具注册表
//!
//! 设计参考：docs/开发文档.md §3 / §5

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod config;
pub mod error;
pub mod message;
pub mod permission;
pub mod permission_config;
pub mod session;
pub mod tool;

pub use config::AgentConfig;
pub use error::{Error, Result};
pub use message::{ContentBlock, Message, MessageRole, ToolCall, ToolResult};
pub use permission::{PermissionLevel, DEFAULT_BASH_RISK_PATTERNS, DEFAULT_BLOCKED_PATHS};
pub use permission_config::PermissionConfig;
pub use session::{Session, SessionId, SessionManager};
pub use tool::{Tool, ToolRegistry, ToolSchema};
