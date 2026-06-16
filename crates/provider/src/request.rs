//! Chat 请求结构
//!
//! 设计参考：docs/开发文档.md §5.1 消息结构 / §5.4 Reasoning

use serde::{Deserialize, Serialize};

/// 角色（OpenAI 兼容）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ChatRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// 消息内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatContentPart {
    /// 文本
    Text { text: String },
    /// 图像 URL
    ImageUrl { url: String, detail: Option<String> },
    /// 图像 base64
    ImageBase64 { data: String, mime_type: String },
    /// 工具调用（assistant 发出）
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// 工具结果（tool 响应）
    ToolResult {
        tool_call_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: Vec<ChatContentPart>,
    /// 助手消息的 reasoning_content（M3 interleaved thinking 需要）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// tool call id（role=tool 时必填）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: vec![ChatContentPart::Text { text: text.into() }],
            reasoning_content: None,
            tool_call_id: None,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: vec![ChatContentPart::Text { text: text.into() }],
            reasoning_content: None,
            tool_call_id: None,
        }
    }

    pub fn user_image(text: impl Into<String>, image_url: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: vec![
                ChatContentPart::Text { text: text.into() },
                ChatContentPart::ImageUrl {
                    url: image_url.into(),
                    detail: None,
                },
            ],
            reasoning_content: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: vec![ChatContentPart::Text { text: text.into() }],
            reasoning_content: None,
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: vec![ChatContentPart::Text { text: content.into() }],
            reasoning_content: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// 拼接所有文本
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|p| match p {
                ChatContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具类型（OpenAI: "function"）
    #[serde(rename = "type")]
    pub tool_type: String,
    /// 函数信息
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    /// JSON Schema
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            tool_type: "function".into(),
            function: ToolFunction {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// 聊天请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    /// M3 / OpenAI o-series: reasoning_effort
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// M3: reasoning_split=true 启用 interleaved thinking
    #[serde(default, skip_serializing_if = "is_false")]
    pub reasoning_split: Option<bool>,
    /// 停止词
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// 流式
    #[serde(default)]
    pub stream: bool,
    /// 用户标识（追踪用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

fn is_false(b: &Option<bool>) -> bool {
    matches!(b, Some(false) | None)
}

impl ChatRequest {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: Vec::new(),
            tools: Vec::new(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            reasoning_split: None,
            stop: Vec::new(),
            stream: false,
            user: None,
        }
    }

    pub fn with_message(mut self, msg: ChatMessage) -> Self {
        self.messages.push(msg);
        self
    }

    pub fn with_messages(mut self, msgs: impl IntoIterator<Item = ChatMessage>) -> Self {
        self.messages.extend(msgs);
        self
    }

    pub fn with_tool(mut self, tool: ToolDefinition) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }

    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    pub fn with_reasoning_split(mut self, split: bool) -> Self {
        self.reasoning_split = Some(split);
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_text() {
        let m = ChatMessage::user("hello");
        assert_eq!(m.role, ChatRole::User);
        assert_eq!(m.text_content(), "hello");
    }

    #[test]
    fn test_chat_request_build() {
        let req = ChatRequest::new("MiniMax-M3")
            .with_message(ChatMessage::user("hi"))
            .with_max_tokens(1024)
            .with_reasoning_effort("high")
            .with_reasoning_split(true);
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.max_tokens, Some(1024));
        assert_eq!(req.reasoning_split, Some(true));
    }
}