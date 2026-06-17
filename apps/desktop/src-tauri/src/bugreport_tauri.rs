//! v1.3：自动 bug 报告 + 崩溃捕获
//!
//! - panic hook 写 crash log 到 ~/.agentshell/crashes/
//! - frontend 上报 error → 存 log
//! - 生成 GitHub issue URL（含系统信息 + 错误 stack trace + 用户描述）

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashEntry {
    pub id: String,
    pub timestamp: u64,
    pub source: String, // "panic" | "frontend" | "promise" | "manual"
    pub severity: String, // "fatal" | "error" | "warning"
    pub message: String,
    pub stack: Option<String>,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub context: Option<serde_json::Value>,
    pub user_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReportContext {
    pub os: String,
    pub arch: String,
    pub app_version: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub last_messages: Option<String>,
    pub workspace_id: Option<String>,
    pub active_theme: Option<String>,
    pub routing_strategy_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReport {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub github_url: String,
}

pub struct BugReportState {
    pub crashes_dir: PathBuf,
    pub reports: Mutex<Vec<CrashEntry>>,
}

impl BugReportState {
    pub fn new() -> Self {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "/tmp".to_string());
        let dir = PathBuf::from(home).join(".agentshell").join("crashes");
        let _ = std::fs::create_dir_all(&dir);
        BugReportState {
            crashes_dir: dir,
            reports: Mutex::new(Vec::new()),
        }
    }

    pub fn record(&self, entry: CrashEntry) -> std::io::Result<()> {
        // 写 log 文件
        let filename = format!("{}-{}.json", entry.timestamp, short_id(&entry.id));
        let path = self.crashes_dir.join(filename);
        let json = serde_json::to_string_pretty(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let mut f = std::fs::File::create(&path)?;
        f.write_all(json.as_bytes())?;
        // 写内存
        let mut g = self.reports.lock().unwrap();
        g.push(entry);
        // 限制内存中最多 200 条
        if g.len() > 200 {
            let drop_n = g.len() - 200;
            g.drain(0..drop_n);
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<CrashEntry> {
        self.reports.lock().unwrap().clone()
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

#[derive(Debug, Deserialize)]
pub struct RecordCrashArgs {
    pub source: String,
    pub severity: String,
    pub message: String,
    pub stack: Option<String>,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub context: Option<serde_json::Value>,
    pub user_note: Option<String>,
}

#[tauri::command]
pub fn bug_report_record(
    state: tauri::State<'_, BugReportState>,
    args: RecordCrashArgs,
) -> Result<String, String> {
    let id = format!("crash_{}_{}", chrono_now(), short_id(&args.message));
    let entry = CrashEntry {
        id: id.clone(),
        timestamp: chrono_now(),
        source: args.source,
        severity: args.severity,
        message: args.message,
        stack: args.stack,
        session_id: args.session_id,
        model: args.model,
        context: args.context,
        user_note: args.user_note,
    };
    state
        .record(entry)
        .map_err(|e| format!("write crash log failed: {e}"))?;
    Ok(id)
}

#[tauri::command]
pub fn bug_report_list(state: tauri::State<'_, BugReportState>) -> Vec<CrashEntry> {
    state.list()
}

#[tauri::command]
pub fn bug_report_clear(state: tauri::State<'_, BugReportState>) -> Result<(), String> {
    let mut g = state.reports.lock().unwrap();
    g.clear();
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct BuildArgs {
    pub message: String,
    pub stack: Option<String>,
    pub context: BugReportContext,
    pub user_note: Option<String>,
}

#[tauri::command]
pub fn bug_report_build(args: BuildArgs) -> Result<BugReport, String> {
    let title = if args.message.chars().count() > 80 {
        let s: String = args.message.chars().take(80).collect();
        format!("🐛 {}", s)
    } else {
        format!("🐛 {}", args.message)
    };
    let mut body = String::new();
    body.push_str("## Bug 描述\n\n");
    body.push_str(&format!("{}\n\n", args.message));

    if let Some(note) = &args.user_note {
        if !note.trim().is_empty() {
            body.push_str("## 用户补充\n\n");
            body.push_str(&format!("{}\n\n", note));
        }
    }

    body.push_str("## 环境\n\n");
    body.push_str(&format!("- OS: {}\n", args.context.os));
    body.push_str(&format!("- Arch: {}\n", args.context.arch));
    body.push_str(&format!("- App version: {}\n", args.context.app_version));
    if let Some(sid) = &args.context.session_id {
        body.push_str(&format!("- Session: `{}`\n", sid));
    }
    if let Some(m) = &args.context.model {
        body.push_str(&format!("- Model: `{}`\n", m));
    }
    if let Some(w) = &args.context.workspace_id {
        body.push_str(&format!("- Workspace: `{}`\n", w));
    }
    if let Some(t) = &args.context.active_theme {
        body.push_str(&format!("- Theme: `{}`\n", t));
    }
    if let Some(r) = &args.context.routing_strategy_id {
        body.push_str(&format!("- Routing rule: `{}`\n", r));
    }
    body.push_str("\n");

    if let Some(stack) = &args.stack {
        body.push_str("## Stack Trace\n\n```\n");
        body.push_str(stack);
        body.push_str("\n```\n\n");
    }

    if let Some(msgs) = &args.context.last_messages {
        body.push_str("## 最近消息（最多 5 条）\n\n```\n");
        body.push_str(msgs);
        body.push_str("\n```\n\n");
    }

    body.push_str("---\n*自动生成 by Codex gx bug report*");

    let labels = vec!["bug".to_string(), "auto-reported".to_string()];
    let url = format!(
        "https://github.com/opc007/codex-gx/issues/new?title={}&body={}&labels={}",
        urlencoding_encode(&title),
        urlencoding_encode(&body),
        urlencoding_encode(&labels.join(",")),
    );

    Ok(BugReport {
        title,
        body,
        labels,
        github_url: url,
    })
}

fn urlencoding_encode(s: &str) -> String {
    // 简化版 percent encoding
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

fn chrono_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 安装 panic hook —— 在 builder.setup() 里调用
pub fn install_panic_hook(state: std::sync::Arc<BugReportState>) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let msg = format!("Panic: {} (at {})", info, loc);
        let entry = CrashEntry {
            id: format!("panic_{}_{}", chrono_now(), short_id(&msg)),
            timestamp: chrono_now(),
            source: "panic".to_string(),
            severity: "fatal".to_string(),
            message: msg.clone(),
            stack: Some(loc),
            session_id: None,
            model: None,
            context: None,
            user_note: None,
        };
        let _ = state.record(entry);
        eprintln!("[v1.3 bugreport] {msg}");
        // 调用前一个 hook（保证默认行为，比如打印 stack）
        prev(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encoding() {
        assert_eq!(
            urlencoding_encode("hello world"),
            "hello%20world".to_string()
        );
        assert_eq!(
            urlencoding_encode("a+b=c"),
            "a%2Bb%3Dc".to_string()
        );
    }

    #[test]
    fn test_record_and_list() {
        let dir = std::env::temp_dir().join("agentshell_bug_test");
        let _ = std::fs::create_dir_all(&dir);
        let st = BugReportState {
            crashes_dir: dir.clone(),
            reports: Mutex::new(Vec::new()),
        };
        let entry = CrashEntry {
            id: "x".to_string(),
            timestamp: 1,
            source: "test".to_string(),
            severity: "error".to_string(),
            message: "test".to_string(),
            stack: None,
            session_id: None,
            model: None,
            context: None,
            user_note: None,
        };
        st.record(entry).unwrap();
        let list = st.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message, "test");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_bug_report() {
        let report = bug_report_build(BuildArgs {
            message: "崩溃".to_string(),
            stack: Some("at foo.rs:10".to_string()),
            context: BugReportContext {
                os: "macos".to_string(),
                arch: "aarch64".to_string(),
                app_version: "1.3.0".to_string(),
                session_id: Some("s1".to_string()),
                model: Some("MiniMax-M3".to_string()),
                last_messages: Some("Hi".to_string()),
                workspace_id: Some("default".to_string()),
                active_theme: Some("Nord".to_string()),
                routing_strategy_id: Some("default".to_string()),
            },
            user_note: Some("我点了 X 然后崩了".to_string()),
        })
        .unwrap();
        assert!(report.title.starts_with("🐛"));
        assert!(report.body.contains("崩溃"));
        assert!(report.body.contains("我点了 X 然后崩了"));
        assert!(report.body.contains("macos"));
        assert!(report.github_url.contains("github.com"));
    }
}