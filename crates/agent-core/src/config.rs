//! Agent 配置
//!
//! 设计参考：docs/开发文档.md §4.2 配置文件

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 顶层 Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 模型名
    #[serde(default = "default_model")]
    pub model: String,
    /// provider 名
    #[serde(default = "default_provider")]
    pub model_provider: String,
    /// 推理强度（minimal / low / medium / high / xhigh）
    #[serde(default = "default_reasoning")]
    pub model_reasoning_effort: String,
    /// Thinking 模式（enabled / disabled / adaptive）
    #[serde(default = "default_thinking")]
    pub thinking: String,
    /// 审批模式（auto / on-request / on-failure / never）
    #[serde(default = "default_approval")]
    pub approval_mode: String,
    /// 功能开关
    #[serde(default)]
    pub features: Features,
    /// Provider 列表
    #[serde(default)]
    pub model_providers: std::collections::HashMap<String, ProviderConfig>,
}

fn default_model() -> String {
    "MiniMax-M3".into()
}
fn default_provider() -> String {
    "minimax".into()
}
fn default_reasoning() -> String {
    "high".into()
}
fn default_thinking() -> String {
    "adaptive".into()
}
fn default_approval() -> String {
    "on-request".into()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            model_provider: default_provider(),
            model_reasoning_effort: default_reasoning(),
            thinking: default_thinking(),
            approval_mode: default_approval(),
            features: Features::default(),
            model_providers: std::collections::HashMap::new(),
        }
    }
}

/// 特性开关
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Features {
    #[serde(default)]
    pub memories: bool,
    #[serde(default)]
    pub mcp: bool,
    #[serde(default)]
    pub worktree: bool,
    #[serde(default = "default_true")]
    pub multimodal: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Features {
    fn default() -> Self {
        Self {
            memories: false,
            mcp: false,
            worktree: false,
            multimodal: true,
        }
    }
}

/// 单个 provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    #[serde(default = "default_wire_api")]
    pub wire_api: String,
    /// 环境变量名（API key 从 env 读）
    pub env_key: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default)]
    pub supports_video: bool,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default = "default_thinking")]
    pub thinking_default: String,
    #[serde(default = "default_max_context")]
    pub max_context_tokens: u32,
}

fn default_wire_api() -> String {
    "chat".into()
}
fn default_max_context() -> u32 {
    1_000_000
}

impl AgentConfig {
    /// 从 TOML 文件加载
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        toml::from_str(&text).map_err(|e| e.to_string())
    }

    /// 序列化为 TOML
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let c = AgentConfig::default();
        assert_eq!(c.model, "MiniMax-M3");
        assert_eq!(c.approval_mode, "on-request");
    }

    #[test]
    fn test_toml_roundtrip() {
        let c = AgentConfig::default();
        let s = c.to_toml().unwrap();
        let p: AgentConfig = toml::from_str(&s).unwrap();
        assert_eq!(p.model, c.model);
    }
}
