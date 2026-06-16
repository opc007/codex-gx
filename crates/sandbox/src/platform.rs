//! 平台检测

/// 操作系统平台
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// macOS
    Macos,
    /// Windows
    Windows,
    /// Linux
    Linux,
    /// 其他
    Other,
}

impl Platform {
    /// 显示名
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Other => "other",
        }
    }
}

/// 当前平台
pub fn current_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "linux") {
        Platform::Linux
    } else {
        Platform::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform() {
        let p = current_platform();
        // 在 macOS 开发环境下
        assert_eq!(p, Platform::Macos);
    }
}