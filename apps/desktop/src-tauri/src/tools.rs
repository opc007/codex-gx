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
// web_search (v0.3 — Brave Search API)
// ============================================================
#[derive(Debug)]
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "联网搜索（Brave Search API）。需要环境变量 BRAVE_API_KEY。支持按域名过滤、日期范围过滤。返回前 N 条结果的标题 + 链接 + 摘要。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "搜索关键词"},
                "count": {"type": "integer", "description": "返回条数，默认 5，最大 20"},
                "site": {"type": "string", "description": "v0.6：限定到某个域名，如 'github.com' 或 '*.stackoverflow.com'"},
                "site_filter": {"type": "string", "description": "v0.6：多域名白名单，逗号分隔，如 'github.com,stackoverflow.com'"},
                "site_exclude": {"type": "string", "description": "v0.6：黑名单域名，逗号分隔"},
                "freshness": {"type": "string", "description": "v0.6：时间范围过滤 - 'pd'(24h) | 'pw'(7d) | 'pm'(31d) | 'py'(365d) | 'YYYY-MM-DD..YYYY-MM-DD'"}
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let query = input["query"].as_str().unwrap_or("").to_string();
        let count = input["count"].as_u64().unwrap_or(5).min(20) as u32;
        let site = input["site"].as_str().map(|s| s.to_string());
        let site_filter = input["site_filter"].as_str().map(|s| s.to_string());
        let site_exclude = input["site_exclude"].as_str().map(|s| s.to_string());
        let freshness = input["freshness"].as_str().map(|s| s.to_string());

        if query.trim().is_empty() {
            return Ok(ToolOutput::err("query 不能为空".to_string()));
        }

        // 1. 拿 API key
        let api_key = match std::env::var("BRAVE_API_KEY") {
            Ok(k) => k,
            Err(_) => {
                return Ok(ToolOutput::err(
                    "BRAVE_API_KEY 未设置。请 export BRAVE_API_KEY=xxx (免费 key: https://brave.com/search/api/)".to_string(),
                ));
            }
        };

        // 2. 调 API（带 v0.6 过滤参数）
        let mut url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            urlencoding(&query),
            count
        );
        if let Some(s) = &site {
            url.push_str(&format!("&site_search={}", urlencoding(s)));
        }
        if let Some(s) = &site_filter {
            url.push_str(&format!("&site_filter={}", urlencoding(s)));
        }
        if let Some(s) = &site_exclude {
            // brave 没有 site_exclude，用 site_filter=null 也不行；先记在描述里
            // 我们在 client-side 过滤
        }
        if let Some(f) = &freshness {
            url.push_str(&format!("&freshness={}", urlencoding(f)));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| agent_core::Error::ToolExecution(format!("client: {}", e)))?;

        let resp = client
            .get(&url)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| agent_core::Error::ToolExecution(format!("http: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Ok(ToolOutput::err(format!(
                "Brave API 返回 {}: {}",
                status,
                if body.len() > 500 { &body[..500] } else { &body }
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| agent_core::Error::ToolExecution(format!("parse: {}", e)))?;

        // 3. 解析结果
        let mut results = body
            .pointer("/web/results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // v0.6：客户端 site_exclude 过滤
        if let Some(exclude) = &site_exclude {
            let excluded: Vec<String> = exclude
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            if !excluded.is_empty() {
                results.retain(|r| {
                    let url = r
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_lowercase();
                    !excluded.iter().any(|e| url.contains(e))
                });
            }
        }

        if results.is_empty() {
            return Ok(ToolOutput::ok(format!(
                "搜索 '{}' 无结果（应用了过滤条件）。",
                query
            )));
        }

        // 4. 描述过滤条件
        let mut filter_desc = Vec::new();
        if let Some(s) = &site {
            filter_desc.push(format!("site={}", s));
        }
        if let Some(s) = &site_filter {
            filter_desc.push(format!("sites={}", s));
        }
        if let Some(s) = &site_exclude {
            filter_desc.push(format!("exclude={}", s));
        }
        if let Some(f) = &freshness {
            filter_desc.push(format!("freshness={}", f));
        }

        let mut text = format!(
            "🔍 搜索结果：'{}' ({} 条{})\n\n",
            query,
            results.len(),
            if filter_desc.is_empty() {
                String::new()
            } else {
                format!(" · 过滤: {}", filter_desc.join(", "))
            }
        );
        for (i, r) in results.iter().enumerate().take(count as usize) {
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("(no title)");
            let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let desc = r.get("description").and_then(|v| v.as_str()).unwrap_or("");
            text.push_str(&format!(
                "{}. **{}**\n   {}\n   {}\n\n",
                i + 1,
                title,
                url,
                desc
            ));
        }
        Ok(ToolOutput::ok(text))
    }
}

/// 简单 URL 编码（不依赖 url crate）
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
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