//! MCP 协议消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================
// initialize
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "is_false")]
    pub tools: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub resources: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub prompts: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub logging: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: Capabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: Implementation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: Capabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: Implementation,
}

// ============================================================
// tools
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<Content>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { uri: String, text: Option<String> },
}

// ============================================================
// 简化：ClientRequest / ServerResponse enum
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ClientRequest {
    #[serde(rename = "initialize")]
    Initialize(InitializeParams),
    #[serde(rename = "tools/list")]
    ListTools {},
    #[serde(rename = "tools/call")]
    CallTool(ToolCallParams),
    #[serde(rename = "ping")]
    Ping {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum ServerResponse {
    #[serde(rename = "initialize")]
    Initialize(InitializeResult),
    #[serde(rename = "tools")]
    Tools(ListToolsResult),
    #[serde(rename = "tool_result")]
    ToolResult(ToolCallResult),
    #[serde(rename = "pong")]
    Pong {},
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_roundtrip() {
        let p = InitializeParams {
            protocol_version: "2025-06-18".into(),
            capabilities: Capabilities::default(),
            client_info: Implementation {
                name: "AgentShell".into(),
                version: "0.1.0-alpha".into(),
            },
        };
        let s = serde_json::to_string(&p).unwrap();
        let parsed: InitializeParams = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.client_info.name, "AgentShell");
        assert_eq!(parsed.protocol_version, "2025-06-18");
    }

    #[test]
    fn test_tool_call_params() {
        let p = ToolCallParams {
            name: "bash".into(),
            arguments: serde_json::json!({"cmd": "ls"}),
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"name\":\"bash\""));
        assert!(s.contains("\"cmd\":\"ls\""));
    }
}
