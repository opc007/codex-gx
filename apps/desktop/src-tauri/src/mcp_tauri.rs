//! v1.9.8: MCP Tauri 命令 (使用现有 crates/mcp API)

use mcp::{
    ClientRequest, HttpEndpoint, HttpTransport, McpClient, McpPool, ServerResponse, StdioTransport,
    Tool, ToolCallParams, ToolCallResult, Implementation, Capabilities,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// 服务器配置（演示用 — 真实 server 走 stdio/HTTP transport）
pub type McpConfigState = Arc<Mutex<McpConfig>>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    /// 已注册的后端（演示用）
    pub backends: Vec<BackendEntry>,
    /// 全局 capabilities
    pub capabilities: HashMap<String, serde_json::Value>,
    /// 当前 session id
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendEntry {
    pub name: String,
    pub kind: String, // stdio / http
    pub endpoint: String,
    pub enabled: bool,
}

pub fn build_config() -> McpConfigState {
    Arc::new(Mutex::new(McpConfig::load()))
}

impl McpConfig {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            Self::default()
        }
    }
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(home).join(".agentshell").join("mcp-config.json")
}

#[tauri::command]
pub fn mcp_protocol_info() -> serde_json::Value {
    serde_json::json!({
        "name": "agentshell-mcp",
        "version": "v1.9.8",
        "implementation": Implementation {
            name: "agentshell-mcp".into(),
            version: "v1.9.8".into(),
        },
        "capabilities": Capabilities::default(),
        "transports": ["stdio", "http"],
        "methods": ["initialize", "tools/list", "tools/call", "resources/list", "prompts/list"]
    })
}

#[tauri::command]
pub fn mcp_config_path() -> String {
    config_path().display().to_string()
}

#[tauri::command]
pub async fn mcp_get_config(state: tauri::State<'_, McpConfigState>) -> Result<McpConfig, String> {
    let c = state.lock().map_err(|e| e.to_string())?;
    Ok(c.clone())
}

#[derive(Deserialize)]
pub struct AddBackendArgs {
    pub name: String,
    pub kind: String,
    pub endpoint: String,
}

#[tauri::command]
pub async fn mcp_add_backend(
    args: AddBackendArgs,
    state: tauri::State<'_, McpConfigState>,
) -> Result<BackendEntry, String> {
    let entry = BackendEntry {
        name: args.name,
        kind: args.kind,
        endpoint: args.endpoint,
        enabled: true,
    };
    let mut c = state.lock().map_err(|e| e.to_string())?;
    c.backends.retain(|b| b.name != entry.name);
    c.backends.push(entry.clone());
    c.save()?;
    Ok(entry)
}

#[tauri::command]
pub async fn mcp_remove_backend(
    name: String,
    state: tauri::State<'_, McpConfigState>,
) -> Result<bool, String> {
    let mut c = state.lock().map_err(|e| e.to_string())?;
    let before = c.backends.len();
    c.backends.retain(|b| b.name != name);
    let removed = c.backends.len() != before;
    c.save()?;
    Ok(removed)
}

#[tauri::command]
pub async fn mcp_list_backends(state: tauri::State<'_, McpConfigState>) -> Result<Vec<BackendEntry>, String> {
    let c = state.lock().map_err(|e| e.to_string())?;
    Ok(c.backends.clone())
}

/// 演示用：HTTP 调用外部 MCP server
#[derive(Deserialize)]
pub struct McpHttpCallArgs {
    pub url: String,
    pub auth_token: Option<String>,
    pub method: String,
    pub params: serde_json::Value,
}

#[tauri::command]
pub async fn mcp_http_call(args: McpHttpCallArgs) -> Result<serde_json::Value, String> {
    let endpoint = HttpEndpoint {
        url: args.url,
        auth_token: args.auth_token,
        timeout_ms: 30_000,
    };
    let transport = HttpTransport::new(endpoint);
    let resp = transport
        .send_request(&args.method, args.params)
        .await
        .map_err(|e| format!("MCP call failed: {}", e))?;
    serde_json::to_value(&resp).map_err(|e| e.to_string())
}

/// Mock 客户端：列出内置 tools
#[tauri::command]
pub fn mcp_builtin_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "echo".into(),
            description: "Echo back the input text".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
        },
        Tool {
            name: "now".into(),
            description: "Current UTC timestamp".into(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "uuid".into(),
            description: "Generate a new UUID".into(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "add".into(),
            description: "Add two numbers".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" },
                },
                "required": ["a", "b"]
            }),
        },
    ]
}