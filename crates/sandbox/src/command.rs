//! 生成沙箱命令
//!
//! 设计参考：docs/开发文档.md §5.5.7 macOS sandbox-exec

use std::path::Path;

use crate::policy::SandboxPolicy;

/// 沙箱模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    /// 完全沙箱（默认）
    Strict,
    /// 工作目录可写
    WorkspaceWrite,
    /// 全部可访问（仅审批模式用）
    FullAccess,
    /// 不使用沙箱
    None,
}

/// 为给定命令构造包装（macOS sandbox-exec）
pub fn build_sandbox_command(
    policy: &SandboxPolicy,
    mode: SandboxMode,
    cmd: &str,
    args: &[&str],
    cwd: &Path,
) -> Vec<String> {
    match mode {
        SandboxMode::None => {
            let mut out = vec![cmd.to_string()];
            out.extend(args.iter().map(|s| s.to_string()));
            out
        }
        SandboxMode::FullAccess => {
            // 不包 sandbox-exec
            let mut out = vec![cmd.to_string()];
            out.extend(args.iter().map(|s| s.to_string()));
            out
        }
        _ => {
            let profile = generate_seatbelt_profile(policy, mode, cwd);
            let mut out = vec![
                "sandbox-exec".to_string(),
                "-p".to_string(),
                profile,
                cmd.to_string(),
            ];
            out.extend(args.iter().map(|s| s.to_string()));
            out
        }
    }
}

/// 生成 Seatbelt profile 文本
pub fn generate_seatbelt_profile(policy: &SandboxPolicy, mode: SandboxMode, cwd: &Path) -> String {
    let mut s = String::new();
    s.push_str("(version 1)\n");
    s.push_str("(deny default)\n");

    // 基础只读
    s.push_str("(allow process-exec)\n");
    s.push_str("(allow process-fork)\n");
    s.push_str("(allow sysctl-read)\n");
    s.push_str("(allow file-read*)\n");
    s.push_str("(allow file-write* file-write-data file-write-create)\n");

    // cwd
    let cwd_str = cwd.to_string_lossy();
    s.push_str(&format!(
        "(allow file-read* file-write* (subpath \"{}/\"))\n",
        cwd_str
    ));

    // /tmp
    s.push_str("(allow file-read* file-write* (subpath \"/tmp/\"))\n");
    s.push_str("(allow file-read* file-write* (subpath \"/private/tmp/\"))\n");
    s.push_str("(allow file-read* file-write* (subpath \"/var/folders/\"))\n");

    // 文件系统规则
    for rule in &policy.filesystem {
        if !rule.allow {
            continue;
        }
        if rule.writable {
            s.push_str(&format!(
                "(allow file-read* file-write* (subpath \"{}/\"))\n",
                rule.path.trim_end_matches("/*").trim_end_matches("/**")
            ));
        } else {
            s.push_str(&format!(
                "(allow file-read* (subpath \"{}/\"))\n",
                rule.path.trim_end_matches("/*").trim_end_matches("/**")
            ));
        }
    }

    // 网络
    match &policy.network {
        crate::policy::NetworkRule::AllowAll => {
            s.push_str("(allow network*)\n");
        }
        crate::policy::NetworkRule::DenyAll => {
            // 默认 deny 已经处理
        }
        crate::policy::NetworkRule::AllowDomains { .. } => {
            // 简化版：允许所有出站（v0.1 不做域名过滤，留 TODO）
            s.push_str("(allow network-outbound)\n");
        }
    }

    // WorkspaceWrite 模式特殊处理
    if mode == SandboxMode::WorkspaceWrite {
        s.push_str(&format!(
            "(allow file-read* file-write* (subpath \"{}/\"))\n",
            cwd_str
        ));
    }

    // 进程
    if policy.allow_subprocesses {
        s.push_str("(allow process-exec process-fork)\n");
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::NetworkRule;

    #[test]
    fn test_seatbelt_profile_basic() {
        let p = SandboxPolicy::default();
        let s = generate_seatbelt_profile(&p, SandboxMode::Strict, Path::new("/tmp/work"));
        assert!(s.contains("(version 1)"));
        assert!(s.contains("(deny default)"));
        assert!(s.contains("/tmp/work/"));
    }

    #[test]
    fn test_command_construct() {
        let p = SandboxPolicy::default();
        let cmd = build_sandbox_command(&p, SandboxMode::Strict, "ls", &["-la"], Path::new("/tmp"));
        assert_eq!(cmd[0], "sandbox-exec");
        assert_eq!(cmd[1], "-p");
        assert!(cmd[2].contains("version 1"));
        assert_eq!(cmd[3], "ls");
        assert_eq!(cmd[4], "-la");
    }

    #[test]
    fn test_no_sandbox_mode() {
        let p = SandboxPolicy::default();
        let cmd = build_sandbox_command(&p, SandboxMode::None, "ls", &["-la"], Path::new("/tmp"));
        assert_eq!(cmd[0], "ls");
        assert_eq!(cmd[1], "-la");
    }

    #[test]
    fn test_network_allow_all() {
        let p = SandboxPolicy {
            network: NetworkRule::AllowAll,
            ..Default::default()
        };
        let s = generate_seatbelt_profile(&p, SandboxMode::Strict, Path::new("/tmp"));
        assert!(s.contains("network"));
    }
}
