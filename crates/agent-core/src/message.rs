//! 消息 + 内容块
//!
//! 设计参考：docs/开发文档.md §5.1 消息类型 / §5.4 Reasoning

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 消息角色
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// 系统提示
    System,
    /// 用户
    User,
    /// 助手
    Assistant,
    /// 工具
    Tool,
}

impl MessageRole {
    /// OpenAI Chat Completions 协议对应的 role 字符串
    pub fn as_openai_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// 内容块（多模态：文本 / 图像 / 工具调用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// 纯文本
    Text {
        /// 文本内容
        text: String,
    },
    /// 图像 URL 或 base64
    Image {
        /// 来源（"url" 或 "base64"）
        source: String,
        /// 数据
        data: String,
        /// MIME 类型
        #[serde(default)]
        mime_type: Option<String>,
    },
    /// 工具调用（assistant → tool）
    ToolUse(ToolCall),
    /// 工具结果（tool → assistant）
    ToolResult(ToolResult),
    /// 推理内容（interleaved thinking）
    Reasoning {
        /// 思考文本
        text: String,
        /// 推理特征（split / chain / 等等）
        #[serde(default)]
        signature: Option<String>,
    },
}

/// 工具调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用 ID（用于关联 ToolResult）
    pub id: String,
    /// 工具名
    pub name: String,
    /// 参数 JSON 字符串
    pub arguments: String,
}

impl ToolCall {
    /// 解析参数为 JSON Value
    pub fn parsed_args(&self) -> serde_json::Value {
        serde_json::from_str(&self.arguments).unwrap_or(serde_json::json!({}))
    }
}

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// 对应的 ToolCall.id
    pub call_id: String,
    /// 是否成功
    pub success: bool,
    /// 结果文本
    pub content: String,
    /// 错误信息（失败时）
    #[serde(default)]
    pub error: Option<String>,
}

/// 单条消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 唯一 ID
    pub id: Uuid,
    /// 角色
    pub role: MessageRole,
    /// 内容块列表
    pub content: Vec<ContentBlock>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 关联的 session
    pub session_id: Uuid,
}

impl Message {
    /// 创建用户消息
    pub fn user(session_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: text.into() }],
            created_at: Utc::now(),
            session_id,
        }
    }

    /// 创建助手消息
    pub fn assistant(session_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
            created_at: Utc::now(),
            session_id,
        }
    }

    /// 创建系统消息
    pub fn system(session_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::System,
            content: vec![ContentBlock::Text { text: text.into() }],
            created_at: Utc::now(),
            session_id,
        }
    }

    /// 拼接所有文本块
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_openai() {
        assert_eq!(MessageRole::System.as_openai_str(), "system");
        assert_eq!(MessageRole::User.as_openai_str(), "user");
        assert_eq!(MessageRole::Assistant.as_openai_str(), "assistant");
        assert_eq!(MessageRole::Tool.as_openai_str(), "tool");
    }

    #[test]
    fn test_message_text() {
        let sid = Uuid::new_v4();
        let m = Message::user(sid, "hello");
        assert_eq!(m.role, MessageRole::User);
        assert_eq!(m.text_content(), "hello");
    }

    #[test]
    fn test_tool_call_parsed_args() {
        let tc = ToolCall {
            id: "1".into(),
            name: "bash".into(),
            arguments: r#"{"cmd":"ls"}"#.into(),
        };
        let v = tc.parsed_args();
        assert_eq!(v["cmd"], "ls");
    }
}
