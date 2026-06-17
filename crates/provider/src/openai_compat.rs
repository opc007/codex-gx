//! OpenAI Chat Completions 兼容 Provider
//!
//! 适用于：OpenAI / DeepSeek / 任何 OpenAI 协议兼容的服务

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{ProviderError, Result};
use crate::model::{Model, ModelCapabilities, ModelInfo, WireApi};
use crate::request::{ChatMessage, ChatRequest, ChatRole};
use crate::response::ChatResponse;
use crate::stream::{ChatStream, OpenAiStreamChunk, StreamChunk};

/// OpenAI 兼容 Provider
pub struct OpenAiCompatProvider {
    info: ModelInfo,
    base_url: String,
    api_key: String,
    client: Client,
}

impl OpenAiCompatProvider {
    pub fn new(info: ModelInfo, base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            info,
            base_url: base_url.into(),
            api_key: api_key.into(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("build client"),
        }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_split: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

impl From<ChatMessage> for OpenAiMessage {
    fn from(m: ChatMessage) -> Self {
        let content = if m.content.len() == 1 {
            if let crate::request::ChatContentPart::Text { text } = &m.content[0] {
                Some(serde_json::Value::String(text.clone()))
            } else {
                Some(serde_json::to_value(&m.content).unwrap_or(serde_json::json!(null)))
            }
        } else {
            Some(serde_json::to_value(&m.content).unwrap_or(serde_json::json!(null)))
        };

        let tool_calls = if m.role == ChatRole::Assistant {
            let calls: Vec<serde_json::Value> = m
                .content
                .iter()
                .filter_map(|p| match p {
                    crate::request::ChatContentPart::ToolUse { id, name, input } => {
                        Some(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(input).unwrap_or("{}".into()),
                            }
                        }))
                    }
                    _ => None,
                })
                .collect();
            if calls.is_empty() {
                None
            } else {
                Some(calls)
            }
        } else {
            None
        };

        Self {
            role: m.role.as_str().to_string(),
            content,
            reasoning_content: m.reasoning_content,
            tool_calls,
            tool_call_id: m.tool_call_id,
        }
    }
}

#[async_trait]
impl Model for OpenAiCompatProvider {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let openai_req = OpenAiRequest {
            model: req.model.clone(),
            messages: req.messages.into_iter().map(Into::into).collect(),
            tools: req
                .tools
                .into_iter()
                .map(|t| serde_json::to_value(t).unwrap_or(serde_json::json!({})))
                .collect(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_p: req.top_p,
            reasoning_effort: req.reasoning_effort,
            reasoning_split: req.reasoning_split,
            stop: req.stop,
            stream: false,
            user: req.user,
        };

        let resp = self
            .client
            .post(self.url("/chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&openai_req)
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ProviderError::InvalidApiKey);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: body,
            });
        }

        let body = resp.text().await?;
        let parsed: ChatResponse = serde_json::from_str(&body)?;
        Ok(parsed)
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream> {
        let mut openai_req = OpenAiRequest {
            model: req.model.clone(),
            messages: req.messages.into_iter().map(Into::into).collect(),
            tools: req
                .tools
                .into_iter()
                .map(|t| serde_json::to_value(t).unwrap_or(serde_json::json!({})))
                .collect(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_p: req.top_p,
            reasoning_effort: req.reasoning_effort,
            reasoning_split: req.reasoning_split,
            stop: req.stop,
            stream: true,
            user: req.user,
        };
        openai_req.stream = true;

        let resp = self
            .client
            .post(self.url("/chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&openai_req)
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

        let mut stream = resp.bytes_stream();

        let s = async_stream::stream! {
            let mut buffer = String::new();
            while let Some(chunk_res) = stream.next().await {
                let chunk = chunk_res.map_err(ProviderError::Http)?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // SSE: 按 \n\n 分隔
                while let Some(idx) = buffer.find("\n\n") {
                    let event_str: String = buffer.drain(..idx + 2).collect();
                    for line in event_str.lines() {
                        let line = line.trim();
                        if let Some(rest) = line.strip_prefix("data:") {
                            let data = rest.trim();
                            if data == "[DONE]" {
                                yield Ok(StreamChunk::Done);
                                return;
                            }
                            if data.is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<OpenAiStreamChunk>(data) {
                                Ok(chunk) => {
                                    if let Some(usage) = chunk.usage {
                                        yield Ok(StreamChunk::Usage(usage));
                                    }
                                    for choice in chunk.choices {
                                        let delta = choice.delta;
                                        if let Some(content) = delta.content {
                                            if !content.is_empty() {
                                                yield Ok(StreamChunk::Content(content));
                                            }
                                        }
                                        if let Some(reasoning) = delta.reasoning_content {
                                            if !reasoning.is_empty() {
                                                yield Ok(StreamChunk::Reasoning(reasoning));
                                            }
                                        }
                                        for tc in delta.tool_calls {
                                            yield Ok(StreamChunk::ToolCallDelta {
                                                index: tc.index,
                                                id: tc.id,
                                                name: tc.function.as_ref().and_then(|f| f.name.clone()),
                                                arguments_delta: tc.function.and_then(|f| f.arguments).unwrap_or_default(),
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    yield Err(ProviderError::Json(e));
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            yield Ok(StreamChunk::Done);
        };

        Ok(Box::pin(s))
    }
}
