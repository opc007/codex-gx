import { useSessionsStore, type SessionMeta, type PersistedMessage } from "../stores/sessions";
import { exportSession, type ExportFormat } from "../lib/export";
import { useState } from "react";

export function Sidebar() {
  const sessions = useSessionsStore((s) => s.sessions);
  const currentId = useSessionsStore((s) => s.currentId);
  const setCurrent = useSessionsStore((s) => s.setCurrent);
  const create = useSessionsStore((s) => s.create);
  const remove = useSessionsStore((s) => s.remove);
  const messages = useSessionsStore((s) => s.messages);

  const [exportOpen, setExportOpen] = useState<string | null>(null);

  const doExport = async (s: SessionMeta, fmt: ExportFormat) => {
    const msgs: PersistedMessage[] = messages[s.id] ?? [];
    try {
      const path = await exportSession(s, msgs, fmt);
      setExportOpen(null);
      alert(`已导出到：${path}`);
    } catch (e: any) {
      if (!String(e).includes("已取消")) {
        alert(`导出失败：${e}`);
      }
      setExportOpen(null);
    }
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span>会话 ({sessions.length})</span>
        <button
          className="sidebar-new"
          onClick={() => create()}
          title="新建会话"
        >
          +
        </button>
      </div>
      <ul className="session-list">
        {sessions.length === 0 && (
          <li className="session-empty">还没有会话</li>
        )}
        {sessions.map((s) => (
          <li
            key={s.id}
            className={`session-item ${s.id === currentId ? "active" : ""}`}
            onClick={() => setCurrent(s.id)}
          >
            <span className="session-title">{s.title}</span>
            <div className="session-actions">
              <button
                className="session-export"
                onClick={(e) => {
                  e.stopPropagation();
                  setExportOpen(exportOpen === s.id ? null : s.id);
                }}
                title="导出"
              >
                ⬇
              </button>
              <button
                className="session-del"
                onClick={(e) => {
                  e.stopPropagation();
                  if (confirm(`删除 "${s.title}"？`)) remove(s.id);
                }}
                title="删除"
              >
                ×
              </button>
            </div>
            {exportOpen === s.id && (
              <div className="export-menu" onClick={(e) => e.stopPropagation()}>
                <button onClick={() => doExport(s, "markdown")}>📝 Markdown</button>
                <button onClick={() => doExport(s, "html")}>🌐 HTML</button>
                <button onClick={() => doExport(s, "json")}>📦 JSON</button>
              </div>
            )}
          </li>
        ))}
      </ul>
    </aside>
  );
}