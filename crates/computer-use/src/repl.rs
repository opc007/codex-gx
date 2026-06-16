//! JS REPL — Playwright JS 代码执行
//!
//! v0.1 设计：spawn `node` 子进程，通过 stdin pipe 注入脚本
//! 更完整的方案是 v0.4 用 Playwright MCP server

use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::error::{ComputerUseError, Result};

/// JS REPL
pub struct JsRepl {
    child: Child,
    /// stdin（写）
    stdin: tokio::process::ChildStdin,
    /// stdout（读）
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl JsRepl {
    /// 启动 node 子进程
    pub async fn spawn(node_path: Option<&str>) -> Result<Self> {
        let node = node_path.unwrap_or("node");
        let mut child = Command::new(node)
            .arg("-i")  // interactive
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ComputerUseError::Spawn(format!("node: {}", e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ComputerUseError::Spawn("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ComputerUseError::Spawn("no stdout".into()))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// 执行一段 JS 并返回 stdout
    pub async fn eval(&mut self, script: &str) -> Result<String> {
        // 用 console.log 包装，让 node 输出到 stdout
        let wrapped = format!(
            "console.log(JSON.stringify((async () => {{ {} }})()))",
            script.replace('\n', " ")
        );
        self.stdin
            .write_all(wrapped.as_bytes())
            .await
            .map_err(ComputerUseError::Io)?;
        self.stdin.write_all(b"\n").await.map_err(ComputerUseError::Io)?;
        self.stdin.flush().await.map_err(ComputerUseError::Io)?;

        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .await
            .map_err(ComputerUseError::Io)?;
        Ok(line.trim().to_string())
    }

    /// 关闭
    pub async fn shutdown(mut self) -> Result<()> {
        self.child.kill().await.map_err(ComputerUseError::Io)?;
        Ok(())
    }
}

/// 查找 Playwright JS 路径（项目内 / node_modules / global）
pub fn find_playwright_path() -> Option<PathBuf> {
    // TODO v0.4：检测 ~/.agentshell/playwright
    None
}

#[cfg(test)]
mod tests {
    // REPL 测试需要 node 环境；CI 中跳过
}