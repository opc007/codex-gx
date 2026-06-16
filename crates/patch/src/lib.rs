//! AgentShell apply_patch — Codex 风格
//!
//! 设计参考：docs/开发文档.md §5.2 apply_patch 协议
//!
//! ## Patch 语法
//!
//! ```text
//! *** Begin Patch
//! *** Update File: path/to/file
//! @@ context line (unchanged)
//! -removed line
//! +added line
//!  unchanged line
//! *** Add File: path/to/new
//! +line 1
//! +line 2
//! *** Delete File: path/to/old
//! *** End Patch
//! ```
//!
//! ## 关键点
//! - 行首 `-` 删除 / `+` 添加 / 空格 保留
//! - 上下文匹配（context 模式）：未标记的行必须与原文一致，否则报错
//! - 支持 hunk 多段：每个 hunk 前必须有空行或 `@@` 上下文标记

#![warn(missing_docs)]
#![warn(clippy::all)]

mod parser;
mod apply;
mod format;

pub use parser::{parse_patch, PatchParseError, PatchOperation, PatchHunk, PatchLine};
pub use apply::{apply_patch, apply_to_dir, PatchApplyError, PatchResult};
pub use format::{format_patch, summarize};