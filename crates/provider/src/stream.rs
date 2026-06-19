//! 流式响应（SSE）
//!
//! 设计参考：docs/开发文档.md §5.1.3 流式输出

use std::pin::Pin;

use futures::stream::Stream;
use serde::Deserialize;

use crate::error::Result;
use crate::model::Usage;
use crate::response::{AssistantMessage, ChatResponse, ToolCallFunction};

/// 流式 chunk 类型
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// 增量文本
    Content(String),
    /// 增量推理内容
    Reasoning(String),
    /// 工具调用增量
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    /// 用量（最后一个 chunk 携带）
    Usage(Usage),
    /// 流结束
    Done,
}

/// 流（Pin<Box<dyn Stream>>）
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Sync>>;

/// 流最终聚合
#[derive(Debug, Default, Clone)]
pub struct StreamAccumulator {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCallAccum>,
    pub usage: Usage,
}

#[derive(Debug, Default, Clone)]
pub struct ToolCallAccum {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl StreamAccumulator {
    /// 应用一个 chunk
    pub fn apply(&mut self, chunk: &StreamChunk) {
        match chunk {
            StreamChunk::Content(s) => self.content.push_str(s),
            StreamChunk::Reasoning(s) => self.reasoning.push_str(s),
            StreamChunk::ToolCallDelta {
                index,
                id,
                name,
                arguments_delta,
            } => {
                while self.tool_calls.len() <= *index {
                    self.tool_calls.push(ToolCallAccum::default());
                }
                let tc = &mut self.tool_calls[*index];
                if let Some(i) = id {
                    tc.id = i.clone();
                }
                if let Some(n) = name {
                    tc.name = n.clone();
                }
                tc.arguments.push_str(arguments_delta);
            }
            StreamChunk::Usage(u) => self.usage = u.clone(),
            StreamChunk::Done => {}
        }
    }

    /// 转为 ChatResponse
    pub fn into_response(self, id: String, model: String) -> ChatResponse {
        let tool_calls: Vec<crate::response::ToolCall> = self
            .tool_calls
            .into_iter()
            .map(|t| crate::response::ToolCall {
                id: t.id,
                call_type: "function".into(),
                function: ToolCallFunction {
                    name: t.name,
                    arguments: t.arguments,
                },
            })
            .collect();

        ChatResponse {
            id,
            model,
            choices: Some(vec![crate::response::ChatChoice {
                index: 0,
                message: AssistantMessage {
                    role: "assistant".into(),
                    content: self.content,
                    reasoning_content: if self.reasoning.is_empty() {
                        None
                    } else {
                        Some(self.reasoning)
                    },
                    tool_calls,
                },
                finish_reason: Some(crate::model::StopReason::EndTurn),
            }]),
            usage: Some(self.usage),
            created: Some(chrono::Utc::now().timestamp()),
            system_fingerprint: None,
        }
    }
}

/// OpenAI 流式 chunk（部分字段）
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiStreamChunk {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<OpenAiChoiceChunk>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChoiceChunk {
    pub index: u32,
    #[serde(default)]
    pub delta: OpenAiDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct OpenAiDelta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<OpenAiToolCallDelta>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct OpenAiToolCallDelta {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    #[serde(default)]
    pub function: Option<OpenAiFunctionDelta>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct OpenAiFunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}
