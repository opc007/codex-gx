//! v1.9：截图 + 相对坐标协议 Tauri 命令
//!
//! ## 注册的命令
//! - `screen_list`             — 列出所有 displays
//! - `screen_primary`          — 主屏信息
//! - `screen_to_absolute`      — 相对坐标 → 绝对坐标
//! - `screen_screenshot`       — 截图（占位 — 真实接 screencapture/scrot）
//! - `screen_protocol_prompt`  — M3 system prompt addon

use screenshot::{
    multi_screen_relative_to_absolute, relative_to_absolute, AbsoluteCoord, RelativeCoord, Screen,
    ScreenshotMeta, PROTOCOL_PROMPT,
};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri::Emitter;

#[derive(Serialize)]
pub struct ScreenInfo {
    pub display_id: String,
    pub physical_width: u32,
    pub physical_height: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale: f32,
    pub is_primary: bool,
}

impl From<&Screen> for ScreenInfo {
    fn from(s: &Screen) -> Self {
        ScreenInfo {
            display_id: s.display_id.clone(),
            physical_width: s.physical_width,
            physical_height: s.physical_height,
            logical_width: s.logical_width,
            logical_height: s.logical_height,
            scale: s.scale,
            is_primary: s.is_primary,
        }
    }
}

#[tauri::command]
pub fn screen_list() -> Vec<ScreenInfo> {
    // 演示版：只返回主屏。真实版枚举 NSScreen / EnumDisplayMonitors
    vec![ScreenInfo::from(&Screen::default_primary())]
}

#[tauri::command]
pub fn screen_primary() -> ScreenInfo {
    ScreenInfo::from(&Screen::default_primary())
}

#[derive(Deserialize)]
pub struct ToAbsoluteArgs {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub element_index: Option<u32>,
}

#[tauri::command]
pub fn screen_to_absolute(args: ToAbsoluteArgs) -> Result<AbsoluteCoord, String> {
    let rel = RelativeCoord {
        x: args.x,
        y: args.y,
        reason: args.reason,
        element_index: args.element_index,
    };
    screenshot::validate(&rel).map_err(|e| e.to_string())?;
    let screen = Screen::default_primary();
    Ok(relative_to_absolute(&rel, &screen))
}

#[tauri::command]
pub async fn screen_screenshot(app: AppHandle) -> Result<ScreenshotMeta, String> {
    // 演示版：返回占位 base64。真实版：
    // macOS: CGDisplayCreateImage 或 screencapture 命令
    // Windows: BitBlt
    // Linux: scrot / grim
    let _ = app.emit("screen:screenshot:start", serde_json::json!({}));
    let screen = Screen::default_primary();
    let meta = ScreenshotMeta {
        width: screen.physical_width,
        height: screen.physical_height,
        scale: screen.scale,
        timestamp: chrono::Utc::now().timestamp(),
        display_id: screen.display_id.clone(),
        display_origin: (screen.origin_x, screen.origin_y),
        format: "png".into(),
        // 演示：1x1 透明 PNG
        data_base64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=".into(),
    };
    let _ = app.emit("screen:screenshot:done", &meta);
    Ok(meta)
}

#[tauri::command]
pub fn screen_protocol_prompt() -> String {
    PROTOCOL_PROMPT.to_string()
}

/// 内部使用：多屏换算
#[tauri::command]
pub fn screen_multi_to_absolute(
    rel_x: f32,
    rel_y: f32,
    screenshot: ScreenshotMeta,
) -> Result<AbsoluteCoord, String> {
    let rel = RelativeCoord { x: rel_x, y: rel_y, reason: None, element_index: None };
    screenshot::validate(&rel).map_err(|e| e.to_string())?;
    let screens = vec![Screen::default_primary()];
    multi_screen_relative_to_absolute(&rel, &screenshot, &screens)
        .ok_or_else(|| "no display".to_string())
}
