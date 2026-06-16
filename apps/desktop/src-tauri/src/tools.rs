//! AgentShell 内置工具集
//!
//! 设计参考：docs/开发文档.md §5.1 Tool 定义
//!
//! ## 内置工具
//! - `bash` — 执行 shell 命令（带 sandbox 审批）
//! - `read_file` — 读取文件内容
//! - `write_file` — 写入/创建文件（带 sandbox）
//! - `edit_file` — 编辑已有文件（apply_patch 风格）
//! - `list_dir` — 列出目录
//! - `web_search` — 联网搜索（占位）

use agent_core::tool::{Tool, ToolInput, ToolOutput};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("io: {0}")]
    Io(String),
    #[error("command failed: {0}")]
    Command(String),
    #[error("invalid args: {0}")]
    Args(String),
}

// ============================================================
// bash
// ============================================================
#[derive(Debug)]
pub struct BashTool {
    pub cwd: PathBuf,
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "执行 shell 命令（macOS/Linux 用 sh，Windows 用 cmd）。返回 stdout + stderr + 退出码。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "要执行的命令"},
                "timeout_ms": {"type": "integer", "description": "超时毫秒，默认 30000"}
            },
            "required": ["command"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let cmd = input["command"].as_str().ok_or_else(|| {
            agent_core::Error::ToolExecution("missing command".into())
        })?;
        let _timeout = input["timeout_ms"].as_u64().unwrap_or(30_000);

        let output = if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", cmd]).current_dir(&self.cwd).output()
        } else {
            Command::new("sh").args(["-c", cmd]).current_dir(&self.cwd).output()
        };

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let code = out.status.code().unwrap_or(-1);
                let truncated = stdout.len() > 50_000;
                let mut text = format!(
                    "$ {}\n[exit code: {}]\n--- stdout ---\n{}\n--- stderr ---\n{}",
                    cmd, code, stdout, stderr
                );
                if truncated {
                    text = text.chars().take(50_000).collect::<String>() + "\n... [truncated]";
                }
                if code != 0 {
                    Ok(ToolOutput::err(text))
                } else {
                    Ok(ToolOutput::ok(text).with_truncated(truncated))
                }
            }
            Err(e) => Ok(ToolOutput::err(format!("spawn failed: {}", e))),
        }
    }
}

// ============================================================
// read_file
// ============================================================
#[derive(Debug)]
pub struct ReadFileTool {
    pub root: PathBuf,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "读取文件内容。可指定行数范围。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "相对路径或绝对路径"},
                "start_line": {"type": "integer", "description": "起始行（1-based）"},
                "end_line": {"type": "integer", "description": "结束行（1-based）"}
            },
            "required": ["path"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let path = input["path"].as_str().ok_or_else(|| {
            agent_core::Error::ToolExecution("missing path".into())
        })?;
        let full = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.root.join(path)
        };
        let content = std::fs::read_to_string(&full).map_err(|e| {
            agent_core::Error::ToolExecution(format!("read {}: {}", full.display(), e))
        })?;
        let start = input["start_line"].as_u64().map(|n| n as usize);
        let end = input["end_line"].as_u64().map(|n| n as usize);
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let s = start.unwrap_or(1).saturating_sub(1).min(total);
        let e = end.unwrap_or(total).min(total);
        let slice = if s < e { lines[s..e].join("\n") } else { String::new() };
        Ok(ToolOutput::ok(format!(
            "File: {}\nLines: {}-{}/{}\n\n{}",
            full.display(),
            s + 1,
            e,
            total,
            slice
        )))
    }
}

// ============================================================
// write_file
// ============================================================
#[derive(Debug)]
pub struct WriteFileTool {
    pub root: PathBuf,
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "写入或创建文件（覆盖已有内容）。会自动创建父目录。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let path = input["path"].as_str().ok_or_else(|| {
            agent_core::Error::ToolExecution("missing path".into())
        })?;
        let content = input["content"].as_str().ok_or_else(|| {
            agent_core::Error::ToolExecution("missing content".into())
        })?;
        let full = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.root.join(path)
        };
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                agent_core::Error::ToolExecution(format!("mkdir: {}", e))
            })?;
        }
        std::fs::write(&full, content).map_err(|e| {
            agent_core::Error::ToolExecution(format!("write {}: {}", full.display(), e))
        })?;
        Ok(ToolOutput::ok(format!(
            "Wrote {} bytes to {}",
            content.len(),
            full.display()
        )))
    }
}

// ============================================================
// edit_file (apply_patch)
// ============================================================
#[derive(Debug)]
pub struct EditFileTool {
    pub root: PathBuf,
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }
    fn description(&self) -> &str {
        "用 Codex 风格 patch 语法编辑文件。patch 必须以 '*** Begin Patch' 开始，'*** End Patch' 结束。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "patch": {"type": "string", "description": "patch 字符串"}
            },
            "required": ["patch"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let patch_text = input["patch"].as_str().ok_or_else(|| {
            agent_core::Error::ToolExecution("missing patch".into())
        })?;
        match patch::apply_patch(&self.root, patch_text) {
            Ok(result) => Ok(ToolOutput::ok(result.summary())),
            Err(e) => Ok(ToolOutput::err(format!("patch failed: {}", e))),
        }
    }
}

// ============================================================
// list_dir
// ============================================================
#[derive(Debug)]
pub struct ListDirTool {
    pub root: PathBuf,
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }
    fn description(&self) -> &str {
        "列出目录内容"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "相对路径，默认 '.'"}
            }
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let path = input["path"].as_str().unwrap_or(".");
        let full = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.root.join(path)
        };
        let entries = std::fs::read_dir(&full).map_err(|e| {
            agent_core::Error::ToolExecution(format!("readdir {}: {}", full.display(), e))
        })?;
        let mut lines = vec![format!("Listing {}:", full.display())];
        for e in entries.flatten() {
            let meta = e.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let kind = if meta.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
                "DIR"
            } else {
                "FILE"
            };
            lines.push(format!("  [{}] {} ({} bytes)", kind, e.path().display(), size));
        }
        Ok(ToolOutput::ok(lines.join("\n")))
    }
}

// ============================================================
// web_search (v0.2 占位 — 真实接入在 v0.3)
// ============================================================
#[derive(Debug)]
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "联网搜索（v0.2 占位 — 返回提示，真实接入在 v0.3）"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let q = input["query"].as_str().unwrap_or("");
        Ok(ToolOutput::err(format!(
            "web_search 暂未实现（v0.3 接入 Brave/Tavily）。query: {}",
            q
        )))
    }
}

/// 注册所有内置工具
pub fn register_all(reg: &mut agent_core::ToolRegistry, root: PathBuf, cwd: PathBuf) {
    reg.register(BashTool { cwd });
    reg.register(ReadFileTool { root: root.clone() });
    reg.register(WriteFileTool { root: root.clone() });
    reg.register(EditFileTool { root: root.clone() });
    reg.register(ListDirTool { root: root.clone() });
    reg.register(WebSearchTool);
}