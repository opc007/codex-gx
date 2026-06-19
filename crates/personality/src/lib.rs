//! v1.7：Personality 沟通风格
//!
//! 设计参考：docs/开发文档.md §5.20
//!
//! ## 4 档
//! - `concise`        — 极简：少废话、≤3 句
//! - `explanatory`    — 详细：解释为什么、给例子
//! - `collaborative`  — 对话：反问、确认意图
//! - `custom`         — 用户自定义 prompt（读 `~/.agentshell/personality/custom.txt`）
//!
//! ## 行为
//! - 切档后立即生效（下一轮对话开始用新的 system prompt addon）
//! - 状态栏显示当前 personality
//! - 切换走 `PersonalityRegistry::set()`

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 3 档预设（v1.7 第一版）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PersonalityPreset {
    /// 极简
    Concise,
    /// 详细
    Explanatory,
    /// 对话
    Collaborative,
}

impl PersonalityPreset {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Concise => "Concise · 极简",
            Self::Explanatory => "Explanatory · 详细",
            Self::Collaborative => "Collaborative · 对话",
        }
    }

    pub fn short(&self) -> &'static str {
        match self {
            Self::Concise => "concise",
            Self::Explanatory => "explanatory",
            Self::Collaborative => "collaborative",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "concise" | "c" => Some(Self::Concise),
            "explanatory" | "e" => Some(Self::Explanatory),
            "collaborative" | "collab" | "l" => Some(Self::Collaborative),
            _ => None,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::Concise, Self::Explanatory, Self::Collaborative]
    }
}

/// 当前激活的 personality
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Personality {
    /// 预设
    Preset(PersonalityPreset),
    /// 自定义
    Custom,
}

impl Default for Personality {
    fn default() -> Self {
        Self::Preset(PersonalityPreset::Concise)
    }
}

impl Personality {
    pub fn display_name(&self) -> String {
        match self {
            Self::Preset(p) => p.display_name().to_string(),
            Self::Custom => "Custom · 自定义".to_string(),
        }
    }

    pub fn short(&self) -> &'static str {
        match self {
            Self::Preset(p) => p.short(),
            Self::Custom => "custom",
        }
    }
}

/// 内置 system prompt 增量
mod prompts {
    pub const CONCISE: &str = "Be concise. Skip pleasantries. Answer in ≤3 sentences unless code is requested. Prefer bullet points for lists. No filler words.";

    pub const EXPLANATORY: &str = "Be explanatory. Show reasoning, alternatives, and trade-offs. Use Markdown sections. Include 1-2 examples when introducing a new concept. When a decision has multiple paths, briefly state why you chose one.";

    pub const COLLABORATIVE: &str = "Be collaborative. Ask 1-2 clarifying questions before big actions. Propose a plan first and get user confirmation. After each major step, briefly summarize what was done and what's next.";
}

/// 自定义 prompt 文件路径
pub fn custom_prompt_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".agentshell")
        .join("personality")
        .join("custom.txt")
}

/// Personality registry（跨 session 持久化）
pub struct PersonalityRegistry {
    state: Personality,
    custom_path: PathBuf,
}

impl PersonalityRegistry {
    /// 加载（读 `~/.agentshell/personality/state.json`，默认 concise）
    pub fn load() -> Self {
        let state = load_state().unwrap_or_default();
        let custom_path = custom_prompt_path();
        Self { state, custom_path }
    }

    /// 强制指定（v1.7 测试用）
    pub fn with_state(state: Personality) -> Self {
        let custom_path = custom_prompt_path();
        Self { state, custom_path }
    }

    /// 当前
    pub fn current(&self) -> &Personality {
        &self.state
    }

    /// 设置 + 持久化
    pub fn set(&mut self, p: Personality) -> Result<(), PersonalityError> {
        self.state = p;
        save_state(&self.state)
    }

    /// 切到预设
    pub fn set_preset(&mut self, preset: PersonalityPreset) -> Result<(), PersonalityError> {
        self.set(Personality::Preset(preset))
    }

    /// 切到自定义
    pub fn set_custom(&mut self) -> Result<(), PersonalityError> {
        self.set(Personality::Custom)
    }

    /// 当前 personality 的 system prompt 增量（追加到 5.3 末尾）
    pub fn system_prompt_addon(&self) -> String {
        match &self.state {
            Personality::Preset(p) => match p {
                PersonalityPreset::Concise => prompts::CONCISE.to_string(),
                PersonalityPreset::Explanatory => prompts::EXPLANATORY.to_string(),
                PersonalityPreset::Collaborative => prompts::COLLABORATIVE.to_string(),
            },
            Personality::Custom => {
                std::fs::read_to_string(&self.custom_path).unwrap_or_else(|_| {
                    format!(
                        "[Custom personality file not found at {} — falling back to concise.]",
                        self.custom_path.display()
                    )
                })
            }
        }
    }
}

/// 持久化文件
fn state_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".agentshell")
        .join("personality")
        .join("state.json")
}

fn load_state() -> Option<Personality> {
    let path = state_path();
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_state(p: &Personality) -> Result<(), PersonalityError> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PersonalityError::Io)?;
    }
    let text = serde_json::to_string_pretty(p).map_err(PersonalityError::Json)?;
    std::fs::write(&path, text).map_err(PersonalityError::Io)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum PersonalityError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_parse() {
        assert_eq!(
            PersonalityPreset::parse("concise"),
            Some(PersonalityPreset::Concise)
        );
        assert_eq!(
            PersonalityPreset::parse("EXPLANATORY"),
            Some(PersonalityPreset::Explanatory)
        );
        assert_eq!(
            PersonalityPreset::parse("collab"),
            Some(PersonalityPreset::Collaborative)
        );
        assert_eq!(PersonalityPreset::parse("unknown"), None);
    }

    #[test]
    fn test_default_personality_is_concise() {
        let r = PersonalityRegistry::with_state(Personality::default());
        assert_eq!(r.current(), &Personality::Preset(PersonalityPreset::Concise));
        let addon = r.system_prompt_addon();
        assert!(addon.contains("concise"));
    }

    #[test]
    fn test_set_preset() {
        // set_preset 内部调 save_state 写 $HOME/.agentshell/personality/state.json，
        // 临时把 HOME 指到 tempdir 避免污染真实 home（CI / sandbox 安全）。
        let dir = std::env::temp_dir().join(format!(
            "agentshell-personality-test-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let prev = std::env::var("HOME").ok();
        // SAFETY: 测试串行执行
        unsafe { std::env::set_var("HOME", &dir) }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut r = PersonalityRegistry::with_state(Personality::default());
            r.set_preset(PersonalityPreset::Explanatory).unwrap();
            assert_eq!(
                r.current(),
                &Personality::Preset(PersonalityPreset::Explanatory)
            );
            let addon = r.system_prompt_addon();
            assert!(addon.contains("Markdown"));
        }));

        match prev {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        let _ = std::fs::remove_dir_all(&dir);
        if result.is_err() {
            std::panic::resume_unwind(result.unwrap_err());
        }
    }

    #[test]
    fn test_custom_fallback_when_no_file() {
        let r = PersonalityRegistry::with_state(Personality::Custom);
        let addon = r.system_prompt_addon();
        // fallback 提示
        assert!(addon.contains("Custom personality file not found"));
    }
}
