import { useSessionsStore, type SessionMeta, type PersistedMessage } from "../stores/sessions";
import { exportSession, type ExportFormat } from "../lib/export";
import { useState } from "react";
import { useOpenTabs, openTab, closeTab, closeOtherTabs, closeAllTabs } from "../stores/tabs";

export function Sidebar() {
  const sessions = useSessionsStore((s) => s.sessions);
  const currentId = useSessionsStore((s) => s.currentId);
  const setCurrent = useSessionsStore((s) => s.setCurrent);
  const create = useSessionsStore((s) => s.create);
  const remove = useSessionsStore((s) => s.remove);
  const messages = useSessionsStore((s) => s.messages);
  const openTabs = useOpenTabs();

  const [exportOpen, setExportOpen] = useState<string | null>(null);
  const [tabMenuOpen, setTabMenuOpen] = useState(false);
  const [redactOnExport, setRedactOnExport] = useState(true);

  const doExport = async (s: SessionMeta, fmt: ExportFormat) => {
    const msgs: PersistedMessage[] = messages[s.id] ?? [];
    try {
      const path = await exportSession(s, msgs, fmt, undefined, redactOnExport);
      setExportOpen(null);
      alert(
        `已导出到：${path}\n${redactOnExport ? "（已脱敏敏感数据）" : "（未脱敏）"}`,
      );
    } catch (e: any) {
      if (!String(e).includes("已取消")) {
        alert(`导出失败：${e}`);
      }
      setExportOpen(null);
    }
  };

  const tabSessions = openTabs
    .map((id) => sessions.find((s) => s.id === id))
    .filter((s): s is SessionMeta => Boolean(s));

  return (
    <aside className="sidebar">
      {/* v1.1：标签栏 */}
      {tabSessions.length > 0 && (
        <div className="tab-bar">
          <div className="tab-list">
            {tabSessions.map((s) => (
              <div
                key={s.id}
                className={`tab ${s.id === currentId ? "active" : ""}`}
                onClick={() => setCurrent(s.id)}
                title={s.title}
              >
                <span className="tab-title">
                  {s.title.length > 12 ? s.title.slice(0, 12) + "…" : s.title}
                </span>
                <button
                  className="tab-close"
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTab(s.id);
                  }}
                  title="关闭标签"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
          <div className="tab-menu-wrap">
            <button
              className="tab-menu-btn"
              onClick={() => setTabMenuOpen(!tabMenuOpen)}
              title="标签管理"
            >
              ⋯
            </button>
            {tabMenuOpen && (
              <div className="tab-menu" onClick={() => setTabMenuOpen(false)}>
                {currentId && (
                  <button onClick={() => closeOtherTabs(currentId)}>
                    关闭其他
                  </button>
                )}
                <button onClick={closeAllTabs}>关闭所有</button>
              </div>
            )}
          </div>
        </div>
      )}

      <div className="sidebar-header">
        <span>会话 ({sessions.length})</span>
        <button
          className="sidebar-new"
          onClick={() => {
            const s = create();
            openTab(s.id);
          }}
          title="新建会话 (并打开标签)"
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
            className={`session-item ${s.id === currentId ? "active" : ""} ${
              openTabs.includes(s.id) ? "tabbed" : ""
            }`}
            onClick={() => {
              setCurrent(s.id);
              openTab(s.id); // 点击侧边栏自动 pin 到 tab
            }}
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
                  if (confirm(`删除 "${s.title}"？`)) {
                    remove(s.id);
                    closeTab(s.id);
                  }
                }}
                title="删除"
              >
                ×
              </button>
            </div>
            {exportOpen === s.id && (
              <div className="export-menu" onClick={(e) => e.stopPropagation()}>
                <label className="export-redact">
                  <input
                    type="checkbox"
                    checked={redactOnExport}
                    onChange={(e) => setRedactOnExport(e.target.checked)}
                  />
                  脱敏敏感数据
                </label>
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