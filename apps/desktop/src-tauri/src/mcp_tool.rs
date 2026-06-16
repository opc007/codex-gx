//! MCP 集成 — 把外部 MCP server 的工具暴露给 Agent
//!
//! 用法：
//! 1. 在 ~/.agentshell/mcp.json 写 server 配置
//! 2. 启动时连接所有 server
//! 3. 注册每个 server 的 tool 到 ToolRegistry
//! 4. Agent 调用时转发到对应 server

use agent_core::tool::{Tool, ToolInput, ToolOutput};
use async_trait::async_trait;
use mcp::{Capabilities, Implementation, McpClient};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::OnceCell;

/// 单个 MCP server 配置
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// 全局 MCP manager
pub struct McpManager {
    /// name -> client
    servers: HashMap<String, Arc<Mutex<McpClient>>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self { servers: HashMap::new() }
    }

    /// 从 ~/.agentshell/mcp.json 加载配置并启动所有 server
    pub async fn load_from_config() -> anyhow::Result<Self> {
        let path = mcp_config_path();
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let configs: Vec<McpServerConfig> = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("parse mcp.json: {}", e))?;
        let mut mgr = Self::new();
        for cfg in configs {
            match mgr.start_server(&cfg).await {
                Ok(_) => println!("[MCP] started server: {}", cfg.name),
                Err(e) => eprintln!("[MCP] failed to start {}: {}", cfg.name, e),
            }
        }
        Ok(mgr)
    }

    /// 启动一个 server
    pub async fn start_server(&mut self, cfg: &McpServerConfig) -> anyhow::Result<()> {
        let cmd = cfg.command.clone();
        let args: Vec<&str> = cfg.args.iter().map(|s| s.as_str()).collect();
        let mut client = McpClient::spawn(&cmd, &args).await?;
        client
            .initialize(
                Implementation {
                    name: "AgentShell".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                },
                Capabilities::default(),
            )
            .await?;
        self.servers.insert(cfg.name.clone(), Arc::new(Mutex::new(client)));
        Ok(())
    }

    /// 列出所有 server
    pub fn server_names(&self) -> Vec<String> {
        self.servers.keys().cloned().collect()
    }

    /// 取 client
    pub fn get(&self, name: &str) -> Option<Arc<Mutex<McpClient>>> {
        self.servers.get(name).cloned()
    }

    /// 列出所有 server 的所有工具
    pub async fn list_all_tools(&self) -> Vec<(String, mcp::Tool)> {
        let mut out = Vec::new();
        for (name, client) in &self.servers {
            let c = client.lock().await;
            if let Ok(tools) = c.list_tools().await {
                for t in tools {
                    out.push((name.clone(), t));
                }
            }
        }
        out
    }
}

impl Default for McpManager {
    fn default() -> Self { Self::new() }
}

fn mcp_config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".agentshell").join("mcp.json")
}

/// 全局 MCP manager（lazy）
static MCP_MANAGER: OnceCell<Arc<Mutex<McpManager>>> = OnceCell::const_new();

pub async fn mcp_manager() -> Arc<Mutex<McpManager>> {
    let arc = MCP_MANAGER
        .get_or_init(|| async {
            let mgr = McpManager::load_from_config()
                .await
                .unwrap_or_else(|e| {
                    eprintln!("[MCP] load config failed: {}", e);
                    McpManager::new()
                });
            Arc::new(Mutex::new(mgr))
        })
        .await;
    arc.clone()
}

// ============================================================
// 包装 MCP tool 为 AgentShell Tool
// ============================================================
pub struct McpToolWrapper {
    pub server: String,
    pub original_name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub client: Arc<Mutex<McpClient>>,
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

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        // 命名加前缀避免冲突
        // 用 Box::leak 维持 'static 生命周期（简化）
        Box::leak(format!("mcp__{}__{}", self.server, self.original_name).into_boxed_str())
    }
    fn description(&self) -> &str {
        Box::leak(
            format!("[MCP:{}] {}", self.server, self.description).into_boxed_str(),
        )
    }
    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters.clone()
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let client = self.client.clone();
        let name = self.original_name.clone();
        let args = input;
        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move {
                let c = client.lock().await;
                c.call_tool(&name, args).await
            })
        })
        .await
        .map_err(|e| agent_core::Error::ToolExecution(format!("join: {}", e)))?
        .map_err(|e| agent_core::Error::ToolExecution(format!("mcp: {}", e)))?;

        // 拼接所有 content
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
        if result.is_error {
            Ok(ToolOutput::err(text))
        } else {
            Ok(ToolOutput::ok(text))
        }
    }
}

/// 把 MCP 工具注册到 ToolRegistry
pub async fn register_mcp_tools(reg: &mut agent_core::ToolRegistry) {
    let mgr = mcp_manager().await;
    let mgr_lock = mgr.lock().await;
    let tools = mgr_lock.list_all_tools().await;
    for (server_name, tool) in tools {
        if let Some(client) = mgr_lock.get(&server_name) {
            let wrapper = McpToolWrapper {
                server: server_name,
                original_name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
                client,
            };
            reg.register(wrapper);
        }
    }
}