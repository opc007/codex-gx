//! AgentShell 桌面端入口 (Tauri 2)
//!
//! 详细设计见 docs/开发文档.md §3 / §6

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tools;

use agent_core::tool::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use provider::{
    request::ToolDefinition, AnthropicProvider, ChatMessage, ChatRequest, DeepSeekProvider,
    MinimaxProvider, Model,
};

/// 全局 provider 缓存（lazy，按 model id）
type ProviderCache = Arc<Mutex<Option<Box<dyn Model>>>>;

/// 全局工具注册表
#[derive(Default)]
struct ToolRegistryState(Mutex<ToolRegistry>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .manage(ProviderCache::default())
        .manage(ToolRegistryState::default())
        .invoke_handler(tauri::generate_handler![
            ping,
            chat,
            chat_stream,
            list_providers,
            list_tools,
            execute_tool,
            cancel_chat,
            activate_license,
            get_license_status,
            deactivate_license,
            get_ide_context,
            get_git_diff,
            list_git_branches,
        ])
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 简单的 ping 命令
#[tauri::command]
fn ping() -> String {
    let v = env!("CARGO_PKG_VERSION");
    format!("AgentShell Rust backend v{}", v)
}

/// 同步聊天（保留以兼容）
#[tauri::command]
async fn chat(req: ChatRequestPayload) -> Result<ChatResponsePayload, String> {
    let provider = create_provider(&req.model).await?;
    let chat_req = build_chat_request(&req.model, &req.message, false);
    let resp = provider.chat(chat_req).await.map_err(|e| e.to_string())?;
    let msg = resp.first_message().cloned().unwrap_or_else(|| AssistantMessage {
        role: "assistant".into(),
        content: "(empty)".into(),
        reasoning_content: None,
        tool_calls: vec![],
    });
    Ok(ChatResponsePayload {
        content: msg.content,
        thinking: msg.reasoning_content.unwrap_or_default(),
        usage: UsageInfo {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        },
    })
}

/// 真流式聊天 — 后端 spawn stream + emit Tauri event
/// 前端通过 `listen("chat-chunk:{session_id}", ...)` 收
#[tauri::command]
async fn chat_stream(
    app: AppHandle,
    cache: tauri::State<'_, ProviderCache>,
    req: ChatRequestPayload,
) -> Result<String, String> {
    let _ = &cache; // 暂时未用，保留以备后续
    let session_id = req.session_id.clone();
    let model = req.model.clone();
    let message = req.message.clone();
    let ret_session_id = session_id.clone();

    let provider = create_provider(&model).await?;
    let chat_req = build_chat_request(&model, &message, true);

    // 在后台 task 里跑 stream
    tokio::spawn(async move {
        let event = format!("chat-chunk:{}", session_id);
        let stream = match provider.chat_stream(chat_req).await {
            Ok(s) => s,
            Err(e) => {
                let _ = app.emit(
                    &event,
                    ChatChunkEvent {
                        kind: "error".into(),
                        delta: e.to_string(),
                        usage: None,
                        done: true,
                    },
                );
                return;
            }
        };

        use futures::StreamExt;
        let mut stream = Box::pin(stream);
        let mut total_input = 0u32;
        let mut total_output = 0u32;

        while let Some(chunk_res) = stream.next().await {
            match chunk_res {
                Ok(chunk) => {
                    use provider::stream::StreamChunk;
                    let (kind, delta) = match chunk {
                        StreamChunk::Content(s) => ("content", s),
                        StreamChunk::Reasoning(s) => ("thinking", s),
                        StreamChunk::ToolCallDelta { index, id, name, arguments_delta } => {
                            let payload = serde_json::json!({
                                "index": index,
                                "id": id,
                                "name": name,
                                "arguments_delta": arguments_delta,
                            });
                            let _ = app.emit(
                                &event,
                                ChatChunkEvent {
                                    kind: "tool_call_delta".into(),
                                    delta: payload.to_string(),
                                    usage: None,
                                    done: false,
                                },
                            );
                            continue;
                        }
                        StreamChunk::Usage(u) => {
                            total_input = u.input_tokens;
                            total_output = u.output_tokens;
                            let _ = app.emit(
                                &event,
                                ChatChunkEvent {
                                    kind: "usage".into(),
                                    delta: String::new(),
                                    usage: Some(UsageInfo {
                                        input_tokens: total_input,
                                        output_tokens: total_output,
                                    }),
                                    done: false,
                                },
                            );
                            continue;
                        }
                        StreamChunk::Done => {
                            let _ = app.emit(
                                &event,
                                ChatChunkEvent {
                                    kind: "done".into(),
                                    delta: String::new(),
                                    usage: Some(UsageInfo {
                                        input_tokens: total_input,
                                        output_tokens: total_output,
                                    }),
                                    done: true,
                                },
                            );
                            return;
                        }
                    };
                    let _ = app.emit(
                        &event,
                        ChatChunkEvent {
                            kind: kind.into(),
                            delta,
                            usage: None,
                            done: false,
                        },
                    );
                }
                Err(e) => {
                    let _ = app.emit(
                        &event,
                        ChatChunkEvent {
                            kind: "error".into(),
                            delta: e.to_string(),
                            usage: None,
                            done: true,
                        },
                    );
                    return;
                }
            }
        }

        // 流自然结束
        let _ = app.emit(
            &event,
            ChatChunkEvent {
                kind: "done".into(),
                delta: String::new(),
                usage: Some(UsageInfo {
                    input_tokens: total_input,
                    output_tokens: total_output,
                }),
                done: true,
            },
        );
    });

    Ok(ret_session_id)
}

/// 取消进行中的聊天（通过 app data 标记 session_id 为 cancelled）
/// v0.2 简化：前端忽略后续 chunk 即可
#[tauri::command]
async fn cancel_chat(_session_id: String) -> Result<(), String> {
    Ok(())
}

/// 列出已注册工具
#[tauri::command]
async fn list_tools(app: AppHandle) -> Result<Vec<ToolDefDto>, String> {
    let state = app.state::<ToolRegistryState>();
    let mut reg = state.0.lock().await;
    // lazy 初始化（用 cwd）
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if reg.is_empty() {
        tools::register_all(&mut reg, cwd.clone(), cwd);
    }
    Ok(reg.schemas().into_iter().map(|s| ToolDefDto {
        name: s.name,
        description: s.description,
        parameters: s.parameters,
    }).collect())
}

/// 执行工具
#[tauri::command]
async fn execute_tool(
    app: AppHandle,
    name: String,
    arguments: serde_json::Value,
) -> Result<ToolExecDto, String> {
    let state = app.state::<ToolRegistryState>();
    let mut reg = state.0.lock().await;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if reg.is_empty() {
        tools::register_all(&mut reg, cwd.clone(), cwd);
    }
    let tool = reg.get(&name).ok_or_else(|| format!("tool not found: {}", name))?;
    let out = tool.execute(arguments).await.map_err(|e| e.to_string())?;
    Ok(ToolExecDto {
        success: out.success,
        output: out.output,
        error: out.error,
        truncated: out.truncated,
    })
}

// ============================================================
// IDE / Git Diff / Review
// ============================================================

/// 获取 IDE context（VSCode / Cursor）
#[tauri::command]
fn get_ide_context() -> IdeContextDto {
    let ctx = context::ide::detect_from_env().unwrap_or_default();
    IdeContextDto {
        ide: ctx.ide,
        current_file: ctx.current_file,
        selection: ctx.selection,
        cursor_line: ctx.cursor_line,
        cursor_column: ctx.cursor_column,
    }
}

/// 获取 git diff（v0.2: 工作区 vs HEAD）
#[tauri::command]
fn get_git_diff() -> Result<GitDiffDto, String> {
    use std::process::Command;
    let output = Command::new("git")
        .args(["diff", "--stat", "HEAD"])
        .output()
        .map_err(|e| format!("git failed: {}", e))?;
    if !output.status.success() {
        // 可能在没 git 仓库
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(if stderr.is_empty() {
            "当前目录不是 git 仓库".into()
        } else {
            stderr
        });
    }
    let stat = String::from_utf8_lossy(&output.stdout).to_string();
    let output2 = Command::new("git")
        .args(["diff", "HEAD"])
        .output()
        .map_err(|e| format!("git failed: {}", e))?;
    let diff_text = String::from_utf8_lossy(&output2.stdout).to_string();
    let truncated = diff_text.len() > 200_000;
    let diff = if truncated {
        diff_text.chars().take(200_000).collect::<String>()
    } else {
        diff_text
    };
    Ok(GitDiffDto {
        stat,
        diff,
        truncated,
    })
}

/// 列出 git 分支
#[tauri::command]
fn list_git_branches() -> Result<Vec<String>, String> {
    use std::process::Command;
    let output = Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .output()
        .map_err(|e| format!("git failed: {}", e))?;
    if !output.status.success() {
        return Err("git 不可用".into());
    }
    let s = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdeContextDto {
    ide: String,
    current_file: Option<String>,
    selection: Option<String>,
    cursor_line: Option<u32>,
    cursor_column: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitDiffDto {
    stat: String,
    diff: String,
    truncated: bool,
}

// ============================================================
// License
// ============================================================

/// 激活码 demo 密钥（生产用 RSA public key 从服务端下发的 license_verify.pem）
const LICENSE_DEMO_KEY: &[u8] = b"agentshell-demo-license-key-v0.2";

fn license_storage_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".agentshell")
        .join("license.toml")
}

/// 激活 License
#[tauri::command]
fn activate_license(app: AppHandle, code: String) -> Result<LicenseStatusDto, String> {
    let parsed = license::LicenseCode::from_user_code(&code)
        .map_err(|e| format!("码格式错误: {}", e))?;
    let device = license::DeviceFingerprint::current();
    license::verify::verify_code(&parsed, &device, LICENSE_DEMO_KEY)
        .map_err(|e| e.to_string())?;

    // 存储
    let path = license_storage_path();
    let storage = license::LicenseStorage::new(&path);
    let stored = license::StoredLicense {
        code: parsed.clone(),
        installed_at: chrono_now(),
        device_id: device.to_id(),
    };
    storage.save(&stored).map_err(|e| format!("保存失败: {}", e))?;

    let _ = app.emit("license:changed", ());

    Ok(LicenseStatusDto::from(&parsed))
}

/// 查看 License 状态
#[tauri::command]
fn get_license_status() -> LicenseStatusDto {
    let path = license_storage_path();
    let storage = license::LicenseStorage::new(&path);
    match storage.load() {
        Ok(Some(stored)) => LicenseStatusDto::from(&stored.code),
        _ => LicenseStatusDto::none(),
    }
}

/// 清除 License
#[tauri::command]
fn deactivate_license(app: AppHandle) -> Result<(), String> {
    let path = license_storage_path();
    let storage = license::LicenseStorage::new(&path);
    storage.clear().map_err(|e| e.to_string())?;
    let _ = app.emit("license:changed", ());
    Ok(())
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseStatusDto {
    active: bool,
    tier: String,
    tier_display: String,
    activated_at: Option<String>,
    expires_at: Option<String>,
    remaining_days: Option<i64>,
    device_id: Option<String>,
}

impl LicenseStatusDto {
    fn none() -> Self {
        Self {
            active: false,
            tier: "none".into(),
            tier_display: "未激活".into(),
            activated_at: None,
            expires_at: None,
            remaining_days: None,
            device_id: None,
        }
    }

    fn from(code: &license::LicenseCode) -> Self {
        let now = chrono::Utc::now();
        let remaining = code.payload.remaining_days(now);
        let active = code.payload.is_active(now);
        Self {
            active,
            tier: format!("{:?}", code.payload.tier).to_lowercase(),
            tier_display: code.payload.tier.display_name().into(),
            activated_at: Some(code.payload.activated_at.to_rfc3339()),
            expires_at: code.payload.expires_at.map(|e| e.to_rfc3339()),
            remaining_days: remaining,
            device_id: Some(code.payload.device.to_id()),
        }
    }
}

fn build_chat_request(model: &str, message: &str, stream: bool) -> ChatRequest {
    let mut req = ChatRequest::new(model)
        .with_message(ChatMessage::user(message))
        .with_max_tokens(4096)
        .with_reasoning_effort("high")
        .with_reasoning_split(true);
    req.stream = stream;
    req
}

/// 按 model id 创建对应 provider
async fn create_provider(model: &str) -> Result<Box<dyn Model>, String> {
    match model {
        "MiniMax-M3" | "m3" => {
            let key = std::env::var("MINIMAX_API_KEY")
                .map_err(|_| "MINIMAX_API_KEY 环境变量未设置。请 export MINIMAX_API_KEY=xxx".to_string())?;
            Ok(Box::new(MinimaxProvider::new(key, None)))
        }
        m if m.starts_with("claude-") => {
            let key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| "ANTHROPIC_API_KEY 环境变量未设置".to_string())?;
            Ok(Box::new(AnthropicProvider::new(m, key, None)))
        }
        m if m.starts_with("deepseek-") => {
            let key = std::env::var("DEEPSEEK_API_KEY")
                .map_err(|_| "DEEPSEEK_API_KEY 环境变量未设置".to_string())?;
            Ok(Box::new(DeepSeekProvider::new(m, key, None)))
        }
        m if m.starts_with("gpt-") => {
            // OpenAI 也走 OpenAI 兼容 provider
            let key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| "OPENAI_API_KEY 环境变量未设置".to_string())?;
            let info = provider::model::ModelInfo {
                id: m.into(),
                name: m.into(),
                provider: "openai".into(),
                max_context: 128_000,
                max_output: 8_192,
                capabilities: Default::default(),
                input_price_per_m: 5.0,
                output_price_per_m: 15.0,
                cache_read_price_per_m: 0.0,
                reasoning_efforts: vec![],
            };
            Ok(Box::new(provider::OpenAiCompatProvider::new(
                info,
                "https://api.openai.com/v1",
                key,
            )))
        }
        other => Err(format!("未知模型: {}", other)),
    }
}

/// 列出所有 provider / 模型
#[tauri::command]
fn list_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "minimax".into(),
            name: "MiniMax (国产)".into(),
            models: vec!["MiniMax-M3".into()],
            default_model: "MiniMax-M3".into(),
            env_key: "MINIMAX_API_KEY".into(),
        },
        ProviderInfo {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            models: vec!["claude-opus-4-8".into(), "claude-sonnet-4-5".into()],
            default_model: "claude-sonnet-4-5".into(),
            env_key: "ANTHROPIC_API_KEY".into(),
        },
        ProviderInfo {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            models: vec!["deepseek-v4-pro".into()],
            default_model: "deepseek-v4-pro".into(),
            env_key: "DEEPSEEK_API_KEY".into(),
        },
        ProviderInfo {
            id: "openai".into(),
            name: "OpenAI".into(),
            models: vec!["gpt-5.5".into(), "gpt-5-mini".into()],
            default_model: "gpt-5-mini".into(),
            env_key: "OPENAI_API_KEY".into(),
        },
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequestPayload {
    model: String,
    message: String,
    session_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatResponsePayload {
    content: String,
    thinking: String,
    usage: UsageInfo,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatChunkEvent {
    kind: String,
    delta: String,
    usage: Option<UsageInfo>,
    done: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UsageInfo {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInfo {
    id: String,
    name: String,
    models: Vec<String>,
    default_model: String,
    env_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolDefDto {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolExecDto {
    success: bool,
    output: String,
    error: Option<String>,
    truncated: bool,
}

// 把 provider 的 AssistantMessage 引入到本 crate
use provider::response::AssistantMessage as InternalAssistantMessage;
type AssistantMessage = InternalAssistantMessage;