//! MiniMax 多模态生成：图像 / 视频
//!
//! 端点：
//! - `POST /v1/image_generation` — 文/图生图（同步返回 image_urls）
//! - `POST /v1/video_generation` — 提交任务（返回 task_id）
//! - `POST /v1/query/video_generation` — 轮询任务状态
//!
//! 设计参考：[MiniMax Image API docs](https://platform.MiniMax.io/docs/api-reference/image-generation-t2i)
//! 与 [Video Generation guide](https://platform.MiniMax.io/docs/guides/video-generation)

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 最小 percent-encoding（仅编码 task_id 等 URL 路径参数）
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            let bytes = c.encode_utf8(&mut buf).as_bytes();
            for b in bytes {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

const DEFAULT_BASE_URL: &str = "https://api.minimax.chat";

/// 图像生成请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationRequest {
    /// 模型 ID：image-01 / image-02（按 MiniMax 后端支持的填）
    #[serde(default = "default_image_model")]
    pub model: String,
    /// 提示词
    pub prompt: String,
    /// 宽（默认 1024）
    #[serde(default = "default_width")]
    pub width: u32,
    /// 高（默认 1024）
    #[serde(default = "default_height")]
    pub height: u32,
    /// 一次生成张数（默认 1）
    #[serde(default = "default_n")]
    pub n: u32,
    /// 图像参考（可选，图生图）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_urls: Option<Vec<String>>,
    /// 提示词优化开关
    #[serde(default)]
    pub prompt_optimizer: bool,
}

fn default_image_model() -> String {
    "image-01".into()
}
fn default_width() -> u32 {
    1024
}
fn default_height() -> u32 {
    1024
}
fn default_n() -> u32 {
    1
}

/// 图像生成响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationResponse {
    #[serde(default)]
    pub id: String,
    pub data: ImageData,
    #[serde(default)]
    pub metadata: ImageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
    /// 返回的图像 URL（OSS 签名 URL，会过期）
    #[serde(default)]
    pub image_urls: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageMetadata {
    #[serde(default)]
    pub failed_count: String,
    #[serde(default)]
    pub success_count: String,
}

/// 视频生成请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenerationRequest {
    /// 模型：MiniMax-Hailuo-2.3 等
    #[serde(default = "default_video_model")]
    pub model: String,
    /// 提示词
    pub prompt: String,
    /// 时长（秒）：6 / 10
    #[serde(default = "default_duration")]
    pub duration: u32,
    /// 分辨率：720P / 1080P
    #[serde(default = "default_resolution")]
    pub resolution: String,
    /// 首帧图 URL（图生视频模式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_frame_image: Option<String>,
    /// 主体参考图 URL（subject-reference 模式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_reference: Option<Vec<String>>,
}

fn default_video_model() -> String {
    "MiniMax-Hailuo-2.3".into()
}
fn default_duration() -> u32 {
    6
}
fn default_resolution() -> String {
    "768P".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSubmitResponse {
    pub task_id: String,
    #[serde(default)]
    pub base_resp: BaseResp,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BaseResp {
    #[serde(default)]
    pub status_code: i32,
    #[serde(default)]
    pub status_msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoQueryResponse {
    pub task_id: String,
    /// status: Queueing / Processing / Success / Failure
    pub status: String,
    /// 视频文件 ID（Success 时返回）
    #[serde(default)]
    pub file_id: Option<String>,
    /// 视频直链（Success 时返回，OSS 签名 URL）
    #[serde(default)]
    pub video_url: Option<String>,
    /// 错误原因
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub base_resp: BaseResp,
}

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("MiniMax API error (status {status}): {message}")]
    Api { status: u16, message: String },
    #[error("MiniMax API: {0}")]
    ApiMsg(String),
    #[error("Image generation failed: 0 images returned")]
    NoImages,
    #[error("Video generation failed: {0}")]
    VideoFailed(String),
}

pub struct MinimaxMedia {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl MinimaxMedia {
    pub fn new(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            api_key: api_key.into().trim().to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("build media client"),
        }
    }

    pub fn from_env() -> Option<Self> {
        std::env::var("MINIMAX_API_KEY")
            .ok()
            .map(|k| Self::new(k, None))
    }

    /// 文/图生图
    pub async fn generate_image(
        &self,
        req: &ImageGenerationRequest,
    ) -> Result<ImageGenerationResponse, MediaError> {
        let url = format!("{}/v1/image_generation", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(req)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MediaError::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        let parsed: ImageGenerationResponse = resp.json().await?;
        if parsed.data.image_urls.is_empty() {
            return Err(MediaError::NoImages);
        }
        Ok(parsed)
    }

    /// 提交视频生成任务
    pub async fn submit_video(
        &self,
        req: &VideoGenerationRequest,
    ) -> Result<VideoSubmitResponse, MediaError> {
        let url = format!("{}/v1/video_generation", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(req)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MediaError::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        let parsed: VideoSubmitResponse = resp.json().await?;
        if parsed.base_resp.status_code != 0 {
            return Err(MediaError::ApiMsg(
                parsed.base_resp.status_msg.clone(),
            ));
        }
        Ok(parsed)
    }

    /// 轮询视频任务
    pub async fn query_video(&self, task_id: &str) -> Result<VideoQueryResponse, MediaError> {
        let url = format!(
            "{}/v1/query/video_generation?task_id={}",
            self.base_url,
            urlencode(task_id)
        );
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MediaError::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        let parsed: VideoQueryResponse = resp.json().await?;
        Ok(parsed)
    }

    /// 阻塞轮询视频，直到完成或失败（最多 wait_secs 秒）
    pub async fn wait_video(
        &self,
        task_id: &str,
        wait_secs: u64,
    ) -> Result<VideoQueryResponse, MediaError> {
        let start = std::time::Instant::now();
        let interval = Duration::from_secs(5);
        loop {
            let q = self.query_video(task_id).await?;
            match q.status.as_str() {
                "Success" | "Finished" | "Completed" => {
                    // MiniMax API 不会在 query 中直接给 video_url，需要再调一次 files/retrieve
                    let mut result = q;
                    if result.video_url.is_none() {
                        if let Some(fid) = result.file_id.as_deref() {
                            match self.retrieve_file_url(fid).await {
                                Ok(url) => result.video_url = Some(url),
                                Err(e) => result.message = Some(format!("file retrieve: {}", e)),
                            }
                        }
                    }
                    return Ok(result);
                }
                "Failure" | "Failed" => {
                    return Err(MediaError::VideoFailed(
                        q.message.unwrap_or_else(|| "未知错误".into()),
                    ));
                }
                _ => {
                    if start.elapsed().as_secs() >= wait_secs {
                        return Err(MediaError::VideoFailed(format!(
                            "等待超时（{} 秒）",
                            wait_secs
                        )));
                    }
                    tokio::time::sleep(interval).await;
                }
            }
        }
    }

    /// 通过 file_id 拿到 video 文件的 download_url
    pub async fn retrieve_file_url(&self, file_id: &str) -> Result<String, MediaError> {
        let url = format!(
            "{}/v1/files/retrieve?file_id={}",
            self.base_url,
            urlencode(file_id)
        );
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MediaError::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        #[derive(Deserialize)]
        struct FileResp {
            file: FileBody,
        }
        #[derive(Deserialize)]
        struct FileBody {
            download_url: String,
        }
        let parsed: FileResp = resp.json().await?;
        Ok(parsed.file.download_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 MINIMAX_API_KEY"]
    async fn live_generate_image() {
        let m = MinimaxMedia::from_env().expect("MINIMAX_API_KEY");
        let req = ImageGenerationRequest {
            model: "image-01".into(),
            prompt: "a tiny cat, anime style".into(),
            width: 512,
            height: 512,
            n: 1,
            image_urls: None,
            prompt_optimizer: false,
        };
        let resp = m.generate_image(&req).await.expect("image");
        assert!(!resp.data.image_urls.is_empty());
    }

    #[tokio::test]
    #[ignore = "需要 MINIMAX_API_KEY + 视频生成可能收费"]
    async fn live_generate_video() {
        let m = MinimaxMedia::from_env().expect("MINIMAX_API_KEY");
        let req = VideoGenerationRequest {
            model: "MiniMax-Hailuo-2.3".into(),
            prompt: "A cat walks on the grass".into(),
            duration: 6,
            resolution: "768P".into(),
            first_frame_image: None,
            subject_reference: None,
        };
        let sub = m.submit_video(&req).await.expect("submit");
        let q = m.wait_video(&sub.task_id, 120).await.expect("wait");
        assert!(q.video_url.is_some(), "no video url: {:?}", q);
    }
}
