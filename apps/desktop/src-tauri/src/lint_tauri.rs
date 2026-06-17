//! v1.4：代码 review / 静态分析 tauri 命令

use lint::{Linter, LintReport};
use std::path::PathBuf;

#[tauri::command]
pub async fn lint_run(path: Option<String>) -> Result<Vec<LintReport>, String> {
    let root = PathBuf::from(path.unwrap_or_else(|| ".".to_string()));
    let linter = Linter::new();
    Ok(linter.run_all(&root).await)
}

/// 摘要：返回总问题数 + 各检查器结果
#[tauri::command]
pub async fn lint_run_summary(path: Option<String>) -> Result<LintSummary, String> {
    let root = PathBuf::from(path.unwrap_or_else(|| ".".to_string()));
    let linter = Linter::new();
    let reports = linter.run_all(&root).await;
    let total_errors: u32 = reports
        .iter()
        .flat_map(|r| r.issues.iter())
        .filter(|i| i.severity == lint::Severity::Error)
        .count() as u32;
    let total_warnings: u32 = reports
        .iter()
        .flat_map(|r| r.issues.iter())
        .filter(|i| i.severity == lint::Severity::Warning)
        .count() as u32;
    let total_infos: u32 = reports
        .iter()
        .flat_map(|r| r.issues.iter())
        .filter(|i| i.severity == lint::Severity::Info)
        .count() as u32;
    let total_ms: u64 = reports.iter().map(|r| r.duration_ms).sum();
    Ok(LintSummary {
        total_errors,
        total_warnings,
        total_infos,
        total_ms,
        reports,
    })
}

#[derive(serde::Serialize)]
pub struct LintSummary {
    pub total_errors: u32,
    pub total_warnings: u32,
    pub total_infos: u32,
    pub total_ms: u64,
    pub reports: Vec<LintReport>,
}