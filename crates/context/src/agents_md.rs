//! AGENTS.md 文件加载
//!
//! 设计参考：docs/开发文档.md §5.9 AGENTS.md
//!
//! 约定：
//! - 从 cwd 向上逐层找 AGENTS.md
//! - 也支持当前目录的 `AGENTS.override.md`
//! - 找到后注入到 system prompt

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsMd {
    /// 各级目录合并后的内容
    pub content: String,
    /// 来自哪些文件
    pub sources: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// 从 cwd 向上找 AGENTS.md（最多 8 层）
pub fn load_agents_md(cwd: &Path) -> Result<AgentsMd, LoadError> {
    let mut combined = String::new();
    let mut sources = Vec::new();

    let mut current = cwd.to_path_buf();
    for _ in 0..8 {
        let candidates = ["AGENTS.md", "AGENTS.override.md", "agents.md"];
        for c in &candidates {
            let p = current.join(c);
            if p.is_file() {
                let content = std::fs::read_to_string(&p)?;
                if !combined.is_empty() {
                    combined.push_str("\n\n---\n\n");
                }
                combined.push_str(&format!("# {}\n\n", p.display()));
                combined.push_str(&content);
                sources.push(p.to_string_lossy().to_string());
            }
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }

    Ok(AgentsMd {
        content: combined,
        sources,
    })
}

/// 注入到 system message
pub fn inject_into_system(existing: &str, agents_md: &AgentsMd, cwd: &Path) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "You are AgentShell, an AI coding agent running in `{}`.\n\n",
        cwd.display()
    ));
    if !existing.is_empty() {
        out.push_str(existing);
        out.push_str("\n\n");
    }
    if !agents_md.content.is_empty() {
        out.push_str("# Repository Guidelines (AGENTS.md)\n\n");
        out.push_str(&agents_md.content);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_agents_md() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("AGENTS.md"),
            "# Project rules\nUse Rust 2021 edition.",
        )
        .unwrap();
        let a = load_agents_md(dir.path()).unwrap();
        assert!(a.content.contains("Rust 2021"));
        // sources 至少包含我们写的那个（可能上层目录也有同名文件）
        assert!(!a.sources.is_empty());
        assert!(a.sources.iter().any(|s| s.contains("AGENTS.md")));
    }

    #[test]
    fn test_load_agents_md_hierarchy() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# Top\nBe polite.").unwrap();
        std::fs::write(sub.join("AGENTS.md"), "# Sub\nUse snake_case.").unwrap();

        let a = load_agents_md(&sub).unwrap();
        // 应该包含两层
        assert!(a.content.contains("Be polite"));
        assert!(a.content.contains("snake_case"));
        // sources 至少 2 个
        assert!(a.sources.len() >= 2);
    }

    #[test]
    fn test_no_agents_md() {
        let dir = tempdir().unwrap();
        // 用一个绝对不会有 AGENTS.md 的目录
        let isolated = dir.path().join("isolated");
        std::fs::create_dir(&isolated).unwrap();
        let a = load_agents_md(&isolated).unwrap();
        // 注：向上找可能找到祖先目录的 AGENTS.md，所以不强求 0
        // 只检查 isolated 自己没创建文件时 sources 不应该包含 isolated 路径
        for s in &a.sources {
            assert!(!s.contains("isolated"));
        }
    }

    #[test]
    fn test_inject_into_system() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "Use Rust 2021 edition.").unwrap();
        let a = load_agents_md(dir.path()).unwrap();
        let s = inject_into_system("You are helpful.", &a, dir.path());
        assert!(s.contains("AgentShell"));
        assert!(s.contains("You are helpful"));
        assert!(s.contains("Use Rust 2021"));
    }
}
