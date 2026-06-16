//! MCP stdio 传输
//!
//! 进程 stdin/stdout 双向通信（每条消息以 \n 分隔）

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::error::{McpError, Result};
use crate::jsonrpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// Stdio 传输
pub struct StdioTransport {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    child: Mutex<Child>,
    next_id: Mutex<u64>,
}

impl StdioTransport {
    /// 启动子进程
    pub async fn spawn(cmd: &str, args: &[&str]) -> Result<Self> {
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| McpError::Protocol(format!("spawn {}: {}", cmd, e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Protocol("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Protocol("no stdout".into()))?;

        Ok(Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            child: Mutex::new(child),
            next_id: Mutex::new(1),
        })
    }

    /// 下一个 ID
    pub async fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock().await;
        let cur = *id;
        *id += 1;
        cur
    }

    /// 发送请求
    pub async fn send_request(&self, method: &str, params: serde_json::Value) -> Result<JsonRpcResponse> {
        let id = self.next_id().await;
        let req = JsonRpcRequest::new(method, params, id);
        let line = serde_json::to_string(&req)?;
        self.send_raw(&line).await?;
        self.recv_response(id).await
    }

    /// 发送通知
    pub async fn send_notification(&self, method: &str, params: serde_json::Value) -> Result<()> {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        };
        let line = serde_json::to_string(&notif)?;
        self.send_raw(&line).await
    }

    /// 发送原始行
    async fn send_raw(&self, line: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    /// 接收响应（按 ID）
    pub async fn recv_response(&self, expected_id: u64) -> Result<JsonRpcResponse> {
        let mut stdout = self.stdout.lock().await;
        let mut line = String::new();
        loop {
            line.clear();
            let n = stdout.read_line(&mut line).await?;
            if n == 0 {
                return Err(McpError::TransportClosed);
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // 解析
            let resp: JsonRpcResponse = serde_json::from_str(line)?;
            if resp.id == expected_id {
                return Ok(resp);
            }
            // 其他 ID 跳过（理论上不应该出现）
        }
    }

    /// 关闭子进程
    pub async fn shutdown(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        child.kill().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Stdio 传输需要子进程，这里不写集成测试（在 client.rs 里有 mock）
}