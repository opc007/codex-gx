//! v1.2：Voice input — 本地 Whisper 转写 + 模型管理
//!
//! 设计：
//! - 模型文件存到 `~/.agentshell/models/whisper/`
//! - 默认模型 `ggml-base.bin`（~140MB）
//! - 通过 `whisper-cli` 子进程转写（用户需安装 whisper.cpp；找不到时给出友好提示）
//! - 兼容 macOS / Windows / Linux
//!
//! 关键 API：
//! - `list_models()` — 列出已下载模型
//! - `download_model(name, progress_cb)` — 从 HuggingFace 镜像下载
//! - `transcribe(wav_path, model_name)` — 调 whisper-cli 转写
//! - `check_whisper_cli()` — 检查 whisper-cli 是否可用

#![warn(missing_docs)]

use async_trait::async_trait;
use base64::Engine;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

/// Voice 模块错误
#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("whisper-cli 未找到，请先安装 whisper.cpp: {0}")]
    WhisperCliNotFound(String),
    #[error("模型不存在: {0}")]
    ModelNotFound(String),
    #[error("下载失败: {0}")]
    Download(String),
    #[error("转写失败: {0}")]
    Transcribe(String),
    #[error("base64 解码失败: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("其他: {0}")]
    Other(String),
}

/// Voice 模块的 Result 别名
pub type Result<T> = std::result::Result<T, VoiceError>;

/// Whisper 模型元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub path: PathBuf,
    pub description: String,
    pub download_url: String,
}

/// 推荐的 Whisper 模型
pub const RECOMMENDED_MODELS: &[(&str, &str, u64, &str, &str)] = &[
    (
        "ggml-tiny.bin",
        "Tiny (~75 MB)",
        75_000_000,
        "最快，精度较低",
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
    ),
    (
        "ggml-base.bin",
        "Base (~140 MB)",
        142_000_000,
        "推荐，平衡速度与精度",
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
    ),
    (
        "ggml-small.bin",
        "Small (~460 MB)",
        466_000_000,
        "高精度，慢",
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
    ),
    (
        "ggml-medium.bin",
        "Medium (~1.5 GB)",
        1_500_000_000,
        "很高精度，很慢",
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
    ),
];

/// 转写结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub duration_sec: Option<f32>,
    pub model: String,
    pub elapsed_ms: u64,
}

/// 进度回调
#[async_trait]
pub trait ProgressCallback: Send + Sync {
    async fn progress(&self, pct: f32, downloaded_bytes: u64, total_bytes: u64);
}

/// 不做任何事的默认进度回调
pub struct NoopProgress;
#[async_trait]
impl ProgressCallback for NoopProgress {
    async fn progress(&self, _pct: f32, _downloaded: u64, _total: u64) {}
}

/// 闭包形式的进度回调
pub struct FnProgress<F>(pub F);
#[async_trait]
impl<F> ProgressCallback for FnProgress<F>
where
    F: Fn(f32, u64, u64) + Send + Sync,
{
    async fn progress(&self, pct: f32, downloaded: u64, total: u64) {
        (self.0)(pct, downloaded, total);
    }
}

/// Voice 管理器
pub struct VoiceManager {
    pub models_dir: PathBuf,
    pub tmp_dir: PathBuf,
}

impl VoiceManager {
    pub fn new() -> Result<Self> {
        let home = dirs_home();
        let models_dir = home.join(".agentshell").join("models").join("whisper");
        let tmp_dir = std::env::temp_dir().join("agentshell_voice");
        std::fs::create_dir_all(&models_dir)?;
        std::fs::create_dir_all(&tmp_dir)?;
        Ok(Self {
            models_dir,
            tmp_dir,
        })
    }

    pub fn with_dirs(models_dir: PathBuf, tmp_dir: PathBuf) -> Self {
        Self {
            models_dir,
            tmp_dir,
        }
    }

    pub fn tmp_dir_path(&self) -> PathBuf {
        self.tmp_dir.clone()
    }

    pub fn models_dir_path(&self) -> PathBuf {
        self.models_dir.clone()
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    pub fn list_models(&self) -> Vec<ModelInfo> {
        RECOMMENDED_MODELS
            .iter()
            .map(|(name, display, size, desc, url)| {
                let path = self.models_dir.join(name);
                let downloaded = path.exists();
                ModelInfo {
                    name: name.to_string(),
                    display_name: display.to_string(),
                    size_bytes: *size,
                    downloaded,
                    path,
                    description: desc.to_string(),
                    download_url: url.to_string(),
                }
            })
            .collect()
    }

    pub fn find_downloaded_model(&self, name: &str) -> Option<PathBuf> {
        let p = self.models_dir.join(name);
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    pub fn default_model(&self) -> Option<ModelInfo> {
        for pref in &["ggml-base.bin", "ggml-small.bin", "ggml-tiny.bin"] {
            if let Some(p) = self.find_downloaded_model(pref) {
                return self.list_models().into_iter().find(|m| m.path == p);
            }
        }
        None
    }

    pub async fn download_model(
        &self,
        name: &str,
        progress: Option<&dyn ProgressCallback>,
    ) -> Result<PathBuf> {
        let info = self
            .list_models()
            .into_iter()
            .find(|m| m.name == name)
            .ok_or_else(|| VoiceError::ModelNotFound(name.to_string()))?;
        if info.path.exists() {
            return Ok(info.path);
        }
        tracing::info!("[voice] 下载模型 {} -> {}", name, info.path.display());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60 * 30))
            .build()
            .map_err(|e| VoiceError::Download(e.to_string()))?;
        let resp = client
            .get(&info.download_url)
            .send()
            .await
            .map_err(|e| VoiceError::Download(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(VoiceError::Download(format!("HTTP {}", resp.status())));
        }
        let total = resp.content_length().unwrap_or(info.size_bytes);
        let mut stream = resp.bytes_stream();
        let mut file = std::fs::File::create(&info.path)?;
        let mut downloaded: u64 = 0;
        let mut last_pct: f32 = -1.0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| VoiceError::Download(e.to_string()))?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            let pct = if total > 0 {
                downloaded as f32 / total as f32
            } else {
                0.0
            };
            if let Some(cb) = progress {
                if (pct - last_pct).abs() >= 0.02 || pct >= 1.0 {
                    cb.progress(pct, downloaded, total).await;
                    last_pct = pct;
                }
            }
        }
        file.flush()?;
        tracing::info!("[voice] 下载完成 {} ({} bytes)", name, downloaded);
        Ok(info.path)
    }

    pub fn delete_model(&self, name: &str) -> Result<()> {
        let p = self.models_dir.join(name);
        if p.exists() {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }

    pub fn write_temp_audio(&self, name: &str, base64_data: &str) -> Result<PathBuf> {
        let bytes = base64::engine::general_purpose::STANDARD.decode(base64_data)?;
        let path = self.tmp_dir.join(name);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    pub fn check_whisper_cli(&self) -> Result<PathBuf> {
        let candidates = ["whisper-cli", "whisper", "main"];
        for c in &candidates {
            if let Ok(p) = which(c) {
                return Ok(p);
            }
        }
        if let Ok(out) = Command::new("which").arg("whisper-cli").output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Ok(PathBuf::from(p));
                }
            }
        }
        if let Ok(out) = Command::new("which").arg("whisper").output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Ok(PathBuf::from(p));
                }
            }
        }
        Err(VoiceError::WhisperCliNotFound(
            "请到 https://github.com/ggerganov/whisper.cpp 安装并把 main / whisper-cli 加到 PATH"
                .into(),
        ))
    }

    pub fn transcribe_with_model_path<A: AsRef<Path>, M: AsRef<Path>>(
        &self,
        audio_path: A,
        model_path: M,
        model_name: &str,
    ) -> Result<Transcript> {
        let cli = self.check_whisper_cli()?;
        let start = std::time::Instant::now();
        let output = Command::new(&cli)
            .arg("-m")
            .arg(model_path.as_ref())
            .arg("-f")
            .arg(audio_path.as_ref())
            .arg("--output-txt")
            .arg("--no-prints")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(VoiceError::Transcribe(err));
        }
        let txt_path = audio_path.as_ref().with_extension("txt");
        let text = std::fs::read_to_string(&txt_path)
            .or_else(|_| {
                Ok::<String, std::io::Error>(String::from_utf8_lossy(&output.stdout).to_string())
            })?
            .trim()
            .to_string();
        let elapsed = start.elapsed().as_millis() as u64;
        Ok(Transcript {
            text,
            language: None,
            duration_sec: None,
            model: model_name.to_string(),
            elapsed_ms: elapsed,
        })
    }

    pub fn transcribe<P: AsRef<Path>>(&self, audio_path: P, model: &str) -> Result<Transcript> {
        let model_path = self
            .find_downloaded_model(model)
            .ok_or_else(|| VoiceError::ModelNotFound(model.to_string()))?;
        self.transcribe_with_model_path(audio_path, model_path, model)
    }

    pub fn transcribe_base64(
        &self,
        name: &str,
        base64_data: &str,
        model: &str,
    ) -> Result<Transcript> {
        let path = self.write_temp_audio(name, base64_data)?;
        let r = self.transcribe(&path, model)?;
        let _ = std::fs::remove_file(path.with_extension("txt"));
        let _ = std::fs::remove_file(&path);
        Ok(r)
    }

    pub fn cleanup_tmp(&self) -> Result<()> {
        if self.tmp_dir.exists() {
            for entry in std::fs::read_dir(&self.tmp_dir)? {
                let entry = entry?;
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

impl Default for VoiceManager {
    fn default() -> Self {
        Self::new().expect("VoiceManager init")
    }
}

fn dirs_home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    if let Ok(h) = std::env::var("USERPROFILE") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    std::env::temp_dir()
}

fn which(name: &str) -> std::result::Result<PathBuf, ()> {
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            for ext in ["", ".exe", ".bat", ".cmd"] {
                let p = path.join(format!("{}{}", name, ext));
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_manager_init() {
        let m = VoiceManager::new().expect("init");
        assert!(m.models_dir().ends_with("models/whisper"));
    }

    #[test]
    fn list_models_has_four() {
        let m = VoiceManager::new().unwrap();
        let models = m.list_models();
        assert_eq!(models.len(), 4);
        assert!(models.iter().any(|m| m.name == "ggml-base.bin"));
    }

    #[test]
    fn model_info_fields() {
        let m = VoiceManager::new().unwrap();
        let models = m.list_models();
        for mi in &models {
            assert!(!mi.name.is_empty());
            assert!(mi.size_bytes > 0);
            assert!(mi.download_url.starts_with("https://"));
        }
    }

    #[test]
    fn base64_roundtrip() {
        let m = VoiceManager::new().unwrap();
        let original = b"fake wav bytes";
        let b64 = base64::engine::general_purpose::STANDARD.encode(original);
        let path = m
            .write_temp_audio("test_roundtrip.wav", &b64)
            .expect("write");
        let read = std::fs::read(&path).expect("read");
        assert_eq!(read, original);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn write_temp_audio_bad_base64() {
        let m = VoiceManager::new().unwrap();
        let r = m.write_temp_audio("bad.wav", "not base64!");
        assert!(r.is_err());
    }

    #[test]
    fn noop_progress_does_not_panic() {
        let cb = NoopProgress;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            cb.progress(0.5, 100, 200).await;
        });
    }

    #[test]
    fn fn_progress_invokes_callback() {
        let cb = FnProgress(|pct, dl, total| {
            assert!((0.0..=1.0).contains(&pct));
            assert!(dl <= total);
        });
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            cb.progress(0.5, 100, 200).await;
        });
    }
}
