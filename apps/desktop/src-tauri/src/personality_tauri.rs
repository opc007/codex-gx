//! v1.7：Personality Tauri 命令
//!
//! 设计参考：docs/开发文档.md §5.20
//!
//! ## 注册的命令
//! - `personality_get`          — 当前 personality + system prompt addon
//! - `personality_list_presets` — 3 档预设列表
//! - `personality_set_preset`   — 切到预设
//! - `personality_set_custom`   — 切到自定义
//! - `personality_save_custom`  — 写自定义 prompt 文件
//! - `personality_load_custom`  — 读自定义 prompt 文件

use personality::{Personality, PersonalityPreset, PersonalityRegistry};
use std::sync::Arc;
use tauri::State;

pub type PersonalityState = Arc<std::sync::Mutex<PersonalityRegistry>>;

pub fn build_state() -> PersonalityState {
    Arc::new(std::sync::Mutex::new(PersonalityRegistry::load()))
}

/// 返回给前端的 summary
#[derive(serde::Serialize)]
pub struct PersonalityInfo {
    pub current_kind: String,
    pub current_display: String,
    pub current_short: String,
    pub system_prompt_addon: String,
}

#[tauri::command]
pub async fn personality_get(state: State<'_, PersonalityState>) -> Result<PersonalityInfo, String> {
    let reg = state.lock().map_err(|e| e.to_string())?;
    Ok(PersonalityInfo {
        current_kind: reg.current().short().to_string(),
        current_display: reg.current().display_name(),
        current_short: reg.current().short().to_string(),
        system_prompt_addon: reg.system_prompt_addon(),
    })
}

#[tauri::command]
pub fn personality_list_presets() -> Vec<PresetInfo> {
    PersonalityPreset::all()
        .into_iter()
        .map(|p| PresetInfo {
            name: p.short().to_string(),
            display: p.display_name().to_string(),
        })
        .collect()
}

#[derive(serde::Serialize)]
pub struct PresetInfo {
    pub name: String,
    pub display: String,
}

#[tauri::command]
pub async fn personality_set_preset(
    preset: String,
    state: State<'_, PersonalityState>,
) -> Result<PersonalityInfo, String> {
    let p = PersonalityPreset::parse(&preset)
        .ok_or_else(|| format!("unknown preset: {preset}"))?;
    let mut reg = state.lock().map_err(|e| e.to_string())?;
    reg.set_preset(p).map_err(|e| e.to_string())?;
    Ok(PersonalityInfo {
        current_kind: reg.current().short().to_string(),
        current_display: reg.current().display_name(),
        current_short: reg.current().short().to_string(),
        system_prompt_addon: reg.system_prompt_addon(),
    })
}

#[tauri::command]
pub async fn personality_set_custom(
    state: State<'_, PersonalityState>,
) -> Result<PersonalityInfo, String> {
    let mut reg = state.lock().map_err(|e| e.to_string())?;
    reg.set_custom().map_err(|e| e.to_string())?;
    Ok(PersonalityInfo {
        current_kind: "custom".into(),
        current_display: reg.current().display_name(),
        current_short: "custom".into(),
        system_prompt_addon: reg.system_prompt_addon(),
    })
}

#[tauri::command]
pub fn personality_load_custom() -> Result<String, String> {
    let path = personality::custom_prompt_path();
    std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
}

#[tauri::command]
pub async fn personality_save_custom(
    text: String,
    state: State<'_, PersonalityState>,
) -> Result<PersonalityInfo, String> {
    // 简单校验：拒绝含 `<|system|>` 注入
    if text.contains("<|system|>") || text.contains("<|im_start|>") {
        return Err("包含注入标记，已拒绝".to_string());
    }
    let path = personality::custom_prompt_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    // 切到 custom
    let mut reg = state.lock().map_err(|e| e.to_string())?;
    reg.set_custom().map_err(|e| e.to_string())?;
    Ok(PersonalityInfo {
        current_kind: "custom".into(),
        current_display: reg.current().display_name(),
        current_short: "custom".into(),
        system_prompt_addon: reg.system_prompt_addon(),
    })
}
