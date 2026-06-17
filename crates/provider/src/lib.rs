//! AgentShell LLM Provider 适配
//!
//! 设计参考：docs/开发文档.md §4.3 多 provider + §5.3 Protocol
//!
//! 所有 provider 都暴露统一的 [`Model`] trait；具体实现走 OpenAI 兼容 Chat Completions API。
//! MiniMax M3 也走 OpenAI Chat Completions 协议（tool_calls + reasoning_split）

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod error;
pub mod model;
pub mod request;
pub mod response;
pub mod stream;
pub mod openai_compat;
pub mod minimax;
pub mod anthropic;
pub mod deepseek;
pub mod local;
pub mod registry;

pub use error::{ProviderError, Result};
pub use model::{Model, ModelCapabilities, ModelInfo, StopReason, Usage};
pub use request::{ChatRequest, ChatMessage, ChatRole, ToolDefinition};
pub use response::ChatResponse;
pub use openai_compat::OpenAiCompatProvider;
pub use minimax::MinimaxProvider;
pub use anthropic::AnthropicProvider;
pub use deepseek::DeepSeekProvider;
pub use local::{
    discover_all, llama_cpp_info, LlamaCppEntry, LlamaCppProvider, LocalDiscovery, ollama_info,
    OllamaModelEntry, OllamaProvider,
};
pub use registry::ProviderRegistry;