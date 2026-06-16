//! AgentShell 桌面端入口 (Tauri 2)
//!
//! 详细设计见 docs/开发文档.md §3 / §6

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use provider::{MinimaxProvider, Model, ChatRequest, ChatMessage};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .invoke_handler(tauri::generate_handler![
            ping,
            chat,
            list_providers,
        ])
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 简单的 ping 命令（验证 Rust ↔ WebView 联通）
#[tauri::command]
fn ping() -> String {
    let v = env!("CARGO_PKG_VERSION");
    format!("AgentShell Rust backend v{}", v)
}

/// 聊天命令（同步版本，返回完整响应；流式在 v0.2）
#[tauri::command]
async fn chat(req: ChatRequestPayload) -> Result<ChatResponsePayload, String> {
    let key = std::env::var("MINIMAX_API_KEY")
        .map_err(|_| "MINIMAX_API_KEY not set".to_string())?;
    let provider = MinimaxProvider::new(key, None);

    let chat_req = ChatRequest::new(&req.model)
        .with_message(ChatMessage::user(&req.message))
        .with_max_tokens(4096)
        .with_reasoning_effort("high")
        .with_reasoning_split(true);

    let resp = provider
        .chat(chat_req)
        .await
        .map_err(|e| e.to_string())?;

    let msg = resp.first_message().cloned().unwrap_or_else(|| {
        provider::response::AssistantMessage {
            role: "assistant".into(),
            content: "(empty)".into(),
            reasoning_content: None,
            tool_calls: vec![],
        }
    });
    Ok(ChatResponsePayload {
        content: msg.content,
        thinking: msg.reasoning_content.unwrap_or_default(),
    })
}

/// 列出所有 provider / 模型
#[tauri::command]
fn list_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "minimax".into(),
            name: "MiniMax (国内)".into(),
            models: vec!["MiniMax-M3".into()],
        },
        ProviderInfo {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            models: vec!["claude-opus-4-8".into(), "claude-sonnet-4-5".into()],
        },
        ProviderInfo {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            models: vec!["deepseek-v4-pro".into()],
        },
    ]
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequestPayload {
    model: String,
    message: String,
    session_id: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatResponsePayload {
    content: String,
    thinking: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInfo {
    id: String,
    name: String,
    models: Vec<String>,
}