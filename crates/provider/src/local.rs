//! v1.4：本地 LLM Provider
//!
//! 支持两种后端：
//! - **Ollama**：本地 HTTP API（默认 http://127.0.0.1:11434）
//!   协议：POST /api/chat（NDJSON 流式）
//! - **llama.cpp server**（OpenAI 兼容模式）：HTTP /v1/chat/completions
//!
//! 两个后端都实现 [`Model`] trait。

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{ProviderError, Result};
use crate::model::{Model, ModelCapabilities, ModelInfo, Usage};
use crate::request::{ChatMessage, ChatRequest, ChatRole};
use crate::response::ChatResponse;
use crate::stream::{ChatStream, StreamChunk};

// =============================================================================
// Ollama provider
// =============================================================================

/// Ollama provider（http://127.0.0.1:11434）
pub struct OllamaProvider {
    info: ModelInfo,
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    pub fn new(info: ModelInfo, base_url: impl Into<String>) -> Self {
        Self {
            info,
            base_url: base_url.into(),
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    /// 列出本机可用的 Ollama 模型
    pub async fn list_local(base_url: &str) -> Result<Vec<OllamaModelEntry>> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .build()
            .map_err(|e| ProviderError::Http(e))?;
        let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e))?;
        if !resp.status().is_success() {
            return Err(ProviderError::Api {
                status: resp.status().as_u16(),
                message: "ollama /api/tags failed".to_string(),
            });
        }
        let body: OllamaTagsResponse = resp.json().await.map_err(|e| ProviderError::Http(e))?;
        Ok(body.models)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelEntry {
    pub name: String,
    pub modified_at: Option<String>,
    pub size: Option<u64>,
    pub digest: Option<String>,
    pub details: Option<OllamaModelDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelDetails {
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelEntry>,
}

#[async_trait]
impl Model for OllamaProvider {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let body = ollama_build_body(&self.info.id, &req, false);
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: text.clone(),
            });
        }
        let parsed: OllamaChatResponse = resp.json().await.map_err(|e| ProviderError::Http(e))?;
        let done = parsed.done;
        let prompt_eval_count = parsed.prompt_eval_count;
        let eval_count = parsed.eval_count;
        let mut text = parsed.message.content.unwrap_or_default();
        let reasoning = if text.contains("<think>") {
            extract_thinking(&mut text)
        } else {
            None
        };
        let usage = ollama_usage(prompt_eval_count, eval_count);
        let stop_reason = if done {
            crate::model::StopReason::EndTurn
        } else {
            crate::model::StopReason::MaxTokens
        };
        Ok(ChatResponse {
            id: format!("ollama-{}", chrono_now()),
            model: self.info.id.clone(),
            created: chrono_now() as i64,
            choices: vec![crate::response::ChatChoice {
                index: 0,
                message: crate::response::AssistantMessage {
                    role: "assistant".to_string(),
                    content: text,
                    reasoning_content: reasoning,
                    tool_calls: vec![],
                },
                finish_reason: Some(stop_reason),
            }],
            usage,
            system_fingerprint: None,
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream> {
        let body = ollama_build_body(&self.info.id, &req, true);
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: text.clone(),
            });
        }
        let model_id = self.info.id.clone();
        let byte_stream = resp.bytes_stream();
        let s = async_stream::try_stream! {
            let mut bs = byte_stream;
            while let Some(chunk_res) = bs.next().await {
                let bytes = chunk_res.map_err(|e| ProviderError::Http(e))?;
                if let Some(c) = parse_ollama_ndjson(&bytes, &model_id)? {
                    yield c;
                }
            }
            yield StreamChunk::Done;
        };
        Ok(Box::pin(s))
    }
}

fn ollama_build_body(model: &str, req: &ChatRequest, stream: bool) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = req.messages.iter().map(ollama_message).collect();
    serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": stream,
        "options": {
            "temperature": req.temperature.unwrap_or(0.7),
            "num_predict": req.max_tokens.unwrap_or(2048),
        }
    })
}

fn ollama_message(m: &ChatMessage) -> serde_json::Value {
    let role = match m.role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
    };
    // m.content 是 Vec<ChatContentPart>，我们合并所有 text part
    let text: String = m
        .content
        .iter()
        .filter_map(|p| match p {
            crate::request::ChatContentPart::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::json!({
        "role": role,
        "content": text,
    })
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    #[serde(default)]
    message: OllamaResponseMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    total_duration: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

fn ollama_usage(prompt_eval: Option<u32>, eval: Option<u32>) -> Usage {
    if prompt_eval.is_none() && eval.is_none() {
        return Usage::default();
    }
    Usage {
        input_tokens: prompt_eval.unwrap_or(0),
        output_tokens: eval.unwrap_or(0),
        ..Default::default()
    }
}

fn parse_ollama_ndjson(bytes: &[u8], _model_id: &str) -> Result<Option<StreamChunk>> {
    let text = std::str::from_utf8(bytes).map_err(|e| ProviderError::Internal(e.to_string()))?;
    let mut last_chunk: Option<StreamChunk> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| ProviderError::Internal(format!("ollama ndjson: {e}")))?;
        if let Some(msg) = v.get("message") {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                if !content.is_empty() {
                    last_chunk = Some(StreamChunk::Content(content.to_string()));
                }
            }
        }
        if v.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
            let in_tok = v
                .get("prompt_eval_count")
                .and_then(|x| x.as_u64())
                .unwrap_or(0) as u32;
            let out_tok = v.get("eval_count").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            last_chunk = Some(StreamChunk::Usage(Usage {
                input_tokens: in_tok,
                output_tokens: out_tok,
                ..Default::default()
            }));
        }
    }
    Ok(last_chunk)
}

fn extract_thinking(text: &mut String) -> Option<String> {
    if let Some(start) = text.find("<think>") {
        if let Some(end) = text.find("</think>") {
            let think = text[start + 7..end].to_string();
            *text = format!("{}{}", &text[..start], &text[end + 8..]);
            return Some(think);
        }
    }
    None
}

fn chrono_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// =============================================================================
// Llama.cpp provider (OpenAI 兼容 server)
// =============================================================================

/// llama.cpp server provider（OpenAI 兼容 HTTP）
pub struct LlamaCppProvider {
    info: ModelInfo,
    base_url: String,
    client: Client,
}

impl LlamaCppProvider {
    pub fn new(info: ModelInfo, base_url: impl Into<String>) -> Self {
        Self {
            info,
            base_url: base_url.into(),
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    /// 列出本机可用的模型（GET /v1/models）
    pub async fn list_local(base_url: &str) -> Result<Vec<String>> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .build()
            .map_err(|e| ProviderError::Http(e))?;
        let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e))?;
        if !resp.status().is_success() {
            return Err(ProviderError::Api {
                status: resp.status().as_u16(),
                message: "llama.cpp /v1/models failed".to_string(),
            });
        }
        let body: LlamaCppModelsResponse = resp.json().await.map_err(|e| ProviderError::Http(e))?;
        Ok(body.data.into_iter().map(|m| m.id).collect())
    }
}

#[derive(Debug, Deserialize)]
struct LlamaCppModelsResponse {
    data: Vec<LlamaCppModelEntry>,
}

#[derive(Debug, Deserialize)]
struct LlamaCppModelEntry {
    id: String,
}

#[async_trait]
impl Model for LlamaCppProvider {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let body = llama_cpp_build_body(&self.info.id, &req, false);
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: text.clone(),
            });
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| ProviderError::Http(e))?;
        let text = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let finish = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("finish_reason"))
            .and_then(|f| f.as_str())
            .unwrap_or("stop");
        let stop_reason = match finish {
            "length" => crate::model::StopReason::MaxTokens,
            "tool_calls" => crate::model::StopReason::ToolUse,
            _ => crate::model::StopReason::EndTurn,
        };
        let usage = v
            .get("usage")
            .map(|u| Usage {
                input_tokens: u.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                output_tokens: u
                    .get("completion_tokens")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0) as u32,
                ..Default::default()
            })
            .unwrap_or_default();
        Ok(ChatResponse {
            id: v
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("llamacpp")
                .to_string(),
            model: self.info.id.clone(),
            created: chrono_now() as i64,
            choices: vec![crate::response::ChatChoice {
                index: 0,
                message: crate::response::AssistantMessage {
                    role: "assistant".to_string(),
                    content: text,
                    reasoning_content: None,
                    tool_calls: vec![],
                },
                finish_reason: Some(stop_reason),
            }],
            usage,
            system_fingerprint: None,
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream> {
        let body = llama_cpp_build_body(&self.info.id, &req, true);
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: text.clone(),
            });
        }
        let model_id = self.info.id.clone();
        let byte_stream = resp.bytes_stream();
        let s = async_stream::try_stream! {
            let mut bs = byte_stream;
            while let Some(chunk_res) = bs.next().await {
                let bytes = chunk_res.map_err(|e| ProviderError::Http(e))?;
                if let Some(c) = parse_llama_cpp_sse(&bytes, &model_id)? {
                    yield c;
                }
            }
            yield StreamChunk::Done;
        };
        Ok(Box::pin(s))
    }
}

fn llama_cpp_build_body(model: &str, req: &ChatRequest, stream: bool) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| {
            let role = match m.role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "tool",
            };
            let text: String = m
                .content
                .iter()
                .filter_map(|p| match p {
                    crate::request::ChatContentPart::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            serde_json::json!({ "role": role, "content": text })
        })
        .collect();
    serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": stream,
        "temperature": req.temperature.unwrap_or(0.7),
        "max_tokens": req.max_tokens.unwrap_or(2048),
    })
}

fn parse_llama_cpp_sse(bytes: &[u8], _model_id: &str) -> Result<Option<StreamChunk>> {
    let text = std::str::from_utf8(bytes).map_err(|e| ProviderError::Internal(e.to_string()))?;
    let mut last: Option<StreamChunk> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("data: ") {
            if rest == "[DONE]" {
                last = Some(StreamChunk::Done);
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(rest)
                .map_err(|e| ProviderError::Internal(format!("llama.cpp sse: {e}")))?;
            if let Some(delta) = v
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                if !delta.is_empty() {
                    last = Some(StreamChunk::Content(delta.to_string()));
                }
            }
        }
    }
    Ok(last)
}

// =============================================================================
// 工厂函数（构造 ModelInfo）
// =============================================================================

/// 构造一个 Ollama ModelInfo
pub fn ollama_info(name: &str) -> ModelInfo {
    ModelInfo {
        id: format!("ollama:{}", name),
        name: format!("Ollama · {}", name),
        provider: "ollama".to_string(),
        max_context: 8192,
        max_output: 4096,
        capabilities: ModelCapabilities {
            tools: false,
            vision: name.contains("vision") || name.contains("llava") || name.contains("vl"),
            reasoning: name.contains("r1") || name.contains("qwq") || name.contains("deepseek-r1"),
            ..Default::default()
        },
        input_price_per_m: 0.0,
        output_price_per_m: 0.0,
        cache_read_price_per_m: 0.0,
        reasoning_efforts: vec![],
    }
}

/// 构造一个 llama.cpp ModelInfo
pub fn llama_cpp_info(name: &str) -> ModelInfo {
    ModelInfo {
        id: format!("llamacpp:{}", name),
        name: format!("llama.cpp · {}", name),
        provider: "llamacpp".to_string(),
        max_context: 4096,
        max_output: 2048,
        capabilities: ModelCapabilities {
            tools: false,
            vision: false,
            reasoning: false,
            ..Default::default()
        },
        input_price_per_m: 0.0,
        output_price_per_m: 0.0,
        cache_read_price_per_m: 0.0,
        reasoning_efforts: vec![],
    }
}

/// 探测本机可用的所有本地模型
pub async fn discover_all(ollama_url: Option<&str>, llamacpp_url: Option<&str>) -> LocalDiscovery {
    let mut result = LocalDiscovery::default();
    if let Some(url) = ollama_url {
        match OllamaProvider::list_local(url).await {
            Ok(list) => {
                result.ollama_url = Some(url.to_string());
                result.ollama_models = list;
            }
            Err(e) => {
                result.ollama_error = Some(e.to_string());
            }
        }
    }
    if let Some(url) = llamacpp_url {
        match LlamaCppProvider::list_local(url).await {
            Ok(list) => {
                result.llamacpp_url = Some(url.to_string());
                result.llamacpp_models = list.into_iter().map(|id| LlamaCppEntry { id }).collect();
            }
            Err(e) => {
                result.llamacpp_error = Some(e.to_string());
            }
        }
    }
    result
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LocalDiscovery {
    pub ollama_url: Option<String>,
    pub ollama_models: Vec<OllamaModelEntry>,
    pub ollama_error: Option<String>,
    pub llamacpp_url: Option<String>,
    pub llamacpp_models: Vec<LlamaCppEntry>,
    pub llamacpp_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaCppEntry {
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_info_basic() {
        let i = ollama_info("qwen2.5:7b");
        assert_eq!(i.id, "ollama:qwen2.5:7b");
        assert_eq!(i.provider, "ollama");
        assert!(!i.capabilities.reasoning);
    }

    #[test]
    fn ollama_info_vision() {
        let i = ollama_info("llava:13b");
        assert!(i.capabilities.vision);
    }

    #[test]
    fn ollama_info_thinking() {
        let i = ollama_info("deepseek-r1:7b");
        assert!(i.capabilities.reasoning);
    }

    #[test]
    fn llama_cpp_info_basic() {
        let i = llama_cpp_info("qwen2");
        assert_eq!(i.provider, "llamacpp");
    }

    #[test]
    fn extract_thinking_basic() {
        let mut t = "<think>I think...</think>\nThe answer is 42.".to_string();
        let think = extract_thinking(&mut t).unwrap();
        assert!(think.contains("I think"));
        assert!(t.contains("The answer is 42"));
    }

    #[test]
    fn extract_thinking_none() {
        let mut t = "just text".to_string();
        assert!(extract_thinking(&mut t).is_none());
        assert_eq!(t, "just text");
    }

    #[test]
    fn parse_ollama_ndjson_chunk() {
        let bytes =
            b"{\"message\":{\"content\":\"Hi\"},\"done\":false}\n{\"done\":true,\"prompt_eval_count\":5,\"eval_count\":3}\n";
        let r = parse_ollama_ndjson(bytes, "test").unwrap();
        match r {
            Some(StreamChunk::Usage(u)) => {
                assert_eq!(u.input_tokens, 5);
                assert_eq!(u.output_tokens, 3);
            }
            _ => panic!("expected Usage"),
        }
    }

    #[test]
    fn parse_llama_cpp_sse_basic() {
        let bytes = b"data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n";
        let r = parse_llama_cpp_sse(bytes, "test").unwrap();
        match r {
            Some(StreamChunk::Done) => {}
            _ => panic!("expected Done (last chunk)"),
        }
    }
}
