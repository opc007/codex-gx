//! AgentShell 上下文管理
//!
//! 设计参考：docs/开发文档.md §5.9 AGENTS.md / §5.42 IDE context / §8.5 Compact
//!
//! ## 模块
//! - [`agents_md`]：AGENTS.md 文件加载 + 注入到 system prompt
//! - [`ide`]：从 VSCode / Cursor 拉取当前打开的文件
//! - [`compact`]：对话历史压缩（token 超限时裁剪）
//! - [`file_search`]：fuzzy 文件搜索（@ mention）

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod agents_md;
pub mod compact;
pub mod file_search;
pub mod ide;

pub use agents_md::{load_agents_md, AgentsMd};
pub use compact::{compact_messages, estimate_tokens, CompactionStrategy};
