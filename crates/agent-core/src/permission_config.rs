//! v1.2：用户权限配置

use crate::permission::PermissionLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 工具级权限覆盖
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionConfig {
    /// 严格模式（moderate 也要确认）
    #[serde(default)]
    pub strict_mode: bool,
    /// 工具 -> 权限级别 覆盖
    #[serde(default)]
    pub tool_overrides: HashMap<String, PermissionLevel>,
    /// 用户批准过的工具调用模式（"bash:rm specific file"）
    /// 一旦命中同模式 → 自动放行
    #[serde(default)]
    pub approved_patterns: Vec<String>,
    /// 用户拒绝过的模式（防止 LLM 反复尝试）
    #[serde(default)]
    pub denied_patterns: Vec<String>,
    /// 自定义 risk patterns（覆盖默认）
    #[serde(default)]
    pub custom_risk_patterns: Vec<String>,
    /// 自定义 blocked 路径
    #[serde(default)]
    pub custom_blocked_paths: Vec<String>,
}

impl PermissionConfig {
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(cfg) = serde_json::from_str::<PermissionConfig>(&text) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, text)
    }

    /// 工具的最终权限级别
    pub fn resolve(&self, tool: &str, base: PermissionLevel) -> PermissionLevel {
        self.tool_overrides.get(tool).copied().unwrap_or(base)
    }

    /// 命中已批准的 pattern？
    pub fn is_approved(&self, key: &str) -> bool {
        self.approved_patterns.iter().any(|p| p == key)
    }

    /// 命中已拒绝的 pattern？
    pub fn is_denied(&self, key: &str) -> bool {
        self.denied_patterns.iter().any(|p| p == key)
    }

    /// 记录批准
    pub fn record_approval(&mut self, key: String) {
        if !self.approved_patterns.contains(&key) {
            self.approved_patterns.push(key);
        }
    }

    /// 记录拒绝
    pub fn record_denial(&mut self, key: String) {
        if !self.denied_patterns.contains(&key) {
            self.denied_patterns.push(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn default_config() {
        let c = PermissionConfig::default();
        assert!(!c.strict_mode);
        assert!(c.tool_overrides.is_empty());
        assert!(c.approved_patterns.is_empty());
    }

    #[test]
    fn resolve_uses_override() {
        let mut c = PermissionConfig::default();
        c.tool_overrides
            .insert("bash".to_string(), PermissionLevel::Dangerous);
        assert_eq!(
            c.resolve("bash", PermissionLevel::Safe),
            PermissionLevel::Dangerous
        );
    }

    #[test]
    fn resolve_uses_base_when_no_override() {
        let c = PermissionConfig::default();
        assert_eq!(
            c.resolve("read_file", PermissionLevel::Safe),
            PermissionLevel::Safe
        );
    }

    #[test]
    fn record_approval_no_dup() {
        let mut c = PermissionConfig::default();
        c.record_approval("bash:ls".to_string());
        c.record_approval("bash:ls".to_string());
        assert_eq!(c.approved_patterns.len(), 1);
    }

    #[test]
    fn record_denial_no_dup() {
        let mut c = PermissionConfig::default();
        c.record_denial("bash:rm -rf /".to_string());
        c.record_denial("bash:rm -rf /".to_string());
        assert_eq!(c.denied_patterns.len(), 1);
    }

    #[test]
    fn save_and_load() {
        let mut c = PermissionConfig::default();
        c.strict_mode = true;
        c.tool_overrides
            .insert("bash".to_string(), PermissionLevel::Dangerous);
        let dir = env::temp_dir().join("agentshell_perm_test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("perm.json");
        c.save(&p).expect("save");
        let loaded = PermissionConfig::load_or_default(&p);
        assert!(loaded.strict_mode);
        assert_eq!(
            loaded.tool_overrides.get("bash").copied(),
            Some(PermissionLevel::Dangerous)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_approved_works() {
        let mut c = PermissionConfig::default();
        c.record_approval("foo".to_string());
        assert!(c.is_approved("foo"));
        assert!(!c.is_approved("bar"));
    }
}
