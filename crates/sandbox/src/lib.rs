//! AgentShell 沙箱
//!
//! 设计参考：docs/开发文档.md §5.5.7 沙箱 / §7.4 安全模型
//!
//! - **macOS**：使用 `sandbox-exec` + Seatbelt profile（Codex 风格）
//! - **Windows**：使用 Job Objects + AppContainer（v0.4 实现）
//! - **Linux**：使用 Landlock / seccomp（v0.4 实现）
//!
//! 当前版本提供：
//! - 跨平台 `SandboxPolicy` 抽象
//! - TOML 策略加载
//! - 路径规则（allow/deny）
//! - 命令生成（macOS sandbox-exec 完整 + 其他平台 stub）

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod command;
pub mod platform;
pub mod policy;

pub use command::{build_sandbox_command, SandboxMode};
pub use platform::{current_platform, Platform};
pub use policy::{Decision, FilesystemRule, LoadPolicyError, NetworkRule, SandboxPolicy};
