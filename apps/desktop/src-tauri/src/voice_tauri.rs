// v1.2：Voice input — Tauri commands
//
// 命令：
// - voice_check           → 检查 whisper-cli 是否可用 + 列出本地模型
// - voice_download_model  → 后台下载模型（带进度事件）
// - voice_transcribe      → 转写 base64 音频
// - voice_cleanup         → 清理临时文件

use crate::VoiceState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Serialize)]
pub struct VoiceStatus {
    /// whisper-cli 是否找到
    pub cli_available: bool,
    /// whisper-cli 路径（如果找到）
    pub cli_path: Option<String>,
    /// whisper-cli 提示（未找到时）
    pub cli_hint: Option<String>,
    /// 已下载的模型列表
    pub models: Vec<voice::ModelInfo>,
    /// 默认推荐模型
    pub default_model: Option<String>,
}

#[tauri::command]
pub async fn voice_check(state: tauri::State<'_, VoiceState>) -> Result<VoiceStatus, String> {
    let mgr = state.inner().lock().await;
    let cli = mgr.check_whisper_cli();
    let models = mgr.list_models();
    let default_model = mgr.default_model().map(|m| m.name);
    let (cli_available, cli_path, cli_hint) = match cli {
        Ok(p) => (true, Some(p.to_string_lossy().to_string()), None),
        Err(e) => (false, None, Some(e.to_string())),
    };
    Ok(VoiceStatus {
        cli_available,
        cli_path,
        cli_hint,
        models,
        default_model,
    })
}

#[derive(Debug, Deserialize)]
pub struct VoiceDownloadArgs {
    /// 模型名（如 "ggml-base.bin"）
    pub model: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct VoiceDownloadEvent {
    pub model: String,
    pub pct: f32,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn voice_download_model(
    app: AppHandle,
    state: tauri::State<'_, VoiceState>,
    args: VoiceDownloadArgs,
) -> Result<String, String> {
    // 取必要信息后立即释放 lock
    let (model_path, _tmp_dir, _models_dir) = {
        let mgr = state.inner().lock().await;
        let info = mgr
            .list_models()
            .into_iter()
            .find(|m| m.name == args.model)
            .ok_or_else(|| format!("未知模型: {}", args.model))?;
        (info.path.clone(), mgr.tmp_dir_path(), mgr.models_dir_path())
    };
    let app_clone = app.clone();
    let model_name = args.model.clone();
    let model_path_for_cb = model_path.clone();
    let cb = move |pct: f32, downloaded: u64, total: u64| {
        let evt = VoiceDownloadEvent {
            model: model_name.clone(),
            pct,
            downloaded,
            total,
            done: pct >= 1.0,
            error: None,
        };
        let _ = app_clone.emit("voice:download_progress", &evt);
    };
    // 直接 await（download_model 是 async，不阻塞）
    let mgr = state.inner().lock().await;
    let result = mgr
        .download_model(&args.model, Some(&voice::FnProgress(cb)))
        .await;
    // 防止 unused 警告
    let _ = model_path_for_cb;
    match result {
        Ok(path) => Ok(path.to_string_lossy().to_string()),
        Err(e) => {
            let _ = app.emit(
                "voice:download_progress",
                &VoiceDownloadEvent {
                    model: args.model.clone(),
                    pct: 0.0,
                    downloaded: 0,
                    total: 0,
                    done: true,
                    error: Some(e.to_string()),
                },
            );
            Err(e.to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct VoiceTranscribeArgs {
    /// base64 编码的音频数据
    pub base64: String,
    /// 文件名后缀（如 "rec.wav"）
    pub filename: Option<String>,
    /// 模型名（可选；默认用 default_model）
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VoiceTranscribeResult {
    pub text: String,
    pub model: String,
    pub elapsed_ms: u64,
    pub language: Option<String>,
}

#[tauri::command]
pub async fn voice_transcribe(
    state: tauri::State<'_, VoiceState>,
    args: VoiceTranscribeArgs,
) -> Result<VoiceTranscribeResult, String> {
    // 先取出 (model_name, model_path, tmp_dir, whisper_cli_path)，释放 lock
    let (model_name, model_path, tmp_dir, cli_path) = {
        let mgr = state.inner().lock().await;
        let model_name = args
            .model
            .clone()
            .or_else(|| mgr.default_model().map(|m| m.name))
            .ok_or_else(|| "没有可用模型，请先下载".to_string())?;
        let model_path = mgr
            .find_downloaded_model(&model_name)
            .ok_or_else(|| format!("模型未下载: {}", model_name))?;
        let cli = mgr.check_whisper_cli().map_err(|e| e.to_string())?;
        (
            model_name,
            model_path,
            mgr.tmp_dir_path(),
            cli.to_string_lossy().to_string(),
        )
    };
    let filename = args.filename.unwrap_or_else(|| "recording.wav".to_string());
    let base64 = args.base64;
    // 同步 IO 必须在 spawn_blocking 里跑
    let result = tokio::task::spawn_blocking(move || {
        // 临时构造 manager
        let mgr = voice::VoiceManager::with_dirs(tmp_dir.clone(), tmp_dir);
        // 写临时文件
        let path = match mgr.write_temp_audio(&filename, &base64) {
            Ok(p) => p,
            Err(e) => return Err(e.to_string()),
        };
        // 同步转写（不走 mgr.transcribe 以避免再次 check_whisper_cli）
        let r = mgr.transcribe_with_model_path(&path, &model_path, &model_name);
        let _ = std::fs::remove_file(path.with_extension("txt"));
        let _ = std::fs::remove_file(&path);
        r.map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("join error: {}", e))?;
    let _ = cli_path; // 暂未使用
    result.map(|t| VoiceTranscribeResult {
        text: t.text,
        model: t.model,
        elapsed_ms: t.elapsed_ms,
        language: t.language,
    })
}

#[tauri::command]
pub async fn voice_cleanup(state: tauri::State<'_, VoiceState>) -> Result<(), String> {
    let mgr = state.inner().lock().await;
    mgr.cleanup_tmp().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn voice_delete_model(
    state: tauri::State<'_, VoiceState>,
    args: VoiceDownloadArgs,
) -> Result<(), String> {
    let mgr = state.inner().lock().await;
    mgr.delete_model(&args.model).map_err(|e| e.to_string())
}

// ===== v1.8 5.26 Voice 双向对讲 =====
//
// 完整流式 TTS + 持续 STT 的双工会话。
// 演示版：模拟 TTS 输出（chunk 流），发送 `voice:duplex:event` 事件给前端。
// 真实版会接 TTS provider（OpenAI TTS / ElevenLabs / 本地 piper）。

use std::sync::atomic::{AtomicU64, Ordering};
use tauri::async_runtime::spawn as tauri_spawn;

#[derive(Debug, Deserialize)]
pub struct DuplexStartArgs {
    pub text: String,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub chunk_size: Option<usize>,
}

static DUPLEX_SESSION_ID: AtomicU64 = AtomicU64::new(1);

/// 启动一次 duplex 流式 TTS session
/// 返回 session_id, 同时开始通过 `voice:duplex:event` 事件发音频 chunk
#[tauri::command]
pub async fn voice_duplex_start(
    app: AppHandle,
    args: DuplexStartArgs,
) -> Result<u64, String> {
    let session_id = DUPLEX_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    let chunk_size = args.chunk_size.unwrap_or(50); // 50 字符 / chunk
    let voice = args.voice.unwrap_or_else(|| "alloy".to_string());

    let app_clone = app.clone();
    tauri_spawn(async move {
        let text = args.text.clone();
        let chunks: Vec<String> = text
            .chars()
            .collect::<Vec<_>>()
            .chunks(chunk_size)
            .map(|c| c.iter().collect::<String>())
            .collect();

        // 1. start event
        let _ = app_clone.emit(
            "voice:duplex:event",
            DuplexEvent {
                kind: "start".into(),
                session_id,
                seq: 0,
                text: Some(text.clone()),
                voice: Some(voice.clone()),
                chunk: None,
                done: false,
            },
        );

        // 2. chunks
        for (i, chunk) in chunks.iter().enumerate() {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            let _ = app_clone.emit(
                "voice:duplex:event",
                DuplexEvent {
                    kind: "chunk".into(),
                    session_id,
                    seq: (i + 1) as u64,
                    text: None,
                    voice: None,
                    chunk: Some(format!("[{} bytes audio for: {}]", chunk.len() * 8, chunk)),
                    done: false,
                },
            );
        }

        // 3. done event
        let _ = app_clone.emit(
            "voice:duplex:event",
            DuplexEvent {
                kind: "done".into(),
                session_id,
                seq: (chunks.len() + 1) as u64,
                text: None,
                voice: None,
                chunk: None,
                done: true,
            },
        );
    });

    Ok(session_id)
}

#[derive(Debug, Serialize, Clone)]
pub struct DuplexEvent {
    pub kind: String, // start / chunk / done
    pub session_id: u64,
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk: Option<String>,
    pub done: bool,
}

/// duplex session 元数据（供前端查询）
#[tauri::command]
pub fn voice_duplex_status() -> serde_json::Value {
    serde_json::json!({
        "current_session": DUPLEX_SESSION_ID.load(Ordering::SeqCst) - 1,
        "max_concurrent": 4,
        "supported_voices": ["alloy", "echo", "fable", "onyx", "nova", "shimmer"],
    })
}
