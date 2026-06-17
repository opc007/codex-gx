//! Agent 主循环 — 处理 tool_calls 流式循环
//!
//! 详细设计见 docs/开发文档.md §6 Agent Core
//!
//! ## 流程
//! ```text
//! user_message →
//!   loop {
//!     response = provider.chat_stream(req, tools)
//!     while chunk = response.next():
//!       content/thinking → emit "content"/"thinking"
//!       tool_call_delta → 累积
//!       done → execute each tool_call → emit "tool_result" → 喂回 history
//!     if no tool_calls → break
//!   }
//! ```text

use agent_core::tool::ToolRegistry;
use provider::model::Usage;
use provider::request::{ChatContentPart, ChatMessage, ChatRequest, ToolDefinition};
use provider::response::AssistantMessage;
use provider::stream::{ChatStream, StreamChunk};
use provider::Model;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex};

/// 单次 tool call（前端展示 + 批准）
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEvent {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub session_id: String,
}

/// 单次 tool call 结果
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultEvent {
    pub call_id: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub session_id: String,
}

/// v0.5：阶段标记（think → plan → act → verify）
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StageEvent {
    pub stage: String, // "think" | "plan" | "act" | "verify" | "done"
    pub label: String,
    pub detail: Option<String>,
}

/// v0.6：plan 完整内容（一次 emit）
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlanEvent {
    pub plan: String,
    pub plan_id: String,
}

/// v0.6：流式 chunk 事件
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StreamEvent {
    pub session_id: String,
    pub kind: String,
    pub delta: String,
    pub usage: Option<UsageDto>,
    pub done: bool,
    pub tool_call: Option<ToolCallEvent>,
    pub tool_result: Option<ToolResultEvent>,
    /// v0.5：阶段信息
    pub stage: Option<StageEvent>,
    /// v0.6：plan 内容
    pub plan: Option<PlanEvent>,
    /// v0.7：sub-agent 状态
    pub subagent: Option<SubAgentEvent>,
}

/// v0.7：子 Agent 状态事件
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentEvent {
    pub subagent_id: String,
    pub role: String,
    pub status: String,
    pub task: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// 流式 chunk 事件（v0.6 — 加 plan 字段）

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageDto {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl From<Usage> for UsageDto {
    fn from(u: Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
        }
    }
}

/// Agent runner
pub struct AgentRunner {
    pub app: AppHandle,
    pub session_id: String,
    pub provider: Arc<dyn Model>,
    pub tool_registry: Arc<Mutex<ToolRegistry>>,
    pub history: Vec<ChatMessage>,
    pub max_steps: usize,
    /// v0.4：取消标志
    pub cancelled: Arc<AtomicBool>,
    /// v0.4：每 session 一个 approval receiver，等待前端响应
    pub approval_rx: Arc<Mutex<Option<oneshot::Sender<ApprovalResponse>>>>,
    /// v0.4：是否启用 approval（false = auto-approve，true = 必须前端同意）
    pub require_approval: bool,
    /// v0.6：是否启用 plan mode（先输出 plan，批准后才执行）
    pub plan_mode: bool,
    /// v0.6：plan approval receiver（用户批准 plan）
    pub plan_approval_rx: Arc<Mutex<Option<oneshot::Sender<PlanApproval>>>>,
}

/// v0.6：plan 审批响应
#[derive(Debug, Clone)]
pub enum PlanApproval {
    Approve,
    Deny(String),
    Edit(String), // 编辑后批准
}

/// v0.4：approval 响应
#[derive(Debug, Clone)]
pub enum ApprovalResponse {
    Approve,
    Deny(String),
}

impl AgentRunner {
    pub fn new(
        app: AppHandle,
        session_id: String,
        provider: Arc<dyn Model>,
        tool_registry: Arc<Mutex<ToolRegistry>>,
    ) -> Self {
        Self {
            app,
            session_id,
            provider,
            tool_registry,
            history: Vec::new(),
            max_steps: 10,
            cancelled: Arc::new(AtomicBool::new(false)),
            approval_rx: Arc::new(Mutex::new(None)),
            require_approval: true,
            plan_mode: false,
            plan_approval_rx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_history(mut self, history: Vec<ChatMessage>) -> Self {
        self.history = history;
        self
    }

    pub fn with_max_steps(mut self, max: usize) -> Self {
        self.max_steps = max;
        self
    }

    pub fn with_require_approval(mut self, require: bool) -> Self {
        self.require_approval = require;
        self
    }

    pub fn with_plan_mode(mut self, mode: bool) -> Self {
        self.plan_mode = mode;
        self
    }

    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    /// 主循环
    pub async fn run(&mut self, user_message: String) {
        // 追加 user message
        self.history.push(ChatMessage::user(user_message));

        let mut total_input = 0u32;
        let mut total_output = 0u32;
        let mut step = 0;

        loop {
            // v0.4：检查取消
            if self.cancelled.load(Ordering::Relaxed) {
                self.emit_error("用户取消".into());
                return;
            }

            // v0.6：plan mode — 第一轮不带 tools，只让模型输出 markdown plan
            if self.plan_mode && step == 0 {
                self.execute_plan_phase().await;
                if self.cancelled.load(Ordering::Relaxed) {
                    return;
                }
                // plan 已批准，关闭 plan_mode，进入正常执行
                self.plan_mode = false;
                // 把"执行 plan"作为 system hint 喂给模型
                self.history.push(ChatMessage::system(
                    "用户已批准计划。请按计划逐步执行，使用合适的工具。",
                ));
            }

            step += 1;
            if step > self.max_steps {
                self.emit_error("超过最大步数（10），强制结束".into());
                return;
            }

            // v0.5：emit "think" stage
            self.emit_event(
                "stage",
                String::new(),
                None,
                false,
                None,
                None,
                Some(StageEvent {
                    stage: "think".into(),
                    label: format!("Step {}/{} - 思考中...", step, self.max_steps),
                    detail: None,
                }),
                None,                None,

            );

            // 构建请求（含 tools schema）
            let req = {
                let reg = self.tool_registry.lock().await;
                let mut req = ChatRequest::new(self.provider.info().id.as_str())
                    .with_messages(self.history.clone())
                    .with_max_tokens(4096)
                    .with_reasoning_effort("high")
                    .with_reasoning_split(true);
                for s in reg.schemas() {
                    req = req.with_tool(ToolDefinition::new(
                        s.name,
                        s.description,
                        s.parameters,
                    ));
                }
                req
            };

            // 调 provider 流式
            let stream_res = self.provider.chat_stream(req).await;
            let stream = match stream_res {
                Ok(s) => s,
                Err(e) => {
                    self.emit_error(format!("provider error: {}", e));
                    return;
                }
            };

            // 累积
            let mut acc_content = String::new();
            let mut acc_thinking = String::new();
            let mut tc_buffers: std::collections::BTreeMap<u32, (String, String, String)> =
                std::collections::BTreeMap::new();
            let mut finish_chunk = false;

            let mut pinned = Box::pin(stream);
            while let Some(chunk_res) = futures::StreamExt::next(&mut pinned).await {
                match chunk_res {
                    Ok(StreamChunk::Content(s)) => {
                        acc_content.push_str(&s);
                        self.emit_event("content", s, None, false, None, None, None, None, None);
                    }
                    Ok(StreamChunk::Reasoning(s)) => {
                        acc_thinking.push_str(&s);
                        self.emit_event("thinking", s, None, false, None, None, None, None, None);
                    }
                    Ok(StreamChunk::ToolCallDelta {
                        index,
                        id,
                        name,
                        arguments_delta,
                    }) => {
                        let entry = tc_buffers
                            .entry(index as u32)
                            .or_insert_with(|| (String::new(), String::new(), String::new()));
                        if let Some(i) = id {
                            if !i.is_empty() {
                                entry.0 = i;
                            }
                        }
                        if let Some(n) = name {
                            if !n.is_empty() {
                                entry.1 = n;
                            }
                        }
                        entry.2.push_str(&arguments_delta);
                        let payload = serde_json::json!({
                            "index": index,
                            "arguments_delta": arguments_delta,
                        });
                        self.emit_event("tool_call_delta", payload.to_string(), None, false, None, None, None, None, None);
                    }
Ok(StreamChunk::Usage(u)) => {
                            total_input = u.input_tokens;
                            total_output = u.output_tokens;
                            self.emit_event(
                                "usage",
                                String::new(),
                                Some(UsageDto::from(u)),
                                false,
                                None,
                                None,
                                None,
                                None,                                None,

                            );
                        }
                    Ok(StreamChunk::Done) => {
                        finish_chunk = true;
                        break;
                    }
                    Err(e) => {
                        self.emit_error(format!("stream error: {}", e));
                        return;
                    }
                }
            }

            // 转 tool_calls
            let tool_calls: Vec<(String, String, serde_json::Value)> = tc_buffers
                .into_iter()
                .map(|(_, (id, name, args_json))| {
                    let args: serde_json::Value = serde_json::from_str(&args_json)
                        .unwrap_or_else(|_| serde_json::Value::String(args_json));
                    (id, name, args)
                })
                .collect();

            // 追加 assistant message 到 history（content + 可能的 tool_use）
            let mut assistant_parts = Vec::new();
            if !acc_content.is_empty() {
                assistant_parts.push(ChatContentPart::Text {
                    text: acc_content.clone(),
                });
            }
            for (id, name, input) in &tool_calls {
                assistant_parts.push(ChatContentPart::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
            let assistant_msg = ChatMessage {
                role: provider::request::ChatRole::Assistant,
                content: assistant_parts,
                reasoning_content: if acc_thinking.is_empty() {
                    None
                } else {
                    Some(acc_thinking.clone())
                },
                tool_call_id: None,
            };
            self.history.push(assistant_msg);

            // 整 turn 完成
            self.emit_event(
                "assistant_turn",
                String::new(),
                None,
                false,
                None,
                None,
                None,
                None,                None,

            );

            // 没 tool_call 就结束
            if tool_calls.is_empty() {
                // v0.5: verify stage
                self.emit_event(
                    "stage",
                    String::new(),
                    None,
                    false,
                    None,
                    None,
                    Some(StageEvent {
                        stage: "verify".into(),
                        label: "生成最终回答".into(),
                        detail: None,
                    }),
                    None,                    None,

                );
                self.emit_event(
                    "done",
                    String::new(),
                    Some(UsageDto {
                        input_tokens: total_input,
                        output_tokens: total_output,
                    }),
                    true,
                    None,
                    None,
                    Some(StageEvent {
                        stage: "done".into(),
                        label: "完成".into(),
                        detail: None,
                    }),
                    None,                    None,

                );
                return;
            }

            // v0.5: act stage
            self.emit_event(
                "stage",
                String::new(),
                None,
                false,
                None,
                None,
                Some(StageEvent {
                    stage: "act".into(),
                    label: format!("执行 {} 个 tool call", tool_calls.len()),
                    detail: Some(
                        tool_calls
                            .iter()
                            .map(|(_, n, _)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                }),
                None,                None,

            );

            // 执行每个 tool_call
            for (id, name, args) in &tool_calls {
                let tc_evt = ToolCallEvent {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: args.clone(),
                    session_id: self.session_id.clone(),
                };
                self.emit_event(
                    "tool_call_complete",
                    String::new(),
                    None,
                    false,
                    Some(tc_evt.clone()),
                    None,
                    None,
                    None,                    None,

                );

                // v0.4：检查取消
                if self.cancelled.load(Ordering::Relaxed) {
                    self.emit_error("用户取消".into());
                    return;
                }

                // v0.4：请求 approval
                let approved = if self.require_approval {
                    let (tx, rx) = oneshot::channel::<ApprovalResponse>();
                    {
                        let mut slot = self.approval_rx.lock().await;
                        *slot = Some(tx);
                    }
                    // emit approval_request
                    self.emit_event(
                        "approval_request",
                        String::new(),
                        None,
                        false,
                        Some(tc_evt.clone()),
                        None,
                        Some(StageEvent {
                            stage: "approval".into(),
                            label: "等待用户批准".into(),
                            detail: Some(name.clone()),
                        }),
                        None,                        None,

                    );
                    // 等响应（带超时 5 分钟）
                    match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
                        Ok(Ok(ApprovalResponse::Approve)) => true,
                        Ok(Ok(ApprovalResponse::Deny(reason))) => {
                            self.emit_event(
                                "tool_result",
                                String::new(),
                                None,
                                false,
                                None,
                                Some(ToolResultEvent {
                                    call_id: id.clone(),
                                    success: false,
                                    output: format!("[被用户拒绝: {}]", reason),
                                    error: Some("denied".into()),
                                    session_id: self.session_id.clone(),
                                }),
                                None,
                                None,                                None,

                            );
                            // 仍要喂回 history，让模型知道被拒
                            self.history.push(ChatMessage::tool(
                                id.clone(),
                                format!("[Denied by user] {}", reason),
                            ));
                            continue;
                        }
                        Ok(Err(_)) => false, // channel closed
                        Err(_) => {
                            // timeout
                            self.emit_error("approval 超时（5 分钟）".into());
                            return;
                        }
                    }
                } else {
                    true
                };
                if !approved {
                    continue;
                }

                let (success, output, error) = {
                    let mut reg = self.tool_registry.lock().await;
                    match reg.get(name) {
                        Some(tool) => match tool.execute(args.clone()).await {
                            Ok(out) => (out.success, out.output, out.error),
                            Err(e) => (false, String::new(), Some(e.to_string())),
                        },
                        None => (false, String::new(), Some(format!("tool not found: {}", name))),
                    }
                };

                let tr_evt = ToolResultEvent {
                    call_id: id.clone(),
                    success,
                    output: output.clone(),
                    error: error.clone(),
                    session_id: self.session_id.clone(),
                };
                self.emit_event(
                    "tool_result",
                    String::new(),
                    None,
                    false,
                    None,
                    Some(tr_evt),
                    None,
                    None,                    None,

                );

                // 把结果喂回 history
                let result_text = if success {
                    output
                } else {
                    format!("[ERROR] {}", error.unwrap_or_else(|| "no message".into()))
                };
                self.history.push(ChatMessage::tool(id, result_text));
            }

            if !finish_chunk && step >= self.max_steps {
                break;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_event(
        &self,
        kind: &str,
        delta: String,
        usage: Option<UsageDto>,
        done: bool,
        tool_call: Option<ToolCallEvent>,
        tool_result: Option<ToolResultEvent>,
        stage: Option<StageEvent>,
        plan: Option<PlanEvent>,
        subagent: Option<SubAgentEvent>, // v0.7
    ) {
        let evt = StreamEvent {
            session_id: self.session_id.clone(),
            kind: kind.into(),
            delta,
            usage,
            done,
            tool_call,
            tool_result,
            stage,
            plan,
            subagent,
        };
        let _ = self.app.emit("agent:event", evt);
    }

    fn emit_error(&self, msg: String) {
        self.emit_event("error", msg, None, true, None, None, None, None, None);
    }

    /// v0.6：plan mode 第一轮 —— 不带 tools 让模型输出 markdown plan，
    /// emit "plan" 事件给前端，等待 PlanApproval。
    async fn execute_plan_phase(&mut self) {
        // v0.5: plan stage
        self.emit_event(
            "stage",
            String::new(),
            None,
            false,
            None,
            None,
            Some(StageEvent {
                stage: "plan".into(),
                label: "制定计划中...".into(),
                detail: None,
            }),
            None,            None,

        );

        // 不带 tools，让模型只输出计划文本
        let req = ChatRequest {
            model: String::new(),
            messages: self.history.clone(),
            tools: Vec::new(), // 关键：不提供 tools
            temperature: Some(0.4),
            max_tokens: Some(800),
            top_p: None,
            reasoning_effort: None,
            reasoning_split: None,
            stop: Vec::new(),
            stream: true,
            user: None,
        };

        // 收集完整 plan 文本
        let mut plan_text = String::new();
        let stream = match self.provider.chat_stream(req).await {
            Ok(s) => s,
            Err(e) => {
                self.emit_error(format!("plan 生成失败: {}", e));
                return;
            }
        };
        use futures::StreamExt;
        let mut stream = Box::pin(stream);
        let mut last_usage: Option<Usage> = None;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(StreamChunk::Content(s)) => {
                    plan_text.push_str(&s);
                    // 同步把 plan 内容作为 content 推到前端，让用户实时看到
                    self.emit_event(
                        "content",
                        s,
                        None,
                        false,
                        None,
                        None,
                        None,
                        None,                        None,

                    );
                }
                Ok(StreamChunk::Reasoning(s)) => {
                    self.emit_event(
                        "thinking",
                        s,
                        None,
                        false,
                        None,
                        None,
                        None,
                        None,                        None,

                    );
                }
                Ok(StreamChunk::Usage(u)) => last_usage = Some(u),
                Ok(StreamChunk::Done) => break,
                Err(e) => {
                    self.emit_error(format!("plan stream error: {}", e));
                    return;
                }
                _ => {}
            }
        }
        if let Some(u) = last_usage {
            self.emit_event(
                "usage",
                String::new(),
                Some(UsageDto::from(u)),
                false,
                None,
                None,
                None,
                None,                None,

            );
        }

        if plan_text.trim().is_empty() {
            self.emit_error("模型未生成任何 plan 文本".into());
            return;
        }

        // 推一个 plan 事件给前端（让 PlanDialog 弹窗）
        let plan_id = format!("plan_{}", uuid_v4());
        self.emit_event(
            "plan",
            String::new(),
            None,
            false,
            None,
            None,
            None,
            Some(PlanEvent {
                plan: plan_text.clone(),
                plan_id: plan_id.clone(),
            }),
            None,
        );

        // 等待前端 respond_plan 响应（带超时 10 分钟）
        let (tx, rx) = oneshot::channel::<PlanApproval>();
        {
            let mut slot = self.plan_approval_rx.lock().await;
            *slot = Some(tx);
        }
        let approval = match tokio::time::timeout(
            std::time::Duration::from_secs(600),
            rx,
        )
        .await
        {
            Ok(Ok(a)) => a,
            Ok(Err(_)) => {
                self.emit_error("plan 审批通道关闭".into());
                return;
            }
            Err(_) => {
                self.emit_error("plan 审批超时（10 分钟）".into());
                return;
            }
        };

        match approval {
            PlanApproval::Approve => {
                // 把 plan 喂回 history 作为 assistant 消息，让后续轮次知道这个计划
                self.history.push(ChatMessage::assistant(plan_text));
            }
            PlanApproval::Edit(edited) => {
                self.history.push(ChatMessage::assistant(edited));
            }
            PlanApproval::Deny(reason) => {
                self.emit_error(format!("plan 被用户拒绝: {}", reason));
                // 用 cancelled 标记中止
                self.cancelled.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:x}_{:x}", now.as_nanos(), std::process::id())
}