//! AgentShell Computer Use
//!
//! 设计参考：docs/开发文档.md §5.10 Computer Use 浏览器层
//!
//! 当前实现：JS REPL（Playwright JS）
//! - 后端 Rust 通过调用 browser MCP server / 直接 spawn playwright
//! - 模型可发出 `computer_use` 工具调用 → Rust 转发 → JS 执行 → 截图返回
//!
//! 桌面 CUA 在 v0.4 实现（用 macOS AXUIElement / Windows UI Automation）

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod action;
pub mod browser;
pub mod error;
pub mod repl;
pub mod schema;

pub use action::Action;
pub use browser::BrowserSession;
pub use error::{ComputerUseError, Result};
pub use repl::JsRepl;
pub use schema::{BrowserAction, BrowserActionResult, ScreenshotFormat, ViewportSize};