//! MCP 传输层
//!
//! - `stdio`：子进程 stdin/stdout（v0.5）
//! - `http`：streamable HTTP（SSE 流式响应，可选 POST 普通 JSON-RPC，v0.9）
//! - `pool`：连接池 + 自动重连（v0.9）

pub mod http;
pub mod pool;
pub mod stdio;
