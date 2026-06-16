//! 浏览器操作 schema

use serde::{Deserialize, Serialize};

/// 视口大小
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
}

impl Default for ViewportSize {
    fn default() -> Self {
        Self { width: 1280, height: 720 }
    }
}

/// 截图格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
}

impl Default for ScreenshotFormat {
    fn default() -> Self {
        Self::Png
    }
}

/// 浏览器动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserAction {
    /// 打开 URL
    Navigate {
        /// 目标 URL
        url: String,
    },
    /// 点击（CSS selector / xpath）
    Click {
        /// 元素 selector
        selector: String,
        /// 等待毫秒
        #[serde(default)]
        delay_ms: Option<u32>,
    },
    /// 双击
    DoubleClick {
        selector: String,
    },
    /// 悬停
    Hover {
        selector: String,
    },
    /// 输入文本
    Type {
        selector: String,
        text: String,
    },
    /// 按键
    Press {
        key: String,
    },
    /// 滚动
    Scroll {
        /// X delta
        #[serde(default)]
        dx: i32,
        /// Y delta
        #[serde(default)]
        dy: i32,
    },
    /// 截图
    Screenshot {
        #[serde(default)]
        full_page: bool,
        #[serde(default)]
        format: Option<ScreenshotFormat>,
    },
    /// 提取 HTML
    GetHtml {
        #[serde(default)]
        selector: Option<String>,
    },
    /// 提取文本
    GetText {
        selector: String,
    },
    /// 执行 JS
    Evaluate {
        script: String,
    },
    /// 等待元素
    WaitFor {
        selector: String,
        /// 超时毫秒
        #[serde(default = "default_timeout")]
        timeout_ms: u32,
    },
    /// 关闭
    Close,
}

fn default_timeout() -> u32 {
    5000
}

/// 浏览器动作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserActionResult {
    /// 动作类型
    pub action: String,
    /// 成功
    pub success: bool,
    /// 文本输出（如 get_text）
    #[serde(default)]
    pub text: Option<String>,
    /// HTML 输出
    #[serde(default)]
    pub html: Option<String>,
    /// 截图（base64）
    #[serde(default)]
    pub screenshot: Option<String>,
    /// URL（navigate 后）
    #[serde(default)]
    pub url: Option<String>,
    /// 错误信息
    #[serde(default)]
    pub error: Option<String>,
}

impl BrowserActionResult {
    pub fn ok(action: &str) -> Self {
        Self {
            action: action.into(),
            success: true,
            text: None,
            html: None,
            screenshot: None,
            url: None,
            error: None,
        }
    }

    pub fn err(action: &str, msg: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            success: false,
            text: None,
            html: None,
            screenshot: None,
            url: None,
            error: Some(msg.into()),
        }
    }
}