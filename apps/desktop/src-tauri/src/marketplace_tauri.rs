// v1.2：Marketplace Tauri commands
//
// - marketplace_fetch_index   拉取远程注册表
// - marketplace_list_installed 列出本地已安装
// - marketplace_install       安装插件
// - marketplace_uninstall     卸载插件
// - marketplace_set_index_url 切换注册表 URL

use crate::MarketplaceState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct MarketplaceSnapshot {
    pub index_url: String,
    pub plugins: Vec<marketplace::PluginManifest>,
    pub installed: Vec<marketplace::InstalledPlugin>,
    /// 错误（拉取失败时）
    pub error: Option<String>,
}

#[tauri::command]
pub async fn marketplace_fetch_index(
    state: tauri::State<'_, MarketplaceState>,
) -> Result<Vec<marketplace::PluginManifest>, String> {
    let mgr = state.inner().lock().await;
    let idx = mgr.fetch_index().await.map_err(|e| e.to_string())?;
    Ok(idx.plugins)
}

#[derive(Debug, Serialize)]
pub struct InstalledSummary {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub plugin_type: String,
    pub installed_at: String,
    pub local_path: String,
}

#[tauri::command]
pub async fn marketplace_list_installed(
    state: tauri::State<'_, MarketplaceState>,
) -> Result<Vec<InstalledSummary>, String> {
    let mgr = state.inner().lock().await;
    let list = mgr.load_installed().map_err(|e| e.to_string())?;
    Ok(list
        .plugins
        .into_iter()
        .map(|p| InstalledSummary {
            name: p.name,
            version: p.version,
            plugin_type: p.plugin_type.as_str().to_string(),
            installed_at: p.installed_at,
            local_path: p.local_path.to_string_lossy().to_string(),
        })
        .collect())
}

#[derive(Debug, Deserialize)]
pub struct InstallArgs {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallResult {
    pub name: String,
    pub version: String,
    pub local_path: String,
}

#[tauri::command]
pub async fn marketplace_install(
    state: tauri::State<'_, MarketplaceState>,
    args: InstallArgs,
) -> Result<InstallResult, String> {
    // 拉取注册表找到插件
    let manifest = {
        let mgr = state.inner().lock().await;
        let idx = mgr.fetch_index().await.map_err(|e| e.to_string())?;
        idx.plugins
            .into_iter()
            .find(|p| p.name == args.name)
            .ok_or_else(|| format!("插件不存在: {}", args.name))?
    };
    // 复制必要信息后释放 lock，安装在 spawn_blocking 中跑（下载是 IO 密集）
    let (plugins_dir, tools_dir, mcp_dir, skills_dir) = paths().map_err(|e| e.to_string())?;
    let mgr2 = state.inner().lock().await;
    let result = mgr2.install(&manifest).await.map_err(|e| e.to_string());
    let _ = (plugins_dir, tools_dir, mcp_dir, skills_dir);
    result.map(|rec| InstallResult {
        name: rec.name,
        version: rec.version,
        local_path: rec.local_path.to_string_lossy().to_string(),
    })
}

#[derive(Debug, Deserialize)]
pub struct UninstallArgs {
    pub name: String,
}

#[tauri::command]
pub async fn marketplace_uninstall(
    state: tauri::State<'_, MarketplaceState>,
    args: UninstallArgs,
) -> Result<(), String> {
    let mgr = state.inner().lock().await;
    mgr.uninstall(&args.name).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct SetIndexUrlArgs {
    pub url: String,
}

#[tauri::command]
pub async fn marketplace_set_index_url(
    state: tauri::State<'_, MarketplaceState>,
    args: SetIndexUrlArgs,
) -> Result<(), String> {
    let mut mgr = state.inner().lock().await;
    mgr.index_url = args.url;
    Ok(())
}

#[tauri::command]
pub async fn marketplace_get_index_url(
    state: tauri::State<'_, MarketplaceState>,
) -> Result<String, String> {
    let mgr = state.inner().lock().await;
    Ok(mgr.index_url.clone())
}

fn paths() -> std::result::Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
    let home = dirs_home().ok_or_else(|| "找不到 HOME".to_string())?;
    let root = home.join(".agentshell");
    Ok((
        root.join("marketplace"),
        root.join("tools"),
        root.join("marketplace"),
        root.join("skills"),
    ))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}