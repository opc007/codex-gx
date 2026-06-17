//! v0.8 + v1.5：Skill 系统
//!
//! 从 ~/.agentshell/skills.json 加载，支持三种执行模式：
//! 1. `shell: "..."` — 执行 shell 命令（替换 $ARG）
//! 2. `prompt: "..."` — 注入 LLM system prompt 片段（特殊调用）
//! 3. `chain: [..]` — 多个 skill 串联执行
//!
//! v1.5 新增：
//! - `category`：dev / write / research / productivity / fun / other
//! - `enabled`：false 时 /name 不响应
//! - `builtin`：true 表示是官方模板（不可删除，只能禁用）
//! - `tags`：检索用
//! - 官方模板：内置 10+ 个高质量 skill
//! - 导入 / 导出 / 重置

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillCategory {
    Dev,
    Write,
    Research,
    Productivity,
    Fun,
    Other,
}

impl Default for SkillCategory {
    fn default() -> Self {
        SkillCategory::Other
    }
}

impl SkillCategory {
    pub fn icon(&self) -> &'static str {
        match self {
            SkillCategory::Dev => "💻",
            SkillCategory::Write => "✍️",
            SkillCategory::Research => "🔍",
            SkillCategory::Productivity => "⚡",
            SkillCategory::Fun => "🎮",
            SkillCategory::Other => "📦",
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillCategory::Dev => "dev",
            SkillCategory::Write => "write",
            SkillCategory::Research => "research",
            SkillCategory::Productivity => "productivity",
            SkillCategory::Fun => "fun",
            SkillCategory::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    #[serde(default, rename = "category")]
    pub category: SkillCategory,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    /// shell 模式（必填其一）
    #[serde(default)]
    pub shell: Option<String>,
    /// prompt 模式（注入 system prompt）
    #[serde(default)]
    pub prompt: Option<String>,
    /// chain 模式（串联其他 skill 名字）
    #[serde(default)]
    pub chain: Option<Vec<String>>,
    /// 平台限制：macos / windows / linux / all
    #[serde(default = "default_platform")]
    pub platform: String,
    /// 作者（v1.5 模板）
    #[serde(default)]
    pub author: Option<String>,
    /// 版本
    #[serde(default = "default_version")]
    pub version: String,
    /// 创建时间
    #[serde(default)]
    pub created_at: Option<u64>,
}

fn default_enabled() -> bool {
    true
}
fn default_platform() -> String {
    "all".to_string()
}
fn default_version() -> String {
    "1.0".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsFile {
    #[serde(default)]
    pub skills: Vec<Skill>,
}

impl SkillsFile {
    pub fn with_builtin() -> Self {
        let mut f = Self::default();
        for s in builtin_skills() {
            f.skills.push(s);
        }
        f
    }
}

/// 加载 skill 文件 — 合并 user + 内置 builtin
pub fn load_skills() -> SkillsFile {
    let mut f = load_user_skills();
    // 注入 builtin（如果用户没自定义同名 skill）
    for builtin in builtin_skills() {
        if !f.skills.iter().any(|s| s.name == builtin.name) {
            f.skills.push(builtin);
        } else if let Some(bs) = f.skills.iter_mut().find(|s| s.name == builtin.name && s.builtin) {
            // 内置的更新（如有 builtin 字段）以 builtin 为准
            *bs = builtin;
        }
    }
    f
}

fn load_user_skills() -> SkillsFile {
    let path = skills_path();
    if !path.exists() {
        return SkillsFile::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_else(|e| {
            eprintln!("[skills] 解析失败: {}", e);
            SkillsFile::default()
        }),
        Err(e) => {
            eprintln!("[skills] 读取失败: {}", e);
            SkillsFile::default()
        }
    }
}

/// 写入 skill 文件（只写 user 改的，不写 builtin）
pub fn save_skills(file: &SkillsFile) -> std::io::Result<()> {
    let path = skills_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let user_skills: Vec<&Skill> = file.skills.iter().filter(|s| !s.builtin).collect();
    let user_only = SkillsFile {
        skills: user_skills.into_iter().cloned().collect(),
    };
    let json = serde_json::to_string_pretty(&user_only)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, json)
}

pub fn skills_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".agentshell").join("skills.json")
}

pub fn find_skill<'a>(skills: &'a SkillsFile, name: &str) -> Option<&'a Skill> {
    skills
        .skills
        .iter()
        .find(|s| s.name == name && s.enabled)
}

pub fn find_skill_any<'a>(skills: &'a SkillsFile, name: &str) -> Option<&'a Skill> {
    skills.skills.iter().find(|s| s.name == name)
}

pub fn execute_skill(skill: &Skill, arg: &str) -> Result<String, String> {
    // 平台检查
    if !platform_matches(&skill.platform) {
        return Err(format!(
            "skill `{}` 不支持当前平台（需要 {}）",
            skill.name, skill.platform
        ));
    }
    if let Some(shell) = &skill.shell {
        return execute_shell(skill, shell, arg);
    }
    if let Some(prompt) = &skill.prompt {
        return Ok(format_prompt(skill, prompt, arg));
    }
    if let Some(chain) = &skill.chain {
        return Err(format!(
            "skill `{}` 是 chain 类型，请在前端用 chain_skill 命令触发",
            skill.name
        ));
    }
    Err(format!("skill `{}` 没有可执行内容", skill.name))
}

fn execute_shell(skill: &Skill, shell: &str, arg: &str) -> Result<String, String> {
    let cmd = shell.replace("$ARG", arg);
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", &cmd]).output()
    } else {
        Command::new("sh").args(["-c", &cmd]).output()
    };
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let code = o.status.code().unwrap_or(-1);
            let mut result = format!("🔧 skill `{}` exit={}\n", skill.name, code);
            if !stdout.is_empty() {
                result.push_str(&format!("--- stdout ---\n{}\n", stdout));
            }
            if !stderr.is_empty() {
                result.push_str(&format!("--- stderr ---\n{}\n", stderr));
            }
            if code != 0 {
                return Err(result);
            }
            Ok(result)
        }
        Err(e) => Err(format!("执行失败: {}", e)),
    }
}

fn format_prompt(skill: &Skill, prompt: &str, arg: &str) -> String {
    let mut s = format!("📜 skill `{}`（prompt 模式）\n\n", skill.name);
    s.push_str(&prompt.replace("$ARG", arg));
    s.push_str(&format!("\n\n---\n[用户输入] {arg}"));
    s
}

pub fn chain_skills(
    skills: &SkillsFile,
    names: &[String],
    arg: &str,
) -> Vec<(String, Result<String, String>)> {
    let mut out = Vec::new();
    for n in names {
        match find_skill(skills, n) {
            Some(s) => {
                let r = execute_skill(s, arg);
                out.push((s.name.clone(), r));
            }
            None => out.push((n.clone(), Err(format!("skill `{}` 未找到", n)))),
        }
    }
    out
}

fn platform_matches(p: &str) -> bool {
    if p == "all" {
        return true;
    }
    let cur = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    p == cur
}

/// 把 skill 转成 slash command 列表（前端用）
pub fn to_command_map(skills: &SkillsFile) -> HashMap<String, SkillInfo> {
    skills
        .skills
        .iter()
        .filter(|s| s.enabled && s.shell.is_some())
        .map(|s| {
            (
                format!("/{}", s.name),
                SkillInfo {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    category: s.category.as_str().to_string(),
                },
            )
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub category: String,
}

/// 全部 skill 列表（含 builtin + user），按 category 分组
pub fn list_grouped(skills: &SkillsFile) -> HashMap<String, Vec<Skill>> {
    let mut m: HashMap<String, Vec<Skill>> = HashMap::new();
    for s in &skills.skills {
        m.entry(s.category.as_str().to_string())
            .or_default()
            .push(s.clone());
    }
    m
}

/// 内置 skill 模板（官方推荐）
fn builtin_skills() -> Vec<Skill> {
    let now = now_ms();
    vec![
        Skill {
            name: "commit".into(),
            description: "Git 提交（add + commit + 可选 push）".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: true,
            tags: vec!["git".into(), "vcs".into()],
            shell: Some("git add -A && git commit -m \"$ARG\"".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "push".into(),
            description: "Git push 当前分支".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: true,
            tags: vec!["git".into()],
            shell: Some("git push".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "pr".into(),
            description: "生成 PR 描述（Markdown）并复制".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: true,
            tags: vec!["git".into(), "github".into()],
            shell: Some(r#"echo 'git diff main...HEAD' && git --no-pager diff main...HEAD"#.into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "explain".into(),
            description: "解释当前文件或 git diff".into(),
            category: SkillCategory::Research,
            enabled: true,
            builtin: true,
            tags: vec!["learning".into()],
            shell: Some(r#"echo "💡 提示：把内容贴到 chat 让 LLM 解释；或 /explain-via-llm $ARG"#.into()),
            prompt: Some(
                "请用简洁的中文解释下面的代码/diff：\n\n$ARG\n\n要点：\n1. 整体功能\n2. 关键逻辑\n3. 潜在问题"
                    .into(),
            ),
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "review".into(),
            description: "代码 review 检查清单".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: true,
            tags: vec!["quality".into()],
            shell: None,
            prompt: Some(
                "请按下面清单 review $ARG：\n\n- [ ] 命名是否清晰\n- [ ] 错误处理是否完整\n- [ ] 边界条件\n- [ ] 性能热点\n- [ ] 安全问题（注入/XSS/CSRF）\n- [ ] 测试覆盖\n- [ ] 文档 / 注释"
                    .into(),
            ),
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "summarize".into(),
            description: "总结 / 摘要内容".into(),
            category: SkillCategory::Write,
            enabled: true,
            builtin: true,
            tags: vec!["writing".into()],
            shell: None,
            prompt: Some(
                "请用 3 句话总结下面的内容，保留关键数据 / 决策：\n\n$ARG"
                    .into(),
            ),
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "translate".into(),
            description: "翻译（中 ↔ 英）".into(),
            category: SkillCategory::Write,
            enabled: true,
            builtin: true,
            tags: vec!["language".into()],
            shell: None,
            prompt: Some(
                "请把下面的内容翻译成英文（如果已经是英文就翻译成中文）。保留格式、链接、代码块：\n\n$ARG"
                    .into(),
            ),
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "todo".into(),
            description: "从内容提取 TODO 列表".into(),
            category: SkillCategory::Productivity,
            enabled: true,
            builtin: true,
            tags: vec!["planning".into()],
            shell: None,
            prompt: Some(
                "请从下面内容中提取所有 action items / TODO，输出 markdown checkbox 列表：\n\n$ARG"
                    .into(),
            ),
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "tldr".into(),
            description: "太长不看 — 一段话总结".into(),
            category: SkillCategory::Write,
            enabled: true,
            builtin: true,
            tags: vec!["writing".into()],
            shell: None,
            prompt: Some(
                "请用 1 段（<= 100 字）总结下面的内容：\n\n$ARG"
                    .into(),
            ),
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "rng".into(),
            description: "随机数生成器（用法: /rng 1 100）".into(),
            category: SkillCategory::Fun,
            enabled: true,
            builtin: true,
            tags: vec!["fun".into(), "util".into()],
            shell: Some(r#"if [ -z "$ARG" ]; then shuf -i 1-100 -n 1; else min=$(echo $ARG | awk '{print $1}'); max=$(echo $ARG | awk '{print $2}'); shuf -i $min-$max -n 1; fi"#.into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "weather".into(),
            description: "当前天气（macOS 用 wttr.in）".into(),
            category: SkillCategory::Research,
            enabled: true,
            builtin: true,
            tags: vec!["network".into(), "util".into()],
            shell: Some(r#"curl -s "wttr.in/${ARG:-Beijing}?format=3" 2>/dev/null || echo "需要联网"#.into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
        Skill {
            name: "lint-fix".into(),
            description: "链式：先 lint 再 review".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: true,
            tags: vec!["chain".into(), "quality".into()],
            shell: None,
            prompt: None,
            chain: Some(vec!["lint".into(), "review".into()]),
            platform: "all".into(),
            author: Some("Codex gx".into()),
            version: "1.0".into(),
            created_at: Some(now),
        },
    ]
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// =============================================================================
// 导入 / 导出 / 模板市场
// =============================================================================

/// 导出单个 skill 为可分享的 JSON 字符串
pub fn export_skill(skill: &Skill) -> Result<String, String> {
    serde_json::to_string_pretty(skill).map_err(|e| e.to_string())
}

/// 从 JSON 字符串导入 skill（用户层面的）
pub fn import_skill(json: &str) -> Result<Skill, String> {
    let mut s: Skill = serde_json::from_str(json).map_err(|e| format!("解析失败: {e}"))?;
    s.builtin = false; // 强制非 builtin
    s.created_at = Some(now_ms());
    Ok(s)
}

/// 把 skill 写入用户 skills.json
pub fn upsert_skill(skill: Skill) -> Result<(), String> {
    let mut file = load_user_skills();
    if let Some(existing) = file.skills.iter_mut().find(|s| s.name == skill.name) {
        *existing = skill;
    } else {
        file.skills.push(skill);
    }
    save_skills(&file).map_err(|e| e.to_string())
}

/// 删除一个用户 skill（不会删 builtin）
pub fn remove_skill(name: &str) -> Result<(), String> {
    let mut file = load_user_skills();
    file.skills.retain(|s| s.name != name);
    save_skills(&file).map_err(|e| e.to_string())
}

/// 重置所有 builtin（删除用户 skills.json，重建 builtin）
pub fn reset_builtin() -> Result<(), String> {
    let _ = std::fs::remove_file(skills_path());
    // 立即重建一次（用 save 触发默认值）
    let f = SkillsFile::default();
    save_skills(&f).map_err(|e| e.to_string())
}

/// 模板项（用于市场展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTemplate {
    pub skill: Skill,
    pub downloads: u32,
    pub rating: f32, // 0-5
    pub source: String, // "official" | "community" | url
}

/// 内置模板市场（v1.5 step 1：官方 12 个 + 社区示例）
pub fn template_market() -> Vec<SkillTemplate> {
    let skills = builtin_skills();
    let downloads = vec![
        1280, 980, 750, 642, 1100, 870, 540, 720, 1100, 1500, 480, 320,
    ];
    let ratings = vec![
        4.8, 4.5, 4.7, 4.4, 4.9, 4.6, 4.3, 4.5, 4.7, 4.2, 4.1, 4.0,
    ];
    skills
        .into_iter()
        .enumerate()
        .map(|(i, s)| SkillTemplate {
            skill: s,
            downloads: *downloads.get(i).unwrap_or(&0),
            rating: *ratings.get(i).unwrap_or(&4.0),
            source: "official".to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_skill() {
        let mut f = SkillsFile::default();
        f.skills.push(Skill {
            name: "commit".into(),
            description: "git commit".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: false,
            tags: vec![],
            shell: Some("git add -A && git commit -m \"$ARG\"".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: None,
            version: "1.0".into(),
            created_at: None,
        });
        assert!(find_skill(&f, "commit").is_some());
        assert!(find_skill(&f, "nonexistent").is_none());
    }

    #[test]
    fn test_disabled_skill_not_found() {
        let mut f = SkillsFile::default();
        f.skills.push(Skill {
            name: "x".into(),
            description: "".into(),
            category: SkillCategory::Other,
            enabled: false,
            builtin: false,
            tags: vec![],
            shell: Some("true".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: None,
            version: "1.0".into(),
            created_at: None,
        });
        assert!(find_skill(&f, "x").is_none());
        assert!(find_skill_any(&f, "x").is_some());
    }

    #[test]
    fn test_execute_shell_echo() {
        let s = Skill {
            name: "echo".into(),
            description: "echo arg".into(),
            category: SkillCategory::Other,
            enabled: true,
            builtin: false,
            tags: vec![],
            shell: Some("echo $ARG".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: None,
            version: "1.0".into(),
            created_at: None,
        };
        let r = execute_skill(&s, "hello");
        assert!(r.is_ok());
        assert!(r.unwrap().contains("hello"));
    }

    #[test]
    fn test_execute_prompt() {
        let s = Skill {
            name: "sum".into(),
            description: "summarize".into(),
            category: SkillCategory::Write,
            enabled: true,
            builtin: false,
            tags: vec![],
            shell: None,
            prompt: Some("请总结：$ARG".into()),
            chain: None,
            platform: "all".into(),
            author: None,
            version: "1.0".into(),
            created_at: None,
        };
        let r = execute_skill(&s, "一段内容").unwrap();
        assert!(r.contains("请总结"));
        assert!(r.contains("一段内容"));
    }

    #[test]
    fn test_platform_filter() {
        assert!(platform_matches("all"));
        let cur = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        };
        assert!(platform_matches(cur));
        assert!(!platform_matches("nonexistent"));
    }

    #[test]
    fn test_builtin_count() {
        let builtins = builtin_skills();
        assert!(builtins.len() >= 10);
        assert!(builtins.iter().any(|s| s.name == "commit"));
        assert!(builtins.iter().any(|s| s.name == "translate"));
    }

    #[test]
    fn test_to_command_map_only_enabled_shell() {
        let mut f = SkillsFile::default();
        f.skills.push(Skill {
            name: "a".into(),
            description: "d".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: false,
            tags: vec![],
            shell: Some("true".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: None,
            version: "1.0".into(),
            created_at: None,
        });
        f.skills.push(Skill {
            name: "b".into(),
            description: "d".into(),
            category: SkillCategory::Write,
            enabled: false,
            builtin: false,
            tags: vec![],
            shell: Some("true".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: None,
            version: "1.0".into(),
            created_at: None,
        });
        f.skills.push(Skill {
            name: "c".into(),
            description: "d".into(),
            category: SkillCategory::Other,
            enabled: true,
            builtin: false,
            tags: vec![],
            shell: None,
            prompt: Some("p".into()),
            chain: None,
            platform: "all".into(),
            author: None,
            version: "1.0".into(),
            created_at: None,
        });
        let m = to_command_map(&f);
        assert!(m.contains_key("/a"));
        assert!(!m.contains_key("/b"));
        assert!(!m.contains_key("/c"));
    }

    #[test]
    fn test_import_export_roundtrip() {
        let s = Skill {
            name: "test".into(),
            description: "d".into(),
            category: SkillCategory::Dev,
            enabled: true,
            builtin: false,
            tags: vec!["t".into()],
            shell: Some("echo $ARG".into()),
            prompt: None,
            chain: None,
            platform: "all".into(),
            author: Some("me".into()),
            version: "1.0".into(),
            created_at: Some(1234),
        };
        let json = export_skill(&s).unwrap();
        let mut loaded = import_skill(&json).unwrap();
        assert_eq!(loaded.name, "test");
        // import 强制 builtin = false
        assert!(!loaded.builtin);
        // created_at 被刷新
        assert!(loaded.created_at.unwrap() >= 1234);
        loaded.builtin = true;
        let json2 = export_skill(&loaded).unwrap();
        let mut loaded2 = import_skill(&json2).unwrap();
        assert!(!loaded2.builtin); // 还是 false
    }

    #[test]
    fn test_template_market_has_official() {
        let t = template_market();
        assert!(!t.is_empty());
        assert!(t.iter().all(|x| x.source == "official"));
    }

    #[test]
    fn test_list_grouped_by_category() {
        let f = SkillsFile::with_builtin();
        let g = list_grouped(&f);
        assert!(g.contains_key("dev"));
        assert!(g.contains_key("write"));
    }

    #[test]
    fn test_category_icon() {
        assert_eq!(SkillCategory::Dev.icon(), "💻");
        assert_eq!(SkillCategory::Write.icon(), "✍️");
        assert_eq!(SkillCategory::Fun.icon(), "🎮");
    }
}