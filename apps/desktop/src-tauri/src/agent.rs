//! Agent 主循环 — 处理 tool_calls 流式循环
//!
//! 详细设计见 docs/开发文档.md §6 Agent Core
//!
//! ## 流程
//! ```
//! user_message →
//!   loop {
//!     response = provider.chat_stream(req, tools)
//!     while chunk = response.next():
//!       content/thinking → emit "content"/"thinking"
//!       tool_call_delta → 累积
//!       done → execute each tool_call → emit "tool_result" → 喂回 history
//!     if no tool_calls → break
//!   }
//! ```

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

/// 流式 chunk 事件
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
}

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
            step += 1;
            if step > self.max_steps {
                self.emit_error("超过最大步数（10），强制结束".into());
                return;
            }

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
                        self.emit_event("content", s, None, false, None, None);
                    }
                    Ok(StreamChunk::Reasoning(s)) => {
                        acc_thinking.push_str(&s);
                        self.emit_event("thinking", s, None, false, None, None);
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
                        self.emit_event("tool_call_delta", payload.to_string(), None, false, None, None);
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
            );

            // 没 tool_call 就结束
            if tool_calls.is_empty() {
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
                );
                return;
            }

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
    ) {
        let evt = StreamEvent {
            session_id: self.session_id.clone(),
            kind: kind.into(),
            delta,
            usage,
            done,
            tool_call,
            tool_result,
        };
        let _ = self.app.emit("agent:event", evt);
    }

    fn emit_error(&self, msg: String) {
        self.emit_event("error", msg, None, true, None, None);
    }
}