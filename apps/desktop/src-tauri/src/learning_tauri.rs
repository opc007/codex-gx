//! v1.4：learning tauri 命令

use learning::Learning;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct LearningState {
    pub inner: Arc<RwLock<Learning>>,
}

impl LearningState {
    pub async fn new() -> Self {
        let l = Learning::load().await;
        LearningState {
            inner: Arc::new(RwLock::new(l)),
        }
    }
}

#[tauri::command]
pub async fn learning_get(
    state: tauri::State<'_, LearningState>,
) -> Result<Learning, String> {
    Ok(state.inner.read().await.clone())
}

#[tauri::command]
pub async fn learning_record_chat(
    model: String,
    user_msg: String,
    state: tauri::State<'_, LearningState>,
) -> Result<(), String> {
    let mut l = state.inner.write().await;
    l.record_chat(&model, &user_msg);
    l.compute_preferences();
    l.save().await
}

#[tauri::command]
pub async fn learning_record_tool(
    tool: String,
    state: tauri::State<'_, LearningState>,
) -> Result<(), String> {
    let mut l = state.inner.write().await;
    l.record_tool_call(&tool);
    l.compute_preferences();
    l.save().await
}

#[tauri::command]
pub async fn learning_record_slash(
    cmd: String,
    state: tauri::State<'_, LearningState>,
) -> Result<(), String> {
    let mut l = state.inner.write().await;
    l.record_slash_command(&cmd);
    l.compute_preferences();
    l.save().await
}

#[tauri::command]
pub async fn learning_record_feedback(
    positive: bool,
    state: tauri::State<'_, LearningState>,
) -> Result<(), String> {
    let mut l = state.inner.write().await;
    l.record_feedback(positive);
    l.save().await
}

#[tauri::command]
pub async fn learning_reset(
    state: tauri::State<'_, LearningState>,
) -> Result<(), String> {
    let mut l = state.inner.write().await;
    l.reset();
    l.save().await
}

#[tauri::command]
pub async fn learning_inject(
    state: tauri::State<'_, LearningState>,
) -> Result<String, String> {
    let l = state.inner.read().await;
    Ok(l.inject_text())
}
