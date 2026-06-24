//! v2.0: 完全免费 — 永久社区版
//!
//! 所有 License 验证逻辑已废除。任何人可免费使用完整功能。

use license::{LicenseSummary, LicenseStatus, LicenseTier};
use std::sync::Arc;
use tauri::State;

pub type LicenseManagerState = Arc<()>;

pub fn build_state() -> LicenseManagerState {
    Arc::new(())
}

pub async fn initial_check(_state: &LicenseManagerState) -> LicenseSummary {
    build_free_summary()
}

fn build_free_summary() -> LicenseSummary {
    LicenseSummary {
        status: LicenseStatus::Valid {
            tier: LicenseTier::Community,
            remaining_days: None,
            activated_at: 0,
            expires_at: None,
        },
        last_validated_at: chrono::Utc::now().timestamp(),
        offline: false,
    }
}

#[tauri::command]
pub async fn license_status(
    _state: State<'_, LicenseManagerState>,
) -> Result<LicenseSummary, String> {
    Ok(build_free_summary())
}

#[tauri::command]
pub async fn license_activate(
    _code: String,
    _state: State<'_, LicenseManagerState>,
) -> Result<LicenseSummary, String> {
    Ok(build_free_summary())
}

#[tauri::command]
pub async fn license_deactivate(
    _state: State<'_, LicenseManagerState>,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn license_refresh(
    _state: State<'_, LicenseManagerState>,
) -> Result<LicenseSummary, String> {
    Ok(build_free_summary())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierInfo {
    pub tier: String,
    pub display_name: &'static str,
    pub duration_days: Option<i64>,
    pub price_yuan: f32,
    pub features: Vec<&'static str>,
    pub recommended: bool,
}

#[tauri::command]
pub fn license_tiers() -> Vec<TierInfo> {
    vec![TierInfo {
        tier: "community".to_string(),
        display_name: "社区版",
        duration_days: None,
        price_yuan: 0.0,
        features: vec![
            "完整功能免费使用",
            "支持 M3 / DeepSeek / OpenAI / Claude",
            "Computer Use 多模态",
            "Skills 插件市场",
            "社区共同维护",
            "MIT 开源协议",
        ],
        recommended: true,
    }]
}

#[tauri::command]
pub fn license_demo_code(_tier: String) -> Result<String, String> {
    Ok("COMMUNITY-FREE-OPEN".to_string())
}
