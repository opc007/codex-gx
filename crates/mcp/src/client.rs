//! MCP 客户端

use crate::error::Result;
use crate::message::{
    Capabilities, Implementation, InitializeParams, InitializeResult, ListToolsResult,
    Tool, ToolCallParams, ToolCallResult,
};
use crate::transport::stdio::StdioTransport;

/// MCP 客户端
pub struct McpClient {
    transport: StdioTransport,
    server_info: Option<Implementation>,
    server_caps: Option<Capabilities>,
}

impl McpClient {
    /// 启动一个 MCP server 子进程
    pub async fn spawn(cmd: &str, args: &[&str]) -> Result<Self> {
        let transport = StdioTransport::spawn(cmd, args).await?;
        Ok(Self {
            transport,
            server_info: None,
            server_caps: None,
        })
    }

    /// initialize 握手
    pub async fn initialize(
        &mut self,
        client_info: Implementation,
        capabilities: Capabilities,
    ) -> Result<InitializeResult> {
        let params = InitializeParams {
            protocol_version: "2025-06-18".into(),
            capabilities,
            client_info,
        };
        let resp = self
            .transport
            .send_request("initialize", serde_json::to_value(&params)?)
            .await?;
        if let Some(err) = resp.error {
            return Err(crate::error::McpError::JsonRpc(err.code, err.message));
        }
        let result: InitializeResult = serde_json::from_value(
            resp.result.ok_or_else(|| crate::error::McpError::Protocol("no result".into()))?,
        )?;
        self.server_info = Some(result.server_info.clone());
        self.server_caps = Some(result.capabilities.clone());
        // send initialized notification
        self.transport
            .send_notification("notifications/initialized", serde_json::json!({}))
            .await?;
        Ok(result)
    }

    /// 列出 server 的工具
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let resp = self
            .transport
            .send_request("tools/list", serde_json::json!({}))
            .await?;
        if let Some(err) = resp.error {
            return Err(crate::error::McpError::JsonRpc(err.code, err.message));
        }
        let result: ListToolsResult = serde_json::from_value(
            resp.result.ok_or_else(|| crate::error::McpError::Protocol("no result".into()))?,
        )?;
        Ok(result.tools)
    }

    /// 调用一个工具
    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<ToolCallResult> {
        let params = ToolCallParams {
            name: name.into(),
            arguments,
        };
        let resp = self
            .transport
            .send_request("tools/call", serde_json::to_value(&params)?)
            .await?;
        if let Some(err) = resp.error {
            return Err(crate::error::McpError::JsonRpc(err.code, err.message));
        }
        let result: ToolCallResult = serde_json::from_value(
            resp.result.ok_or_else(|| crate::error::McpError::Protocol("no result".into()))?,
        )?;
        Ok(result)
    }

    /// ping
    pub async fn ping(&self) -> Result<()> {
        let resp = self
            .transport
            .send_request("ping", serde_json::json!({}))
            .await?;
        if let Some(err) = resp.error {
            return Err(crate::error::McpError::JsonRpc(err.code, err.message));
        }
        Ok(())
    }

    /// 关闭
    pub async fn shutdown(self) -> Result<()> {
        self.transport.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    // 集成测试需要真的 MCP server（不在这里写）
    use super::*;
    use crate::message::Implementation;

    #[test]
    fn test_client_struct_construction() {
        // 仅验证结构字段可访问
        let _info = Implementation {
            name: "AgentShell".into(),
            version: "0.1.0".into(),
        };
    }
}