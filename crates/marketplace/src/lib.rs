//! v1.2：Plugin marketplace — 社区 skills / tools / mcp-servers 一键安装
//!
//! 设计：
//! - 注册表：一个 JSON 文件，包含多个 plugin
//!   格式：{ plugins: [{ name, version, type, description, ... }] }
//! - 安装位置：
//!   - skill → `~/.agentshell/skills/<name>.json`
//!   - tool  → `~/.agentshell/tools/<name>/` (目录 + 配置文件)
//!   - mcp-server → 写入 `~/.agentshell/mcp.json`
//! - 安装记录：`~/.agentshell/marketplace/installed.json`
//! - 安全：第一次安装需用户显式确认（前端 UI）
//! - 校验：可选的 sha256 校验（manifest 提供时）
//!
//! 注册表 URL（默认）：
//!   https://raw.githubusercontent.com/opc007/codex-gx-plugins/main/index.json

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Marketplace 错误
#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP 错误: {0}")]
    Http(String),
    #[error("插件不存在: {0}")]
    PluginNotFound(String),
    #[error("插件类型不支持: {0}")]
    UnsupportedType(String),
    #[error("校验失败: {0}")]
    ChecksumMismatch(String),
    #[error("其他: {0}")]
    Other(String),
}

/// Result 别名
pub type Result<T> = std::result::Result<T, MarketplaceError>;

/// 插件类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    /// 斜杠命令 skill
    Skill,
    /// 工具
    Tool,
    /// MCP 服务器
    McpServer,
}

impl PluginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginType::Skill => "skill",
            PluginType::Tool => "tool",
            PluginType::McpServer => "mcp_server",
        }
    }
}

/// 插件 manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    pub description: String,
    /// 作者
    #[serde(default)]
    pub author: Option<String>,
    /// 主页 / 仓库
    #[serde(default)]
    pub homepage: Option<String>,
    /// 标签
    #[serde(default)]
    pub tags: Vec<String>,
    /// 资源（按类型不同含义不同）
    #[serde(default)]
    pub artifacts: Vec<PluginArtifact>,
    /// 安装说明（可选）
    #[serde(default)]
    pub install_instructions: Option<String>,
    /// 必需权限
    #[serde(default)]
    pub required_permissions: Vec<String>,
}

/// 单个 artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginArtifact {
    /// 资源 URL
    pub url: String,
    /// sha256 校验（hex）
    #[serde(default)]
    pub sha256: Option<String>,
    /// 文件名（下载后保存为）
    #[serde(default)]
    pub filename: Option<String>,
    /// 平台限制（如 ["macos", "windows"]）
    #[serde(default)]
    pub platforms: Vec<String>,
    /// 架构限制（如 ["x86_64", "aarch64"]）
    #[serde(default)]
    pub archs: Vec<String>,
}

/// 注册表根对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginIndex {
    /// 注册表版本
    #[serde(default = "default_index_version")]
    pub index_version: String,
    /// 最后更新
    #[serde(default)]
    pub updated_at: Option<String>,
    /// 插件列表
    pub plugins: Vec<PluginManifest>,
}

fn default_index_version() -> String {
    "1".to_string()
}

/// 已安装插件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub name: String,
    pub version: String,
    pub plugin_type: PluginType,
    pub installed_at: String,
    /// 安装源 URL
    pub source: String,
    /// 安装的本地路径
    pub local_path: PathBuf,
}

/// 已安装清单根对象
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledList {
    pub plugins: Vec<InstalledPlugin>,
}

/// Marketplace 管理器
pub struct MarketplaceManager {
    /// 根目录
    pub agentshell_dir: PathBuf,
    /// 已安装清单路径
    pub installed_file: PathBuf,
    /// 注册表 URL
    pub index_url: String,
}

impl MarketplaceManager {
    pub fn new() -> Result<Self> {
        let home = dirs_home();
        let agentshell_dir = home.join(".agentshell");
        let installed_file = agentshell_dir
            .join("marketplace")
            .join("installed.json");
        std::fs::create_dir_all(agentshell_dir.join("marketplace"))?;
        std::fs::create_dir_all(agentshell_dir.join("skills"))?;
        std::fs::create_dir_all(agentshell_dir.join("tools"))?;
        Ok(Self {
            agentshell_dir,
            installed_file,
            index_url: "https://raw.githubusercontent.com/opc007/codex-gx-plugins/main/index.json".to_string(),
        })
    }

    /// 自定义注册表 URL
    pub fn with_index_url(mut self, url: impl Into<String>) -> Self {
        self.index_url = url.into();
        self
    }

    /// 获取注册表（远程）
    pub async fn fetch_index(&self) -> Result<PluginIndex> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        let resp = client
            .get(&self.index_url)
            .send()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MarketplaceError::Http(format!(
                "HTTP {} (url={})",
                resp.status(),
                self.index_url
            )));
        }
        let text = resp
            .text()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        let idx: PluginIndex = serde_json::from_str(&text)?;
        Ok(idx)
    }

    /// 读取已安装清单
    pub fn load_installed(&self) -> Result<InstalledList> {
        if !self.installed_file.exists() {
            return Ok(InstalledList::default());
        }
        let text = std::fs::read_to_string(&self.installed_file)?;
        let list: InstalledList = serde_json::from_str(&text).unwrap_or_default();
        Ok(list)
    }

    /// 保存已安装清单
    pub fn save_installed(&self, list: &InstalledList) -> Result<()> {
        let text = serde_json::to_string_pretty(list)?;
        std::fs::write(&self.installed_file, text)?;
        Ok(())
    }

    /// 安装插件（下载 + 校验 + 落盘 + 记录）
    pub async fn install(
        &self,
        manifest: &PluginManifest,
    ) -> Result<InstalledPlugin> {
        // 检查平台匹配
        let my_platform = current_platform();
        let my_arch = current_arch();
        let artifact = self.pick_artifact(manifest, &my_platform, &my_arch)?;

        // 下载到临时文件
        let bytes = self
            .download_artifact(artifact)
            .await?;

        // 校验
        if let Some(expected) = &artifact.sha256 {
            let actual = sha256_hex(&bytes);
            if actual.to_lowercase() != expected.to_lowercase() {
                return Err(MarketplaceError::ChecksumMismatch(format!(
                    "expected={} actual={}",
                    expected, actual
                )));
            }
        }

        // 按类型落盘
        let local_path = match manifest.plugin_type {
            PluginType::Skill => {
                let p = self.agentshell_dir.join("skills").join(format!("{}.json", manifest.name));
                if let Some(filename) = &artifact.filename {
                    let p2 = self
                        .agentshell_dir
                        .join("skills")
                        .join(format!("{}.{}", manifest.name, extension_of(filename)));
                    std::fs::write(&p2, &bytes)?;
                    p2
                } else {
                    std::fs::write(&p, &bytes)?;
                    p
                }
            }
            PluginType::Tool => {
                let dir = self.agentshell_dir.join("tools").join(&manifest.name);
                std::fs::create_dir_all(&dir)?;
                let filename = artifact
                    .filename
                    .clone()
                    .unwrap_or_else(|| "plugin.bin".to_string());
                let p = dir.join(&filename);
                std::fs::write(&p, &bytes)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&p)?.permissions();
                    perms.set_mode(0o755);
                    std::fs::set_permissions(&p, perms)?;
                }
                p
            }
            PluginType::McpServer => {
                let p = self
                    .agentshell_dir
                    .join("marketplace")
                    .join(format!("mcp-{}.json", manifest.name));
                std::fs::write(&p, &bytes)?;
                p
            }
        };

        let record = InstalledPlugin {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            plugin_type: manifest.plugin_type,
            installed_at: chrono::Utc::now().to_rfc3339(),
            source: self.index_url.clone(),
            local_path,
        };

        // 追加到已安装清单
        let mut list = self.load_installed()?;
        list.plugins.retain(|p| p.name != record.name);
        list.plugins.push(record.clone());
        self.save_installed(&list)?;
        Ok(record)
    }

    /// 卸载插件（删除文件 + 从清单移除）
    pub fn uninstall(&self, name: &str) -> Result<()> {
        let mut list = self.load_installed()?;
        let p = list.plugins.iter().find(|p| p.name == name).cloned();
        if let Some(p) = p {
            // 删文件
            if p.local_path.exists() {
                if p.local_path.is_dir() {
                    std::fs::remove_dir_all(&p.local_path).ok();
                } else {
                    std::fs::remove_file(&p.local_path).ok();
                }
            }
            // 如果是 mcp_server，删 mcp-<name>.json
            let mcp_file = self
                .agentshell_dir
                .join("marketplace")
                .join(format!("mcp-{}.json", name));
            if mcp_file.exists() {
                std::fs::remove_file(mcp_file).ok();
            }
            list.plugins.retain(|x| x.name != name);
            self.save_installed(&list)?;
        }
        Ok(())
    }

    /// 选择匹配的 artifact（按 platform/arch 过滤后返回第一个；fallback 任意第一个）
    fn pick_artifact<'a>(
        &self,
        manifest: &'a PluginManifest,
        platform: &str,
        arch: &str,
    ) -> Result<&'a PluginArtifact> {
        for a in &manifest.artifacts {
            let platform_ok = a.platforms.is_empty()
                || a.platforms.iter().any(|p| p == platform);
            let arch_ok = a.archs.is_empty() || a.archs.iter().any(|p| p == arch);
            if platform_ok && arch_ok {
                return Ok(a);
            }
        }
        let name = manifest.name.clone();
        manifest
            .artifacts
            .first()
            .ok_or_else(|| MarketplaceError::PluginNotFound(format!("{}: 无 artifact", name)))
    }

    /// 下载 artifact
    async fn download_artifact(&self, artifact: &PluginArtifact) -> Result<Vec<u8>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        let resp = client
            .get(&artifact.url)
            .send()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MarketplaceError::Http(format!(
                "HTTP {} 下载 {}",
                resp.status(),
                artifact.url
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        Ok(bytes.to_vec())
    }
}

impl Default for MarketplaceManager {
    fn default() -> Self {
        Self::new().expect("MarketplaceManager init")
    }
}

fn dirs_home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    if let Ok(h) = std::env::var("USERPROFILE") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    std::env::temp_dir()
}

fn current_platform() -> String {
    if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        "unknown".to_string()
    }
}

fn current_arch() -> String {
    if cfg!(target_arch = "x86_64") {
        "x86_64".to_string()
    } else if cfg!(target_arch = "aarch64") {
        "aarch64".to_string()
    } else {
        "unknown".to_string()
    }
}

fn extension_of(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("json")
        .to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 给测试用：独立的临时目录，避免污染 ~/.agentshell
    fn fresh_manager(test_name: &str) -> MarketplaceManager {
        let dir = std::env::temp_dir().join(format!("agentshell_mp_test_{}", test_name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut m = MarketplaceManager::new().expect("init");
        m.agentshell_dir = dir.clone();
        m.installed_file = dir.join("installed.json");
        std::fs::create_dir_all(dir.join("skills")).unwrap();
        std::fs::create_dir_all(dir.join("tools")).unwrap();
        std::fs::create_dir_all(dir.join("marketplace")).unwrap();
        m
    }

    fn sample_manifest() -> PluginManifest {
        serde_json::from_value(json!({
            "name": "test-plugin",
            "version": "1.0.0",
            "type": "skill",
            "description": "test",
            "artifacts": []
        }))
        .unwrap()
    }

    #[test]
    fn manager_init() {
        let m = fresh_manager("init");
        assert!(m.installed_file.to_string_lossy().contains("installed.json"));
    }

    #[test]
    fn load_installed_empty() {
        let m = fresh_manager("load_empty");
        let list = m.load_installed().expect("load");
        assert_eq!(list.plugins.len(), 0);
    }

    #[test]
    fn save_and_load_installed() {
        let m = fresh_manager("save_load");
        let rec = InstalledPlugin {
            name: "demo".to_string(),
            version: "0.1.0".to_string(),
            plugin_type: PluginType::Skill,
            installed_at: chrono::Utc::now().to_rfc3339(),
            source: "test".to_string(),
            local_path: PathBuf::from("/tmp/demo"),
        };
        m.save_installed(&InstalledList {
            plugins: vec![rec.clone()],
        })
        .expect("save");
        let loaded = m.load_installed().expect("load");
        assert_eq!(loaded.plugins.len(), 1);
        assert_eq!(loaded.plugins[0].name, "demo");
    }

    #[test]
    fn uninstall_nonexistent_is_ok() {
        let m = fresh_manager("uninstall_none");
        m.uninstall("never-installed").expect("ok");
    }

    #[test]
    fn parse_manifest_json() {
        let m: PluginManifest = serde_json::from_value(json!({
            "name": "x",
            "version": "1.0.0",
            "type": "tool",
            "description": "x",
            "tags": ["a", "b"],
            "artifacts": [{"url": "https://x.com/y"}],
        }))
        .unwrap();
        assert_eq!(m.plugin_type, PluginType::Tool);
        assert_eq!(m.tags, vec!["a", "b"]);
    }

    #[test]
    fn plugin_type_as_str() {
        assert_eq!(PluginType::Skill.as_str(), "skill");
        assert_eq!(PluginType::Tool.as_str(), "tool");
        assert_eq!(PluginType::McpServer.as_str(), "mcp_server");
    }

    #[test]
    fn sha256_hex_test() {
        let s = sha256_hex(b"hello");
        // 已知 sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        assert_eq!(
            s,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn extension_of_simple() {
        assert_eq!(extension_of("foo.json"), "json");
        assert_eq!(extension_of("noext"), "json");
        assert_eq!(extension_of("a.tar.gz"), "gz");
    }

    #[test]
    fn pick_artifact_default() {
        let m = fresh_manager("pick_default");
        let manifest = serde_json::from_value(json!({
            "name": "x",
            "version": "1.0.0",
            "type": "skill",
            "description": "x",
            "artifacts": [{"url": "https://a.com/x"}]
        }))
        .unwrap();
        let a = m
            .pick_artifact(&manifest, "macos", "aarch64")
            .expect("pick");
        assert_eq!(a.url, "https://a.com/x");
    }

    #[test]
    fn pick_artifact_with_platform_filter() {
        let m = fresh_manager("pick_filter");
        let manifest = serde_json::from_value(json!({
            "name": "x",
            "version": "1.0.0",
            "type": "tool",
            "description": "x",
            "artifacts": [
                {"url": "https://a.com/mac", "platforms": ["macos"]},
                {"url": "https://a.com/win", "platforms": ["windows"]}
            ]
        }))
        .unwrap();
        let a = m.pick_artifact(&manifest, "windows", "x86_64").expect("pick");
        assert_eq!(a.url, "https://a.com/win");
    }

    #[test]
    fn sample_manifest_works() {
        let _ = sample_manifest();
    }
}