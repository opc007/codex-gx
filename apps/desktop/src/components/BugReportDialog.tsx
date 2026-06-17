// v1.3：Bug Report 弹窗
// - 显示已记录 crashes
// - 一键生成 GitHub issue URL
// - 支持手动填写 / 上报 bug

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { useSessionsStore, type PersistedMessage } from "../stores/sessions";
import { useCurrentWorkspaceId } from "../stores/workspace";
import { useThemeStore } from "../stores/theme";

type CrashEntry = {
  id: string;
  timestamp: number;
  source: string;
  severity: string;
  message: string;
  stack: string | null;
  session_id: string | null;
  model: string | null;
  context: unknown;
  user_note: string | null;
};

type Props = {
  onClose: () => void;
};

export function BugReportDialog({ onClose }: Props) {
  const [crashes, setCrashes] = useState<CrashEntry[]>([]);
  const [message, setMessage] = useState("");
  const [userNote, setUserNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [lastReport, setLastReport] = useState<{
    title: string;
    body: string;
    github_url: string;
  } | null>(null);
  const currentId = useSessionsStore((s) => s.currentId);
  const messages = useSessionsStore((s) => s.messages);
  const workspaceId = useCurrentWorkspaceId();
  const activeTheme = useThemeStore((s) => s.activeThemeId);

  const refresh = async () => {
    try {
      const list = await invoke<CrashEntry[]>("bug_report_list");
      setCrashes(list.slice().reverse()); // 最新在前
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const handleBuild = async () => {
    if (!message.trim()) {
      alert("请先填写 bug 描述");
      return;
    }
    setBusy(true);
    try {
      // 取最近 5 条消息
      const lastMsgs = (messages[currentId ?? ""] ?? [])
        .slice(-5)
        .map((m: PersistedMessage) => `[${m.role}] ${m.text.slice(0, 200)}`)
        .join("\n");
      const r = await invoke<{
        title: string;
        body: string;
        github_url: string;
      }>("bug_report_build", {
        args: {
          message: message.trim(),
          stack: null,
          context: {
            os: `${navigator.platform} ${navigator.userAgent}`,
            arch: "unknown",
            app_version: "1.3.0",
            session_id: currentId ?? null,
            model: null, // 可扩展
            last_messages: lastMsgs || null,
            workspace_id: workspaceId,
            active_theme: activeTheme,
            routing_strategy_id: "default",
          },
          user_note: userNote.trim() || null,
        },
      });
      setLastReport(r);
      // 同时记录到 crash 列表
      await invoke("bug_report_record", {
        args: {
          source: "manual",
          severity: "error",
          message: message.trim(),
          stack: null,
          session_id: currentId ?? null,
          model: null,
          context: null,
          user_note: userNote.trim() || null,
        },
      });
      void refresh();
    } catch (e) {
      alert(`❌ 生成失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleOpenGitHub = async () => {
    if (!lastReport) return;
    try {
      await openUrl(lastReport.github_url);
    } catch {
      // 复制到剪贴板
      await navigator.clipboard.writeText(lastReport.github_url);
      alert("已复制 issue URL 到剪贴板");
    }
  };

  const handleCopyBody = async () => {
    if (!lastReport) return;
    await navigator.clipboard.writeText(lastReport.body);
    alert("已复制 issue body 到剪贴板");
  };

  const handleClear = async () => {
    if (!confirm("清空所有 crash 记录？")) return;
    await invoke("bug_report_clear");
    setCrashes([]);
  };

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 760, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🐞 Bug 报告（v1.3）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">
            ×
          </button>
        </div>

        <div className="modal-body theme-body">
          <div className="bug-form">
            <h4>📝 上报新 bug</h4>
            <textarea
              className="vault-password-input"
              style={{ width: "100%", minHeight: 80, marginBottom: 8 }}
              placeholder="描述你遇到的 bug（必填）"
              value={message}
              onChange={(e) => setMessage(e.target.value)}
            />
            <textarea
              className="vault-password-input"
              style={{ width: "100%", minHeight: 50, marginBottom: 8 }}
              placeholder="补充：复现步骤、期望行为、截图说明等（可选）"
              value={userNote}
              onChange={(e) => setUserNote(e.target.value)}
            />
            <div className="modal-actions" style={{ marginBottom: 16 }}>
              <button className="btn primary" onClick={handleBuild} disabled={busy}>
                {busy ? "生成中…" : "生成 Issue"}
              </button>
            </div>

            {lastReport && (
              <div className="bug-report-result">
                <div style={{ marginBottom: 8 }}>
                  <strong>{lastReport.title}</strong>
                </div>
                <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                  <button className="btn primary" onClick={handleOpenGitHub}>
                    打开 GitHub Issue →
                  </button>
                  <button className="btn" onClick={handleCopyBody}>
                    复制内容
                  </button>
                </div>
                <details style={{ marginTop: 10 }}>
                  <summary style={{ cursor: "pointer", color: "var(--text-muted)" }}>
                    预览 issue body
                  </summary>
                  <pre
                    style={{
                      background: "var(--bg-secondary)",
                      padding: 10,
                      borderRadius: 4,
                      maxHeight: 300,
                      overflow: "auto",
                      fontSize: 12,
                      marginTop: 8,
                    }}
                  >
                    {lastReport.body}
                  </pre>
                </details>
              </div>
            )}
          </div>

          <div className="bug-list">
            <h4>
              📜 Crash 历史（{crashes.length}）
              <button
                className="btn small"
                onClick={handleClear}
                style={{ marginLeft: 12 }}
                disabled={crashes.length === 0}
              >
                清空
              </button>
              <button
                className="btn small"
                onClick={refresh}
                style={{ marginLeft: 6 }}
              >
                刷新
              </button>
            </h4>
            {crashes.length === 0 ? (
              <p style={{ color: "var(--text-muted)", fontSize: 13 }}>
                暂无 crash 记录。
              </p>
            ) : (
              crashes.map((c) => (
                <div key={c.id} className={`bug-entry bug-${c.severity}`}>
                  <div className="bug-entry-head">
                    <span className={`bug-severity ${c.severity}`}>
                      {c.severity === "fatal"
                        ? "💥"
                        : c.severity === "error"
                          ? "❌"
                          : "⚠️"}
                    </span>
                    <span className="bug-source">{c.source}</span>
                    <span className="bug-time">
                      {new Date(c.timestamp * 1000).toLocaleString()}
                    </span>
                  </div>
                  <div className="bug-message">{c.message}</div>
                  {c.stack && (
                    <pre className="bug-stack">{c.stack}</pre>
                  )}
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}