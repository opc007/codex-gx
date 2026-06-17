//! v1.7：SKILL.md 开放标准 Tauri 命令
//!
//! 设计参考：docs/开发文档.md §5.21
//!
//! ## 注册的命令
//! - `skillmd_list`       — 列出所有已加载的 SKILL.md skills
//! - `skillmd_get`        — 按 name 取一个 skill
//! - `skillmd_match`      — 模糊匹配 prompt → skill
//! - `skillmd_reload`     — 重新扫描
//! - `skillmd_install`    — 写一个新的 SKILL.md 到 user dir
//! - `skillmd_uninstall`  — 删除 user dir 的 skill
//! - `skillmd_paths`      — 显示 builtin/user/project dirs

use skills_md::{self, Skill, SkillIndex, SkillSource};
use std::sync::{Arc, Mutex};
use tauri::State;

pub type SkillIndexState = Arc<Mutex<SkillIndex>>;

pub fn build_state() -> SkillIndexState {
    let idx = skills_md::load_all().unwrap_or_else(|_| {
        SkillIndex::from_skills(Vec::new())
    });
    Arc::new(Mutex::new(idx))
}

fn reload_locked() -> SkillIndex {
    skills_md::load_all().unwrap_or_else(|_| SkillIndex::from_skills(Vec::new()))
}

#[derive(serde::Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub source: String,
    pub source_label: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub trust: Option<String>,
    pub path: String,
}

impl From<&Skill> for SkillSummary {
    fn from(s: &Skill) -> Self {
        SkillSummary {
            name: s.frontmatter.name.clone(),
            description: s.frontmatter.description.clone(),
            triggers: s.frontmatter.triggers.clone(),
            source: format!("{:?}", s.source).to_lowercase(),
            source_label: s.source.label().to_string(),
            author: s.frontmatter.author.clone(),
            version: s.frontmatter.version.clone(),
            trust: s.frontmatter.trust.clone(),
            path: s.path.display().to_string(),
        }
    }
}

#[tauri::command]
pub async fn skillmd_list(
    state: State<'_, SkillIndexState>,
) -> Result<Vec<SkillSummary>, String> {
    let idx = state.lock().map_err(|e| e.to_string())?;
    Ok(idx.list().into_iter().map(SkillSummary::from).collect())
}

#[tauri::command]
pub async fn skillmd_get(
    name: String,
    state: State<'_, SkillIndexState>,
) -> Result<Option<SkillFull>, String> {
    let idx = state.lock().map_err(|e| e.to_string())?;
    Ok(idx.get(&name).map(SkillFull::from))
}

#[derive(serde::Serialize)]
pub struct SkillFull {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub source: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub trust: Option<String>,
    pub body: String,
    pub system_prompt_addon: String,
}

impl From<&Skill> for SkillFull {
    fn from(s: &Skill) -> Self {
        SkillFull {
            name: s.frontmatter.name.clone(),
            description: s.frontmatter.description.clone(),
            triggers: s.frontmatter.triggers.clone(),
            source: format!("{:?}", s.source).to_lowercase(),
            author: s.frontmatter.author.clone(),
            version: s.frontmatter.version.clone(),
            trust: s.frontmatter.trust.clone(),
            body: s.body.clone(),
            system_prompt_addon: skills_md::skill_prompt_addon(s),
        }
    }
}

#[tauri::command]
pub async fn skillmd_match(
    prompt: String,
    state: State<'_, SkillIndexState>,
) -> Result<Option<SkillSummary>, String> {
    let idx = state.lock().map_err(|e| e.to_string())?;
    Ok(idx.match_prompt(&prompt).map(SkillSummary::from))
}

#[tauri::command]
pub async fn skillmd_reload(state: State<'_, SkillIndexState>) -> Result<usize, String> {
    let new_idx = reload_locked();
    let count = new_idx.list().len();
    let mut idx = state.lock().map_err(|e| e.to_string())?;
    *idx = new_idx;
    Ok(count)
}

#[tauri::command]
pub fn skillmd_paths() -> serde_json::Value {
    serde_json::json!({
        "builtin": skills_md::builtin_skills_dir().display().to_string(),
        "user": skills_md::user_skills_dir().display().to_string(),
        "project": skills_md::project_skills_dir(&std::env::current_dir().unwrap_or_default()).display().to_string(),
    })
}

#[derive(serde::Deserialize)]
pub struct InstallArgs {
    pub name: String,
    pub content: String,
}

#[tauri::command]
pub async fn skillmd_install(
    args: InstallArgs,
    state: State<'_, SkillIndexState>,
) -> Result<SkillSummary, String> {
    let user_dir = skills_md::user_skills_dir().join(&args.name);
    std::fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
    let path = user_dir.join("SKILL.md");
    std::fs::write(&path, &args.content).map_err(|e| e.to_string())?;
    // reload
    let new_idx = reload_locked();
    let mut idx = state.lock().map_err(|e| e.to_string())?;
    *idx = new_idx;
    let s = idx
        .get(&args.name)
        .ok_or_else(|| format!("installed but not found: {}", args.name))?;
    Ok(SkillSummary::from(s))
}

#[tauri::command]
pub async fn skillmd_uninstall(
    name: String,
    state: State<'_, SkillIndexState>,
) -> Result<bool, String> {
    let user_dir = skills_md::user_skills_dir().join(&name);
    let removed = if user_dir.exists() {
        std::fs::remove_dir_all(&user_dir).map_err(|e| e.to_string())?;
        true
    } else {
        false
    };
    let new_idx = reload_locked();
    let mut idx = state.lock().map_err(|e| e.to_string())?;
    *idx = new_idx;
    Ok(removed)
}
