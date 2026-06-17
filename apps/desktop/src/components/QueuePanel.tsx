// v1.4：任务队列面板

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

type TaskStatus = "pending" | "running" | "completed" | "failed" | "cancelled";
type TaskKind = "agent" | "command" | "lint" | "custom";

type Task = {
  id: string;
  kind: TaskKind;
  title: string;
  description: string | null;
  status: TaskStatus;
  progress: number;
  log: string[];
  result: string | null;
  error: string | null;
  created_at: number;
  started_at: number | null;
  completed_at: number | null;
  session_id: string | null;
  input: unknown;
};

type Props = {
  onClose: () => void;
  sessionId?: string;
};

export function QueuePanel({ onClose, sessionId }: Props) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState<"all" | "active" | "done" | "failed">("all");

  const refresh = async () => {
    setBusy(true);
    try {
      const list = await invoke<Task[]>("queue_list");
      setTasks(list);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
    let unlisten: UnlistenFn | null = null;
    listen<{ kind: string; payload: Task | { id: string; progress: number; log: string | null } }>(
      "queue:event",
      () => {
        void refresh();
      },
    ).then((u) => {
      unlisten = u;
    });
    return () => {
      if (unlisten) unlisten();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleCancel = async (id: string) => {
    await invoke("queue_cancel", { id });
    await refresh();
  };

  const handleClear = async () => {
    const n = await invoke<number>("queue_clear_finished");
    alert(`已清理 ${n} 个完成任务`);
    await refresh();
  };

  const handleNewCommand = async () => {
    const cmd = prompt("输入 shell 命令：");
    if (!cmd) return;
    await invoke("queue_enqueue", {
      args: {
        kind: "command",
        title: cmd.slice(0, 40),
        input: { cmd },
        session_id: sessionId ?? null,
        description: null,
      },
    });
    await refresh();
  };

  const handleNewLint = async () => {
    const path = prompt("项目路径（默认 .）：", ".");
    if (path === null) return;
    await invoke("queue_enqueue", {
      args: {
        kind: "lint",
        title: `lint ${path}`,
        input: { path: path || "." },
        session_id: sessionId ?? null,
        description: null,
      },
    });
    await refresh();
  };

  const filtered = tasks.filter((t) => {
    if (filter === "all") return true;
    if (filter === "active")
      return t.status === "pending" || t.status === "running";
    if (filter === "done")
      return t.status === "completed" || t.status === "cancelled";
    if (filter === "failed") return t.status === "failed";
    return true;
  });

  const statusIcon = (s: TaskStatus) =>
    s === "pending" ? "⏳" :
    s === "running" ? "▶️" :
    s === "completed" ? "✅" :
    s === "failed" ? "❌" :
    "🚫";

  const kindIcon = (k: TaskKind) =>
    k === "agent" ? "🤖" :
    k === "command" ? "💻" :
    k === "lint" ? "🔍" :
    "📦";

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 800, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>📋 任务队列（v1.4）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">×</button>
        </div>

        <div className="modal-body theme-body">
          <div className="queue-toolbar">
            <button className="btn small" onClick={refresh} disabled={busy}>
              {busy ? "刷新…" : "🔄 刷新"}
            </button>
            <button className="btn small" onClick={handleNewCommand}>
              💻 新建命令
            </button>
            <button className="btn small" onClick={handleNewLint}>
              🔍 新建 lint
            </button>
            <button className="btn small" onClick={handleClear}>
              🧹 清理已完成
            </button>
            <div style={{ flex: 1 }} />
            <div className="queue-filter">
              {(["all", "active", "done", "failed"] as const).map((f) => (
                <button
                  key={f}
                  className={`btn small ${filter === f ? "primary" : ""}`}
                  onClick={() => setFilter(f)}
                >
                  {f === "all" ? "全部" :
                   f === "active" ? "进行中" :
                   f === "done" ? "完成" : "失败"}
                </button>
              ))}
            </div>
          </div>

          {filtered.length === 0 ? (
            <p style={{ color: "var(--text-muted)", textAlign: "center", padding: 24 }}>
              队列为空 — 点击"新建命令"或"新建 lint"试试
            </p>
          ) : (
            <div className="queue-list">
              {filtered.map((t) => (
                <div key={t.id} className={`queue-item queue-${t.status}`}>
                  <div className="queue-item-head">
                    <span className="queue-item-icon">{statusIcon(t.status)}</span>
                    <span className="queue-item-kind">{kindIcon(t.kind)}</span>
                    <span className="queue-item-title" title={t.title}>
                      {t.title}
                    </span>
                    <span className="queue-item-id">{t.id.slice(0, 8)}</span>
                    <span style={{ flex: 1 }} />
                    {(t.status === "pending" || t.status === "running") && (
                      <button
                        className="btn small"
                        onClick={() => handleCancel(t.id)}
                        title="取消"
                      >
                        ⏹
                      </button>
                    )}
                  </div>

                  {t.status === "running" && (
                    <div className="queue-progress">
                      <div
                        className="queue-progress-bar"
                        style={{ width: `${Math.max(2, t.progress * 100)}%` }}
                      />
                      <span className="queue-progress-text">
                        {Math.round(t.progress * 100)}%
                      </span>
                    </div>
                  )}

                  {t.description && (
                    <div className="queue-item-desc">{t.description}</div>
                  )}

                  {t.log.length > 0 && (
                    <details className="queue-item-log">
                      <summary>日志 ({t.log.length})</summary>
                      <pre>{t.log.slice(-30).join("\n")}</pre>
                    </details>
                  )}

                  {t.result && t.status === "completed" && (
                    <details className="queue-item-result">
                      <summary>结果</summary>
                      <pre>{t.result.slice(0, 2000)}</pre>
                    </details>
                  )}

                  {t.error && t.status === "failed" && (
                    <div className="queue-item-error">❌ {t.error}</div>
                  )}

                  <div className="queue-item-meta">
                    创建: {new Date(t.created_at).toLocaleTimeString()}
                    {t.started_at && ` · 启动: ${new Date(t.started_at).toLocaleTimeString()}`}
                    {t.completed_at && ` · 耗时 ${(t.completed_at - (t.started_at ?? t.created_at))}ms`}
                    {t.session_id && ` · 📎 ${t.session_id.slice(0, 8)}`}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}