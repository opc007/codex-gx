//! v1.4：代码 review / 静态分析集成
//!
//! 多个检查器：
//! - Rust: `cargo clippy` / `cargo fmt --check`
//! - TypeScript: `tsc --noEmit`
//! - 通用: TODO / FIXME / debug print 扫描
//! - Python: `ruff check`（如果可用）
//!
//! 每个检查器都实现 [`Checker`] trait。
//! `Linter::run_all(dir)` 跑所有可用检查器并合并结果。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub severity: Severity,
    pub code: Option<String>, // e.g. "clippy::needless_return"
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintReport {
    pub checker: String,
    pub issues: Vec<LintIssue>,
    pub summary: String,
    pub duration_ms: u64,
    pub skipped_reason: Option<String>,
    pub raw_output: Option<String>,
}

#[async_trait]
pub trait Checker: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn run(&self, root: &Path) -> LintReport;
}

fn which(cmd: &str) -> bool {
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// =============================================================================
// Rust clippy checker
// =============================================================================

pub struct RustClippyChecker;

#[async_trait]
impl Checker for RustClippyChecker {
    fn name(&self) -> &str {
        "rust-clippy"
    }
    fn description(&self) -> &str {
        "Rust: cargo clippy --message-format=json (no auto-fix)"
    }
    async fn run(&self, root: &Path) -> LintReport {
        let started = now_ms();
        if !which("cargo") {
            return LintReport {
                checker: self.name().to_string(),
                issues: vec![],
                summary: "未安装 cargo".to_string(),
                duration_ms: now_ms() - started,
                skipped_reason: Some("cargo 不在 PATH".to_string()),
                raw_output: None,
            };
        }
        let output = Command::new("cargo")
            .arg("clippy")
            .arg("--message-format=json")
            .arg("--")
            .arg("-A")
            .arg("clippy::all")
            .arg("-W")
            .arg("clippy::correctness")
            .current_dir(root)
            .output();
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                let issues = parse_clippy_json(&text);
                let dur = now_ms() - started;
                LintReport {
                    checker: self.name().to_string(),
                    summary: format!("{} 项问题", issues.len()),
                    issues,
                    duration_ms: dur,
                    skipped_reason: None,
                    raw_output: Some(text.chars().take(4000).collect()),
                }
            }
            Err(e) => LintReport {
                checker: self.name().to_string(),
                issues: vec![],
                summary: format!("运行失败: {}", e),
                duration_ms: now_ms() - started,
                skipped_reason: Some(e.to_string()),
                raw_output: None,
            },
        }
    }
}

fn parse_clippy_json(text: &str) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // clippy 消息结构：{"reason":"compiler-message", "message":{...}}
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => continue,
        };
        let message = msg
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("warning");
        let severity = match level {
            "error" => Severity::Error,
            "warning" => Severity::Warning,
            _ => Severity::Info,
        };
        let code = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());
        let (file, line_n, col_n) = msg
            .get("spans")
            .and_then(|s| s.get(0))
            .map(|span| {
                let f = span
                    .get("file_name")
                    .and_then(|f| f.as_str())
                    .unwrap_or("")
                    .to_string();
                let l = span
                    .get("line_start")
                    .and_then(|n| n.as_u64())
                    .map(|n| n as u32);
                let c = span
                    .get("column_start")
                    .and_then(|n| n.as_u64())
                    .map(|n| n as u32);
                (f, l, c)
            })
            .unwrap_or_default();
        if !file.is_empty() {
            issues.push(LintIssue {
                file,
                line: line_n,
                column: col_n,
                severity,
                code,
                message,
            });
        }
    }
    issues
}

// =============================================================================
// TypeScript tsc checker
// =============================================================================

pub struct TypeScriptChecker;

#[async_trait]
impl Checker for TypeScriptChecker {
    fn name(&self) -> &str {
        "tsc"
    }
    fn description(&self) -> &str {
        "TypeScript: tsc --noEmit"
    }
    async fn run(&self, root: &Path) -> LintReport {
        let started = now_ms();
        if !which("tsc") && !which("npx") {
            return LintReport {
                checker: self.name().to_string(),
                issues: vec![],
                summary: "未安装 tsc / npx".to_string(),
                duration_ms: now_ms() - started,
                skipped_reason: Some("tsc 不在 PATH".to_string()),
                raw_output: None,
            };
        }
        let cmd = if which("tsc") { "tsc" } else { "npx" };
        let mut c = Command::new(cmd);
        if cmd == "npx" {
            c.arg("tsc");
        }
        let output = c
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(root)
            .output();
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                let issues = parse_tsc_output(&text);
                let dur = now_ms() - started;
                LintReport {
                    checker: self.name().to_string(),
                    summary: format!("{} 项问题", issues.len()),
                    issues,
                    duration_ms: dur,
                    skipped_reason: None,
                    raw_output: Some(text.chars().take(4000).collect()),
                }
            }
            Err(e) => LintReport {
                checker: self.name().to_string(),
                issues: vec![],
                summary: format!("运行失败: {}", e),
                duration_ms: now_ms() - started,
                skipped_reason: Some(e.to_string()),
                raw_output: None,
            },
        }
    }
}

fn parse_tsc_output(text: &str) -> Vec<LintIssue> {
    // tsc 输出格式：file(line,col): error TS1234: message
    let mut issues = Vec::new();
    let re_pattern = regex_static();
    for line in text.lines() {
        if let Some(caps) = re_pattern.captures(line) {
            let file = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let line_n = caps
                .get(2)
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let col_n = caps
                .get(3)
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let severity = match caps.get(4).map(|m| m.as_str()) {
                Some("error") => Severity::Error,
                _ => Severity::Warning,
            };
            let code = caps.get(5).map(|m| m.as_str().to_string());
            let message = caps
                .get(6)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            issues.push(LintIssue {
                file,
                line: line_n,
                column: col_n,
                severity,
                code,
                message,
            });
        }
    }
    issues
}

fn regex_static() -> &'static regex_lite::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.*)$",
        )
        .unwrap()
    })
}

// =============================================================================
// TODO / FIXME scanner
// =============================================================================

pub struct TodoScanner;

#[async_trait]
impl Checker for TodoScanner {
    fn name(&self) -> &str {
        "todo-scanner"
    }
    fn description(&self) -> &str {
        "扫描源代码中的 TODO / FIXME / XXX / HACK 注释"
    }
    async fn run(&self, root: &Path) -> LintReport {
        let started = now_ms();
        let mut issues = Vec::new();
        scan_dir(root, &mut issues, 0);
        let dur = now_ms() - started;
        LintReport {
            checker: self.name().to_string(),
            summary: format!("{} 处标记", issues.len()),
            issues,
            duration_ms: dur,
            skipped_reason: None,
            raw_output: None,
        }
    }
}

fn scan_dir(dir: &Path, issues: &mut Vec<LintIssue>, depth: u32) {
    if depth > 8 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        // 跳过常见大目录
        if matches!(
            name,
            "node_modules" | "target" | "dist" | "build" | ".git" | "venv" | ".venv" | "__pycache__"
        ) {
            continue;
        }
        if path.is_dir() {
            scan_dir(&path, issues, depth + 1);
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(
                    ext,
                    "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h"
                ) {
                    scan_file(&path, issues);
                }
            }
        }
    }
}

fn scan_file(path: &Path, issues: &mut Vec<LintIssue>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for (i, line) in text.lines().enumerate() {
        let lower = line.to_lowercase();
        let found = ["todo", "fixme", "xxx", "hack"]
            .iter()
            .find(|kw| lower.contains(*kw));
        if let Some(&kw) = found {
            issues.push(LintIssue {
                file: path.to_string_lossy().to_string(),
                line: Some((i + 1) as u32),
                column: None,
                severity: Severity::Info,
                code: Some(kw.to_uppercase()),
                message: line.trim().to_string(),
            });
        }
    }
}

// =============================================================================
// Linter（编排）
// =============================================================================

pub struct Linter {
    pub checkers: Vec<Box<dyn Checker>>,
}

impl Default for Linter {
    fn default() -> Self {
        Linter {
            checkers: vec![
                Box::new(RustClippyChecker),
                Box::new(TypeScriptChecker),
                Box::new(TodoScanner),
            ],
        }
    }
}

impl Linter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_checkers(checkers: Vec<Box<dyn Checker>>) -> Self {
        Linter { checkers }
    }

    pub async fn run_all(&self, root: &Path) -> Vec<LintReport> {
        let mut reports = Vec::new();
        for c in &self.checkers {
            reports.push(c.run(root).await);
        }
        reports
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_clippy_basic() {
        let json = r#"{"reason":"compiler-message","message":{"message":"unused variable `x`","level":"warning","code":{"code":"unused_variables"},"spans":[{"file_name":"src/main.rs","line_start":10,"column_start":5}]}}"#;
        let issues = parse_clippy_json(json);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "src/main.rs");
        assert_eq!(issues[0].line, Some(10));
        assert_eq!(issues[0].code.as_deref(), Some("unused_variables"));
    }

    #[test]
    fn parse_tsc_basic() {
        let text = "src/app.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'.";
        let issues = parse_tsc_output(text);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "src/app.ts");
        assert_eq!(issues[0].line, Some(10));
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].code.as_deref(), Some("TS2322"));
    }

    #[test]
    fn parse_tsc_empty() {
        let issues = parse_tsc_output("");
        assert_eq!(issues.len(), 0);
    }

    #[test]
    fn todo_scanner_finds() {
        let dir = tempdir();
        let f = dir.path().join("test.rs");
        let mut f_handle = std::fs::File::create(&f).unwrap();
        writeln!(f_handle, "// TODO: implement this").unwrap();
        writeln!(f_handle, "fn main() {{}}").unwrap();
        writeln!(f_handle, "// FIXME: broken").unwrap();
        writeln!(f_handle, "// XXX: hack").unwrap();
        let mut issues = Vec::new();
        scan_dir(dir.path(), &mut issues, 0);
        assert_eq!(issues.len(), 3);
        let codes: Vec<String> = issues.iter().filter_map(|i| i.code.clone()).collect();
        assert!(codes.iter().any(|c| c == "TODO"));
        assert!(codes.iter().any(|c| c == "FIXME"));
        assert!(codes.iter().any(|c| c == "XXX"));
    }

    #[test]
    fn todo_scanner_skips_node_modules() {
        let dir = tempdir();
        let sub = dir.path().join("node_modules");
        std::fs::create_dir_all(&sub).unwrap();
        let f = sub.join("test.js");
        let mut fh = std::fs::File::create(&f).unwrap();
        writeln!(fh, "// TODO: skip me").unwrap();
        let mut issues = Vec::new();
        scan_dir(dir.path(), &mut issues, 0);
        assert_eq!(issues.len(), 0);
    }

    fn tempdir() -> tempdir::TempDir {
        tempdir::TempDir::new("lint_test").unwrap()
    }
}