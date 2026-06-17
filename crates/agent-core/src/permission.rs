//! v1.2：工具权限级别
//!
//! 工具执行前按权限分级：
//! - **Safe**: 无需用户确认（grep / read_file / glob）
//! - **Moderate**: 在严格模式（strict）下需确认（bash 一般命令 / write_file）
//! - **Dangerous**: 总是需用户确认 + 显示命令（rm / chmod / 网络下载）
//! - **Blocked**: 永远禁止（`rm -rf /` / 写 system path / `sudo`）

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    Safe,
    Moderate,
    Dangerous,
    Blocked,
}

impl PermissionLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionLevel::Safe => "safe",
            PermissionLevel::Moderate => "moderate",
            PermissionLevel::Dangerous => "dangerous",
            PermissionLevel::Blocked => "blocked",
        }
    }

    /// 是否需要用户确认
    pub fn requires_approval(&self, strict: bool) -> bool {
        match self {
            PermissionLevel::Safe => false,
            PermissionLevel::Moderate => strict,
            PermissionLevel::Dangerous => true,
            PermissionLevel::Blocked => true, // 会被特殊处理为拒绝
        }
    }
}

impl Default for PermissionLevel {
    fn default() -> Self {
        PermissionLevel::Safe
    }
}

/// 默认的危险命令模式（用于 bash 工具）
pub const DEFAULT_BASH_RISK_PATTERNS: &[&str] = &[
    r"\brm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+)?-[a-zA-Z]*r[a-zA-Z]*\s+/",
    r"\bsudo\b",
    r"\bchmod\s+777\b",
    r"\bchown\s+-R\s+root\b",
    r"\bmkfs\b",
    r"\bdd\s+if=.*of=/dev/",
    r"\bcurl\s+.*\|\s*(sh|bash)\b",
    r"\bwget\s+.*\|\s*(sh|bash)\b",
    r">\s*/etc/",
    r">\s*/System/",
    r">\s*/usr/",
    r">\s*/var/",
    r"\bdiskutil\s+(erase|partition)\b",
    r"\bformat\s+",
    r"\bnc\s+-l\b",
];

/// 默认的禁止写入路径（用于 write_file 工具）
pub const DEFAULT_BLOCKED_PATHS: &[&str] = &[
    "/etc/",
    "/usr/",
    "/var/",
    "/System/",
    "/bin/",
    "/sbin/",
    "C:\\Windows\\",
    "C:\\Program Files\\",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_strings() {
        assert_eq!(PermissionLevel::Safe.as_str(), "safe");
        assert_eq!(PermissionLevel::Moderate.as_str(), "moderate");
        assert_eq!(PermissionLevel::Dangerous.as_str(), "dangerous");
        assert_eq!(PermissionLevel::Blocked.as_str(), "blocked");
    }

    #[test]
    fn requires_approval_safe() {
        assert!(!PermissionLevel::Safe.requires_approval(false));
        assert!(!PermissionLevel::Safe.requires_approval(true));
    }

    #[test]
    fn requires_approval_moderate() {
        assert!(!PermissionLevel::Moderate.requires_approval(false));
        assert!(PermissionLevel::Moderate.requires_approval(true));
    }

    #[test]
    fn requires_approval_dangerous() {
        assert!(PermissionLevel::Dangerous.requires_approval(false));
        assert!(PermissionLevel::Dangerous.requires_approval(true));
    }

    #[test]
    fn requires_approval_blocked() {
        assert!(PermissionLevel::Blocked.requires_approval(false));
    }

    #[test]
    fn default_is_safe() {
        assert_eq!(PermissionLevel::default(), PermissionLevel::Safe);
    }

    #[test]
    fn risk_patterns_non_empty() {
        assert!(!DEFAULT_BASH_RISK_PATTERNS.is_empty());
        assert!(DEFAULT_BASH_RISK_PATTERNS.iter().any(|p| p.contains("sudo")));
    }

    #[test]
    fn blocked_paths_includes_system() {
        assert!(DEFAULT_BLOCKED_PATHS.iter().any(|p| p.contains("System")));
    }
}