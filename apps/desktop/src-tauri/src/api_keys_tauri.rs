//! API Key 本地存储 — ~/.agentshell/secrets.json（仅本机，不进 git）
//! 注意：当前为明文存储（带 0600 权限）。
//! 后续优化建议：迁移到 macOS Keychain / Windows Credential Manager（更安全）。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// v1.9.16：自定义 provider — 用户填 base_url + api_key + 默认模型即可用
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomProvider {
    pub name: String,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub default_model: String,
    /// 逗号分隔的模型名列表（可选）
    #[serde(default)]
    pub extra_models: Vec<String>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zhipu_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimo_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moonshot_api_key: Option<String>,
    /// v1.9.16：用户自定义的 provider（OpenAI 协议兼容）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_provider: Option<CustomProvider>,
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

/// 优先返回 secrets.json 中的 MiniMax Key；为空时回退到环境变量
pub fn resolve_minimax_key() -> Result<String, String> {
    let s = load_secrets();
    if let Some(k) = s.minimax_api_key.filter(|k| !k.is_empty()) {
        return Ok(k);
    }
    std::env::var("MINIMAX_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "MINIMAX_API_KEY 未配置".into())
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
    if let Some(k) = s.zhipu_api_key.filter(|k| !k.is_empty()) {
        std::env::set_var("ZHIPU_API_KEY", k);
    }
    if let Some(k) = s.mimo_api_key.filter(|k| !k.is_empty()) {
        std::env::set_var("MIMO_API_KEY", k);
    }
    if let Some(k) = s.moonshot_api_key.filter(|k| !k.is_empty()) {
        std::env::set_var("MOONSHOT_API_KEY", k);
    }
    if let Some(cp) = s.custom_provider.as_ref() {
        if let Some(k) = cp.api_key.as_ref().filter(|k| !k.is_empty()) {
            std::env::set_var("CUSTOM_API_KEY", k);
        }
        std::env::set_var("CUSTOM_BASE_URL", &cp.base_url);
        std::env::set_var("CUSTOM_DEFAULT_MODEL", &cp.default_model);
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
    pub zhipu_configured: bool,
    pub zhipu_masked: Option<String>,
    pub mimo_configured: bool,
    pub mimo_masked: Option<String>,
    pub moonshot_configured: bool,
    pub moonshot_masked: Option<String>,
    pub custom_provider: Option<CustomProvider>,
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
        zhipu_configured: configured(&s.zhipu_api_key),
        zhipu_masked: s.zhipu_api_key.as_ref().filter(|k| !k.is_empty()).map(|k| mask_key(k)),
        mimo_configured: configured(&s.mimo_api_key),
        mimo_masked: s.mimo_api_key.as_ref().filter(|k| !k.is_empty()).map(|k| mask_key(k)),
        moonshot_configured: configured(&s.moonshot_api_key),
        moonshot_masked: s.moonshot_api_key.as_ref().filter(|k| !k.is_empty()).map(|k| mask_key(k)),
        custom_provider: s.custom_provider,
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
        "zhipu" => s.zhipu_api_key = if key.is_empty() { None } else { Some(key) },
        "mimo" => s.mimo_api_key = if key.is_empty() { None } else { Some(key) },
        "moonshot" => s.moonshot_api_key = if key.is_empty() { None } else { Some(key) },
        other => return Err(format!("未知 provider: {other}")),
    }
    save_secrets(&s)?;
    apply_secrets_to_env();
    Ok(api_keys_status())
}

/// v1.9.16：保存/清空自定义 OpenAI 协议 provider
#[derive(Debug, Deserialize)]
pub struct SetCustomProviderArgs {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    /// 逗号分隔的额外模型名（可选，留空自动为 ["<default_model>"]）
    #[serde(default)]
    pub extra_models: Vec<String>,
}

#[tauri::command]
pub fn api_keys_set_custom(args: SetCustomProviderArgs) -> Result<ApiKeysStatus, String> {
    let name = args.name.trim().to_string();
    let base_url = args.base_url.trim().to_string();
    let api_key = args.api_key.trim().to_string();
    let default_model = args.default_model.trim().to_string();

    // "清空" 路径：name + base_url + default_model 全空 → 移除自定义 provider
    if name.is_empty() && base_url.is_empty() && default_model.is_empty() {
        let mut s = load_secrets();
        s.custom_provider = None;
        save_secrets(&s)?;
        apply_secrets_to_env();
        return Ok(api_keys_status());
    }
    if name.is_empty() {
        return Err("请填写「名称」".to_string());
    }
    if base_url.is_empty() || !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err("Base URL 必须以 http:// 或 https:// 开头".to_string());
    }
    if default_model.is_empty() {
        return Err("请填写「默认模型」".to_string());
    }

    let extra = if args.extra_models.is_empty() {
        vec![default_model.clone()]
    } else {
        args.extra_models
    };

    let mut s = load_secrets();
    s.custom_provider = Some(CustomProvider {
        name,
        base_url,
        api_key: if api_key.is_empty() { None } else { Some(api_key) },
        default_model,
        extra_models: extra,
    });
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
            let p = provider::DeepSeekProvider::new("deepseek-chat", key, None);
            test_provider_chat(&p, "deepseek-chat", "DeepSeek").await
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

        "custom" => {
            let cp = load_secrets()
                .custom_provider
                .ok_or_else(|| "请先在「自定义 provider」表单中填写并保存".to_string())?;
            let key = std::env::var("CUSTOM_API_KEY").unwrap_or_default();
            let info = provider::model::ModelInfo {
                id: cp.default_model.clone(),
                name: cp.name.clone(),
                provider: "custom".into(),
                max_context: 128_000,
                max_output: 8_192,
                capabilities: Default::default(),
                input_price_per_m: 0.0,
                output_price_per_m: 0.0,
                cache_read_price_per_m: 0.0,
                reasoning_efforts: vec![],
            };
            let p = provider::OpenAiCompatProvider::new(info, cp.base_url, key);
            test_provider_chat(&p, &cp.default_model, &cp.name).await
        }
        other => Err(format!("未知 provider: {other}")),
    }
}

/// v1.9.16：决定某个 provider id 是否在 `list_providers` 列表里出现。
/// 返回 true → 显示在 Composer 模型菜单里。
/// 设计原则：用户填好 Key 保存后立即出现，无需「启用」按钮；
/// 未配 Key 的 provider 默认隐藏，避免用户点错触发 401。
/// 本地 provider（ollama / llamacpp）即使无 Key 也常驻显示（它们不需要）。
pub fn is_provider_configured(provider_id: &str) -> bool {
    let s = load_secrets();
    let env_set = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty()).is_some();
    match provider_id {
        "minimax" => configured(&s.minimax_api_key) || env_set("MINIMAX_API_KEY"),
        "anthropic" => configured(&s.anthropic_api_key) || env_set("ANTHROPIC_API_KEY"),
        "deepseek" => configured(&s.deepseek_api_key) || env_set("DEEPSEEK_API_KEY"),
        "openai" => configured(&s.openai_api_key) || env_set("OPENAI_API_KEY"),
        "zhipu" => configured(&s.zhipu_api_key) || env_set("ZHIPU_API_KEY"),
        "mimo" => configured(&s.mimo_api_key) || env_set("MIMO_API_KEY"),
        "moonshot" => configured(&s.moonshot_api_key) || env_set("MOONSHOT_API_KEY"),
        "custom" => s.custom_provider.is_some() && env_set("CUSTOM_BASE_URL"),
        // 本地 provider 无需 key：常驻显示
        "ollama" | "llamacpp" => true,
        // 未知 provider：保守隐藏
        _ => false,
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
