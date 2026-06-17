//! Computer Use tool — 包装 computer-use crate 提供 browser_navigate / browser_screenshot 等
//!
//! v0.3 — 走 JS REPL（node + playwright js）
//! v0.4 — 切到 Playwright MCP server 或 macOS AXUIElement

use agent_core::tool::{Tool, ToolInput, ToolOutput};
use async_trait::async_trait;
use computer_use::{BrowserAction, JsRepl, ViewportSize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::OnceCell;

/// 全局共享的 JS REPL（避免重复 spawn node）
static REPL: OnceCell<Arc<Mutex<Option<JsRepl>>>> = OnceCell::const_new();

async fn repl() -> Option<Arc<Mutex<Option<JsRepl>>>> {
    let arc = REPL
        .get_or_init(|| async { Arc::new(Mutex::new(None)) })
        .await;
    Some(arc.clone())
}

async fn get_or_init_repl() -> Result<Arc<Mutex<Option<JsRepl>>>, String> {
    let cell = repl().await.ok_or("REPL cell init failed")?;
    let mut guard = cell.lock().await;
    if guard.is_none() {
        match JsRepl::spawn(None).await {
            Ok(r) => *guard = Some(r),
            Err(e) => {
                return Err(format!(
                    "启动 node REPL 失败: {}（需要先装 Node.js + npm install playwright）",
                    e
                ))
            }
        }
    }
    Ok(cell.clone())
}

// ============================================================
// browser_navigate
// ============================================================
#[derive(Debug)]
pub struct BrowserNavigateTool;

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn name(&self) -> &str {
        "browser_navigate"
    }
    fn description(&self) -> &str {
        "用 Playwright 打开一个 URL。返回页面 title + 当前 URL。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "要打开的 URL"}
            },
            "required": ["url"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let url = input["url"].as_str().unwrap_or("").to_string();
        if url.is_empty() {
            return Ok(ToolOutput::err("url 不能为空".to_string()));
        }
        let cell = match get_or_init_repl().await {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::err(e)),
        };
        let mut guard = cell.lock().await;
        let repl = match guard.as_mut() {
            Some(r) => r,
            None => return Ok(ToolOutput::err("REPL 未初始化".to_string())),
        };
        let script = format!(
            r#"
            const {{ chromium }} = require('playwright');
            (async () => {{
                if (!globalThis._browser) {{
                    globalThis._browser = await chromium.launch();
                    globalThis._ctx = await globalThis._browser.newContext({{ viewport: {{ width: 1280, height: 800 }} }});
                    globalThis._page = await globalThis._ctx.newPage();
                }}
                await globalThis._page.goto("{}", {{ waitUntil: "networkidle", timeout: 30000 }});
                return {{ url: globalThis._page.url(), title: await globalThis._page.title() }};
            }})()
            "#,
            url.replace('"', "\\\"")
        );
        match repl.eval(&script).await {
            Ok(out) => Ok(ToolOutput::ok(format!(
                "✅ 打开成功：\nURL: {}\nResult: {}",
                url, out
            ))),
            Err(e) => Ok(ToolOutput::err(format!("navigate 失败: {}", e))),
        }
    }
}

// ============================================================
// browser_screenshot
// ============================================================
#[derive(Debug)]
pub struct BrowserScreenshotTool;

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn name(&self) -> &str {
        "browser_screenshot"
    }
    fn description(&self) -> &str {
        "截当前页面，返回 base64 PNG 编码（默认截全页，可指定元素选择器）。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {"type": "string", "description": "可选 CSS 选择器，只截这个元素"},
                "full_page": {"type": "boolean", "description": "是否截全页（默认 false，仅可视区）"}
            }
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let cell = match get_or_init_repl().await {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::err(e)),
        };
        let mut guard = cell.lock().await;
        let repl = match guard.as_mut() {
            Some(r) => r,
            None => return Ok(ToolOutput::err("REPL 未初始化".to_string())),
        };
        let selector = input["selector"].as_str();
        let full_page = input["full_page"].as_bool().unwrap_or(false);
        let script = if let Some(sel) = selector {
            format!(
                r#"
                (async () => {{
                    if (!globalThis._page) return {{ error: "no page, navigate first" }};
                    const el = await globalThis._page.$("{}");
                    if (!el) return {{ error: "selector not found: {}" }};
                    const buf = await el.screenshot();
                    return {{ png: buf.toString("base64"), size: buf.length }};
                }})()
                "#,
                sel.replace('"', "\\\""),
                sel.replace('"', "\\\"")
            )
        } else {
            format!(
                r#"
                (async () => {{
                    if (!globalThis._page) return {{ error: "no page, navigate first" }};
                    const buf = await globalThis._page.screenshot({{ fullPage: {} }});
                    return {{ png: buf.toString("base64"), size: buf.length }};
                }})()
                "#,
                full_page
            )
        };
        match repl.eval(&script).await {
            Ok(out) => Ok(ToolOutput::ok(format!("📸 screenshot 结果：\n{}", out))),
            Err(e) => Ok(ToolOutput::err(format!("screenshot 失败: {}", e))),
        }
    }
}

// ============================================================
// browser_click
// ============================================================
#[derive(Debug)]
pub struct BrowserClickTool;

#[async_trait]
impl Tool for BrowserClickTool {
    fn name(&self) -> &str {
        "browser_click"
    }
    fn description(&self) -> &str {
        "点击页面元素（按 CSS 选择器）。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {"type": "string", "description": "要点击的元素 CSS 选择器"}
            },
            "required": ["selector"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let sel = input["selector"].as_str().unwrap_or("").to_string();
        if sel.is_empty() {
            return Ok(ToolOutput::err("selector 不能为空".to_string()));
        }
        let cell = match get_or_init_repl().await {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::err(e)),
        };
        let mut guard = cell.lock().await;
        let repl = match guard.as_mut() {
            Some(r) => r,
            None => return Ok(ToolOutput::err("REPL 未初始化".to_string())),
        };
        let script = format!(
            r#"
            (async () => {{
                if (!globalThis._page) return {{ error: "no page, navigate first" }};
                await globalThis._page.click("{}", {{ timeout: 5000 }});
                return {{ clicked: true }};
            }})()
            "#,
            sel.replace('"', "\\\"")
        );
        match repl.eval(&script).await {
            Ok(out) => Ok(ToolOutput::ok(format!("✅ 已点击 {}\n{}", sel, out))),
            Err(e) => Ok(ToolOutput::err(format!("click 失败: {}", e))),
        }
    }
}

// ============================================================
// browser_type
// ============================================================
#[derive(Debug)]
pub struct BrowserTypeTool;

#[async_trait]
impl Tool for BrowserTypeTool {
    fn name(&self) -> &str {
        "browser_type"
    }
    fn description(&self) -> &str {
        "在输入框里打字。先点击元素获得焦点，然后输入文本。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {"type": "string"},
                "text": {"type": "string"}
            },
            "required": ["selector", "text"]
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let sel = input["selector"].as_str().unwrap_or("").to_string();
        let text = input["text"].as_str().unwrap_or("").to_string();
        if sel.is_empty() || text.is_empty() {
            return Ok(ToolOutput::err("selector 和 text 必填".to_string()));
        }
        let cell = match get_or_init_repl().await {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::err(e)),
        };
        let mut guard = cell.lock().await;
        let repl = match guard.as_mut() {
            Some(r) => r,
            None => return Ok(ToolOutput::err("REPL 未初始化".to_string())),
        };
        let script = format!(
            r#"
            (async () => {{
                if (!globalThis._page) return {{ error: "no page, navigate first" }};
                await globalThis._page.fill("{}", "{}");
                return {{ typed: true }};
            }})()
            "#,
            sel.replace('"', "\\\""),
            text.replace('"', "\\\"").replace('\n', "\\n")
        );
        match repl.eval(&script).await {
            Ok(out) => Ok(ToolOutput::ok(format!(
                "✅ 已在 {} 输入 {} 字符\n{}",
                sel,
                text.len(),
                out
            ))),
            Err(e) => Ok(ToolOutput::err(format!("type 失败: {}", e))),
        }
    }
}

// ============================================================
// browser_get_text
// ============================================================
#[derive(Debug)]
pub struct BrowserGetTextTool;

#[async_trait]
impl Tool for BrowserGetTextTool {
    fn name(&self) -> &str {
        "browser_get_text"
    }
    fn description(&self) -> &str {
        "获取当前页面的文本内容（或指定元素的文本）。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {"type": "string", "description": "可选 CSS 选择器"}
            }
        })
    }
    async fn execute(&self, input: ToolInput) -> agent_core::Result<ToolOutput> {
        let cell = match get_or_init_repl().await {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::err(e)),
        };
        let mut guard = cell.lock().await;
        let repl = match guard.as_mut() {
            Some(r) => r,
            None => return Ok(ToolOutput::err("REPL 未初始化".to_string())),
        };
        let selector = input["selector"].as_str();
        let script = if let Some(sel) = selector {
            format!(
                r#"
                (async () => {{
                    if (!globalThis._page) return {{ error: "no page, navigate first" }};
                    const text = await globalThis._page.textContent("{}");
                    return {{ text: text || "" }};
                }})()
                "#,
                sel.replace('"', "\\\"")
            )
        } else {
            r#"
            (async () => {
                if (!globalThis._page) return { error: "no page, navigate first" };
                const text = await globalThis._page.evaluate(() => document.body.innerText);
                return { text };
            })()
            "#
            .to_string()
        };
        match repl.eval(&script).await {
            Ok(out) => Ok(ToolOutput::ok(format!("📄 页面文本：\n{}", out))),
            Err(e) => Ok(ToolOutput::err(format!("get_text 失败: {}", e))),
        }
    }
}

pub fn register_computer_use(reg: &mut agent_core::ToolRegistry) {
    reg.register(BrowserNavigateTool);
    reg.register(BrowserScreenshotTool);
    reg.register(BrowserClickTool);
    reg.register(BrowserTypeTool);
    reg.register(BrowserGetTextTool);
}
