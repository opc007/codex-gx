//! v1.7：Goal Tauri 命令
//!
//! ## 注册的命令
//! - `goal_list`              — 所有 goal
//! - `goal_get`               — 按 id
//! - `goal_active_for_thread` — 当前 thread 的 active goal
//! - `goal_create`            — 新建
//! - `goal_add_todo`          — 加 todo
//! - `goal_mark_done`         — 标记 done
//! - `goal_mark_in_progress`
//! - `goal_mark_blocked`
//! - `goal_pause`
//! - `goal_resume`
//! - `goal_abandon`
//! - `goal_delete`
//! - `goal_to_prompt`         — 渲染成 system prompt addon

use goal::{Goal, GoalRegistry};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;

pub type GoalState = Arc<Mutex<GoalRegistry>>;

pub fn build_state() -> GoalState {
    Arc::new(Mutex::new(GoalRegistry::load()))
}

#[derive(Serialize)]
pub struct GoalInfo {
    pub id: String,
    pub thread_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub progress: f32,
    pub progress_str: String,
    pub todo_count: usize,
    pub done_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&Goal> for GoalInfo {
    fn from(g: &Goal) -> Self {
        let done = g
            .todos
            .iter()
            .filter(|t| t.status == goal::TodoStatus::Done)
            .count();
        GoalInfo {
            id: g.id.clone(),
            thread_id: g.thread_id.clone(),
            title: g.title.clone(),
            description: g.description.clone(),
            status: format!("{:?}", g.status).to_lowercase(),
            progress: g.progress(),
            progress_str: g.progress_str(),
            todo_count: g.todos.len(),
            done_count: done,
            created_at: g.created_at,
            updated_at: g.updated_at,
        }
    }
}

#[tauri::command]
pub async fn goal_list(state: State<'_, GoalState>) -> Result<Vec<GoalInfo>, String> {
    let r = state.lock().map_err(|e| e.to_string())?;
    Ok(r.list().into_iter().map(GoalInfo::from).collect())
}

#[tauri::command]
pub async fn goal_get(id: String, state: State<'_, GoalState>) -> Result<Option<GoalFull>, String> {
    let r = state.lock().map_err(|e| e.to_string())?;
    Ok(r.get(&id).map(GoalFull::from))
}

#[derive(Serialize)]
pub struct GoalFull {
    pub info: GoalInfo,
    pub todos: Vec<TodoInfo>,
    pub system_prompt_addon: String,
}

#[derive(Serialize)]
pub struct TodoInfo {
    pub id: String,
    pub content: String,
    pub status: String,
    pub completed_at: Option<i64>,
    pub evidence: Option<String>,
}

impl From<&goal::Todo> for TodoInfo {
    fn from(t: &goal::Todo) -> Self {
        TodoInfo {
            id: t.id.clone(),
            content: t.content.clone(),
            status: format!("{:?}", t.status).to_snake_case(),
            completed_at: t.completed_at,
            evidence: t.evidence.clone(),
        }
    }
}

trait ToSnakeCase {
    fn to_snake_case(&self) -> String;
}

impl ToSnakeCase for String {
    fn to_snake_case(&self) -> String {
        // PascalCase → snake_case
        let mut s = String::new();
        for (i, c) in self.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                s.push('_');
            }
            s.push(c.to_ascii_lowercase());
        }
        s
    }
}

impl From<&Goal> for GoalFull {
    fn from(g: &Goal) -> Self {
        GoalFull {
            info: GoalInfo::from(g),
            todos: g.todos.iter().map(TodoInfo::from).collect(),
            system_prompt_addon: goal::goal_prompt_addon(g),
        }
    }
}

#[tauri::command]
pub async fn goal_active_for_thread(
    thread_id: String,
    state: State<'_, GoalState>,
) -> Result<Option<GoalInfo>, String> {
    let r = state.lock().map_err(|e| e.to_string())?;
    Ok(r.find_active_for_thread(&thread_id).map(GoalInfo::from))
}

#[derive(Deserialize)]
pub struct CreateArgs {
    pub thread_id: String,
    pub title: String,
    pub description: String,
}

#[tauri::command]
pub async fn goal_create(
    args: CreateArgs,
    state: State<'_, GoalState>,
) -> Result<GoalInfo, String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    let g = r.create(&args.thread_id, &args.title, &args.description);
    Ok(GoalInfo::from(&g))
}

#[derive(Deserialize)]
pub struct AddTodoArgs {
    pub goal_id: String,
    pub content: String,
}

#[tauri::command]
pub async fn goal_add_todo(
    args: AddTodoArgs,
    state: State<'_, GoalState>,
) -> Result<String, String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    r.add_todo(&args.goal_id, &args.content).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct MarkArgs {
    pub goal_id: String,
    pub todo_id: String,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[tauri::command]
pub async fn goal_mark_done(
    args: MarkArgs,
    state: State<'_, GoalState>,
) -> Result<(), String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    r.mark_done(&args.goal_id, &args.todo_id, args.evidence)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn goal_mark_in_progress(
    args: MarkArgs,
    state: State<'_, GoalState>,
) -> Result<(), String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    r.mark_in_progress(&args.goal_id, &args.todo_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn goal_mark_blocked(
    args: MarkArgs,
    state: State<'_, GoalState>,
) -> Result<(), String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    r.mark_blocked(&args.goal_id, &args.todo_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn goal_pause(id: String, state: State<'_, GoalState>) -> Result<(), String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    r.pause(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn goal_resume(id: String, state: State<'_, GoalState>) -> Result<(), String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    r.resume(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn goal_abandon(id: String, state: State<'_, GoalState>) -> Result<(), String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    r.abandon(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn goal_delete(id: String, state: State<'_, GoalState>) -> Result<bool, String> {
    let mut r = state.lock().map_err(|e| e.to_string())?;
    Ok(r.delete(&id))
}

#[tauri::command]
pub fn goal_to_prompt(id: String, state: State<'_, GoalState>) -> Result<Option<String>, String> {
    let r = state.lock().map_err(|e| e.to_string())?;
    Ok(r.get(&id).map(goal::goal_prompt_addon))
}
