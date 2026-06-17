//! v1.5：文本转语音（TTS）
//!
//! 平台支持：
//! - macOS: `say` 命令
//! - Windows: PowerShell + System.Speech
//! - Linux: `espeak` / `spd-say` / `festival`（按可用性 fallback）
//!
//! 设计：spawn 后台 tokio 任务执行，不阻塞 chat。
//! 设置持久化到 ~/.agentshell/tts.json

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub enabled: bool,
    pub voice: String,
    pub rate: u32,   // 词 / 分钟
    pub volume: f32, // 0.0 - 1.0
    pub backend: TtsBackend,
    /// 自动播放最后一条 assistant 消息
    pub auto_play: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TtsBackend {
    /// 自动选择（按平台）
    Auto,
    Say,        // macOS
    Espeak,     // Linux
    SpdSay,     // Linux
    Festival,   // Linux
    Powershell, // Windows
}

impl Default for TtsConfig {
    fn default() -> Self {
        TtsConfig {
            enabled: false,
            voice: "auto".to_string(),
            rate: 200,
            volume: 1.0,
            backend: TtsBackend::Auto,
            auto_play: false,
        }
    }
}

impl TtsConfig {
    pub fn config_path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".agentshell").join("tts.json")
    }

    pub async fn load() -> Self {
        let p = Self::config_path();
        match tokio::fs::read_to_string(&p).await {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub async fn save(&self) -> Result<(), String> {
        let p = Self::config_path();
        if let Some(parent) = p.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        tokio::fs::write(&p, text).await.map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsStatus {
    pub available: bool,
    pub backend: TtsBackend,
    pub version: Option<String>,
    pub error: Option<String>,
    pub voices: Vec<String>,
}

/// 探测可用 TTS
pub async fn detect() -> TtsStatus {
    let backend = pick_backend().await;
    match &backend {
        Some((b, _)) => TtsStatus {
            available: true,
            backend: *b,
            version: None,
            error: None,
            voices: list_voices(*b).await.unwrap_or_default(),
        },
        None => TtsStatus {
            available: false,
            backend: TtsBackend::Auto,
            version: None,
            error: Some("未检测到 TTS 工具".to_string()),
            voices: vec![],
        },
    }
}

async fn pick_backend() -> Option<(TtsBackend, String)> {
    if cfg!(target_os = "macos") {
        if which("say").await {
            return Some((TtsBackend::Say, "say".to_string()));
        }
    } else if cfg!(target_os = "windows") {
        // PowerShell 默认存在
        return Some((TtsBackend::Powershell, "powershell".to_string()));
    } else {
        // Linux: 按优先级检测
        for (b, cmd) in [
            (TtsBackend::Espeak, "espeak"),
            (TtsBackend::SpdSay, "spd-say"),
            (TtsBackend::Festival, "festival"),
        ] {
            if which(cmd).await {
                return Some((b, cmd.to_string()));
            }
        }
    }
    None
}

async fn which(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let p = std::path::PathBuf::from(dir).join(cmd);
            if p.exists() {
                return true;
            }
        }
    }
    false
}

async fn list_voices(_backend: TtsBackend) -> Option<Vec<String>> {
    // 简化：仅 macOS `say` 列 voices
    if cfg!(target_os = "macos") {
        let out = Command::new("say").arg("-v").arg("?").output().await.ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        Some(
            text.lines()
                .filter_map(|l| l.split_whitespace().next().map(|s| s.to_string()))
                .take(50)
                .collect(),
        )
    } else {
        Some(vec!["default".to_string()])
    }
}

/// 异步朗读文本（不阻塞 caller）
pub fn speak(text: String, config: TtsConfig) {
    tokio::spawn(async move {
        let _ = speak_blocking(&text, &config).await;
    });
}

pub async fn speak_blocking(text: &str, config: &TtsConfig) -> Result<(), String> {
    if !config.enabled {
        return Err("TTS 未启用".to_string());
    }
    // 清理文本（去 markdown、emoji、HTML、SSML 标签）
    let clean = clean_text(text);
    if clean.trim().is_empty() {
        return Ok(());
    }
    let (backend, cmd) = pick_backend()
        .await
        .ok_or_else(|| "未检测到 TTS 后端".to_string())?;
    let backend = if config.backend == TtsBackend::Auto {
        backend
    } else {
        config.backend
    };
    match backend {
        TtsBackend::Say => speak_macos(&clean, config).await,
        TtsBackend::Powershell => speak_windows(&clean, config).await,
        TtsBackend::Espeak => speak_espeak(&clean, config).await,
        TtsBackend::SpdSay => speak_spd_say(&clean, config).await,
        TtsBackend::Festival => speak_festival(&clean, config).await,
        TtsBackend::Auto => unreachable!(),
    }
}

async fn speak_macos(text: &str, config: &TtsConfig) -> Result<(), String> {
    let mut cmd = Command::new("say");
    if config.voice != "auto" {
        cmd.arg("-v").arg(&config.voice);
    }
    // rate 是 words per minute
    cmd.arg("-r").arg(config.rate.to_string());
    cmd.arg(text);
    let status = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("say 退出码: {:?}", status.code()));
    }
    Ok(())
}

async fn speak_windows(text: &str, config: &TtsConfig) -> Result<(), String> {
    // PowerShell + System.Speech
    let rate = (config.rate as i32 - 200).max(-10).min(10);
    let volume = (config.volume * 100.0) as i32;
    let ps_script = format!(
        r#"
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$s.Rate = {rate}
$s.Volume = {volume}
$s.Speak([Console]::In.ReadToEnd())
"#,
        rate = rate,
        volume = volume
    );
    // 简化：直接用 System.Speech 异步读 stdin
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile")
        .arg("-Command")
        .arg(
            "Add-Type -AssemblyName System.Speech; \
             $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             $s.Rate = $args[0]; \
             $s.Volume = $args[1]; \
             $s.Speak($args[2])",
        )
        .arg(rate.to_string())
        .arg(volume.to_string())
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = cmd.status().await.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("powershell 退出码: {:?}", status.code()));
    }
    Ok(())
}

async fn speak_espeak(text: &str, config: &TtsConfig) -> Result<(), String> {
    let speed = (config.rate as i32).max(80).min(500);
    let mut cmd = Command::new("espeak");
    cmd.arg("-s").arg(speed.to_string());
    if config.voice != "auto" {
        cmd.arg("-v").arg(&config.voice);
    }
    cmd.arg(text);
    let status = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("espeak 退出码: {:?}", status.code()));
    }
    Ok(())
}

async fn speak_spd_say(text: &str, config: &TtsConfig) -> Result<(), String> {
    let mut cmd = Command::new("spd-say");
    cmd.arg("-r").arg(config.rate.to_string());
    if config.voice != "auto" {
        cmd.arg("-t").arg(&config.voice);
    }
    cmd.arg(text);
    let status = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("spd-say 退出码: {:?}", status.code()));
    }
    Ok(())
}

async fn speak_festival(text: &str, _config: &TtsConfig) -> Result<(), String> {
    // festival --tts 用 stdin
    use tokio::io::AsyncWriteExt;
    let mut child = Command::new("festival")
        .arg("--tts")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("festival 退出码: {:?}", status.code()));
    }
    Ok(())
}

fn clean_text(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' {
            in_tag = false;
            continue;
        }
        if in_tag {
            continue;
        }
        // 替换 markdown 强调
        if c == '*' || c == '`' || c == '#' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    // 多个换行 → 句号
    let normalized = out
        .split('\n')
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(". ");
    // 截断太长
    if normalized.chars().count() > 2000 {
        let truncated: String = normalized.chars().take(2000).collect();
        format!("{truncated}…")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_strips_markdown() {
        let s = "# Title\n\n**bold** text with `code` and <tag>xml</tag>";
        let c = clean_text(s);
        assert!(!c.contains("#"));
        assert!(!c.contains("**"));
        assert!(!c.contains("`"));
        assert!(!c.contains("<tag>"));
    }

    #[test]
    fn clean_text_joins_lines() {
        let s = "line1\nline2\nline3";
        let c = clean_text(s);
        assert!(c.contains(". line2"));
    }

    #[test]
    fn clean_text_truncates_long() {
        let s: String = "a".repeat(3000);
        let c = clean_text(&s);
        assert!(c.chars().count() <= 2001); // 2000 + …
    }

    #[test]
    fn clean_text_empty_input() {
        assert_eq!(clean_text(""), "");
        assert_eq!(clean_text("   \n\n   "), "");
    }

    #[test]
    fn tts_config_default() {
        let c = TtsConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.rate, 200);
        assert_eq!(c.backend, TtsBackend::Auto);
    }

    #[test]
    fn tts_config_serde() {
        let c = TtsConfig {
            enabled: true,
            voice: "Tingting".into(),
            rate: 180,
            volume: 0.8,
            backend: TtsBackend::Say,
            auto_play: true,
        };
        let s = serde_json::to_string(&c).unwrap();
        let d: TtsConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(d.voice, "Tingting");
        assert!(d.enabled);
        assert_eq!(d.backend, TtsBackend::Say);
    }

    #[test]
    fn backend_serialize_lowercase() {
        let s = serde_json::to_string(&TtsBackend::Powershell).unwrap();
        assert_eq!(s, "\"powershell\"");
    }
}
