//! API Key 本地存储 — ~/.agentshell/secrets.json（仅本机，不进 git）

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Secrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimax_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deepseek_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
}

fn secrets_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".into());
    let dir = PathBuf::from(home).join(".agentshell");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("secrets.json")
}

pub fn load_secrets() -> Secrets {
    let path = secrets_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_secrets(secrets: &Secrets) -> Result<(), String> {
    let path = secrets_path();
    let json = serde_json::to_string_pretty(secrets).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }
    }
    Ok(())
}

/// 启动时把 secrets 注入进程环境变量（兼容现有 create_provider）
pub fn apply_secrets_to_env() {
    let s = load_secrets();
    if let Some(k) = s.minimax_api_key.filter(|k| !k.is_empty()) {
        std::env::set_var("MINIMAX_API_KEY", k);
    }
    if let Some(k) = s.anthropic_api_key.filter(|k| !k.is_empty()) {
        std::env::set_var("ANTHROPIC_API_KEY", k);
    }
    if let Some(k) = s.deepseek_api_key.filter(|k| !k.is_empty()) {
        std::env::set_var("DEEPSEEK_API_KEY", k);
    }
    if let Some(k) = s.openai_api_key.filter(|k| !k.is_empty()) {
        std::env::set_var("OPENAI_API_KEY", k);
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "••••".into();
    }
    format!("{}••••{}", &key[..4], &key[key.len() - 4..])
}

fn configured(opt: &Option<String>) -> bool {
    opt.as_ref().is_some_and(|k| !k.is_empty())
}

#[derive(Debug, Serialize)]
pub struct ApiKeysStatus {
    pub minimax_configured: bool,
    pub minimax_masked: Option<String>,
    pub anthropic_configured: bool,
    pub anthropic_masked: Option<String>,
    pub deepseek_configured: bool,
    pub deepseek_masked: Option<String>,
    pub openai_configured: bool,
    pub openai_masked: Option<String>,
}

#[tauri::command]
pub fn api_keys_status() -> ApiKeysStatus {
    let s = load_secrets();
    ApiKeysStatus {
        minimax_configured: configured(&s.minimax_api_key)
            || std::env::var("MINIMAX_API_KEY").is_ok(),
        minimax_masked: s
            .minimax_api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .map(|k| mask_key(k)),
        anthropic_configured: configured(&s.anthropic_api_key),
        anthropic_masked: s
            .anthropic_api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .map(|k| mask_key(k)),
        deepseek_configured: configured(&s.deepseek_api_key),
        deepseek_masked: s
            .deepseek_api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .map(|k| mask_key(k)),
        openai_configured: configured(&s.openai_api_key),
        openai_masked: s
            .openai_api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .map(|k| mask_key(k)),
    }
}

#[derive(Deserialize)]
pub struct SetApiKeyArgs {
    pub provider: String,
    pub key: String,
}

#[tauri::command]
pub fn api_keys_set(args: SetApiKeyArgs) -> Result<ApiKeysStatus, String> {
    let mut s = load_secrets();
    let key = args.key.trim().to_string();
    match args.provider.as_str() {
        "minimax" => s.minimax_api_key = if key.is_empty() { None } else { Some(key) },
        "anthropic" => s.anthropic_api_key = if key.is_empty() { None } else { Some(key) },
        "deepseek" => s.deepseek_api_key = if key.is_empty() { None } else { Some(key) },
        "openai" => s.openai_api_key = if key.is_empty() { None } else { Some(key) },
        other => return Err(format!("未知 provider: {other}")),
    }
    save_secrets(&s)?;
    apply_secrets_to_env();
    Ok(api_keys_status())
}

#[derive(Deserialize)]
pub struct TestApiKeyArgs {
    pub provider: String,
}

async fn test_provider_chat<P: provider::model::Model + ?Sized>(
    provider: &P,
    model: &str,
    label: &str,
) -> Result<String, String> {
    use provider::model::Model;

    let req = provider::request::ChatRequest::new(model)
        .with_message(provider::request::ChatMessage::user("ping"));
    let resp = provider
        .chat(req)
        .await
        .map_err(|e| format!("连接失败: {e}"))?;
    let text = resp
        .first_message()
        .map(|m| m.content.chars().take(80).collect::<String>())
        .unwrap_or_else(|| "(empty)".into());
    Ok(format!("{label} 连接成功 ✓ 回复: {text}"))
}

#[tauri::command]
pub async fn api_keys_test(args: TestApiKeyArgs) -> Result<String, String> {
    use provider::model::Model;

    apply_secrets_to_env();
    match args.provider.as_str() {
        "minimax" => {
            let key = std::env::var("MINIMAX_API_KEY")
                .map_err(|_| "请先填写并保存 MiniMax API Key".to_string())?;
            let p = provider::MinimaxProvider::new(key, None);
            test_provider_chat(&p, "MiniMax-M3", "MiniMax M3").await
        }
        "deepseek" => {
            let key = std::env::var("DEEPSEEK_API_KEY")
                .map_err(|_| "请先填写并保存 DeepSeek API Key".to_string())?;
            let p = provider::DeepSeekProvider::new("deepseek-v4-pro", key, None);
            test_provider_chat(&p, "deepseek-v4-pro", "DeepSeek").await
        }
        "anthropic" => {
            let key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| "请先填写并保存 Anthropic API Key".to_string())?;
            let p = provider::AnthropicProvider::new("claude-sonnet-4-5", key, None);
            test_provider_chat(&p, "claude-sonnet-4-5", "Anthropic").await
        }
        "openai" => {
            let key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| "请先填写并保存 OpenAI API Key".to_string())?;
            let info = provider::model::ModelInfo {
                id: "gpt-5-mini".into(),
                name: "GPT-5 Mini".into(),
                provider: "openai".into(),
                max_context: 128_000,
                max_output: 8_192,
                capabilities: Default::default(),
                input_price_per_m: 5.0,
                output_price_per_m: 15.0,
                cache_read_price_per_m: 0.0,
                reasoning_efforts: vec![],
            };
            let p = provider::OpenAiCompatProvider::new(info, "https://api.openai.com/v1", key);
            test_provider_chat(&p, "gpt-5-mini", "OpenAI").await
        }
        other => Err(format!("未知 provider: {other}")),
    }
}

/// 兼容旧前端调用
#[tauri::command]
pub async fn api_keys_test_minimax() -> Result<String, String> {
    api_keys_test(TestApiKeyArgs {
        provider: "minimax".into(),
    })
    .await
}
