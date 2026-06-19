//! Anthropic Claude Provider
//!
//! 协议：Anthropic Messages API（独立实现，不走 OpenAI compat）
//! 端点：POST {base_url}/v1/messages
//! 头部：x-api-key: <key> + anthropic-version: 2023-06-01

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{ProviderError, Result};
use crate::model::{Model, ModelCapabilities, ModelInfo};
use crate::request::{ChatRequest, ChatRole};
use crate::response::ChatResponse;
use crate::stream::ChatStream;

/// Anthropic Claude Provider
pub struct AnthropicProvider {
    info: ModelInfo,
    base_url: String,
    api_key: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(
        model_id: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
    ) -> Self {
        let id = model_id.into();
        let info = default_info(&id);
        Self {
            info,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".into()),
            api_key: api_key.into(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("build client"),
        }
    }

    pub fn from_env(model: impl Into<String>) -> Option<Self> {
        std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .map(|k| Self::new(model, k, None))
    }
}

fn default_info(id: &str) -> ModelInfo {
    let caps = ModelCapabilities {
        tools: true,
        vision: true,
        video: false,
        reasoning: true,
        interleaved_thinking: true,
        computer_use: true,
    };
    match id {
        "claude-opus-4-8" | "claude-opus-4-5" => ModelInfo {
            id: id.into(),
            name: "Claude Opus 4.8".into(),
            provider: "anthropic".into(),
            max_context: 200_000,
            max_output: 8_192,
            capabilities: caps,
            input_price_per_m: 15.0,
            output_price_per_m: 75.0,
            cache_read_price_per_m: 1.5,
            reasoning_efforts: vec![],
        },
        "claude-sonnet-4-5" => ModelInfo {
            id: id.into(),
            name: "Claude Sonnet 4.5".into(),
            provider: "anthropic".into(),
            max_context: 200_000,
            max_output: 8_192,
            capabilities: caps,
            input_price_per_m: 3.0,
            output_price_per_m: 15.0,
            cache_read_price_per_m: 0.30,
            reasoning_efforts: vec![],
        },
        _ => ModelInfo {
            id: id.into(),
            name: id.into(),
            provider: "anthropic".into(),
            max_context: 200_000,
            max_output: 8_192,
            capabilities: caps,
            input_price_per_m: 3.0,
            output_price_per_m: 15.0,
            cache_read_price_per_m: 0.30,
            reasoning_efforts: vec![],
        },
    }
}

/// Anthropic 请求体（简化版）
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    system: Vec<AnthropicSystemBlock>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinking>,
}

#[derive(Debug, Serialize)]
struct AnthropicThinking {
    #[serde(rename = "type")]
    kind: String,
    budget_tokens: u32,
}

#[derive(Debug, Serialize)]
struct AnthropicSystemBlock {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    content: Vec<AnthropicContent>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

#[async_trait]
impl Model for AnthropicProvider {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        // 提取 system 消息
        let mut system = Vec::new();
        let mut messages = Vec::new();
        for m in req.messages {
            match m.role {
                ChatRole::System => {
                    for p in m.content {
                        if let crate::request::ChatContentPart::Text { text } = p {
                            system.push(AnthropicSystemBlock {
                                kind: "text".into(),
                                text,
                            });
                        }
                    }
                }
                ChatRole::User => {
                    messages.push(AnthropicMessage {
                        role: "user".into(),
                        content: serde_json::to_value(m.content).unwrap_or(serde_json::json!([])),
                    });
                }
                ChatRole::Assistant => {
                    messages.push(AnthropicMessage {
                        role: "assistant".into(),
                        content: serde_json::to_value(m.content).unwrap_or(serde_json::json!([])),
                    });
                }
                ChatRole::Tool => {
                    // Anthropic: tool_result block in user message
                    let mut content = Vec::new();
                    for p in m.content {
                        if let crate::request::ChatContentPart::Text { text } = p {
                            if let Some(tcid) = &m.tool_call_id {
                                content.push(serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tcid,
                                    "content": text,
                                }));
                            }
                        }
                    }
                    messages.push(AnthropicMessage {
                        role: "user".into(),
                        content: serde_json::json!(content),
                    });
                }
            }
        }

        let thinking = if self.info.capabilities.interleaved_thinking {
            Some(AnthropicThinking {
                kind: "enabled".into(),
                budget_tokens: 4096,
            })
        } else {
            None
        };

        let anthropic_req = AnthropicRequest {
            model: req.model.clone(),
            max_tokens: req.max_tokens.unwrap_or(8192),
            system,
            messages,
            tools: req
                .tools
                .into_iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.function.name,
                        "description": t.function.description,
                        "input_schema": t.function.parameters,
                    })
                })
                .collect(),
            temperature: req.temperature,
            thinking,
        };

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&anthropic_req)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: body,
            });
        }

        let ar: AnthropicResponse = resp.json().await?;
        let mut text_content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls = Vec::new();
        for c in ar.content {
            match c.kind.as_str() {
                "text" => {
                    if let Some(t) = c.text {
                        text_content.push_str(&t);
                    }
                }
                "thinking" => {
                    if let Some(t) = c.thinking {
                        reasoning_content.push_str(&t);
                    }
                }
                "tool_use" => {
                    tool_calls.push(crate::response::ToolCall {
                        id: c.id.unwrap_or_default(),
                        call_type: "function".into(),
                        function: crate::response::ToolCallFunction {
                            name: c.name.unwrap_or_default(),
                            arguments: serde_json::to_string(
                                &c.input.unwrap_or(serde_json::json!({})),
                            )
                            .unwrap_or("{}".into()),
                        },
                    });
                }
                _ => {}
            }
        }

        let stop_reason = match ar.stop_reason.as_deref() {
            Some("end_turn") => crate::model::StopReason::EndTurn,
            Some("tool_use") => crate::model::StopReason::ToolUse,
            Some("max_tokens") => crate::model::StopReason::MaxTokens,
            _ => crate::model::StopReason::EndTurn,
        };

        let usage = crate::model::Usage {
            input_tokens: ar.usage.input_tokens,
            output_tokens: ar.usage.output_tokens,
            cache_read_tokens: ar.usage.cache_read_input_tokens,
            cache_write_tokens: ar.usage.cache_creation_input_tokens,
        };

        Ok(ChatResponse {
            id: ar.id,
            model: ar.model,
            created: Some(chrono::Utc::now().timestamp()),
            choices: Some(vec![crate::response::ChatChoice {
                index: 0,
                message: crate::response::AssistantMessage {
                    role: "assistant".into(),
                    content: text_content,
                    reasoning_content: if reasoning_content.is_empty() {
                        None
                    } else {
                        Some(reasoning_content)
                    },
                    tool_calls,
                },
                finish_reason: Some(stop_reason),
            }]),
            usage: Some(usage),
            system_fingerprint: None,
        })
    }

    async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream> {
        // 流式实现（v0.1 占位，后续接 SSE）
        Err(ProviderError::Protocol(
            "Anthropic stream 暂未实现，请用 chat".into(),
        ))
    }
}
