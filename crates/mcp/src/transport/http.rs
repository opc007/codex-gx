//! MCP streamable HTTP transport（v0.9）
//!
//! 协议：MCP 2025-06-18 streamable HTTP
//! - POST <endpoint> with JSON-RPC body
//!   - Accept: application/json, text/event-stream
//!   - 响应：可能是 JSON 或 SSE 流
//! - 简单版：只做 request/response（不支持 server-to-client notification over HTTP）

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{McpError, Result};
use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

/// HTTP 端点配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpEndpoint {
    pub url: String,
    /// 可选 Bearer token
    #[serde(default)]
    pub auth_token: Option<String>,
    /// 请求超时（毫秒）
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    30_000
}

impl HttpEndpoint {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            auth_token: None,
            timeout_ms: default_timeout(),
        }
    }
}

/// HTTP 传输
pub struct HttpTransport {
    endpoint: HttpEndpoint,
    client: reqwest::Client,
    next_id: AtomicU64,
}

impl HttpTransport {
    pub fn new(endpoint: HttpEndpoint) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(endpoint.timeout_ms))
            .build()
            .unwrap_or_default();
        Self {
            endpoint,
            client,
            next_id: AtomicU64::new(1),
        }
    }

    pub async fn send_request(&self, method: &str, params: Value) -> Result<JsonRpcResponse> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(method, params, id);
        let mut rb = self.client.post(&self.endpoint.url).json(&req);
        if let Some(tok) = &self.endpoint.auth_token {
            rb = rb.bearer_auth(tok);
        }
        let resp = rb.send().await.map_err(McpError::Http)?;
        if !resp.status().is_success() {
            return Err(McpError::Protocol(format!(
                "HTTP {} from {}",
                resp.status(),
                self.endpoint.url
            )));
        }
        // 解析：先看 content-type
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = resp.text().await.map_err(McpError::Http)?;
        if ct.contains("text/event-stream") {
            // 简化：从 SSE 流中提取第一个 data: 行
            for line in body.lines() {
                if let Some(rest) = line.strip_prefix("data:") {
                    let rest = rest.trim();
                    if rest.is_empty() {
                        continue;
                    }
                    return serde_json::from_str(rest).map_err(McpError::Json);
                }
            }
            Err(McpError::Protocol("SSE response has no data line".into()))
        } else {
            serde_json::from_str(&body).map_err(McpError::Json)
        }
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut rb = self.client.post(&self.endpoint.url).json(&req);
        if let Some(tok) = &self.endpoint.auth_token {
            rb = rb.bearer_auth(tok);
        }
        // notifications 期望 202 Accepted
        let _ = rb.send().await.map_err(McpError::Http)?;
        Ok(())
    }

    pub async fn shutdown(self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_new() {
        let ep = HttpEndpoint::new("https://example.com/mcp");
        assert_eq!(ep.url, "https://example.com/mcp");
        assert_eq!(ep.timeout_ms, 30_000);
    }

    #[test]
    fn test_endpoint_serde() {
        let ep = HttpEndpoint {
            url: "https://x.com".into(),
            auth_token: Some("tok".into()),
            timeout_ms: 5000,
        };
        let s = serde_json::to_string(&ep).unwrap();
        let back: HttpEndpoint = serde_json::from_str(&s).unwrap();
        assert_eq!(back.url, "https://x.com");
        assert_eq!(back.auth_token.as_deref(), Some("tok"));
        assert_eq!(back.timeout_ms, 5000);
    }
}
