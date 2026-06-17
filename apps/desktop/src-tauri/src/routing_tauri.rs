//! v1.3：LLM Provider 路由策略 tauri 命令

use crate::routing::{Decision, RouteTarget, RoutingStrategy, TaskType};
use crate::RoutingState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct DecideArgs {
    pub message: String,
    /// 可选：任务类型 hint
    pub task_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DecideResult {
    pub primary_provider: String,
    pub primary_model: String,
    pub fallbacks: Vec<RouteTarget>,
    pub reason: String,
    pub rule_id: Option<String>,
}

impl From<Decision> for DecideResult {
    fn from(d: Decision) -> Self {
        DecideResult {
            primary_provider: d.primary.provider.clone(),
            primary_model: d.primary.model.clone(),
            fallbacks: d.fallbacks,
            reason: d.reason,
            rule_id: d.rule_id,
        }
    }
}

/// 路由决策
#[tauri::command]
pub fn routing_decide(
    state: State<'_, RoutingState>,
    args: DecideArgs,
) -> Result<DecideResult, String> {
    let engine = state.blocking_lock();
    let task_hint = args.task_type.as_deref().and_then(parse_task_type);
    let decision = engine.decide(&args.message, task_hint.as_ref());
    Ok(decision.into())
}

fn parse_task_type(s: &str) -> Option<TaskType> {
    match s.to_lowercase().as_str() {
        "code" => Some(TaskType::Code),
        "reason" | "reasoning" => Some(TaskType::Reason),
        "summary" | "summarize" => Some(TaskType::Summary),
        "translate" => Some(TaskType::Translate),
        "chat" => Some(TaskType::Chat),
        "vision" | "image" => Some(TaskType::Vision),
        "long" => Some(TaskType::Long),
        "quick" | "short" => Some(TaskType::Quick),
        _ => None,
    }
}

/// 读取当前策略
#[tauri::command]
pub fn routing_get_strategy(state: State<'_, RoutingState>) -> Result<RoutingStrategy, String> {
    let engine = state.blocking_lock();
    Ok(engine.strategy().clone())
}

/// 写入策略
#[tauri::command]
pub fn routing_set_strategy(
    state: State<'_, RoutingState>,
    strategy: RoutingStrategy,
) -> Result<(), String> {
    let mut engine = state.blocking_lock();
    engine.set_strategy(strategy);
    engine.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// 重置为内置默认
#[tauri::command]
pub fn routing_reset_to_default(state: State<'_, RoutingState>) -> Result<RoutingStrategy, String> {
    let mut engine = state.blocking_lock();
    let s = RoutingStrategy::builtin();
    engine.set_strategy(s.clone());
    engine.save().map_err(|e| e.to_string())?;
    Ok(s)
}
