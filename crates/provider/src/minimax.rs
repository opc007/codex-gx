//! MiniMax M3 Provider
//!
//! M3 协议 = OpenAI Chat Completions 兼容 + 扩展
//! - 端点：POST {base_url}/v1/text/chatcompletion_v2
//! - 扩展：reasoning_split=true 启用 interleaved thinking
//! - 价格：$0.60/M input / $2.40/M output / $0.12/M cache read
//!
//! 设计参考：docs/开发文档.md §A.0.6 API 接入形式

use async_trait::async_trait;

use crate::error::Result;
use crate::model::{Model, ModelCapabilities, ModelInfo};
use crate::openai_compat::OpenAiCompatProvider;
use crate::request::ChatRequest;
use crate::response::ChatResponse;
use crate::stream::ChatStream;

/// MiniMax M3 Provider
pub struct MinimaxProvider {
    inner: OpenAiCompatProvider,
}

impl MinimaxProvider {
    /// 创建 M3 Provider（API key 从环境变量读或传入）
    pub fn new(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        let info = ModelInfo {
            id: "MiniMax-M3".into(),
            name: "MiniMax M3".into(),
            provider: "minimax".into(),
            max_context: 1_000_000,
            max_output: 16_384,
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                video: true,
                reasoning: true,
                interleaved_thinking: true,
                computer_use: true,
            },
            input_price_per_m: 0.60,
            output_price_per_m: 2.40,
            cache_read_price_per_m: 0.12,
            reasoning_efforts: vec![
                "minimal".into(),
                "low".into(),
                "medium".into(),
                "high".into(),
                "xhigh".into(),
            ],
        };
        let base = base_url.unwrap_or_else(|| "https://api.minimaxi.com/v1".into());
        Self {
            inner: OpenAiCompatProvider::new(info, base, api_key),
        }
    }

    /// 从环境变量 MINIMAX_API_KEY 读 key
    pub fn from_env() -> Option<Self> {
        std::env::var("MINIMAX_API_KEY")
            .ok()
            .map(|k| Self::new(k, None))
    }
}

#[async_trait]
impl Model for MinimaxProvider {
    fn info(&self) -> &ModelInfo {
        self.inner.info()
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        self.inner.chat(req).await
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream> {
        self.inner.chat_stream(req).await
    }
}
