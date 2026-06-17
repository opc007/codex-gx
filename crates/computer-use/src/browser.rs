//! 浏览器会话（high-level API）

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::error::Result;
use crate::repl::JsRepl;
use crate::schema::{BrowserAction, BrowserActionResult, ViewportSize};

/// 浏览器会话
pub struct BrowserSession {
    repl: Option<JsRepl>,
    viewport: ViewportSize,
    current_url: Option<String>,
}

impl BrowserSession {
    /// 创建新会话（lazy：实际启动 REPL 在第一次 navigate 时）
    pub fn new() -> Self {
        Self {
            repl: None,
            viewport: ViewportSize::default(),
            current_url: None,
        }
    }

    /// 设置视口
    pub fn set_viewport(&mut self, v: ViewportSize) {
        self.viewport = v;
    }

    /// 当前 URL
    pub fn current_url(&self) -> Option<&str> {
        self.current_url.as_deref()
    }

    /// 执行动作
    pub async fn execute(&mut self, action: BrowserAction) -> Result<BrowserActionResult> {
        // lazy init
        if self.repl.is_none() {
            // 注意：v0.1 不会真的启动 Playwright（依赖 node + playwright）
            // 这里只演示 JS REPL 调用结构
            let repl = JsRepl::spawn(None).await?;
            self.repl = Some(repl);
        }

        let action_name = format!("{:?}", action_label(&action));
        let js = Action::to_js(&action);

        if let Some(repl) = self.repl.as_mut() {
            let raw = repl.eval(&js).await?;
            let result = Action::parse_result(&action_name, &raw);

            // 更新当前 URL
            if matches!(action, BrowserAction::Navigate { .. }) {
                if let BrowserAction::Navigate { url } = &action {
                    self.current_url = Some(url.clone());
                    if let Some(r) = self.repl.as_mut() {
                        let _ = r.eval(&format!(r#"await page.url()"#)).await.ok();
                    }
                }
            }

            Ok(result)
        } else {
            // 不应该到这里
            Ok(BrowserActionResult::err(&action_name, "no repl"))
        }
    }

    /// 关闭
    pub async fn close(mut self) -> Result<()> {
        if let Some(repl) = self.repl.take() {
            repl.shutdown().await?;
        }
        Ok(())
    }
}

impl Default for BrowserSession {
    fn default() -> Self {
        Self::new()
    }
}

fn action_label(a: &BrowserAction) -> &'static str {
    match a {
        BrowserAction::Navigate { .. } => "navigate",
        BrowserAction::Click { .. } => "click",
        BrowserAction::DoubleClick { .. } => "double_click",
        BrowserAction::Hover { .. } => "hover",
        BrowserAction::Type { .. } => "type",
        BrowserAction::Press { .. } => "press",
        BrowserAction::Scroll { .. } => "scroll",
        BrowserAction::Screenshot { .. } => "screenshot",
        BrowserAction::GetHtml { .. } => "get_html",
        BrowserAction::GetText { .. } => "get_text",
        BrowserAction::Evaluate { .. } => "evaluate",
        BrowserAction::WaitFor { .. } => "wait_for",
        BrowserAction::Close => "close",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub viewport: ViewportSize,
    pub current_url: Option<String>,
}

impl BrowserSession {
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            viewport: self.viewport,
            current_url: self.current_url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let s = BrowserSession::new();
        assert_eq!(s.viewport.width, 1280);
        assert_eq!(s.viewport.height, 720);
        assert!(s.current_url.is_none());
    }

    #[test]
    fn test_set_viewport() {
        let mut s = BrowserSession::new();
        s.set_viewport(ViewportSize {
            width: 800,
            height: 600,
        });
        assert_eq!(s.viewport.width, 800);
    }

    #[test]
    fn test_info() {
        let s = BrowserSession::new();
        let info = s.info();
        assert_eq!(info.viewport.width, 1280);
    }
}
