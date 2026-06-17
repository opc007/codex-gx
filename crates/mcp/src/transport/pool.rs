//! MCP 连接池 + 自动重连（v0.9）
//!
//! 设计：
//! - 每个 server name 持有一个 client slot
//! - 健康检查失败 / 调用失败时自动重连（指数退避）
//! - 支持 stdio + http 两种 backend

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::client::McpClient;
use crate::error::{McpError, Result};
use crate::message::{Implementation, Tool};
use crate::transport::http::{HttpEndpoint, HttpTransport};
use crate::transport::stdio::StdioTransport;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportConfig {
    Stdio {
        cmd: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        auth_token: Option<String>,
    },
}

impl TransportConfig {
    pub fn from_json(s: &str) -> Result<Self> {
        Ok(serde_json::from_str(s)?)
    }
}

/// 后端抽象（stdio / http 共用，Arc 共享）
#[derive(Clone)]
pub enum Backend {
    Stdio(Arc<StdioTransport>),
    Http(Arc<HttpTransport>),
}

impl Backend {
    pub async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<crate::jsonrpc::JsonRpcResponse> {
        match self {
            Backend::Stdio(t) => t.send_request(method, params).await,
            Backend::Http(t) => t.send_request(method, params).await,
        }
    }
    pub async fn send_notification(&self, method: &str, params: serde_json::Value) -> Result<()> {
        match self {
            Backend::Stdio(t) => t.send_notification(method, params).await,
            Backend::Http(t) => t.send_notification(method, params).await,
        }
    }
}

/// Server 池条目（含 client、tools、配置、reconnect 计数）
struct PoolEntry {
    config: TransportConfig,
    backend: Arc<Mutex<Option<Backend>>>,
    tools: Vec<Tool>,
    server_info: Option<Implementation>,
    failed_calls: u32,
    max_retry: u32,
}

const MAX_FAILURES_BEFORE_RECONNECT: u32 = 3;
const MAX_TOTAL_RETRY: u32 = 5;
const BASE_BACKOFF_MS: u64 = 200;

impl PoolEntry {
    async fn connect(config: &TransportConfig) -> Result<Backend> {
        match config {
            TransportConfig::Stdio { cmd, args } => {
                let argv: Vec<&str> = args.iter().map(String::as_str).collect();
                let t = StdioTransport::spawn(cmd, &argv).await?;
                Ok(Backend::Stdio(Arc::new(t)))
            }
            TransportConfig::Http { url, auth_token } => {
                let mut ep = HttpEndpoint::new(url.clone());
                ep.auth_token = auth_token.clone();
                Ok(Backend::Http(Arc::new(HttpTransport::new(ep))))
            }
        }
    }

    async fn ensure_alive(&mut self) -> Result<()> {
        // 第一次未连接，连接 + 握手
        {
            let guard = self.backend.lock().await;
            if guard.is_none() && self.max_retry < MAX_TOTAL_RETRY {
                drop(guard);
                let backend = Self::connect(&self.config).await?;
                let mut g = self.backend.lock().await;
                *g = Some(backend);
                // 握手（mcp initialize）
                let init_params = serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "AgentShell", "version": env!("CARGO_PKG_VERSION")},
                });
                let backend = g.as_ref().expect("just set");
                let resp = backend.send_request("initialize", init_params).await?;
                if let Some(err) = resp.error {
                    return Err(McpError::JsonRpc(err.code, err.message));
                }
                if let Some(result) = resp.result {
                    if let Ok(info) = serde_json::from_value::<Implementation>(
                        result.get("serverInfo").cloned().unwrap_or_default(),
                    ) {
                        self.server_info = Some(info);
                    }
                    backend
                        .send_notification("notifications/initialized", serde_json::json!({}))
                        .await?;
                }
                // list_tools
                let resp = backend
                    .send_request("tools/list", serde_json::json!({}))
                    .await?;
                if let Some(err) = resp.error {
                    return Err(McpError::JsonRpc(err.code, err.message));
                }
                if let Some(result) = resp.result {
                    #[derive(serde::Deserialize)]
                    struct ListResult {
                        tools: Vec<Tool>,
                    }
                    if let Ok(lr) = serde_json::from_value::<ListResult>(result) {
                        self.tools = lr.tools;
                    }
                }
                info!(tools = self.tools.len(), "MCP pool entry connected");
                self.max_retry += 1;
            }
        }
        Ok(())
    }

    async fn reconnect(&mut self) -> Result<()> {
        if self.max_retry >= MAX_TOTAL_RETRY {
            return Err(McpError::Transport(format!(
                "max retry ({}) reached",
                MAX_TOTAL_RETRY
            )));
        }
        self.max_retry += 1;
        let delay = BASE_BACKOFF_MS * 2u64.pow(self.max_retry.min(6));
        warn!("MCP reconnect in {}ms (attempt {})", delay, self.max_retry);
        tokio::time::sleep(Duration::from_millis(delay)).await;
        // 关掉旧
        {
            let mut g = self.backend.lock().await;
            *g = None;
        }
        self.ensure_alive().await
    }
}

/// 全局池
pub struct McpPool {
    entries: Arc<Mutex<HashMap<String, PoolEntry>>>,
}

impl McpPool {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 注册一个 server 配置
    pub async fn register(&self, name: impl Into<String>, config: TransportConfig) -> Result<()> {
        let name = name.into();
        let mut g = self.entries.lock().await;
        g.entry(name.clone()).or_insert_with(|| PoolEntry {
            config,
            backend: Arc::new(Mutex::new(None)),
            tools: Vec::new(),
            server_info: None,
            failed_calls: 0,
            max_retry: 0,
        });
        Ok(())
    }

    /// 列出已注册 server
    pub async fn list_servers(&self) -> Vec<String> {
        self.entries.lock().await.keys().cloned().collect()
    }

    /// 触发所有 server 连接
    pub async fn connect_all(&self) -> Result<()> {
        let names: Vec<String> = {
            let g = self.entries.lock().await;
            g.keys().cloned().collect()
        };
        for n in names {
            let mut g = self.entries.lock().await;
            if let Some(entry) = g.get_mut(&n) {
                if let Err(e) = entry.ensure_alive().await {
                    warn!("connect {} failed: {}", n, e);
                }
            }
        }
        Ok(())
    }

    /// 列出某个 server 的工具
    pub async fn tools_of(&self, server: &str) -> Option<Vec<Tool>> {
        let g = self.entries.lock().await;
        g.get(server).map(|e| e.tools.clone())
    }

    /// 列出所有 server + 工具
    pub async fn list_all_tools(&self) -> Vec<(String, Tool)> {
        let g = self.entries.lock().await;
        let mut out = Vec::new();
        for (name, e) in g.iter() {
            for t in &e.tools {
                out.push((name.clone(), t.clone()));
            }
        }
        out
    }

    /// 调用某个 server 的工具（自动重连）
    pub async fn call(
        &self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<crate::message::ToolCallResult> {
        let mut g = self.entries.lock().await;
        let entry = g
            .get_mut(server)
            .ok_or_else(|| McpError::Protocol(format!("server `{}` not found", server)))?;

        // 尝试调用
        for attempt in 0..2 {
            entry.ensure_alive().await?;
            let backend_opt = entry.backend.lock().await.clone();
            if let Some(backend) = backend_opt {
                let params = serde_json::json!({"name": tool, "arguments": arguments});
                match backend.send_request("tools/call", params).await {
                    Ok(resp) => {
                        if let Some(err) = resp.error {
                            return Err(McpError::JsonRpc(err.code, err.message));
                        }
                        if let Some(result) = resp.result {
                            let parsed: crate::message::ToolCallResult =
                                serde_json::from_value(result)?;
                            entry.failed_calls = 0;
                            return Ok(parsed);
                        }
                        return Err(McpError::Protocol("no result".into()));
                    }
                    Err(e) => {
                        warn!("MCP call failed (attempt {}): {}", attempt, e);
                        entry.failed_calls += 1;
                        if entry.failed_calls >= MAX_FAILURES_BEFORE_RECONNECT {
                            if let Err(rec_e) = entry.reconnect().await {
                                return Err(rec_e);
                            }
                            entry.failed_calls = 0;
                        }
                        if attempt == 1 {
                            return Err(e);
                        }
                    }
                }
            } else {
                return Err(McpError::Transport("no backend".into()));
            }
        }
        Err(McpError::Protocol("unreachable".into()))
    }
}

impl Default for McpPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_list() {
        let pool = McpPool::new();
        pool.register(
            "test",
            TransportConfig::Stdio {
                cmd: "echo".into(),
                args: vec!["hi".into()],
            },
        )
        .await
        .unwrap();
        let servers = pool.list_servers().await;
        assert!(servers.contains(&"test".to_string()));
    }

    #[test]
    fn test_config_serde() {
        let json = r#"{"kind":"http","url":"https://x.com/mcp","auth_token":"t"}"#;
        let cfg: TransportConfig = serde_json::from_str(json).unwrap();
        match cfg {
            TransportConfig::Http { url, auth_token } => {
                assert_eq!(url, "https://x.com/mcp");
                assert_eq!(auth_token.as_deref(), Some("t"));
            }
            _ => panic!("wrong variant"),
        }
    }
}
