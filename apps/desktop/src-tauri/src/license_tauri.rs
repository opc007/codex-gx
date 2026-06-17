//! v1.6：License 商业化 Tauri 命令
//!
//! 设计参考：docs/开发文档.md §6.12 License 管理页 + §13.6 商业化策略
//!
//! ## 注册的命令
//! - `license_status`        — 当前 License summary
//! - `license_activate`      — 用户输入激活码 → 激活
//! - `license_deactivate`    — 移除 License（重置为未激活）
//! - `license_refresh`       — 强制重新校验
//! - `license_tiers`         — 4 档 SKU 静态信息（前端展示）
//! - `license_demo_code`     — 内部生成 demo code（仅 dev）

use license::{LicenseManager, LicenseSummary, LicenseTier};
use std::sync::Arc;
use tauri::State;

pub type LicenseManagerState = Arc<LicenseManager>;

/// 4 档 SKU 展示信息（前端用）
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

/// 4 档 SKU 列表（写死，前端展示）
pub fn tier_list() -> Vec<TierInfo> {
    vec![
        TierInfo {
            tier: "monthly".into(),
            display_name: "月卡",
            duration_days: Some(30),
            price_yuan: 9.9,
            features: vec!["基础 chat", "patch", "多模态输入"],
            recommended: false,
        },
        TierInfo {
            tier: "quarterly".into(),
            display_name: "季卡",
            duration_days: Some(90),
            price_yuan: 29.9,
            features: vec![
                "月卡全部",
                "Computer Use",
                "Memory",
                "Hook",
                "Service Tier 全部",
                "MCP",
            ],
            recommended: false,
        },
        TierInfo {
            tier: "yearly".into(),
            display_name: "年卡（推荐）",
            duration_days: Some(365),
            price_yuan: 99.0,
            features: vec!["季卡全部", "重度个人开发"],
            recommended: true,
        },
        TierInfo {
            tier: "lifetime".into(),
            display_name: "终身",
            duration_days: None,
            price_yuan: 299.0,
            features: vec!["年卡全部", "永久免费升级 v1.x", "团队/企业预留接口"],
            recommended: false,
        },
    ]
}

/// 启动时构造 Manager（在 setup 内调）
pub fn build_state() -> LicenseManagerState {
    Arc::new(LicenseManager::new_default())
}

/// 启动时跑一次校验（best-effort，不阻塞启动）
pub async fn initial_check(state: &LicenseManagerState) -> LicenseSummary {
    state.check().await
}

#[tauri::command]
pub async fn license_status(
    state: State<'_, LicenseManagerState>,
) -> Result<LicenseSummary, String> {
    Ok(state.check().await)
}

#[tauri::command]
pub async fn license_activate(
    code: String,
    state: State<'_, LicenseManagerState>,
) -> Result<LicenseSummary, String> {
    state
        .activate(&code)
        .await
        .map_err(|e| format!("激活失败: {}", e))
}

#[tauri::command]
pub async fn license_deactivate(state: State<'_, LicenseManagerState>) -> Result<(), String> {
    state
        .deactivate()
        .await
        .map_err(|e| format!("清除失败: {}", e))
}

#[tauri::command]
pub async fn license_refresh(
    state: State<'_, LicenseManagerState>,
) -> Result<LicenseSummary, String> {
    Ok(state.refresh().await)
}

#[tauri::command]
pub fn license_tiers() -> Vec<TierInfo> {
    tier_list()
}

/// 内部：生成 demo license code（仅 dev — UI 隐藏入口）
#[tauri::command]
pub fn license_demo_code(tier: String) -> Result<String, String> {
    let tier = match tier.to_lowercase().as_str() {
        "monthly" => LicenseTier::Monthly,
        "quarterly" => LicenseTier::Quarterly,
        "yearly" => LicenseTier::Yearly,
        "lifetime" => LicenseTier::Lifetime,
        _ => return Err(format!("unknown tier: {tier}")),
    };
    let mgr = LicenseManager::new_default();
    mgr.generate_demo_code(tier).map_err(|e| e.to_string())
}
