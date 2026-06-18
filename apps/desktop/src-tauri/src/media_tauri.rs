//! MiniMax 多模态生图 / 生视频 Tauri 命令
//!
//! 暴露给前端用于：
//! - `media_generate_image` — 文/图生图（同步）
//! - `media_generate_video` — 文生视频（异步轮询，最长 ~3 分钟）
//!
//! API 密钥复用 `MINIMAX_API_KEY`（同 Chat Completions）。

use crate::api_keys_tauri;
use provider::{ImageGenerationRequest, MinimaxMedia, VideoGenerationRequest};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, serde::Deserialize)]
pub struct MediaImageArgs {
    pub prompt: String,
    #[serde(default = "default_image_model")]
    pub model: String,
    #[serde(default = "default_image_width")]
    pub width: u32,
    #[serde(default = "default_image_height")]
    pub height: u32,
    #[serde(default = "default_image_n")]
    pub n: u32,
    #[serde(default)]
    pub image_urls: Option<Vec<String>>,
    #[serde(default)]
    pub prompt_optimizer: bool,
}

fn default_image_model() -> String {
    "image-01".into()
}
fn default_image_width() -> u32 {
    1024
}
fn default_image_height() -> u32 {
    1024
}
fn default_image_n() -> u32 {
    1
}

#[derive(Debug, Serialize)]
pub struct MediaImageResult {
    pub id: String,
    pub image_urls: Vec<String>,
    pub success_count: String,
    pub failed_count: String,
}

#[tauri::command]
pub async fn media_generate_image(args: MediaImageArgs) -> Result<MediaImageResult, String> {
    let key = api_keys_tauri::resolve_minimax_key()
        .map_err(|e| format!("未找到 MiniMax API Key：{}（请在「API Key 设置」中填写）", e))?;
    let media = MinimaxMedia::new(key, None);
    let req = ImageGenerationRequest {
        model: args.model,
        prompt: args.prompt,
        width: args.width,
        height: args.height,
        n: args.n,
        image_urls: args.image_urls,
        prompt_optimizer: args.prompt_optimizer,
    };
    let resp = media
        .generate_image(&req)
        .await
        .map_err(|e| format!("图像生成失败：{}", e))?;
    Ok(MediaImageResult {
        id: resp.id,
        image_urls: resp.data.image_urls,
        success_count: resp.metadata.success_count,
        failed_count: resp.metadata.failed_count,
    })
}

#[derive(Debug, serde::Deserialize)]
pub struct MediaVideoArgs {
    pub prompt: String,
    #[serde(default = "default_video_model")]
    pub model: String,
    #[serde(default = "default_video_duration")]
    pub duration: u32,
    #[serde(default = "default_video_resolution")]
    pub resolution: String,
    #[serde(default)]
    pub first_frame_image: Option<String>,
    #[serde(default)]
    pub subject_reference: Option<Vec<String>>,
    /// 轮询等待上限（秒），默认 240
    #[serde(default = "default_video_wait")]
    pub wait_secs: u64,
}

fn default_video_model() -> String {
    "MiniMax-Hailuo-2.3".into()
}
fn default_video_duration() -> u32 {
    6
}
fn default_video_resolution() -> String {
    "768P".into()
}
fn default_video_wait() -> u64 {
    240
}

#[derive(Debug, Serialize)]
pub struct MediaVideoResult {
    pub task_id: String,
    pub status: String,
    pub video_url: Option<String>,
    pub file_id: Option<String>,
    pub message: Option<String>,
    pub elapsed_secs: u64,
}

#[tauri::command]
pub async fn media_generate_video(args: MediaVideoArgs) -> Result<MediaVideoResult, String> {
    let key = api_keys_tauri::resolve_minimax_key()
        .map_err(|e| format!("未找到 MiniMax API Key：{}（请在「API Key 设置」中填写）", e))?;
    let media = MinimaxMedia::new(key, None);
    let req = VideoGenerationRequest {
        model: args.model,
        prompt: args.prompt,
        duration: args.duration,
        resolution: args.resolution,
        first_frame_image: args.first_frame_image,
        subject_reference: args.subject_reference,
    };
    let start = std::time::Instant::now();
    let sub = media
        .submit_video(&req)
        .await
        .map_err(|e| format!("提交视频任务失败：{}", e))?;
    // 大模型视频生成耗时 1-3 分钟；轮询间隔 5s
    let poll = media
        .wait_video(&sub.task_id, args.wait_secs.max(30))
        .await
        .map_err(|e| format!("视频生成失败：{}", e))?;
    Ok(MediaVideoResult {
        task_id: poll.task_id,
        status: poll.status,
        video_url: poll.video_url,
        file_id: poll.file_id,
        message: poll.message,
        elapsed_secs: start.elapsed().as_secs(),
    })
}

#[derive(Debug, Serialize)]
pub struct MediaCapabilities {
    pub image: bool,
    pub video: bool,
    pub image_models: Vec<&'static str>,
    pub video_models: Vec<&'static str>,
}

#[tauri::command]
pub async fn media_capabilities() -> MediaCapabilities {
    MediaCapabilities {
        image: true,
        video: true,
        image_models: vec!["image-01", "image-02"],
        video_models: vec!["MiniMax-Hailuo-2.3", "MiniMax-Hailuo-2"],
    }
}

/// 视频生成后端单次进度回调（前端如果想流式可加 Event；目前先等待最终结果）
pub fn _poll_interval() -> Duration {
    Duration::from_secs(5)
}
