//! v1.9：App 白名单 + 权限系统
//!
//! 设计参考：docs/开发文档.md §5.28
//!
//! ## 三态机
//! - `AlwaysAllow` — 直接执行
//! - `AlwaysAsk`   — 弹权限弹窗
//! - `Denied`      — 拒绝
//!
//! ## 强制黑名单（5.5.7）
//! 银行/支付/证券/密码管理/2FA — 永远 deny

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 权限决策
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Allow,        // 直接执行
    Ask,          // 弹窗
    Deny,         // 拒绝
}

/// 单个 App 的元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMeta {
    pub bundle_id: Option<String>,
    pub display_name: String,
    pub process_name: String,
    pub platform: String, // macos / windows / linux
}

/// 白名单文件
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionList {
    #[serde(default)]
    pub always_allow: Vec<String>,
    #[serde(default)]
    pub always_ask: Vec<String>,
    #[serde(default)]
    pub denied: Vec<String>,
    /// 使用次数统计
    #[serde(default)]
    pub usage_count: HashMap<String, u32>,
}

impl PermissionList {
    pub fn load() -> Self {
        let path = list_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            Self::default_with_sensible_defaults()
        }
    }

    pub fn save(&self) -> Result<(), PermError> {
        let path = list_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(PermError::Io)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(PermError::Json)?;
        std::fs::write(&path, text).map_err(PermError::Io)?;
        Ok(())
    }

    pub fn default_with_sensible_defaults() -> Self {
        Self {
            always_allow: vec![
                "Finder".into(),
                "Explorer".into(),
                "Notepad".into(),
                "TextEdit".into(),
                "Safari".into(),
                "Chrome".into(),
                "Firefox".into(),
                "VSCode".into(),
                "Terminal".into(),
            ],
            always_ask: vec![
                "WeChat".into(),
                "com.tencent.xinWeChat".into(),
                "DingTalk".into(),
                "Lark".into(),
                "Feishu".into(),
            ],
            denied: Vec::new(),
            usage_count: HashMap::new(),
        }
    }

    /// 决策（结合黑名单 + 白名单）
    pub fn decide(&self, app: &AppMeta) -> PermissionDecision {
        // 1. 强制黑名单
        if is_blacklisted(app) {
            return PermissionDecision::Deny;
        }

        let keys = keys_for(app);
        // 2. denied
        if keys.iter().any(|k| self.denied.contains(k)) {
            return PermissionDecision::Deny;
        }
        // 3. always_allow
        if keys.iter().any(|k| self.always_allow.contains(k)) {
            return PermissionDecision::Allow;
        }
        // 4. always_ask
        if keys.iter().any(|k| self.always_ask.contains(k)) {
            return PermissionDecision::Ask;
        }
        // 5. 默认：弹窗
        PermissionDecision::Ask
    }

    pub fn add_allow(&mut self, key: String) {
        if !self.always_allow.contains(&key) {
            self.always_allow.push(key.clone());
        }
        if let Some(idx) = self.always_ask.iter().position(|x| x == &key) {
            self.always_ask.remove(idx);
        }
        *self.usage_count.entry(key).or_insert(0) += 1;
    }

    pub fn add_deny(&mut self, key: String) {
        if !self.denied.contains(&key) {
            self.denied.push(key);
        }
    }

    pub fn clear_allow(&mut self) {
        self.always_allow.clear();
        self.usage_count.clear();
    }
}

fn keys_for(app: &AppMeta) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(bid) = &app.bundle_id {
        keys.push(bid.clone());
    }
    keys.push(app.display_name.clone());
    keys.push(app.process_name.clone());
    keys
}

fn list_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".agentshell")
        .join("desktop-apps.json")
}

/// 强制黑名单（5.5.7）
pub fn is_blacklisted(app: &AppMeta) -> bool {
    let bid = app.bundle_id.as_deref().unwrap_or("").to_lowercase();
    let name = app.display_name.to_lowercase();
    let proc = app.process_name.to_lowercase();

    let blacklisted_patterns: &[&str] = &[
        // 银行
        "bank", "icbc", "ccb", "boc", "abc", "cmb", "cmbc",
        // 支付
        "alipay", "wepay", "wechatpay",
        // 证券
        "securities", "stock", "futures",
        // 密码管理
        "1password", "lastpass", "bitwarden", "keepass",
        // 2FA
        "yubico", "authy", "authenticator", "2fa", "google-authenticator",
    ];

    for pat in blacklisted_patterns {
        if bid.contains(pat) || name.contains(pat) || proc.contains(pat) {
            return true;
        }
    }
    false
}

#[derive(Debug, thiserror::Error)]
pub enum PermError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slack() -> AppMeta {
        AppMeta {
            bundle_id: Some("com.tinyspeck.chatlyio".into()),
            display_name: "Slack".into(),
            process_name: "Slack".into(),
            platform: "macos".into(),
        }
    }

    fn icbc() -> AppMeta {
        AppMeta {
            bundle_id: Some("com.bank.icbc".into()),
            display_name: "中国工商银行".into(),
            process_name: "ICBC".into(),
            platform: "macos".into(),
        }
    }

    #[test]
    fn test_allow_list() {
        let list = PermissionList::load();
        let d = list.decide(&slack());
        // Slack 不在默认 always_allow → ask
        assert!(matches!(d, PermissionDecision::Ask | PermissionDecision::Allow));
    }

    #[test]
    fn test_blacklist_blocks() {
        let list = PermissionList::load();
        let d = list.decide(&icbc());
        assert_eq!(d, PermissionDecision::Deny);
    }

    #[test]
    fn test_1password_blacklisted() {
        let app = AppMeta {
            bundle_id: Some("com.agilebits.onepassword".into()),
            display_name: "1Password".into(),
            process_name: "1Password".into(),
            platform: "macos".into(),
        };
        assert!(is_blacklisted(&app));
    }

    #[test]
    fn test_add_allow_moves_from_ask() {
        let mut list = PermissionList::load();
        list.add_allow("Slack".into());
        let d = list.decide(&slack());
        assert_eq!(d, PermissionDecision::Allow);
    }

    #[test]
    fn test_clear_allow() {
        let mut list = PermissionList::load();
        list.clear_allow();
        assert_eq!(list.always_allow.len(), 0);
    }

    #[test]
    fn test_default_has_sensible_apps() {
        let list = PermissionList::default_with_sensible_defaults();
        assert!(list.always_allow.contains(&"Finder".to_string()));
        assert!(list.always_allow.contains(&"VSCode".to_string()));
    }
}
