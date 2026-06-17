//! AgentShell 桌面端入口 (Tauri 2)
//!
//! 详细设计见 docs/开发文档.md §3 / §6

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod cu_tool;
mod desktop_cua;
mod mcp_tool;
mod skills;
mod subagent_tool;
mod tools;

use agent_core::tool::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// 全局工具注册表 — 直接用 Arc<Mutex<>> 让 AgentRunner 也能 clone
type SharedToolRegistry = Arc<Mutex<ToolRegistry>>;

/// v0.8：跨会话长期记忆
type SharedMemory = Arc<Mutex<memory::MemoryManager>>;

/// v0.4：每个 session 一个 cancel handle + approval sender
#[derive(Default)]
struct SessionControl {
    inner: Mutex<HashMap<String, SessionHandle>>,
}

struct SessionHandle {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    approval_tx: std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<agent::ApprovalResponse>>>>,
    /// v0.6：plan approval sender
    plan_tx: std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<agent::PlanApproval>>>>,
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
        .setup(|app| {
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
            activate_license,
            get_license_status,
            deactivate_license,
            get_ide_context,
            get_git_diff,
            list_git_branches,
            list_mcp_servers,
            reload_mcp,
            route_model_cmd, // v0.7
            remember_memory, // v0.8
            recall_memory, // v0.8
            list_memories, // v0.8
            forget_memory, // v0.8
            clear_memories, // v0.8
            list_skills, // v0.8
            run_skill, // v0.8
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

/// v0.4 Agent 运行入口 — 含 tool_calls 循环 + cancel + approval
#[tauri::command]
async fn agent_run(
    app: AppHandle,
    req: AgentRunPayload,
) -> Result<String, String> {
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

    tokio::spawn(async move {
        let reg_state = app_clone.state::<SharedToolRegistry>();
        let reg_arc: Arc<Mutex<ToolRegistry>> = Arc::clone(&reg_state);
        let mut runner = agent::AgentRunner::new(
            app_clone.clone(),
            session_id.clone(),
            provider_arc,
            reg_arc,
        )
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
            tx.send(resp).map_err(|_| "approval channel closed".to_string())?;
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
    action: String, // "approve" | "deny" | "edit"
    reason: Option<String>, // for deny
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
            tx.send(resp).map_err(|_| "plan channel closed".to_string())?;
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
}

fn default_true() -> bool { true }

#[derive(Deserialize)]
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
    let state = app.state::<SharedToolRegistry>();
    let mut reg = state.lock().await;
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
// MCP
// ============================================================

/// 列出已连接的 MCP server
#[tauri::command]
async fn list_mcp_servers() -> Vec<McpServerDto> {
    let mgr = mcp_tool::mcp_manager().await;
    let mgr_lock = mgr.lock().await;
    let names = mgr_lock.server_names();
    let mut out = Vec::new();
    for n in names {
        let tool_count = if let Some(c) = mgr_lock.get(&n) {
            c.lock().await.list_tools().await.map(|v| v.len()).unwrap_or(0)
        } else {
            0
        };
        out.push(McpServerDto { name: n, tool_count });
    }
    out
}

/// 重新加载 MCP 配置（~/.agentshell/mcp.json）
#[tauri::command]
async fn reload_mcp(app: AppHandle) -> Result<usize, String> {
    // 清掉旧 tools（清空 registry）
    {
        let state = app.state::<SharedToolRegistry>();
        let mut reg = state.lock().await;
        // 注：v0.5 简化 — 直接 push 新 tool 到已有 registry
        mcp_tool::register_mcp_tools(&mut reg).await;
    }
    let mgr = mcp_tool::mcp_manager().await;
    let mgr_lock = mgr.lock().await;
    Ok(mgr_lock.server_names().len())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct McpServerDto {
    name: String,
    tool_count: usize,
}

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
/// v0.7：根据任务内容自动选 model
fn route_model(message: &str) -> String {
    let lower = message.to_lowercase();
    // 代码相关 → DeepSeek（便宜 + 代码强）
    let code_kw = ["code", "function", "fn ", "impl ", "bug", "debug", "error", "rust", "python", "javascript", "typescript", "compile", "refactor", "重构", "编译", "报错", "代码", "写一个", "函数", "bug"];
    // 中文对话 / 创意 → MiniMax-M3
    let m3_kw = ["你好", "请问", "聊聊", "故事", "创作", "诗", "翻译", "总结"];
    // 规划 / 复杂推理 → Claude
    let claude_kw = ["plan", "分析", "规划", "策略", "compare", "tradeoff", "复杂", "深度", "reasoning", "compare"];

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
    let mgr = app
        .state::<SharedMemory>()
        .inner()
        .lock()
        .await;
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
    Ok(skills::to_command_map(&file)
        .into_values()
        .collect())
}

#[tauri::command]
async fn run_skill(name: String, arg: String) -> Result<String, String> {
    let file = skills::load_skills();
    match skills::find_skill(&file, &name) {
        Some(s) => skills::execute_skill(s, &arg),
        None => Err(format!("skill `{}` 未定义。检查 ~/.agentshell/skills.json", name)),
    }
}

async fn create_provider(model: &str) -> Result<Box<dyn Model>, String> {
    // v0.7：模型路由 — "auto" 根据任务自动选 model
    if model == "auto" {
        return Err(
            "auto 路由需要在 agent_run 前先用 route_model 计算实际模型".to_string(),
        );
    }
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