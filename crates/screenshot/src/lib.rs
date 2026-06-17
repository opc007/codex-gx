//! v1.9：截图 + 相对坐标协议
//!
//! 设计参考：docs/开发文档.md §5.27
//!
//! ## 核心协议（v1.9.1 校正）
//! M3 官方协议：`{"x": 0.0-1.0, "y": 0.0-1.0, "reason": "..."}` (float)
//!
//! ## 坐标换算
//! - M3 输出 0.0-1.0 相对坐标
//! - AgentShell 换算到屏幕物理像素
//! - Retina/HiDPI 屏：再 / scale 换算到 logical pixel
//! - 多屏：基于截图元数据 + display origin
//!
//! ## 不做
//! - ❌ 0-1000 整数（M3 协议是 0-1 float）
//! - ❌ 输出绝对像素
//!
//! ## 平台实现
//! - macOS: CGEventPost + CGRequestScreenCapture (需用户授权)
//! - Windows: SendInput + BitBlt
//! - Linux: xdotool + scrot/grim

use serde::{Deserialize, Serialize};

/// 屏幕信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screen {
    pub display_id: String,
    pub physical_width: u32,
    pub physical_height: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale: f32, // 1.0 = 标屏, 2.0 = Retina/HiDPI
    pub origin_x: i32,
    pub origin_y: i32,
    pub is_primary: bool,
}

impl Screen {
    pub fn default_primary() -> Self {
        Self {
            display_id: "primary".into(),
            physical_width: 1920,
            physical_height: 1080,
            logical_width: 1920,
            logical_height: 1080,
            scale: 1.0,
            origin_x: 0,
            origin_y: 0,
            is_primary: true,
        }
    }
}

/// 截图元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotMeta {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub timestamp: i64,
    pub display_id: String,
    pub display_origin: (i32, i32),
    pub format: String, // png / jpeg
    /// 截图的 base64 数据
    pub data_base64: String,
}

/// M3 输出（0.0-1.0 相对坐标）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelativeCoord {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub reason: Option<String>,
    /// 可选：元素索引（AX 节点引用），用于精确点击
    #[serde(default)]
    pub element_index: Option<u32>,
}

/// 换算后绝对坐标（logical pixel）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsoluteCoord {
    pub logical_x: f64,
    pub logical_y: f64,
    pub physical_x: f64,
    pub physical_y: f64,
    pub display_id: String,
}

/// 核心换算函数
pub fn relative_to_absolute(rel: &RelativeCoord, screen: &Screen) -> AbsoluteCoord {
    // 1. 边界 clamp（M3 偶尔输出 0.0001 越界）
    let rel_x = rel.x.clamp(0.0, 1.0) as f64;
    let rel_y = rel.y.clamp(0.0, 1.0) as f64;

    // 2. 物理像素
    let physical_x = rel_x * screen.physical_width as f64;
    let physical_y = rel_y * screen.physical_height as f64;

    // 3. logical pixel (Retina/HiDPI)
    let logical_x = physical_x / screen.scale as f64;
    let logical_y = physical_y / screen.scale as f64;

    AbsoluteCoord {
        logical_x,
        logical_y,
        physical_x,
        physical_y,
        display_id: screen.display_id.clone(),
    }
}

/// 多屏换算
pub fn multi_screen_relative_to_absolute(
    rel: &RelativeCoord,
    screenshot: &ScreenshotMeta,
    screens: &[Screen],
) -> Option<AbsoluteCoord> {
    // 1. 先用截图元数据定位起始 display
    let target_display = screens
        .iter()
        .find(|s| {
            s.display_id == screenshot.display_id
                || (s.origin_x == screenshot.display_origin.0
                    && s.origin_y == screenshot.display_origin.1)
        })
        .or_else(|| screens.first())?;

    Some(relative_to_absolute(rel, target_display))
}

/// 验证 RelativeCoord
pub fn validate(rel: &RelativeCoord) -> Result<(), CoordError> {
    if !(0.0..=1.0).contains(&rel.x) {
        return Err(CoordError::OutOfRange {
            field: "x",
            value: rel.x as f64,
        });
    }
    if !(0.0..=1.0).contains(&rel.y) {
        return Err(CoordError::OutOfRange {
            field: "y",
            value: rel.y as f64,
        });
    }
    Ok(())
}

/// 协议规范 system prompt 注入
pub const PROTOCOL_PROMPT: &str = r#"# 桌面 Computer Use 协议（v1.9.1）

1. 截图 → 拿 `desktop_screenshot()` 视觉
2. 决策 → 输出 JSON `{"x": 0.0-1.0, "y": 0.0-1.0, "reason": "..."}`（**float**，非整数）
3. 执行 → AgentShell 换算坐标 + 调 `desktop_click()`
4. 验证 → 再截图 + AX 树检查预期结果
5. 重试 → 失败 3 次后给"半成品报告"

# 不要做的事
- 不要输出绝对像素坐标（系统已自动换算）
- 不要输出 0-1000 整数（M3 协议是 0-1 float）
- 不要假定屏幕尺寸（始终用 0-1 相对）
- 不要在敏感 App（银行/支付/2FA）执行任何操作
"#;

#[derive(Debug, thiserror::Error)]
pub enum CoordError {
    #[error("coord `{field}`={value} 越界 (0.0-1.0)")]
    OutOfRange { field: &'static str, value: f64 },
    #[error("no display found")]
    NoDisplay,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_relative_to_absolute() {
        let screen = Screen {
            display_id: "primary".into(),
            physical_width: 1920,
            physical_height: 1080,
            logical_width: 1920,
            logical_height: 1080,
            scale: 1.0,
            origin_x: 0,
            origin_y: 0,
            is_primary: true,
        };
        let rel = RelativeCoord { x: 0.5, y: 0.5, reason: None, element_index: None };
        let abs = relative_to_absolute(&rel, &screen);
        assert_eq!(abs.physical_x, 960.0);
        assert_eq!(abs.physical_y, 540.0);
        assert_eq!(abs.logical_x, 960.0); // scale=1
    }

    #[test]
    fn test_retina_scale() {
        // Retina 屏 2880x1800 物理 / 1440x900 logical / scale=2
        let screen = Screen {
            display_id: "retina".into(),
            physical_width: 2880,
            physical_height: 1800,
            logical_width: 1440,
            logical_height: 900,
            scale: 2.0,
            origin_x: 0,
            origin_y: 0,
            is_primary: true,
        };
        let rel = RelativeCoord { x: 0.5, y: 0.5, reason: None, element_index: None };
        let abs = relative_to_absolute(&rel, &screen);
        assert_eq!(abs.physical_x, 1440.0);
        assert_eq!(abs.physical_y, 900.0);
        assert_eq!(abs.logical_x, 720.0);
        assert_eq!(abs.logical_y, 450.0);
    }

    #[test]
    fn test_clamp_out_of_range() {
        let screen = Screen::default_primary();
        let rel = RelativeCoord { x: 1.5, y: -0.1, reason: None, element_index: None };
        let abs = relative_to_absolute(&rel, &screen);
        // clamped to 1.0
        assert_eq!(abs.physical_x, 1920.0);
        // clamped to 0.0
        assert_eq!(abs.physical_y, 0.0);
    }

    #[test]
    fn test_validate() {
        let ok = RelativeCoord { x: 0.5, y: 0.5, reason: None, element_index: None };
        assert!(validate(&ok).is_ok());
        let bad = RelativeCoord { x: 1.5, y: 0.5, reason: None, element_index: None };
        assert!(validate(&bad).is_err());
    }

    #[test]
    fn test_multi_screen() {
        let screens = vec![
            Screen {
                display_id: "primary".into(),
                physical_width: 1920,
                physical_height: 1080,
                logical_width: 1920,
                logical_height: 1080,
                scale: 1.0,
                origin_x: 0,
                origin_y: 0,
                is_primary: true,
            },
            Screen {
                display_id: "external".into(),
                physical_width: 2560,
                physical_height: 1440,
                logical_width: 2560,
                logical_height: 1440,
                scale: 1.0,
                origin_x: 1920,
                origin_y: 0,
                is_primary: false,
            },
        ];
        let ss = ScreenshotMeta {
            width: 4480,
            height: 1080,
            scale: 1.0,
            timestamp: 0,
            display_id: "external".into(),
            display_origin: (1920, 0),
            format: "png".into(),
            data_base64: "".into(),
        };
        let rel = RelativeCoord { x: 0.5, y: 0.5, reason: None, element_index: None };
        let abs = multi_screen_relative_to_absolute(&rel, &ss, &screens).unwrap();
        assert_eq!(abs.display_id, "external");
        assert_eq!(abs.physical_x, 1280.0); // 0.5 * 2560
    }

    #[test]
    fn test_protocol_prompt() {
        assert!(PROTOCOL_PROMPT.contains("0.0-1.0"));
        assert!(PROTOCOL_PROMPT.contains("不要"));
    }
}
