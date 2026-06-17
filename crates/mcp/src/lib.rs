//! AgentShell MCP (Model Context Protocol)
//!
//! 设计参考：docs/开发文档.md §5.20 MCP + §5.33 App-Server
//!
//! MCP 是 Anthropic 主导的开放协议，让 Agent 调用外部工具（数据库 / 浏览器 / IDE / etc）
//! 当前实现：
//! - JSON-RPC 2.0 消息定义
//! - stdio 传输
//! - 客户端（tools/list + tools/call）
//!
//! v0.1 仅做 stdio；WS / UDS 在 v0.4 加

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod client;
pub mod error;
pub mod jsonrpc;
pub mod message;
pub mod transport;

pub use client::McpClient;
pub use error::{McpError, Result};
pub use message::{
    Capabilities, ClientRequest, Content, Implementation, ServerResponse, Tool, ToolCallParams,
    ToolCallResult,
};
pub use transport::http::{HttpEndpoint, HttpTransport};
pub use transport::pool::{Backend, McpPool, TransportConfig};
pub use transport::stdio::StdioTransport;
