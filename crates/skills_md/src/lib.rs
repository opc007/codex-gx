//! v1.7：SKILL.md 开放标准
//!
//! 设计参考：docs/开发文档.md §5.21
//!
//! ## 格式（兼容 Codex 2026 / Claude Code / Cursor）
//! ```markdown
//! ---
//! name: pr-review
//! description: |
//!   对当前 PR 跑 6 维度 code review。
//! triggers:
//!   - "review this PR"
//!   - "check my code"
//! ---
//!
//! # PR Review Skill
//!
//! ## When to use
//! ...
//! ```
//!
//! ## 三类来源
//! - builtin (内置)
//! - user (`~/.agentshell/skills/`)
//! - project (`<cwd>/.agentshell/skills/`)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// SKILL.md frontmatter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillFrontmatter {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    /// 可选字段
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// trusted | untrusted
    #[serde(default)]
    pub trust: Option<String>,
}

/// 完整 skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub source: SkillSource,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Builtin,
    User,
    Project,
    Untrusted,
}

impl SkillSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Builtin => "🟢 builtin",
            Self::User => "🟢 user",
            Self::Project => "🟡 project",
            Self::Untrusted => "🟠 untrusted",
        }
    }
}

/// 解析错误
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("frontmatter 格式错误: {0}")]
    Frontmatter(String),
    #[error("yaml 解析错误: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("路径展开错误: {0}")]
    Walk(String),
}

/// 解析单个 SKILL.md 文件
pub fn parse_skill_file(path: &Path) -> Result<Skill, SkillError> {
    let text = std::fs::read_to_string(path)?;
    parse_skill_text(&text, path)
}

/// 解析 SKILL.md 文本
pub fn parse_skill_text(text: &str, path: &Path) -> Result<Skill, SkillError> {
    // 找 `---` 分隔
    let text = text.trim_start();
    let (frontmatter_str, body) = if let Some(rest) = text.strip_prefix("---") {
        let mut lines = rest.lines();
        let mut fm = String::new();
        let mut found_end = false;
        for line in lines.by_ref() {
            if line.trim() == "---" {
                found_end = true;
                break;
            }
            fm.push_str(line);
            fm.push('\n');
        }
        if !found_end {
            return Err(SkillError::Frontmatter(
                "missing closing `---`".to_string(),
            ));
        }
        let body: String = lines.collect::<Vec<_>>().join("\n");
        (fm, body)
    } else {
        return Err(SkillError::Frontmatter(
            "missing opening `---`".to_string(),
        ));
    };

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)
        .map_err(|e| SkillError::Frontmatter(e.to_string()))?;

    Ok(Skill {
        frontmatter,
        body: body.trim().to_string(),
        source: SkillSource::User, // 调用方覆盖
        path: path.to_path_buf(),
    })
}

/// 扫描目录找所有 SKILL.md
pub fn scan_dir(dir: &Path) -> Result<Vec<Skill>, SkillError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("SKILL.md") {
            match parse_skill_file(path) {
                Ok(mut s) => {
                    // 根据目录来源打标
                    s.source = if path.starts_with("builtin") {
                        SkillSource::Builtin
                    } else if path
                        .components()
                        .any(|c| c.as_os_str() == ".agentshell")
                    {
                        // 项目级（含 .agentshell/skills/）— 但也可能是 user（~/.agentshell/skills/）
                        let s_str = path.to_string_lossy();
                        if s_str.contains("/projects/") {
                            SkillSource::Project
                        } else {
                            SkillSource::User
                        }
                    } else {
                        SkillSource::User
                    };
                    out.push(s);
                }
                Err(e) => {
                    eprintln!("[skills_md] parse failed {}: {}", path.display(), e);
                }
            }
        }
    }
    Ok(out)
}

/// 索引（按 name）
pub struct SkillIndex {
    by_name: HashMap<String, Skill>,
}

impl SkillIndex {
    pub fn from_skills(skills: Vec<Skill>) -> Self {
        let mut by_name = HashMap::new();
        for s in skills {
            by_name.insert(s.frontmatter.name.clone(), s);
        }
        Self { by_name }
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.by_name.get(name)
    }

    pub fn list(&self) -> Vec<&Skill> {
        let mut v: Vec<&Skill> = self.by_name.values().collect();
        v.sort_by(|a, b| a.frontmatter.name.cmp(&b.frontmatter.name));
        v
    }

    /// 模糊匹配（triggers + name + description）
    pub fn match_prompt(&self, prompt: &str) -> Option<&Skill> {
        let p = prompt.to_lowercase();
        // 1. triggers 命中
        for s in self.by_name.values() {
            for t in &s.frontmatter.triggers {
                if p.contains(&t.to_lowercase()) {
                    return Some(s);
                }
            }
        }
        // 2. name 命中
        for s in self.by_name.values() {
            if p.contains(&s.frontmatter.name.to_lowercase()) {
                return Some(s);
            }
        }
        // 3. description 关键字（弱匹配）
        for s in self.by_name.values() {
            let keywords: Vec<&str> = s
                .frontmatter
                .description
                .split_whitespace()
                .filter(|w| w.len() > 4)
                .take(20)
                .collect();
            for kw in keywords {
                if p.contains(&kw.to_lowercase()) {
                    return Some(s);
                }
            }
        }
        None
    }
}

/// 默认目录约定
pub fn builtin_skills_dir() -> PathBuf {
    // crate 内置 skills（编译时嵌入或运行时找）
    // 这里用项目根 docs/skills/ 占位
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.join("docs").join("skills")
}

pub fn user_skills_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".agentshell").join("skills")
}

pub fn project_skills_dir(cwd: &Path) -> PathBuf {
    cwd.join(".agentshell").join("skills")
}

/// 加载所有来源的 skills（builtin + user + project）
pub fn load_all() -> Result<SkillIndex, SkillError> {
    let mut all = Vec::new();
    all.extend(scan_dir(&builtin_skills_dir())?);
    all.extend(scan_dir(&user_skills_dir())?);
    if let Ok(cwd) = std::env::current_dir() {
        all.extend(scan_dir(&project_skills_dir(&cwd))?);
    }
    Ok(SkillIndex::from_skills(all))
}

/// 把 skill 渲染成 system prompt addon
pub fn skill_prompt_addon(s: &Skill) -> String {
    format!(
        "[Active Skill: {}]\n{}\n\n---\n\n{}",
        s.frontmatter.name, s.frontmatter.description, s.body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "skills_md_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("SKILL.md");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        p
    }

    #[test]
    fn test_parse_skill() {
        let text = r#"---
name: pr-review
description: Review code for readability, security, and tests.
triggers:
  - "review this PR"
  - "check my code"
author: test
---

# PR Review

## What to do
1. Run `git diff main...HEAD`
2. Score 0-5 on: readability, security, performance, tests
"#;
        let p = write_tmp(text);
        let s = parse_skill_text(text, &p).unwrap();
        assert_eq!(s.frontmatter.name, "pr-review");
        assert_eq!(s.frontmatter.triggers.len(), 2);
        assert!(s.body.contains("git diff"));
    }

    #[test]
    fn test_match_triggers() {
        let text = r#"---
name: pr-review
description: Review code
triggers:
  - "review this PR"
---
body
"#;
        let p = write_tmp(text);
        let s = parse_skill_text(text, &p).unwrap();
        let idx = SkillIndex::from_skills(vec![s]);
        let m = idx.match_prompt("please review this PR");
        assert!(m.is_some());
        assert_eq!(m.unwrap().frontmatter.name, "pr-review");
    }

    #[test]
    fn test_no_frontmatter_error() {
        let r = parse_skill_text("no frontmatter here", Path::new("/tmp/x"));
        assert!(r.is_err());
    }
}
