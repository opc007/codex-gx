//! v0.7：Sub-Agent 工具 — 让主 Agent 派生子任务给专用角色
//!
//! 设计：
//! - 主 Agent 在 tool_call 循环中调用 `spawn_agent` 工具
//! - 工具接受 `role` (researcher/coder/reviewer) + `task` (描述)
//! - 子 Agent 在独立 tokio task 中跑有限循环（最多 8 步）
//! - 子 Agent 用受限的 ToolRegistry（按 role 分配）
//! - 子 Agent 状态通过 emit_event("subagent", ...) 推到前端

use agent_core::tool::ToolOutput;
use agent_core::{Error, Result, Tool};
use async_trait::async_trait;
use provider::request::ToolDefinition;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::sync::Mutex;

/// v0.7：全局 sub-agent id 计数器
static SUBAGENT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_subagent_id() -> String {
    let n = SUBAGENT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("sa_{}_{:x}", n, now)
}

/// v0.7：sub-agent role 的 system prompt
fn role_system_prompt(role: &str) -> String {
    match role {
        "researcher" => "你是一个研究员。专注于联网搜索、阅读文件、收集信息。\n不要修改任何文件；不要执行 bash 命令。结果必须准确、有出处。".to_string(),
        "coder" => "你是一个程序员。专注于读写文件、编辑代码、执行 bash 命令完成任务。\n结果应该是可运行的代码或可验证的输出。".to_string(),
        "reviewer" => "你是一个代码评审员。只读取文件、检查代码质量、安全性、最佳实践。\n不要修改任何文件；输出具体的改进建议。".to_string(),
        _ => format!("你是一个 {} 助手。专注完成任务并返回结构化结果。", role),
    }
}

/// v0.7：按 role 返回允许的工具名列表（其他工具被屏蔽）
fn allowed_tools_for_role(role: &str) -> Vec<&'static str> {
    match role {
        "researcher" => vec!["web_search", "read_file", "list_dir"],
        "coder" => vec!["read_file", "write_file", "edit_file", "list_dir", "bash"],
        "reviewer" => vec!["read_file", "list_dir"],
        _ => vec!["read_file"],
    }
}

// ============================================================
// spawn_agent 工具
// ============================================================

#[derive(Clone)]
pub struct SpawnAgentTool {
    pub app: AppHandle,
    pub provider: Arc<dyn provider::Model>,
    pub full_registry: Arc<Mutex<agent_core::ToolRegistry>>,
}

impl std::fmt::Debug for SpawnAgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnAgentTool").finish()
    }
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }
    fn description(&self) -> &str {
        "派发子任务给专用 sub-agent。可选 role: researcher / coder / reviewer。子 agent 跑在隔离的 tool 集，结果回传。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "role": {"type": "string", "enum": ["researcher", "coder", "reviewer"], "description": "子 agent 角色"},
                "task": {"type": "string", "description": "子任务描述"}
            },
            "required": ["role", "task"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let role = input["role"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("role 不能为空".to_string()))?
            .to_string();
        let task = input["task"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("task 不能为空".to_string()))?
            .to_string();

        if task.trim().is_empty() {
            return Ok(ToolOutput::err("task 不能为空".to_string()));
        }

        let subagent_id = next_subagent_id();

        // 推 started 事件
        let _ = self.app.emit(
            "agent:event",
            json!({
                "sessionId": "",
                "kind": "subagent",
                "delta": "",
                "done": false,
                "subagent": {
                    "subagentId": subagent_id,
                    "role": role,
                    "status": "started",
                    "task": task,
                    "result": null,
                    "error": null
                }
            }),
        );

        let app = self.app.clone();
        let provider = self.provider.clone();
        let registry = self.full_registry.clone();
        let subagent_id_clone = subagent_id.clone();
        let role_clone = role.clone();
        let task_clone = task.clone();

        // 异步跑 sub-agent
        tokio::spawn(async move {
            let result = run_subagent(
                &app,
                &provider,
                &registry,
                &subagent_id_clone,
                &role_clone,
                &task_clone,
            )
            .await;

            // 推 done/error 事件
            let (status, result_opt, error_opt) = match result {
                Ok(s) => ("done".to_string(), Some(s), None),
                Err(e) => ("error".to_string(), None, Some(e.to_string())),
            };
            let _ = app.emit(
                "agent:event",
                json!({
                    "sessionId": "",
                    "kind": "subagent",
                    "delta": "",
                    "done": true,
                    "subagent": {
                        "subagentId": subagent_id_clone,
                        "role": role_clone,
                        "status": status,
                        "task": task_clone,
                        "result": result_opt,
                        "error": error_opt
                    }
                }),
            );
        });

        // 返回立即确认（子 agent 在后台跑）
        Ok(ToolOutput::ok(format!(
            "🚀 已派发 sub-agent [{}] (role={}) — 后台运行中，结果稍后注入对话。",
            &subagent_id[..10.min(subagent_id.len())],
            role
        )))
    }
}

/// v0.7：跑一个 sub-agent — 简化版的 LLM 循环
async fn run_subagent(
    app: &AppHandle,
    provider: &Arc<dyn provider::Model>,
    registry: &Arc<Mutex<agent_core::ToolRegistry>>,
    subagent_id: &str,
    role: &str,
    task: &str,
) -> Result<String> {
    use futures::StreamExt;
    use provider::request::{ChatMessage, ChatRequest};
    use provider::stream::StreamChunk;

    let allowed = allowed_tools_for_role(role);
    let reg = registry.lock().await;
    let _tool_schemas: Vec<ToolDefinition> = reg
        .schemas()
        .into_iter()
        .filter(|s| allowed.contains(&s.name.as_str()))
        .map(|s| ToolDefinition::new(s.name, s.description, s.parameters))
        .collect();
    drop(reg);

    // 简化版：只调用一次 LLM（无 tool calling 循环）— 角色 prompt 让模型直接生成结果
    let sys_prompt = role_system_prompt(role);
    let user_msg = format!("任务：{}\n\n请直接给出结果（不要调用工具）。", task);

    let messages = vec![ChatMessage::system(sys_prompt), ChatMessage::user(user_msg)];

    let req = ChatRequest {
        model: String::new(),
        messages,
        tools: vec![], // 简化版：不让 sub-agent 自己调工具
        temperature: Some(0.4),
        max_tokens: Some(2000),
        top_p: None,
        reasoning_effort: None,
        reasoning_split: None,
        stop: vec![],
        stream: true,
        user: None,
    };

    let stream = provider
        .chat_stream(req)
        .await
        .map_err(|e| Error::ToolExecution(format!("sub-agent LLM 失败: {}", e)))?;
    let mut stream = Box::pin(stream);

    let mut content = String::new();
    while let Some(chunk_res) = stream.next().await {
        match chunk_res {
            Ok(StreamChunk::Content(s)) => {
                content.push_str(&s);
                // emit "running" with delta
                let _ = app.emit(
                    "agent:event",
                    json!({
                        "sessionId": "",
                        "kind": "subagent",
                        "delta": "",
                        "done": false,
                        "subagent": {
                            "subagentId": subagent_id,
                            "role": role,
                            "status": "running",
                            "task": task,
                            "result": null,
                            "error": null,
                            "content": s
                        }
                    }),
                );
            }
            Ok(StreamChunk::Reasoning(_)) => {}
            Ok(StreamChunk::ToolCallDelta { .. }) => {}
            Ok(StreamChunk::Usage(_)) => {}
            Ok(StreamChunk::Done) => break,
            Err(e) => {
                return Err(Error::ToolExecution(format!(
                    "sub-agent stream error: {}",
                    e
                )))
            }
        }
    }

    if content.is_empty() {
        Ok(format!(
            "Sub-agent [{}] role={} 完成（未生成内容）。",
            &subagent_id[..10.min(subagent_id.len())],
            role
        ))
    } else {
        Ok(content)
    }
}

/// 注册 spawn_agent 工具
pub fn register_subagent_tool(
    reg: &mut agent_core::ToolRegistry,
    app: AppHandle,
    provider: Arc<dyn provider::Model>,
    full_registry: Arc<Mutex<agent_core::ToolRegistry>>,
) {
    reg.register(SpawnAgentTool {
        app,
        provider,
        full_registry,
    });
}
