//! Action 调度

use serde::{Deserialize, Serialize};

use crate::schema::{BrowserAction, BrowserActionResult, ScreenshotFormat};

/// Browser Action dispatcher（把 BrowserAction 转换为 Playwright JS 脚本片段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action;

impl Action {
    /// 把 BrowserAction 转成 JavaScript 片段（用于在 Playwright REPL 执行）
    pub fn to_js(action: &BrowserAction) -> String {
        match action {
            BrowserAction::Navigate { url } => {
                format!(r#"await page.goto({});"#, js_string(url))
            }
            BrowserAction::Click { selector, delay_ms } => {
                let delay = delay_ms.unwrap_or(0);
                format!(
                    r#"await page.click({}, {{ delay: {} }});"#,
                    js_string(selector),
                    delay
                )
            }
            BrowserAction::DoubleClick { selector } => {
                format!(r#"await page.dblclick({});"#, js_string(selector))
            }
            BrowserAction::Hover { selector } => {
                format!(r#"await page.hover({});"#, js_string(selector))
            }
            BrowserAction::Type { selector, text } => {
                format!(
                    r#"await page.fill({}, {});"#,
                    js_string(selector),
                    js_string(text)
                )
            }
            BrowserAction::Press { key } => {
                format!(r#"await page.keyboard.press({});"#, js_string(key))
            }
            BrowserAction::Scroll { dx, dy } => {
                format!(r#"await page.mouse.wheel({}, {});"#, dx, dy)
            }
            BrowserAction::Screenshot { full_page, format } => {
                let fmt = format.unwrap_or(ScreenshotFormat::Png);
                let fmt_str = match fmt {
                    ScreenshotFormat::Png => "png",
                    ScreenshotFormat::Jpeg => "jpeg",
                };
                format!(
                    r#"await page.screenshot({{ fullPage: {}, type: '{}' }});"#,
                    full_page, fmt_str
                )
            }
            BrowserAction::GetHtml { selector } => match selector {
                Some(s) => format!(r#"await page.locator({}).innerHTML();"#, js_string(s)),
                None => "await page.content();".to_string(),
            },
            BrowserAction::GetText { selector } => {
                format!(r#"await page.locator({}).textContent();"#, js_string(selector))
            }
            BrowserAction::Evaluate { script } => script.clone(),
            BrowserAction::WaitFor { selector, timeout_ms } => {
                format!(
                    r#"await page.waitForSelector({}, {{ timeout: {} }});"#,
                    js_string(selector),
                    timeout_ms
                )
            }
            BrowserAction::Close => "await browser.close();".to_string(),
        }
    }

    /// 解析 JS 执行结果为 BrowserActionResult
    pub fn parse_result(action_name: &str, js_result: &str) -> BrowserActionResult {
        let mut r = BrowserActionResult::ok(action_name);
        // 简化：尝试判断是否为 base64 截图（png 以 iVBOR 开头，jpeg 以 /9j/ 开头）
        if js_result.starts_with("iVBOR") || js_result.starts_with("/9j/") {
            r.screenshot = Some(js_result.to_string());
        } else if js_result.starts_with("<") {
            r.html = Some(js_result.to_string());
        } else {
            r.text = Some(js_result.to_string());
        }
        r
    }
}

fn js_string(s: &str) -> String {
    // 简单 JSON encode（避免引号注入）
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s.replace('"', "\\\"")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::BrowserAction;

    #[test]
    fn test_navigate_js() {
        let a = BrowserAction::Navigate {
            url: "https://example.com".into(),
        };
        let js = Action::to_js(&a);
        assert!(js.contains("page.goto"));
        assert!(js.contains("https://example.com"));
    }

    #[test]
    fn test_click_js() {
        let a = BrowserAction::Click {
            selector: "#btn".into(),
            delay_ms: Some(100),
        };
        let js = Action::to_js(&a);
        assert!(js.contains("page.click"));
        assert!(js.contains("100"));
    }

    #[test]
    fn test_type_js() {
        let a = BrowserAction::Type {
            selector: "input".into(),
            text: "hello".into(),
        };
        let js = Action::to_js(&a);
        assert!(js.contains("page.fill"));
    }

    #[test]
    fn test_parse_result_text() {
        let r = Action::parse_result("get_text", "hello world");
        assert!(r.success);
        assert_eq!(r.text, Some("hello world".into()));
    }

    #[test]
    fn test_parse_result_screenshot() {
        let r = Action::parse_result("screenshot", "iVBORw0KGgo...");
        assert_eq!(r.screenshot, Some("iVBORw0KGgo...".into()));
    }

    #[test]
    fn test_parse_result_html() {
        let r = Action::parse_result("get_html", "<div>hello</div>");
        assert_eq!(r.html, Some("<div>hello</div>".into()));
    }

    #[test]
    fn test_js_string_escape() {
        let s = js_string("a\"b");
        assert!(s.contains("\\\""));
    }
}