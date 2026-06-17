// v1.4：代码 review / 静态分析 UI
// - 一键运行所有检查器（clippy / tsc / TODO 扫描）
// - 按文件 / 按严重度分组
// - 点击 issue 在编辑器里打开（占位）

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Severity = "error" | "warning" | "info";

type LintIssue = {
  file: string;
  line: number | null;
  column: number | null;
  severity: Severity;
  code: string | null;
  message: string;
};

type LintReport = {
  checker: string;
  issues: LintIssue[];
  summary: string;
  duration_ms: number;
  skipped_reason: string | null;
  raw_output: string | null;
};

type LintSummary = {
  total_errors: number;
  total_warnings: number;
  total_infos: number;
  total_ms: number;
  reports: LintReport[];
};

type Props = {
  onClose: () => void;
};

export function CodeReviewDialog({ onClose }: Props) {
  const [path, setPath] = useState(".");
  const [summary, setSummary] = useState<LintSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState<"all" | "error" | "warning" | "info">(
    "all",
  );

  const run = async () => {
    setBusy(true);
    try {
      const s = await invoke<LintSummary>("lint_run_summary", { path });
      setSummary(s);
    } catch (e) {
      alert(`❌ 运行失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  const severityClass = (s: Severity) =>
    s === "error" ? "lint-sev-error" : s === "warning" ? "lint-sev-warn" : "lint-sev-info";

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 900, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🔍 代码 review / 静态分析（v1.4）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">
            ×
          </button>
        </div>

        <div className="modal-body theme-body">
          <div className="lint-path-row">
            <label>项目路径</label>
            <input
              className="vault-password-input"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="."
            />
            <button
              className="btn primary"
              onClick={run}
              disabled={busy}
            >
              {busy ? "扫描中…" : "🔍 扫描"}
            </button>
          </div>

          {summary && (
            <>
              <div className="lint-stats">
                <div className="lint-stat lint-sev-error">
                  <span className="lint-stat-n">{summary.total_errors}</span>
                  <span className="lint-stat-l">error</span>
                </div>
                <div className="lint-stat lint-sev-warn">
                  <span className="lint-stat-n">{summary.total_warnings}</span>
                  <span className="lint-stat-l">warning</span>
                </div>
                <div className="lint-stat lint-sev-info">
                  <span className="lint-stat-n">{summary.total_infos}</span>
                  <span className="lint-stat-l">info</span>
                </div>
                <div className="lint-stat">
                  <span className="lint-stat-n">{(summary.total_ms / 1000).toFixed(1)}s</span>
                  <span className="lint-stat-l">耗时</span>
                </div>
              </div>

              <div className="lint-filter">
                <button
                  className={`btn small ${filter === "all" ? "primary" : ""}`}
                  onClick={() => setFilter("all")}
                >
                  全部
                </button>
                <button
                  className={`btn small ${filter === "error" ? "primary" : ""}`}
                  onClick={() => setFilter("error")}
                >
                  error
                </button>
                <button
                  className={`btn small ${filter === "warning" ? "primary" : ""}`}
                  onClick={() => setFilter("warning")}
                >
                  warning
                </button>
                <button
                  className={`btn small ${filter === "info" ? "primary" : ""}`}
                  onClick={() => setFilter("info")}
                >
                  info
                </button>
              </div>

              <div className="lint-reports">
                {summary.reports.map((r) => {
                  const filteredIssues = r.issues.filter(
                    (i) => filter === "all" || i.severity === filter,
                  );
                  return (
                    <div key={r.checker} className="lint-report">
                      <div className="lint-report-head">
                        <span className="lint-checker-name">{r.checker}</span>
                        <span className="lint-checker-summary">{r.summary}</span>
                        <span className="lint-checker-time">
                          {(r.duration_ms / 1000).toFixed(2)}s
                        </span>
                        {r.skipped_reason && (
                          <span className="lint-checker-skip">
                            ⏭️ {r.skipped_reason}
                          </span>
                        )}
                      </div>
                      {r.skipped_reason ? null : filteredIssues.length === 0 ? (
                        <p
                          style={{
                            color: "var(--text-muted)",
                            fontSize: 12,
                            padding: "4px 0",
                          }}
                        >
                          （无）
                        </p>
                      ) : (
                        <div className="lint-issue-list">
                          {filteredIssues.slice(0, 200).map((i, idx) => (
                            <div key={idx} className="lint-issue">
                              <span className={severityClass(i.severity)}>
                                {i.severity === "error"
                                  ? "❌"
                                  : i.severity === "warning"
                                    ? "⚠️"
                                    : "ℹ️"}
                              </span>
                              <span className="lint-issue-file">
                                {i.file.split("/").slice(-3).join("/")}
                                {i.line && `:${i.line}`}
                                {i.column && `:${i.column}`}
                              </span>
                              {i.code && (
                                <code className="lint-issue-code">{i.code}</code>
                              )}
                              <span className="lint-issue-msg">{i.message}</span>
                            </div>
                          ))}
                          {filteredIssues.length > 200 && (
                            <p
                              style={{
                                color: "var(--text-muted)",
                                fontSize: 12,
                                padding: "4px 0",
                              }}
                            >
                              …还有 {filteredIssues.length - 200} 项
                            </p>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </>
          )}

          <div className="lint-help">
            <h4>💡 检查器</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li>
                <strong>rust-clippy</strong>：cargo clippy --message-format=json（仅 correctness 警告，避免噪音）
              </li>
              <li>
                <strong>tsc</strong>：tsc --noEmit（需要 tsc / npx 在 PATH）
              </li>
              <li>
                <strong>todo-scanner</strong>：扫描 .rs / .ts / .tsx / .js / .py / .go 等源文件里的 TODO / FIXME / XXX / HACK
                注释
              </li>
              <li>大型目录（node_modules / target / dist / .git）自动跳过</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}