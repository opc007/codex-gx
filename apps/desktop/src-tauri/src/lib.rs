//! AgentShell 桌面端入口 (Tauri 2)
//!
//! 详细设计见 docs/开发文档.md §3 / §6

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod api_keys_tauri;
mod bugreport_tauri;
mod cu_tool;
mod desktop_cua;
mod graph;
mod learning_tauri;
mod license_tauri;
mod personality_tauri;
mod skills_md_tauri;
mod goal_tauri;
mod background_tauri;
mod screenshot_tauri;
mod desktop_perm_tauri;
mod mobile_tauri;
mod pocket_tauri;
mod vision_tauri;
mod lint_tauri;
mod local_tauri;
mod marketplace_tauri;
mod mcp_tool;
mod media_tauri;
mod p2p_tauri;
mod queue_tauri;
mod routing;
mod routing_tauri;
mod skills;
mod subagent_tool;
mod sync;
mod tools;
mod tts;
mod vault_tauri;
mod voice_tauri;
mod workspace_tauri;

use agent_core::tool::ToolRegistry;
use provider::{
    llama_cpp_info, ollama_info, request::ToolDefinition, AnthropicProvider, ChatMessage,
    ChatRequest, DeepSeekProvider, LlamaCppProvider, MinimaxProvider, Model, OllamaProvider,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

/// 全局 provider 缓存（lazy，按 model id）
type ProviderCache = Arc<Mutex<Option<Box<dyn Model>>>>;

/// 全局工具注册表 — 直接用 Arc<Mutex<>> 让 AgentRunner 也能 clone
type SharedToolRegistry = Arc<Mutex<ToolRegistry>>;

/// v0.8：跨会话长期记忆
type SharedMemory = Arc<Mutex<memory::MemoryManager>>;

/// v1.2：voice 模块共享状态
pub type VoiceState = Arc<Mutex<voice::VoiceManager>>;

/// v1.2：marketplace 模块共享状态
pub type MarketplaceState = Arc<Mutex<marketplace::MarketplaceManager>>;

/// v1.2：vault 模块共享状态
pub type VaultState = Arc<Mutex<vault::Vault>>;

/// v1.3：routing engine 共享状态
pub type RoutingState = Arc<Mutex<crate::routing::RoutingEngine>>;

/// v1.3：bug report 状态
pub type BugReportState = Arc<bugreport_tauri::BugReportState>;

/// v0.4：每个 session 一个 cancel handle + approval sender
#[derive(Default)]
struct SessionControl {
    inner: Mutex<HashMap<String, SessionHandle>>,
}

struct SessionHandle {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    approval_tx: std::sync::Arc<
        tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<agent::ApprovalResponse>>>,
    >,
    /// v0.6：plan approval sender
    plan_tx: std::sync::Arc<
        tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<agent::PlanApproval>>>,
    >,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .manage(ProviderCache::default())
        .manage(SharedToolRegistry::default())
        .manage(SessionControl::default())
        .manage::<VoiceState>(Arc::new(Mutex::new(
            voice::VoiceManager::new().expect("voice manager init"),
        )))
        .manage::<MarketplaceState>(Arc::new(Mutex::new(
            marketplace::MarketplaceManager::new().expect("marketplace init"),
        )))
        .manage::<VaultState>(Arc::new(Mutex::new(
            vault::Vault::new(
                std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .map(|h| {
                        std::path::PathBuf::from(h)
                            .join(".agentshell")
                            .join("vault")
                    })
                    .unwrap_or_else(|_| std::env::temp_dir().join("agentshell_vault")),
            )
            .expect("vault init"),
        )))
        .manage::<RoutingState>(Arc::new(Mutex::new(
            crate::routing::RoutingEngine::load_or_default(),
        )))
        .manage::<BugReportState>(Arc::new(bugreport_tauri::BugReportState::new()))
        .manage::<queue_tauri::QueueState>(queue_tauri::build_state())
        .manage(p2p_tauri::P2pState::new())
        .manage::<license_tauri::LicenseManagerState>(license_tauri::build_state())
        .manage::<personality_tauri::PersonalityState>(personality_tauri::build_state())
        .manage::<skills_md_tauri::SkillIndexState>(skills_md_tauri::build_state())
        .manage::<goal_tauri::GoalState>(goal_tauri::build_state())
        .manage::<background_tauri::BackgroundState>(background_tauri::build_state())
        .manage::<desktop_perm_tauri::PermListState>(desktop_perm_tauri::build_state())
        .manage::<mobile_tauri::MobileState>(mobile_tauri::build_state())
        .manage::<mobile_tauri::MobileHttpServerState>(mobile_tauri::build_http_state())
        .manage::<pocket_tauri::PocketState>(pocket_tauri::build_state())
        .manage::<pocket_tauri::PocketServerState>(pocket_tauri::build_server_state())
        .setup(|app| {
            api_keys_tauri::apply_secrets_to_env();
            // v1.3：安装 panic hook
            if let Some(state) = app.try_state::<BugReportState>() {
                bugreport_tauri::install_panic_hook(state.inner().clone());
            }
            // v1.5：首次启动时安装默认 plugin
            let reg = plugin::load_registry();
            if reg.plugins.is_empty() {
                if let Err(e) = plugin::install_defaults() {
                    eprintln!("[plugin] install defaults failed: {e}");
                }
            }
            // v1.4：启动队列事件 forwarder
            if let Some(state) = app.try_state::<queue_tauri::QueueState>() {
                queue_tauri::start_scheduler(state.inner().clone());
                queue_tauri::spawn_event_forwarder(app.handle().clone(), state.inner().clone());
            }
            // v1.6：启动时 best-effort 校验 License（不阻塞 UI）
            if let Some(state) = app.try_state::<license_tauri::LicenseManagerState>() {
                let license = state.inner().clone();
                tauri::async_runtime::spawn(async move {
                    let summary = license_tauri::initial_check(&license).await;
                    eprintln!("[license] startup check: {:?}", summary.status);
                });
            }
            // v0.8：异步初始化 memory manager
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match memory::MemoryManager::default_path().await {
                    Ok(mgr) => {
                        app_handle.manage(SharedMemory::new(Mutex::new(mgr)));
                        eprintln!("[memory] 已加载 ~/.agentshell/memory.json");
                    }
                    Err(e) => {
                        eprintln!("[memory] 加载失败: {}", e);
                    }
                }
            });
            // v1.4：异步初始化 learning
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let s = learning_tauri::LearningState::new().await;
                eprintln!(
                    "[learning] 已加载 ~/.agentshell/learning.json (chats: {}, tools: {})",
                    s.inner.read().await.signals.total_chats,
                    s.inner.read().await.signals.total_tool_calls,
                );
                app_handle.manage(s);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            chat,
            agent_run,
            cancel_chat,
            respond_approval,
            respond_plan, // v0.6
            list_providers,
            list_tools,
            execute_tool,
            api_keys_tauri::api_keys_status,
            api_keys_tauri::api_keys_set,
            api_keys_tauri::api_keys_test,
            api_keys_tauri::api_keys_test_minimax,
            license_tauri::license_status,     // v1.6
            license_tauri::license_activate,   // v1.6
            license_tauri::license_deactivate, // v1.6
            license_tauri::license_refresh,    // v1.6
            license_tauri::license_tiers,      // v1.6
            license_tauri::license_demo_code,  // v1.6 dev
            personality_tauri::personality_get,           // v1.7
            personality_tauri::personality_list_presets,   // v1.7
            personality_tauri::personality_set_preset,    // v1.7
            personality_tauri::personality_set_custom,    // v1.7
            personality_tauri::personality_load_custom,   // v1.7
            personality_tauri::personality_save_custom,   // v1.7
            skills_md_tauri::skillmd_list,         // v1.7 SKILL.md
            skills_md_tauri::skillmd_get,          // v1.7 SKILL.md
            skills_md_tauri::skillmd_match,        // v1.7 SKILL.md
            skills_md_tauri::skillmd_reload,       // v1.7 SKILL.md
            skills_md_tauri::skillmd_paths,        // v1.7 SKILL.md
            skills_md_tauri::skillmd_install,      // v1.7 SKILL.md
            skills_md_tauri::skillmd_uninstall,    // v1.7 SKILL.md
            goal_tauri::goal_list,                  // v1.7 Goal
            goal_tauri::goal_get,                   // v1.7 Goal
            goal_tauri::goal_active_for_thread,     // v1.7 Goal
            goal_tauri::goal_create,                // v1.7 Goal
            goal_tauri::goal_add_todo,              // v1.7 Goal
            goal_tauri::goal_mark_done,             // v1.7 Goal
            goal_tauri::goal_mark_in_progress,      // v1.7 Goal
            goal_tauri::goal_mark_blocked,          // v1.7 Goal
            goal_tauri::goal_pause,                 // v1.7 Goal
            goal_tauri::goal_resume,                // v1.7 Goal
            goal_tauri::goal_abandon,               // v1.7 Goal
            goal_tauri::goal_delete,                // v1.7 Goal
            goal_tauri::goal_to_prompt,             // v1.7 Goal
            background_tauri::bg_list,                  // v1.8 Background
            background_tauri::bg_list_running,          // v1.8 Background
            background_tauri::bg_get,                   // v1.8 Background
            background_tauri::bg_spawn,                 // v1.8 Background
            background_tauri::bg_stop,                  // v1.8 Background
            background_tauri::bg_stop_all,              // v1.8 Background
            background_tauri::bg_tail,                  // v1.8 Background
            voice_tauri::voice_duplex_start,             // v1.8 5.26 Voice
            voice_tauri::voice_duplex_status,            // v1.8 5.26 Voice
            screenshot_tauri::screen_list,                // v1.9 5.27 Screenshot
            screenshot_tauri::screen_primary,             // v1.9 5.27 Screenshot
            screenshot_tauri::screen_to_absolute,         // v1.9 5.27 Screenshot
            screenshot_tauri::screen_screenshot,          // v1.9 5.27 Screenshot
            screenshot_tauri::screen_protocol_prompt,     // v1.9 5.27 Screenshot
            screenshot_tauri::screen_multi_to_absolute,   // v1.9 5.27 Screenshot
            desktop_perm_tauri::perm_get_list,              // v1.9 5.28 Permission
            desktop_perm_tauri::perm_add_allow,             // v1.9 5.28 Permission
            desktop_perm_tauri::perm_add_deny,              // v1.9 5.28 Permission
            desktop_perm_tauri::perm_clear_allow,           // v1.9 5.28 Permission
            desktop_perm_tauri::perm_decide,                // v1.9 5.28 Permission
            desktop_perm_tauri::perm_is_blacklisted,        // v1.9 5.28 Permission
            desktop_perm_tauri::perm_decide_request,        // v1.9 5.28 Permission
            mobile_tauri::mobile_get_token,                  // v1.9.1 5.30 Mobile
            mobile_tauri::mobile_regen_token,                // v1.9.1 5.30 Mobile
            mobile_tauri::mobile_pair_device,                // v1.9.1 5.30 Mobile
            mobile_tauri::mobile_unpair_device,              // v1.9.1 5.30 Mobile
            mobile_tauri::mobile_list_devices,               // v1.9.1 5.30 Mobile
            mobile_tauri::mobile_verify,                     // v1.9.1 5.30 Mobile
            mobile_tauri::mobile_call,                       // v1.9.1 5.30 Mobile
            mobile_tauri::mobile_server_start,                 // v1.9.5 5.30 Mobile HTTP
            mobile_tauri::mobile_server_stop,                  // v1.9.5 5.30 Mobile HTTP
            mobile_tauri::mobile_server_status,                // v1.9.5 5.30 Mobile HTTP
            mobile_tauri::mobile_server_devices,               // v1.9.5 5.30 Mobile HTTP
            mobile_tauri::mobile_server_notifications,         // v1.9.5 5.30 Mobile HTTP
            mobile_tauri::mobile_server_commands,              // v1.9.5 5.30 Mobile HTTP
            mobile_tauri::mobile_full_status,                  // v1.9.5 5.30 Mobile HTTP
            mobile_tauri::mobile_qr_payload,                   // v1.9.5 5.30 Mobile QR
            pocket_tauri::pocket_list_sources,                // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_list_pairings,               // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_add_pairing,                 // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_remove_pairing,              // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_handle_request,              // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_sign,                        // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_webhook_url,                 // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_status,                      // v1.9.2 5.29 Pocket
            pocket_tauri::pocket_server_start,                // v1.9.3 5.29 Pocket HTTP server
            pocket_tauri::pocket_server_stop,                 // v1.9.3 5.29 Pocket HTTP server
            pocket_tauri::pocket_server_status,               // v1.9.3 5.29 Pocket HTTP server
            pocket_tauri::pocket_inbound_log,                 // v1.9.3 5.29 Pocket HTTP server
            get_ide_context,
            vision_tauri::vision_status,                       // v1.9.4 5.31 Vision
            vision_tauri::vision_meta,                         // v1.9.4 5.31 Vision
            vision_tauri::vision_caption,                     // v1.9.4 5.31 Vision
            media_tauri::media_capabilities,                   // v1.9.6 多模态生图/生视频
            media_tauri::media_generate_image,                 // v1.9.6 多模态生图
            media_tauri::media_generate_video,                 // v1.9.6 多模态生视频
            vision_tauri::vision_ocr,                         // v1.9.4 5.31 Vision
            vision_tauri::vision_annotate,                    // v1.9.4 5.31 Vision
            vision_tauri::vision_formats,                     // v1.9.4 5.31 Vision
            vision_tauri::vision_protocol_prompt,             // v1.9.4 5.31 Vision
            get_git_diff,
            list_git_branches,
            list_mcp_servers,
            reload_mcp,
            route_model_cmd,                               // v0.7
            remember_memory,                               // v0.8
            recall_memory,                                 // v0.8
            list_memories,                                 // v0.8
            forget_memory,                                 // v0.8
            clear_memories,                                // v0.8
            list_skills,                                   // v0.8
            run_skill,                                     // v0.8
            list_skills_grouped,                           // v1.5
            skill_market,                                  // v1.5
            skill_export,                                  // v1.5
            skill_import,                                  // v1.5
            skill_toggle,                                  // v1.5
            skill_remove,                                  // v1.5
            skill_reset_builtin,                           // v1.5
            skill_chain,                                   // v1.5
            tts_detect,                                    // v1.5
            tts_get_config,                                // v1.5
            tts_save_config,                               // v1.5
            tts_speak,                                     // v1.5
            tts_speak_with,                                // v1.5
            graph_from_plan,                               // v1.5
            graph_to_mermaid,                              // v1.5
            sync_publish,                                  // v1.5
            sync_fetch,                                    // v1.5
            sync_list,                                     // v1.5
            sync_remove,                                   // v1.5
            sync_clear_all,                                // v1.5
            sync_schema_version,                           // v1.5
            plugin_list,                                   // v1.5
            plugin_install,                                // v1.5
            plugin_remove,                                 // v1.5
            plugin_reload,                                 // v1.5
            plugin_install_defaults,                       // v1.5
            plugin_run_steps,                              // v1.5
            plugin_invoke,                                 // v1.5
            compress_session,                              // v1.0
            check_update,                                  // v1.0
            voice_tauri::voice_check,                      // v1.2
            voice_tauri::voice_download_model,             // v1.2
            voice_tauri::voice_transcribe,                 // v1.2
            voice_tauri::voice_cleanup,                    // v1.2
            voice_tauri::voice_delete_model,               // v1.2
            marketplace_tauri::marketplace_fetch_index,    // v1.2
            marketplace_tauri::marketplace_list_installed, // v1.2
            marketplace_tauri::marketplace_install,        // v1.2
            marketplace_tauri::marketplace_uninstall,      // v1.2
            marketplace_tauri::marketplace_set_index_url,  // v1.2
            marketplace_tauri::marketplace_get_index_url,  // v1.2
            vault_tauri::vault_is_encrypted,               // v1.2
            vault_tauri::vault_list_encrypted,             // v1.2
            vault_tauri::vault_encrypt_session,            // v1.2
            vault_tauri::vault_decrypt_session,            // v1.2
            vault_tauri::vault_remove_session,             // v1.2
            workspace_tauri::workspace_changed_broadcast,  // v1.3
            routing_tauri::routing_decide,                 // v1.3
            routing_tauri::routing_get_strategy,           // v1.3
            routing_tauri::routing_set_strategy,           // v1.3
            routing_tauri::routing_reset_to_default,       // v1.3
            bugreport_tauri::bug_report_record,            // v1.3
            bugreport_tauri::bug_report_list,              // v1.3
            bugreport_tauri::bug_report_clear,             // v1.3
            bugreport_tauri::bug_report_build,             // v1.3
            local_tauri::local_discover,                   // v1.4
            local_tauri::local_list_models,                // v1.4
            local_tauri::local_ping,                       // v1.4
            lint_tauri::lint_run,                          // v1.4
            lint_tauri::lint_run_summary,                  // v1.4
            queue_tauri::queue_list,                       // v1.4
            queue_tauri::queue_get,                        // v1.4
            queue_tauri::queue_enqueue,                    // v1.4
            queue_tauri::queue_cancel,                     // v1.4
            queue_tauri::queue_clear_finished,             // v1.4
            p2p_tauri::p2p_start_host,                     // v1.4
            p2p_tauri::p2p_stop_host,                      // v1.4
            p2p_tauri::p2p_generate_pairing,               // v1.4
            p2p_tauri::p2p_accept_pairing,                 // v1.4
            p2p_tauri::p2p_reject_pairing,                 // v1.4
            p2p_tauri::p2p_list_peers,                     // v1.4
            p2p_tauri::p2p_connect,                        // v1.4
            learning_tauri::learning_get,                  // v1.4
            learning_tauri::learning_record_chat,          // v1.4
            learning_tauri::learning_record_tool,          // v1.4
            learning_tauri::learning_record_slash,         // v1.4
            learning_tauri::learning_record_feedback,      // v1.4
            learning_tauri::learning_reset,                // v1.4
            learning_tauri::learning_inject,               // v1.4
        ])
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
    let msg = resp
        .first_message()
        .cloned()
        .unwrap_or_else(|| AssistantMessage {
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

/// v0.4 Agent 运行入口 — 含 tool_calls 循环 + cancel + approval
#[tauri::command]
async fn agent_run(app: AppHandle, req: AgentRunPayload) -> Result<String, String> {
    // v0.7：auto 模型路由
    let model_name = if req.model == "auto" {
        route_model(&req.message)
    } else {
        req.model.clone()
    };
    let provider = create_provider(&model_name).await?;
    let provider_arc: Arc<dyn Model> = Arc::from(provider);

    // 确保 tool registry 已初始化
    let reg_arc_for_subagent: Arc<Mutex<ToolRegistry>>;
    {
        let state = app.state::<SharedToolRegistry>();
        let mut reg = state.lock().await;
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        if reg.is_empty() {
            tools::register_all(&mut reg, cwd.clone(), cwd);
            cu_tool::register_computer_use(&mut reg);
            desktop_cua::register_desktop_cua(&mut reg); // v0.6
            mcp_tool::register_mcp_tools(&mut reg).await;
        }
        reg_arc_for_subagent = Arc::clone(&state);
    }

    // v0.7：注册 spawn_agent 工具（需要 provider + 全 registry）
    {
        let state = app.state::<SharedToolRegistry>();
        let mut reg = state.lock().await;
        subagent_tool::register_subagent_tool(
            &mut reg,
            app.clone(),
            provider_arc.clone(),
            reg_arc_for_subagent.clone(),
        );
    }

    // 构造 history
    let mut history: Vec<ChatMessage> = Vec::new();

    // v1.9.x：注入项目组上下文（绑定文件夹、README 摘要、简介）
    if let Some(ctx) = req.project_context.as_ref() {
        history.push(ChatMessage::system(build_project_context(
            "[项目组上下文] 当前 session 属于以下项目组（由 Codex gx 自动注入）：",
            ctx,
        )));
    }

    // v0.8：注入相关历史记忆
    if let Some(mgr_state) = app.try_state::<SharedMemory>() {
        let mgr = mgr_state.inner().lock().await;
        let memory_context = mgr.recall_context(&req.message, 5).await;
        if !memory_context.is_empty() {
            history.push(ChatMessage::system(format!(
                "你可能相关的历史记忆（仅供参考，不要照搬）：{}",
                memory_context
            )));
        }
    }

    for m in &req.messages {
        history.push(match m.role.as_str() {
            "system" => ChatMessage::system(m.content.clone()),
            "assistant" => ChatMessage::assistant(m.content.clone()),
            "tool" => ChatMessage::tool(
                m.tool_call_id.clone().unwrap_or_default(),
                m.content.clone(),
            ),
            _ => ChatMessage::user(m.content.clone()),
        });
    }

    let session_id = req.session_id.clone();
    let user_msg = req.message.clone();
    let require_approval = req.require_approval;
    let plan_mode = req.plan_mode;
    let app_clone = app.clone();

    // v0.9：处理附件图片 —— 编码为 base64 data URL，附到 user_msg
    if !req.images.is_empty() {
        let mut parts = vec![provider::request::ChatContentPart::Text {
            text: user_msg.clone(),
        }];
        for img in &req.images {
            match std::fs::read(&img.path) {
                Ok(bytes) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let mime = img
                        .mime
                        .clone()
                        .unwrap_or_else(|| guess_mime_from_path(&img.path));
                    parts.push(provider::request::ChatContentPart::ImageBase64 {
                        data: b64,
                        mime_type: mime,
                    });
                }
                Err(e) => {
                    eprintln!("[image] 读取 {} 失败: {}", img.path, e);
                }
            }
        }
        // 把多模态 user message 放到 history 最后（替代普通 user 消息）
        history.push(provider::request::ChatMessage {
            role: provider::request::ChatRole::User,
            content: parts,
            reasoning_content: None,
            tool_call_id: None,
        });
    }

    tokio::spawn(async move {
        let reg_state = app_clone.state::<SharedToolRegistry>();
        let reg_arc: Arc<Mutex<ToolRegistry>> = Arc::clone(&reg_state);
        let mut runner =
            agent::AgentRunner::new(app_clone.clone(), session_id.clone(), provider_arc, reg_arc)
                .with_history(history)
                .with_max_steps(10)
                .with_require_approval(require_approval)
                .with_plan_mode(plan_mode);

        // v0.4：注册 cancel handle + approval sender 到 SessionControl
        let cancel = runner.cancel_handle();
        let approval_tx_slot = runner.approval_rx.clone();
        let plan_tx_slot = runner.plan_approval_rx.clone();
        {
            let sc = app_clone.state::<SessionControl>();
            let mut map = sc.inner.lock().await;
            map.insert(
                session_id.clone(),
                SessionHandle {
                    cancel: cancel.clone(),
                    approval_tx: approval_tx_slot.clone(),
                    plan_tx: plan_tx_slot.clone(),
                },
            );
        }

        runner.run(user_msg).await;

        // 跑完清理
        let sc = app_clone.state::<SessionControl>();
        let mut map = sc.inner.lock().await;
        map.remove(&session_id);
    });

    Ok(req.session_id)
}

/// v0.4：取消正在运行的 agent
#[tauri::command]
async fn cancel_chat(session_id: String, app: AppHandle) -> Result<(), String> {
    let sc = app.state::<SessionControl>();
    let map = sc.inner.lock().await;
    if let Some(h) = map.get(&session_id) {
        h.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        // 唤醒 approval（让主循环检测取消）
        let mut tx_slot = h.approval_tx.lock().await;
        if let Some(tx) = tx_slot.take() {
            let _ = tx.send(agent::ApprovalResponse::Deny("cancelled".into()));
        }
        Ok(())
    } else {
        Err(format!("session {} 不在运行中", session_id))
    }
}

/// v0.4：响应 approval 请求
#[tauri::command]
async fn respond_approval(
    session_id: String,
    approve: bool,
    reason: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    let sc = app.state::<SessionControl>();
    let map = sc.inner.lock().await;
    if let Some(h) = map.get(&session_id) {
        let mut tx_slot = h.approval_tx.lock().await;
        if let Some(tx) = tx_slot.take() {
            let resp = if approve {
                agent::ApprovalResponse::Approve
            } else {
                agent::ApprovalResponse::Deny(reason.unwrap_or_else(|| "user denied".into()))
            };
            tx.send(resp)
                .map_err(|_| "approval channel closed".to_string())?;
        }
        Ok(())
    } else {
        Err(format!("session {} 不在运行中", session_id))
    }
}

/// v0.6：响应 plan approval 请求
#[tauri::command]
async fn respond_plan(
    session_id: String,
    action: String,              // "approve" | "deny" | "edit"
    reason: Option<String>,      // for deny
    edited_plan: Option<String>, // for edit
    app: AppHandle,
) -> Result<(), String> {
    let sc = app.state::<SessionControl>();
    let map = sc.inner.lock().await;
    if let Some(h) = map.get(&session_id) {
        let mut tx_slot = h.plan_tx.lock().await;
        if let Some(tx) = tx_slot.take() {
            let resp = match action.as_str() {
                "approve" => agent::PlanApproval::Approve,
                "deny" => agent::PlanApproval::Deny(reason.unwrap_or_else(|| "user denied".into())),
                "edit" => agent::PlanApproval::Edit(edited_plan.unwrap_or_default()),
                _ => return Err(format!("unknown plan action: {}", action)),
            };
            tx.send(resp)
                .map_err(|_| "plan channel closed".to_string())?;
        }
        Ok(())
    } else {
        Err(format!("session {} 不在运行中", session_id))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRunPayload {
    model: String,
    message: String,
    session_id: String,
    #[serde(default)]
    messages: Vec<AgentHistoryMessage>,
    /// v0.4：是否需要用户批准 tool call
    #[serde(default = "default_true")]
    require_approval: bool,
    /// v0.6：plan mode —— 先输出 plan 等用户批准
    #[serde(default)]
    plan_mode: bool,
    /// v0.9：附件图片（绝对路径）
    #[serde(default)]
    images: Vec<ImageAttachment>,
    /// v1.9.x：项目组上下文（注入 system prompt）
    #[serde(default)]
    project_context: Option<ProjectContextPayload>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectContextPayload {
    workspace_id: String,
    name: String,
    #[serde(default)]
    folder_path: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

fn build_project_context(prefix: &str, ctx: &ProjectContextPayload) -> String {
    let mut parts = vec![format!("项目组: {}", ctx.name)];
    parts.push(format!("项目组 ID: {}", ctx.workspace_id));
    if let Some(p) = ctx.folder_path.as_ref().filter(|p| !p.is_empty()) {
        parts.push(format!("项目根目录: {}", p));
        // 尝试读取 README/AGENTS.md 摘要
        for filename in &["README.md", "README.MD", "readme.md", "AGENTS.md", "agents.md"] {
            let candidate = std::path::Path::new(p).join(filename);
            if let Ok(text) = std::fs::read_to_string(&candidate) {
                let summary: String = text.chars().take(600).collect();
                parts.push(format!("{} ({} 摘录):\n{}", filename, candidate.display(), summary));
                break;
            }
        }
    }
    if let Some(d) = ctx.description.as_ref().filter(|d| !d.is_empty()) {
        parts.push(format!("项目简介: {}", d));
    }
    parts.push("在回答与该项目的相关问题时，请优先使用上述项目根目录、README 和项目简介作为上下文。".into());
    format!("{}\n{}", prefix, parts.join("\n"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageAttachment {
    path: String,
    #[serde(default)]
    mime: Option<String>,
}

fn default_true() -> bool {
    true
}

/// v0.9：从扩展名猜 MIME
fn guess_mime_from_path(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        "image/png".into()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".into()
    } else if lower.ends_with(".gif") {
        "image/gif".into()
    } else if lower.ends_with(".webp") {
        "image/webp".into()
    } else {
        "image/png".into()
    }
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AgentHistoryMessage {
    role: String,
    content: String,
    #[serde(default)]
    tool_call_id: Option<String>,
}

/// 取消进行中的聊天（v0.2 占位，已被 v0.4 替换）

/// 列出已注册工具
#[tauri::command]
async fn list_tools(app: AppHandle) -> Result<Vec<ToolDefDto>, String> {
    let state = app.state::<SharedToolRegistry>();
    let mut reg = state.lock().await;
    // lazy 初始化（用 cwd）
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if reg.is_empty() {
        tools::register_all(&mut reg, cwd.clone(), cwd);
    }
    Ok(reg
        .schemas()
        .into_iter()
        .map(|s| ToolDefDto {
            name: s.name,
            description: s.description,
            parameters: s.parameters,
        })
        .collect())
}

/// 执行工具
#[tauri::command]
async fn execute_tool(
    app: AppHandle,
    name: String,
    arguments: serde_json::Value,
) -> Result<ToolExecDto, String> {
    let state = app.state::<SharedToolRegistry>();
    let mut reg = state.lock().await;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if reg.is_empty() {
        tools::register_all(&mut reg, cwd.clone(), cwd);
    }
    let tool = reg
        .get(&name)
        .ok_or_else(|| format!("tool not found: {}", name))?;
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
    Ok(s.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
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
// MCP
// ============================================================

/// 列出已连接的 MCP server
#[tauri::command]
async fn list_mcp_servers() -> Vec<McpServerDto> {
    let pool = mcp_tool::mcp_pool().await;
    let names = pool.list_servers().await;
    let mut out = Vec::new();
    for n in names {
        let tool_count = pool.tools_of(&n).await.map(|v| v.len()).unwrap_or(0);
        out.push(McpServerDto {
            name: n,
            tool_count,
        });
    }
    out
}

/// 重新加载 MCP 配置（~/.agentshell/mcp.json）
#[tauri::command]
async fn reload_mcp(app: AppHandle) -> Result<usize, String> {
    // v0.9：清空 registry 里所有 mcp__ 前缀的 tool，然后重新注册
    {
        let state = app.state::<SharedToolRegistry>();
        let mut reg = state.lock().await;
        // 注：ToolRegistry 没有 unregister_by_prefix；这里只重新注册，重复注册会由 register 去重
        mcp_tool::register_mcp_tools(&mut reg).await;
    }
    let pool = mcp_tool::mcp_pool().await;
    let names = pool.list_servers().await;
    Ok(names.len())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct McpServerDto {
    name: String,
    tool_count: usize,
}

/// 激活码 demo 密钥已迁到 `license_tauri::ActivationCodeProvider::default_demo`（v1.6）
/// 旧实现 `activate_license` / `get_license_status` / `deactivate_license` 替换为：
/// - `license_tauri::license_status`
/// - `license_tauri::license_activate`
/// - `license_tauri::license_deactivate`
/// - `license_tauri::license_refresh`
/// - `license_tauri::license_tiers`
/// - `license_tauri::license_demo_code`（dev 工具）

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
/// v0.7：根据任务内容自动选 model
fn route_model(message: &str) -> String {
    let lower = message.to_lowercase();
    // 代码相关 → DeepSeek（便宜 + 代码强）
    let code_kw = [
        "code",
        "function",
        "fn ",
        "impl ",
        "bug",
        "debug",
        "error",
        "rust",
        "python",
        "javascript",
        "typescript",
        "compile",
        "refactor",
        "重构",
        "编译",
        "报错",
        "代码",
        "写一个",
        "函数",
        "bug",
    ];
    // 中文对话 / 创意 → MiniMax-M3
    let m3_kw = ["你好", "请问", "聊聊", "故事", "创作", "诗", "翻译", "总结"];
    // 规划 / 复杂推理 → Claude
    let claude_kw = [
        "plan",
        "分析",
        "规划",
        "策略",
        "compare",
        "tradeoff",
        "复杂",
        "深度",
        "reasoning",
        "compare",
    ];

    let code_score = code_kw.iter().filter(|k| lower.contains(**k)).count();
    let m3_score = m3_kw.iter().filter(|k| lower.contains(**k)).count();
    let claude_score = claude_kw.iter().filter(|k| lower.contains(**k)).count();

    if code_score >= 2 && code_score > m3_score {
        return "deepseek-chat".to_string();
    }
    if claude_score >= 2 {
        return "claude-sonnet-4-5".to_string();
    }
    if m3_score >= 2 {
        return "MiniMax-M3".to_string();
    }
    // 默认 MiniMax M3
    "MiniMax-M3".to_string()
}

/// v0.7：model routing Tauri 命令
#[tauri::command]
async fn route_model_cmd(message: String) -> Result<String, String> {
    Ok(route_model(&message))
}

// ============================================================
// v0.8：跨会话长期记忆命令
// ============================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryDto {
    id: String,
    content: String,
    tags: Vec<String>,
    importance: u8,
    created_at: String,
    last_accessed_at: Option<String>,
    accessed_count: u32,
    session_id: Option<String>,
}

impl From<memory::Memory> for MemoryDto {
    fn from(m: memory::Memory) -> Self {
        Self {
            id: m.id,
            content: m.content,
            tags: m.tags,
            importance: m.importance,
            created_at: m.created_at.to_rfc3339(),
            last_accessed_at: m.last_accessed_at.map(|d| d.to_rfc3339()),
            accessed_count: m.accessed_count,
            session_id: m.session_id,
        }
    }
}

#[tauri::command]
async fn remember_memory(
    app: AppHandle,
    content: String,
    tags: Option<Vec<String>>,
    importance: Option<u8>,
    session_id: Option<String>,
) -> Result<MemoryDto, String> {
    let mgr = app.state::<SharedMemory>().inner().lock().await;
    let mem = mgr
        .add_with_session(
            content,
            tags.unwrap_or_default(),
            importance.unwrap_or(3),
            session_id,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(mem.into())
}

#[tauri::command]
async fn recall_memory(app: AppHandle, query: String, k: Option<usize>) -> Result<String, String> {
    let mgr = app.state::<SharedMemory>().inner().lock().await;
    Ok(mgr.recall_context(&query, k.unwrap_or(5)).await)
}

#[tauri::command]
async fn list_memories(app: AppHandle) -> Result<Vec<MemoryDto>, String> {
    let mgr = app.state::<SharedMemory>().inner().lock().await;
    let all = mgr.list().await;
    Ok(all.into_iter().map(MemoryDto::from).collect())
}

#[tauri::command]
async fn forget_memory(app: AppHandle, id: String) -> Result<bool, String> {
    let mgr = app.state::<SharedMemory>().inner().lock().await;
    mgr.forget(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_memories(app: AppHandle) -> Result<(), String> {
    let mgr = app.state::<SharedMemory>().inner().lock().await;
    mgr.clear().await.map_err(|e| e.to_string())
}

// ============================================================
// v0.8：Skill 系统命令
// ============================================================

#[tauri::command]
async fn list_skills() -> Result<Vec<skills::SkillInfo>, String> {
    let file = skills::load_skills();
    Ok(skills::to_command_map(&file).into_values().collect())
}

/// v1.5：列出全部 skill（含 builtin / 禁用 / 非 shell），按 category 分组
#[tauri::command]
async fn list_skills_grouped(
) -> Result<std::collections::HashMap<String, Vec<skills::Skill>>, String> {
    let file = skills::load_skills();
    Ok(skills::list_grouped(&file))
}

#[tauri::command]
async fn run_skill(name: String, arg: String) -> Result<String, String> {
    let file = skills::load_skills();
    match skills::find_skill(&file, &name) {
        Some(s) => skills::execute_skill(s, &arg),
        None => Err(format!(
            "skill `{}` 未定义。检查 ~/.agentshell/skills.json",
            name
        )),
    }
}

/// v1.5：模板市场
#[tauri::command]
async fn skill_market() -> Result<Vec<skills::SkillTemplate>, String> {
    Ok(skills::template_market())
}

/// v1.5：导出单个 skill 为 JSON
#[tauri::command]
async fn skill_export(name: String) -> Result<String, String> {
    let file = skills::load_skills();
    match skills::find_skill_any(&file, &name) {
        Some(s) => skills::export_skill(s),
        None => Err(format!("skill `{name}` 不存在")),
    }
}

/// v1.5：从 JSON 导入 skill
#[tauri::command]
async fn skill_import(json: String) -> Result<String, String> {
    let s = skills::import_skill(&json)?;
    let name = s.name.clone();
    skills::upsert_skill(s)?;
    Ok(name)
}

/// v1.5：启用 / 禁用一个 skill（builtin 也可禁，但不能删）
#[tauri::command]
async fn skill_toggle(name: String, enabled: bool) -> Result<(), String> {
    let file = skills::load_skills();
    let s = skills::find_skill_any(&file, &name)
        .ok_or_else(|| format!("skill `{name}` 不存在"))?
        .clone();
    let mut s = s;
    s.enabled = enabled;
    skills::upsert_skill(s)
}

/// v1.5：删除一个 user skill（不会删 builtin）
#[tauri::command]
async fn skill_remove(name: String) -> Result<(), String> {
    skills::remove_skill(&name)
}

/// v1.5：重置 builtin（删 user skills.json）
#[tauri::command]
async fn skill_reset_builtin() -> Result<(), String> {
    skills::reset_builtin()
}

/// v1.5：执行 chain
#[tauri::command]
async fn skill_chain(
    names: Vec<String>,
    arg: String,
) -> Result<Vec<(String, Result<String, String>)>, String> {
    let file = skills::load_skills();
    Ok(skills::chain_skills(&file, &names, &arg))
}

// =============================================================================
// v1.5 TTS 命令
// =============================================================================

#[tauri::command]
async fn tts_detect() -> Result<tts::TtsStatus, String> {
    Ok(tts::detect().await)
}

#[tauri::command]
async fn tts_get_config() -> Result<tts::TtsConfig, String> {
    Ok(tts::TtsConfig::load().await)
}

#[tauri::command]
async fn tts_save_config(config: tts::TtsConfig) -> Result<(), String> {
    config.save().await
}

#[tauri::command]
async fn tts_speak(text: String) -> Result<(), String> {
    let cfg = tts::TtsConfig::load().await;
    if !cfg.enabled {
        return Err("TTS 未启用，请先在设置里开启".to_string());
    }
    tts::speak(text, cfg);
    Ok(())
}

#[tauri::command]
async fn tts_speak_with(text: String, config: tts::TtsConfig) -> Result<(), String> {
    if !config.enabled {
        return Err("TTS 未启用".to_string());
    }
    tts::speak(text, config);
    Ok(())
}

// =============================================================================
// v1.5 Graph 流程图命令
// =============================================================================

/// 从 plan markdown 文本生成图
#[tauri::command]
async fn graph_from_plan(plan: String, title: Option<String>) -> Result<graph::Graph, String> {
    let mut g = graph::from_plan(&plan);
    if let Some(t) = title {
        g.title = Some(t);
    }
    Ok(g)
}

/// 输出 Mermaid 字符串
#[tauri::command]
async fn graph_to_mermaid(g: graph::Graph) -> Result<String, String> {
    Ok(g.to_mermaid())
}

// =============================================================================
// v1.5 Session Sync 命令
// =============================================================================

#[tauri::command]
async fn sync_publish(bundle: sync::SessionBundle) -> Result<(), String> {
    sync::publish(bundle)
}

#[tauri::command]
async fn sync_fetch(session_id: String) -> Result<Option<sync::SessionBundle>, String> {
    sync::fetch(&session_id)
}

#[tauri::command]
async fn sync_list() -> Result<sync::SyncStatus, String> {
    sync::list()
}

#[tauri::command]
async fn sync_remove(session_id: String) -> Result<(), String> {
    sync::remove(&session_id)
}

#[tauri::command]
async fn sync_clear_all() -> Result<usize, String> {
    sync::clear_all()
}

#[tauri::command]
async fn sync_schema_version() -> Result<u32, String> {
    Ok(sync::schema_version())
}

// =============================================================================
// v1.5 Plugin 热加载命令
// =============================================================================

#[tauri::command]
async fn plugin_list() -> Result<Vec<plugin::PluginManifest>, String> {
    let reg = plugin::load_registry();
    Ok(reg.plugins.into_values().collect())
}

#[tauri::command]
async fn plugin_install(json: String) -> Result<String, String> {
    plugin::install(&json)
}

#[tauri::command]
async fn plugin_remove(name: String) -> Result<(), String> {
    plugin::remove(&name)
}

#[tauri::command]
async fn plugin_reload() -> Result<usize, String> {
    let reg = plugin::load_registry();
    Ok(reg.plugins.len())
}

#[tauri::command]
async fn plugin_install_defaults() -> Result<Vec<String>, String> {
    plugin::install_defaults()
}

#[tauri::command]
async fn plugin_run_steps(steps: Vec<plugin::PluginStep>, input: String) -> Result<String, String> {
    Ok(plugin::run_steps(&steps, &input))
}

#[tauri::command]
async fn plugin_invoke(
    kind: String,
    command: Option<String>,
    input: String,
) -> Result<String, String> {
    let k = match kind.as_str() {
        "pre_send" | "PreSend" => plugin::HookKind::PreSend,
        "post_recv" | "PostRecv" => plugin::HookKind::PostRecv,
        "slash" | "Slash" => plugin::HookKind::Slash,
        _ => return Err(format!("unknown hook kind: {kind}")),
    };
    let reg = plugin::load_registry();
    Ok(plugin::invoke(&reg, k, command.as_deref(), &input))
}

// ============================================================
// v1.0：长会话压缩
// ============================================================

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompressPayload {
    model: String,
    /// 旧消息（user/assistant 交替）
    messages: Vec<AgentHistoryMessage>,
    /// 保留最近 K 条不压缩（默认 6）
    #[serde(default = "default_keep_recent")]
    keep_recent: usize,
}

fn default_keep_recent() -> usize {
    6
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompressResult {
    summary: String,
    original_count: usize,
    summary_count: usize,
    /// 压缩后的消息历史（只剩 summary + recent）
    new_messages: Vec<AgentHistoryMessage>,
}

#[tauri::command]
async fn compress_session(req: CompressPayload) -> Result<CompressResult, String> {
    let n = req.messages.len();
    if n <= req.keep_recent + 2 {
        return Err(format!(
            "消息数 ({}) 太少，无需压缩（需要 > {})",
            n,
            req.keep_recent + 2
        ));
    }

    // 构造 prompt
    let mut transcript = String::new();
    for m in &req.messages[..n - req.keep_recent] {
        transcript.push_str(&format!("[{}] {}\n", m.role, m.content));
    }
    let summary_prompt = format!(
        "请用 150 字以内总结以下对话的关键信息（用户目标、决定、待办、关键事实）。保留命令/路径/名称/人名等具体信息。\n\n对话：\n{}\n\n总结：",
        transcript
    );

    // 用指定 model 调用
    let provider = create_provider(&req.model).await?;
    let chat_req = ChatRequest {
        model: req.model.clone(),
        messages: vec![ChatMessage::user(summary_prompt)],
        tools: Vec::new(),
        max_tokens: Some(400),
        temperature: Some(0.3),
        top_p: None,
        reasoning_effort: None,
        reasoning_split: None,
        stop: Vec::new(),
        stream: false,
        user: None,
    };
    let resp = provider.chat(chat_req).await.map_err(|e| e.to_string())?;
    let summary = resp
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_default();
    if summary.is_empty() {
        return Err("LLM 总结返回空".into());
    }

    // 构造新 history：1 条 summary 消息 + 最近 K 条
    let mut new_messages = Vec::new();
    new_messages.push(AgentHistoryMessage {
        role: "system".into(),
        content: format!("[之前的对话摘要] {}", summary),
        tool_call_id: None,
    });
    for m in &req.messages[n - req.keep_recent..] {
        new_messages.push(m.clone());
    }

    Ok(CompressResult {
        summary,
        original_count: n,
        summary_count: new_messages.len(),
        new_messages,
    })
}

// ============================================================
// v1.0：Auto-update 检查
// ============================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    current_version: String,
    latest_version: Option<String>,
    update_available: bool,
    release_url: Option<String>,
    release_notes: Option<String>,
}

#[tauri::command]
async fn check_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    // v1.0 简化：调 GitHub Releases API 检查最新 tag
    let url = "https://api.github.com/repos/opc007/codex-gx/releases/latest";
    let client = reqwest::Client::builder()
        .user_agent("AgentShell")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Ok(UpdateInfo {
            current_version: current,
            latest_version: None,
            update_available: false,
            release_url: None,
            release_notes: None,
        });
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let latest = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches('v').to_string());
    let release_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .map(String::from);
    let release_notes = body.get("body").and_then(|v| v.as_str()).map(String::from);

    // 简单版本比较（vX.Y.Z）
    let update_available = match (&latest, &current) {
        (Some(l), c) => l != c && version_greater(l, c),
        _ => false,
    };

    Ok(UpdateInfo {
        current_version: current,
        latest_version: latest,
        update_available,
        release_url,
        release_notes,
    })
}

/// 比较 vX.Y.Z 字符串
fn version_greater(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.split('-').next().unwrap_or("").parse::<u32>().ok())
            .collect()
    };
    let av = parse(a);
    let bv = parse(b);
    for i in 0..av.len().max(bv.len()) {
        let x = av.get(i).copied().unwrap_or(0);
        let y = bv.get(i).copied().unwrap_or(0);
        if x > y {
            return true;
        }
        if x < y {
            return false;
        }
    }
    false
}

async fn create_provider(model: &str) -> Result<Box<dyn Model>, String> {
    // v0.7：模型路由 — "auto" 根据任务自动选 model
    if model == "auto" {
        return Err("auto 路由需要在 agent_run 前先用 route_model 计算实际模型".to_string());
    }
    match model {
        "MiniMax-M3" | "m3" => {
            let key = std::env::var("MINIMAX_API_KEY").map_err(|_| {
                "未配置 MiniMax API Key。请点击右上角 ⋯ →「API Key 设置」填写并保存。".to_string()
            })?;
            Ok(Box::new(MinimaxProvider::new(key, None)))
        }
        m if m.starts_with("claude-") => {
            let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                "未配置 Anthropic API Key。请点击 ⋯ →「API Key 设置」填写。".to_string()
            })?;
            Ok(Box::new(AnthropicProvider::new(m, key, None)))
        }
        m if m.starts_with("deepseek-") => {
            let key = std::env::var("DEEPSEEK_API_KEY").map_err(|_| {
                "未配置 DeepSeek API Key。请点击 ⋯ →「API Key 设置」填写。".to_string()
            })?;
            Ok(Box::new(DeepSeekProvider::new(m, key, None)))
        }
        m if m.starts_with("gpt-") => {
            let key = std::env::var("OPENAI_API_KEY").map_err(|_| {
                "未配置 OpenAI API Key。请点击 ⋯ →「API Key 设置」填写。".to_string()
            })?;
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
        m if m.starts_with("ollama:") => {
            // v1.4：本地 Ollama
            let name = m.trim_start_matches("ollama:").to_string();
            let base_url = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
            Ok(Box::new(OllamaProvider::new(ollama_info(&name), base_url)))
        }
        m if m.starts_with("llamacpp:") => {
            // v1.4：本地 llama.cpp server（OpenAI 兼容）
            let name = m.trim_start_matches("llamacpp:").to_string();
            let base_url = std::env::var("LLAMACPP_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
            Ok(Box::new(LlamaCppProvider::new(
                llama_cpp_info(&name),
                base_url,
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
            models: vec![
                "deepseek-v4-pro".into(),
                "deepseek-chat".into(),
                "deepseek-reasoner".into(),
            ],
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
