//! MCP 集成 — v0.9 用 McpPool + 自动重连 + streamable HTTP
//!
//! 配置 ~/.agentshell/mcp.json 两种格式：
//! 老式（兼容）：
//! ```json
//! [
//!   {"name": "fs", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]}
//! ]
//! ```
//! 新式（stdio + http）：
//! ```json
//! {
//!   "servers": [
//!     {"name": "fs", "kind": "stdio", "cmd": "npx", "args": ["-y", "..."]},
//!     {"name": "remote", "kind": "http", "url": "https://x.com/mcp", "auth_token": "..."}
//!   ]
//! }
//! ```

use agent_core::tool::{Tool, ToolInput, ToolOutput};
use async_trait::async_trait;
use mcp::{McpPool, ToolCallResult, TransportConfig};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

/// 老式单条 server 配置
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct LegacyMcpConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// 新式配置文件
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct McpConfigFile {
    #[serde(default)]
    pub servers: Vec<NewServerEntry>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewServerEntry {
    Stdio {
        name: String,
        cmd: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Http {
        name: String,
        url: String,
        #[serde(default)]
        auth_token: Option<String>,
    },
}

impl NewServerEntry {
    pub fn name(&self) -> &str {
        match self {
            NewServerEntry::Stdio { name, .. } => name,
            NewServerEntry::Http { name, .. } => name,
        }
    }
    pub fn to_transport_config(&self) -> TransportConfig {
        match self {
            NewServerEntry::Stdio { name: _, cmd, args } => TransportConfig::Stdio {
                cmd: cmd.clone(),
                args: args.clone(),
            },
            NewServerEntry::Http {
                name: _,
                url,
                auth_token,
            } => TransportConfig::Http {
                url: url.clone(),
                auth_token: auth_token.clone(),
            },
        }
    }
}

fn mcp_config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".agentshell").join("mcp.json")
}

/// 解析配置文件（兼容老格式 + 新格式）
fn parse_config(content: &str) -> anyhow::Result<Vec<NewServerEntry>> {
    // 先试新格式 { "servers": [...] }
    if let Ok(f) = serde_json::from_str::<McpConfigFile>(content) {
        if !f.servers.is_empty() {
            return Ok(f.servers);
        }
    }
    // 回退到老格式 [...]
    if let Ok(legacy) = serde_json::from_str::<Vec<LegacyMcpConfig>>(content) {
        return Ok(legacy
            .into_iter()
            .map(|c| NewServerEntry::Stdio {
                name: c.name,
                cmd: c.command,
                args: c.args,
            })
            .collect());
    }
    Err(anyhow::anyhow!("无法解析 mcp.json 为新格式或老格式"))
}

/// 全局 MCP pool（lazy）
static MCP_POOL: OnceCell<Arc<McpPool>> = OnceCell::const_new();

pub async fn mcp_pool() -> Arc<McpPool> {
    MCP_POOL
        .get_or_init(|| async {
            let pool = McpPool::new();
            let path = mcp_config_path();
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match parse_config(&content) {
                        Ok(entries) => {
                            for e in entries {
                                let name = e.name().to_string();
                                let cfg = e.to_transport_config();
                                if let Err(err) = pool.register(&name, cfg).await {
                                    eprintln!("[MCP] register {} failed: {}", name, err);
                                }
                            }
                            if let Err(e) = pool.connect_all().await {
                                eprintln!("[MCP] connect_all: {}", e);
                            }
                        }
                        Err(e) => eprintln!("[MCP] parse mcp.json: {}", e),
                    },
                    Err(e) => eprintln!("[MCP] read mcp.json: {}", e),
                }
            } else {
                eprintln!("[MCP] no mcp.json at {}", path.display());
            }
            Arc::new(pool)
        })
        .await
        .clone()
}

/// 兼容老 API：列出 server 名字
pub async fn list_mcp_servers() -> Vec<String> {
    let pool = mcp_pool().await;
    pool.list_servers().await
}

/// 强制重连
pub async fn reload_mcp() {
    let pool = mcp_pool().await;
    if let Err(e) = pool.connect_all().await {
        eprintln!("[MCP] reload: {}", e);
    }
}

// ============================================================
// 把 pool tool 包装为 AgentShell Tool
// ============================================================
pub struct McpToolWrapper {
    pub server: String,
    pub original_name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub pool: Arc<McpPool>,
}

impl std::fmt::Debug for McpToolWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpToolWrapper")
            .field("server", &self.server)
            .field("original_name", &self.original_name)
            .field("description", &self.description)
            .finish()
    }
}

fn render_tool_result(result: &ToolCallResult) -> (bool, String) {
    let mut text = String::new();
    for c in &result.content {
        match c {
            mcp::Content::Text { text: t } => text.push_str(t),
            mcp::Content::Image { data, mime_type } => {
                text.push_str(&format!("\n[image: {} ({} bytes)]", mime_type, data.len()));
            }
            _ => text.push_str(&format!("\n[content: {:?}]", c)),
        }
    }
    (result.is_error, text)
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        Box::leak(format!("mcp__{}__{}", self.server, self.original_name).into_boxed_str())
    }
    fn description(&self) -> &str {
        Box::leak(format!("[MCP:{}] {}", self.server, self.description).into_boxed_str())
    }
    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters.clone()
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let server = self.server.clone();
        let tool = self.original_name.clone();
        let pool = self.pool.clone();
        let result = pool
            .call(&server, &tool, input)
            .await
            .map_err(|e| agent_core::Error::ToolExecution(format!("mcp: {}", e)))?;
        let (is_err, text) = render_tool_result(&result);
        if is_err {
            Ok(ToolOutput::err(text))
        } else {
            Ok(ToolOutput::ok(text))
        }
    }
}

/// 把 MCP 工具注册到 ToolRegistry
pub async fn register_mcp_tools(reg: &mut agent_core::ToolRegistry) {
    let pool = mcp_pool().await;
    let all = pool.list_all_tools().await;
    let count = all.len();
    for (server_name, tool) in all {
        let wrapper = McpToolWrapper {
            server: server_name,
            original_name: tool.name,
            description: tool.description,
            parameters: tool.input_schema,
            pool: pool.clone(),
        };
        reg.register(wrapper);
    }
    eprintln!("[MCP] 已注册 {} 个工具", count);
}
