//! 沙箱策略

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 文件系统规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemRule {
    /// 路径（glob 风格）
    pub path: String,
    /// 允许 / 拒绝
    pub allow: bool,
    /// 是否可写
    #[serde(default)]
    pub writable: bool,
}

/// 网络规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NetworkRule {
    /// 允许所有出站
    AllowAll,
    /// 完全禁止
    DenyAll,
    /// 仅允许指定域名
    AllowDomains {
        /// 允许的域
        domains: Vec<String>,
    },
}

/// 沙箱策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// 网络规则
    #[serde(default = "default_network")]
    pub network: NetworkRule,
    /// 文件系统规则
    #[serde(default)]
    pub filesystem: Vec<FilesystemRule>,
    /// 工作目录（cwd）是否可写
    #[serde(default = "default_true")]
    pub cwd_writable: bool,
    /// 是否允许子进程
    #[serde(default = "default_true")]
    pub allow_subprocesses: bool,
}

fn default_network() -> NetworkRule {
    NetworkRule::DenyAll
}
fn default_true() -> bool {
    true
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            network: NetworkRule::DenyAll,
            filesystem: vec![
                FilesystemRule {
                    path: "/tmp/**".into(),
                    allow: true,
                    writable: true,
                },
            ],
            cwd_writable: true,
            allow_subprocesses: true,
        }
    }
}

/// 决策
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// 允许
    Allow,
    /// 拒绝
    Deny,
}

#[derive(Debug, Error)]
pub enum LoadPolicyError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

impl SandboxPolicy {
    /// 从 TOML 加载
    pub fn from_toml(text: &str) -> Result<Self, LoadPolicyError> {
        Ok(toml::from_str(text)?)
    }

    /// 从文件加载
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, LoadPolicyError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text)
    }

    /// 决策：路径是否允许写
    pub fn can_write(&self, path: &std::path::Path) -> Decision {
        for rule in &self.filesystem {
            if matches_glob(&rule.path, path) {
                return if rule.allow && rule.writable {
                    Decision::Allow
                } else {
                    Decision::Deny
                };
            }
        }
        // 默认：cwd 可写
        if self.cwd_writable {
            if let Some(cwd) = std::env::current_dir().ok() {
                if path.starts_with(&cwd) {
                    return Decision::Allow;
                }
            }
        }
        // 完全不在任何规则里 + 不在 cwd → 拒绝
        Decision::Deny
    }

    /// 决策：路径是否允许读
    pub fn can_read(&self, path: &std::path::Path) -> Decision {
        for rule in &self.filesystem {
            if matches_glob(&rule.path, path) {
                return if rule.allow {
                    Decision::Allow
                } else {
                    Decision::Deny
                };
            }
        }
        // 默认：cwd 可读
        if let Some(cwd) = std::env::current_dir().ok() {
            if path.starts_with(&cwd) {
                return Decision::Allow;
            }
        }
        Decision::Deny
    }
}

/// 简单的 glob 匹配（支持 `*` 和 `**`）
fn matches_glob(pattern: &str, path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    if pattern == path_str.as_ref() {
        return true;
    }
    // /tmp/** 匹配 /tmp/foo 和 /tmp/foo/bar（多级）
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path_str.starts_with(prefix);
    }
    // /tmp/* 匹配 /tmp/foo（单级，不匹配 /tmp/foo/bar）
    if let Some(prefix) = pattern.strip_suffix("/*") {
        if let Some(rest) = path_str.strip_prefix(prefix) {
            return rest.starts_with('/')
                && !rest.contains("/../")
                && rest != "/"
                && !rest[1..].contains('/');
        }
    }
    // 文件名匹配（不区分路径）
    if let Some(filename) = path.file_name() {
        if pattern == "*" || pattern == filename.to_string_lossy() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let p = SandboxPolicy::default();
        assert!(matches!(p.network, NetworkRule::DenyAll));
        assert!(p.cwd_writable);
    }

    #[test]
    fn test_toml_parse() {
        let toml = r#"
            [network]
            type = "allow_domains"
            domains = ["api.openai.com", "*.anthropic.com"]

            [[filesystem]]
            path = "/tmp/*"
            allow = true
            writable = true

            [[filesystem]]
            path = "/etc"
            allow = false
            writable = false
        "#;
        let p = SandboxPolicy::from_toml(toml).unwrap();
        match p.network {
            NetworkRule::AllowDomains { domains } => {
                assert_eq!(domains.len(), 2);
            }
            _ => panic!("wrong network rule"),
        }
        assert_eq!(p.filesystem.len(), 2);
    }

    #[test]
    fn test_can_write() {
        let p = SandboxPolicy::default();
        assert_eq!(p.can_write(std::path::Path::new("/tmp/foo")), Decision::Allow);
        assert_eq!(p.can_write(std::path::Path::new("/etc/passwd")), Decision::Deny);
        assert_eq!(p.can_write(std::path::Path::new("/var/log/foo")), Decision::Deny);
    }

    #[test]
    fn test_glob_double_star() {
        assert!(matches_glob("/tmp/**", std::path::Path::new("/tmp/foo/bar")));
        assert!(matches_glob("/tmp/**", std::path::Path::new("/tmp/x")));
    }

    #[test]
    fn test_glob_single_star() {
        assert!(matches_glob("/tmp/*", std::path::Path::new("/tmp/foo")));
        assert!(!matches_glob("/tmp/*", std::path::Path::new("/tmp/foo/bar")));
    }
}