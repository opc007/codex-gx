//! DeepSeek Provider（OpenAI 协议兼容）

use async_trait::async_trait;

use crate::error::Result;
use crate::model::{Model, ModelCapabilities, ModelInfo};
use crate::openai_compat::OpenAiCompatProvider;
use crate::request::ChatRequest;
use crate::response::ChatResponse;
use crate::stream::ChatStream;

pub struct DeepSeekProvider {
    inner: OpenAiCompatProvider,
}

impl DeepSeekProvider {
    pub fn new(
        model_id: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
    ) -> Self {
        let id = model_id.into();
        let info = match id.as_str() {
            "deepseek-v4-pro" | "deepseek-v3-pro" => ModelInfo {
                id: id.clone(),
                name: "DeepSeek V4 Pro".into(),
                provider: "deepseek".into(),
                max_context: 128_000,
                max_output: 8_192,
                capabilities: ModelCapabilities {
                    tools: true,
                    vision: false,
                    video: false,
                    reasoning: true,
                    interleaved_thinking: false,
                    computer_use: false,
                },
                input_price_per_m: 0.27,
                output_price_per_m: 1.10,
                cache_read_price_per_m: 0.07,
                reasoning_efforts: vec![],
            },
            _ => ModelInfo {
                id: id.clone(),
                name: id.clone(),
                provider: "deepseek".into(),
                max_context: 64_000,
                max_output: 8_192,
                capabilities: ModelCapabilities {
                    tools: true,
                    ..Default::default()
                },
                input_price_per_m: 0.14,
                output_price_per_m: 0.28,
                cache_read_price_per_m: 0.014,
                reasoning_efforts: vec![],
            },
        };
        let base = base_url.unwrap_or_else(|| "https://api.deepseek.com/v1".into());
        Self {
            inner: OpenAiCompatProvider::new(info, base, api_key),
        }
    }

    pub fn from_env(model: impl Into<String>) -> Option<Self> {
        std::env::var("DEEPSEEK_API_KEY")
            .ok()
            .map(|k| Self::new(model, k, None))
    }
}

#[async_trait]
impl Model for DeepSeekProvider {
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
