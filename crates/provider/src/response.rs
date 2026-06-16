//! 聊天响应
//!
//! 覆盖 OpenAI Chat Completions 兼容格式 + M3 扩展（reasoning_content）

use serde::{Deserialize, Serialize};

use crate::model::{StopReason, Usage};
use crate::request::ChatContentPart;

/// 单个 choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: AssistantMessage,
    #[serde(default)]
    pub finish_reason: Option<StopReason>,
}

/// 助手消息（响应中）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: String,
    /// 推理内容（M3 / Claude extended thinking）
    #[serde(default)]
    pub reasoning_content: Option<String>,
    /// 工具调用
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// 类型（function）
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// JSON 字符串
    pub arguments: String,
}

impl AssistantMessage {
    /// 转 ChatContentPart（用于后续合并到对话历史）
    pub fn to_parts(&self) -> Vec<ChatContentPart> {
        let mut parts = Vec::new();
        if !self.content.is_empty() {
            parts.push(ChatContentPart::Text {
                text: self.content.clone(),
            });
        }
        for tc in &self.tool_calls {
            let input: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
            parts.push(ChatContentPart::ToolUse {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                input,
            });
        }
        parts
    }
}

/// 聊天响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Usage,
    pub created: i64,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
}

impl ChatResponse {
    /// 第一个 choice 的助手消息（克隆）
    pub fn first_message(&self) -> Option<&AssistantMessage> {
        self.choices.first().map(|c| &c.message)
    }

    /// 第一个 choice 的停止原因
    pub fn stop_reason(&self) -> Option<StopReason> {
        self.choices.first().and_then(|c| c.finish_reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_parsing() {
        let json = r#"{
            "id": "test",
            "model": "MiniMax-M3",
            "created": 123,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "hi",
                    "reasoning_content": "thinking..."
                },
                "finish_reason": "end_turn"
            }],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0
            }
        }"#;
        let r: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.choices.len(), 1);
        let m = r.first_message().unwrap();
        assert_eq!(m.content, "hi");
        assert_eq!(m.reasoning_content.as_deref(), Some("thinking..."));
        assert_eq!(r.usage.total(), 15);
    }
}