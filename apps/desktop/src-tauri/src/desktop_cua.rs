//! v0.6：Desktop Computer Use (CUA) 骨架
//!
//! 思路：跨平台 `DesktopCua` trait + 各平台 impl。macOS 通过 `osascript` 调
//! System Events 桥接 AXUIElement；其他平台用 stub 报 "不支持"。
//!
//! Windows 的 UI Automation 实装留到 v0.7。

use agent_core::tool::ToolOutput;
use agent_core::{Error, Result, Tool};
use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

type ToolInput = serde_json::Value; // type alias local

/// 跨平台 Desktop CUA 接口
pub trait DesktopCua: Send + Sync + std::fmt::Debug {
    fn platform(&self) -> &'static str;
    fn list_windows(&self) -> Result<Vec<DesktopWindow>>;
    fn focus_window(&self, app_name: &str, title_contains: Option<&str>) -> Result<String>;
    fn get_app_tree(&self, app_name: &str, depth: u32) -> Result<String>;
    fn click_at(&self, x: i32, y: i32) -> Result<String>;
    fn type_text(&self, text: &str) -> Result<String>;
    fn key_combo(&self, keys: &str) -> Result<String>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DesktopWindow {
    pub app: String,
    pub title: String,
    pub pid: u32,
}

// ============================================================
// macOS 实现：通过 osascript 调 System Events
// ============================================================

#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
pub struct MacOsDesktopCua;

#[cfg(target_os = "macos")]
impl DesktopCua for MacOsDesktopCua {
    fn platform(&self) -> &'static str {
        "macos"
    }

    fn list_windows(&self) -> Result<Vec<DesktopWindow>> {
        let script = r#"
            tell application "System Events"
                set out to ""
                repeat with p in (every process whose visible is true)
                    set pname to name of p
                    repeat with w in (every window of p)
                        try
                            set wtitle to name of w
                            set pid_ to unix id of p
                            set out to out & pname & "|||" & wtitle & "|||" & pid_ & linefeed
                        end try
                    end repeat
                end repeat
                return out
            end tell
        "#;
        let raw = run_osascript(script)?;
        let mut wins = Vec::new();
        for line in raw.lines() {
            let parts: Vec<&str> = line.split("|||").collect();
            if parts.len() == 3 {
                let pid = parts[2].trim().parse().unwrap_or(0);
                if !parts[0].is_empty() && !parts[1].is_empty() {
                    wins.push(DesktopWindow {
                        app: parts[0].to_string(),
                        title: parts[1].to_string(),
                        pid,
                    });
                }
            }
        }
        Ok(wins)
    }

    fn focus_window(&self, app_name: &str, title_contains: Option<&str>) -> Result<String> {
        let title_filter = title_contains.unwrap_or("");
        // 注意：raw string + format! 中要把 literal { 转义为 {{ ，{} 是占位符
        let script = format!(
            r#"
            tell application "System Events"
                set theProcess to first process whose name is "{app}"
                if (count of windows of theProcess) is 0 then
                    return "FAIL: {app} 没有任何窗口"
                end if
                if "{title}" is "" then
                    set frontmost of theProcess to true
                    return "OK: focused {app}"
                else if (count of (every window of theProcess whose name contains "{title}")) is 0 then
                    return "FAIL: {app} 没有标题含 '{title}' 的窗口"
                else
                    set theWindow to first window of theProcess whose name contains "{title}"
                    set frontmost of theProcess to true
                    perform action "AXRaise" of theWindow
                    return "OK: focused {app} / {title}"
                end if
            end tell
            "#,
            app = app_name,
            title = title_filter,
        );
        run_osascript(&script)
    }

    fn get_app_tree(&self, app_name: &str, depth: u32) -> Result<String> {
        // 简化版：dump process 的 UI elements（按 depth 截断）
        let script = format!(
            r#"
            tell application "System Events"
                set theProcess to first process whose name is "{}"
                set out to ""
                tell theProcess
                    repeat with elem in (entire contents)
                        try
                            set out to out & (role of elem as string) & " | " & (description of elem as string) & linefeed
                        end try
                    end repeat
                end tell
                return out
            end tell
            "#,
            app_name
        );
        let raw = run_osascript(&script)?;
        // 按 depth 截断（每行算 1 个）
        let lines: Vec<&str> = raw.lines().take((depth as usize) * 20).collect();
        Ok(format!(
            "📱 App: {}\n🌲 UI Elements (max {} lines):\n{}",
            app_name,
            lines.len(),
            lines.join("\n")
        ))
    }

    fn click_at(&self, x: i32, y: i32) -> Result<String> {
        let script = format!(
            r#"
            do shell script "/usr/bin/python3 -c 'import Quartz; e = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventLeftMouseDown, ({}, {}), Quartz.kCGMouseButtonLeft); Quartz.CGEventPost(Quartz.kCGHIDEventTap, e); e2 = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventLeftMouseUp, ({}, {}), Quartz.kCGMouseButtonLeft); Quartz.CGEventPost(Quartz.kCGHIDEventTap, e2)'"
            "#,
            x, y, x, y
        );
        run_osascript(&script)?;
        Ok(format!("🖱️ 点击 ({}, {})", x, y))
    }

    fn type_text(&self, text: &str) -> Result<String> {
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            r#"tell application "System Events" to keystroke "{}""#,
            escaped
        );
        run_osascript(&script)?;
        Ok(format!("⌨️ 输入 {} 字符", text.chars().count()))
    }

    fn key_combo(&self, keys: &str) -> Result<String> {
        // 简化：keystroke 支持 "a" + {command down} 形式
        let parts: Vec<&str> = keys.split('+').map(|s| s.trim()).collect();
        if parts.is_empty() {
            return Err(Error::ToolExecution("keys 不能为空".to_string()));
        }
        let key = parts[0];
        let modifiers: Vec<&str> = parts[1..].iter().map(|s| {
            match s.to_lowercase().as_str() {
                "cmd" | "command" => "command down",
                "ctrl" | "control" => "control down",
                "alt" | "option" => "option down",
                "shift" => "shift down",
                _ => "command down",
            }
        }).collect();
        let using = if modifiers.is_empty() {
            String::new()
        } else {
            format!(" using {{ {} }}", modifiers.join(", "))
        };
        let script = format!(
            r#"tell application "System Events" to keystroke "{}"{}"#,
            key, using
        );
        run_osascript(&script)?;
        Ok(format!("⌨️ 组合键 {}", keys))
    }
}

// ============================================================
// 非 macOS 平台的 stub
// ============================================================

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Default)]
pub struct StubDesktopCua;

#[cfg(not(target_os = "macos"))]
impl DesktopCua for StubDesktopCua {
    fn platform(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "unknown"
        }
    }
    fn list_windows(&self) -> Result<Vec<DesktopWindow>> {
        eprintln!("[desktop_cua] Desktop CUA 在 {} 上未实装，留到 v0.7", self.platform());
        Ok(Vec::new())
    }
    fn focus_window(&self, _app: &str, _title: Option<&str>) -> Result<String> {
        Err(Error::ToolExecution(format!(
            "Desktop CUA 在 {} 上未实装（v0.7）",
            self.platform()
        )))
    }
    fn get_app_tree(&self, _app: &str, _depth: u32) -> Result<String> {
        Err(Error::ToolExecution(format!(
            "Desktop CUA 在 {} 上未实装（v0.7）",
            self.platform()
        )))
    }
    fn click_at(&self, _x: i32, _y: i32) -> Result<String> {
        Err(Error::ToolExecution(format!(
            "Desktop CUA 在 {} 上未实装（v0.7）",
            self.platform()
        )))
    }
    fn type_text(&self, _text: &str) -> Result<String> {
        Err(Error::ToolExecution(format!(
            "Desktop CUA 在 {} 上未实装（v0.7）",
            self.platform()
        )))
    }
    fn key_combo(&self, _keys: &str) -> Result<String> {
        Err(Error::ToolExecution(format!(
            "Desktop CUA 在 {} 上未实装（v0.7）",
            self.platform()
        )))
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<String> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| Error::ToolExecution(format!("osascript 启动失败: {}", e)))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Error::ToolExecution(format!(
            "osascript 错误: {}",
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ============================================================
// Tools — 把 CUA 包成 agent_core::Tool
// ============================================================

fn cua() -> Box<dyn DesktopCua> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacOsDesktopCua)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(StubDesktopCua)
    }
}

// --- desktop_list_windows ---

#[derive(Debug)]
pub struct DesktopListWindowsTool;
#[async_trait]
impl Tool for DesktopListWindowsTool {
    fn name(&self) -> &str {
        "desktop_list_windows"
    }
    fn description(&self) -> &str {
        "列出所有可见窗口的 {app, title, pid}。macOS 通过 System Events；其他平台 v0.7 实装。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _input: ToolInput) -> Result<ToolOutput> {
        let c = cua();
        let wins = c.list_windows()?;
        if wins.is_empty() {
            return Ok(ToolOutput::ok(format!(
                "🖥️ 平台 {}：无可见窗口（或平台未实装）",
                c.platform()
            )));
        }
        let mut text = format!("🖥️ 平台 {} 可见窗口 ({} 个)：\n\n", c.platform(), wins.len());
        for (i, w) in wins.iter().enumerate().take(50) {
            text.push_str(&format!(
                "{}. **{}** — `{}` (pid {})\n",
                i + 1,
                w.app,
                w.title,
                w.pid
            ));
        }
        if wins.len() > 50 {
            text.push_str(&format!("\n... 共 {} 个（截断）", wins.len()));
        }
        Ok(ToolOutput::ok(text))
    }
}

// --- desktop_focus_window ---

#[derive(Debug)]
pub struct DesktopFocusWindowTool;
#[async_trait]
impl Tool for DesktopFocusWindowTool {
    fn name(&self) -> &str {
        "desktop_focus_window"
    }
    fn description(&self) -> &str {
        "聚焦到指定应用的窗口（可按标题子串过滤）。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "app": {"type": "string", "description": "应用名，如 'Safari' 'Terminal'"},
                "title_contains": {"type": "string", "description": "窗口标题子串（可选）"}
            },
            "required": ["app"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let app = input["app"].as_str().unwrap_or("").to_string();
        if app.is_empty() {
            return Ok(ToolOutput::err("app 不能为空".to_string()));
        }
        let title = input["title_contains"].as_str();
        let r = cua().focus_window(&app, title)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_get_app_tree ---

#[derive(Debug)]
pub struct DesktopGetAppTreeTool;
#[async_trait]
impl Tool for DesktopGetAppTreeTool {
    fn name(&self) -> &str {
        "desktop_get_app_tree"
    }
    fn description(&self) -> &str {
        "获取应用的 UI 元素树（role + description）。depth 控制返回行数（depth*20）。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "app": {"type": "string"},
                "depth": {"type": "integer", "default": 3}
            },
            "required": ["app"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let app = input["app"].as_str().unwrap_or("").to_string();
        if app.is_empty() {
            return Ok(ToolOutput::err("app 不能为空".to_string()));
        }
        let depth = input["depth"].as_u64().unwrap_or(3) as u32;
        let r = cua().get_app_tree(&app, depth)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_click_at ---

#[derive(Debug)]
pub struct DesktopClickAtTool;
#[async_trait]
impl Tool for DesktopClickAtTool {
    fn name(&self) -> &str {
        "desktop_click_at"
    }
    fn description(&self) -> &str {
        "在屏幕坐标 (x, y) 上点击鼠标左键。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "x": {"type": "integer"},
                "y": {"type": "integer"}
            },
            "required": ["x", "y"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let x = input["x"].as_i64().unwrap_or(0) as i32;
        let y = input["y"].as_i64().unwrap_or(0) as i32;
        let r = cua().click_at(x, y)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_type_text ---

#[derive(Debug)]
pub struct DesktopTypeTextTool;
#[async_trait]
impl Tool for DesktopTypeTextTool {
    fn name(&self) -> &str {
        "desktop_type_text"
    }
    fn description(&self) -> &str {
        "向当前聚焦的应用输入文本。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"}
            },
            "required": ["text"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let text = input["text"].as_str().unwrap_or("").to_string();
        if text.is_empty() {
            return Ok(ToolOutput::err("text 不能为空".to_string()));
        }
        let r = cua().type_text(&text)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_key_combo ---

#[derive(Debug)]
pub struct DesktopKeyComboTool;
#[async_trait]
impl Tool for DesktopKeyComboTool {
    fn name(&self) -> &str {
        "desktop_key_combo"
    }
    fn description(&self) -> &str {
        "发送键盘组合键，如 'cmd+c' 'cmd+shift+p'。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "keys": {"type": "string", "description": "如 'cmd+c' 'cmd+shift+4'"}
            },
            "required": ["keys"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let keys = input["keys"].as_str().unwrap_or("").to_string();
        if keys.is_empty() {
            return Ok(ToolOutput::err("keys 不能为空".to_string()));
        }
        let r = cua().key_combo(&keys)?;
        Ok(ToolOutput::ok(r))
    }
}

/// 注册全部桌面 CUA 工具
pub fn register_desktop_cua(reg: &mut agent_core::ToolRegistry) {
    reg.register(DesktopListWindowsTool);
    reg.register(DesktopFocusWindowTool);
    reg.register(DesktopGetAppTreeTool);
    reg.register(DesktopClickAtTool);
    reg.register(DesktopTypeTextTool);
    reg.register(DesktopKeyComboTool);
}
