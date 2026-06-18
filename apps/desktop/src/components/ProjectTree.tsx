import { useEffect, useMemo, useState } from "react";
import {
  useSessionsStore,
  type SessionMeta,
  type PersistedMessage,
} from "../stores/sessions";
import {
  useWorkspaceList,
  useCurrentWorkspaceId,
  switchWorkspace,
} from "../stores/workspace";
import { closeTab, openTab } from "../stores/tabs";
import { formatRelativeTime } from "../lib/formatRelativeTime";
import { exportSession, type ExportFormat } from "../lib/export";

type Props = {
  encryptedSet: Set<string>;
  onVaultPrompt: (p: { sessionId: string; mode: "encrypt" | "decrypt" }) => void;
  onNewProject: () => void;
};

type Section = "projects" | "chats";

export function ProjectTree({
  encryptedSet,
  onVaultPrompt,
  onNewProject,
}: Props) {
  const workspaces = useWorkspaceList();
  const allSessions = useSessionsStore((s) => s.sessions);
  const currentId = useSessionsStore((s) => s.currentId);
  const setCurrent = useSessionsStore((s) => s.setCurrent);
  const create = useSessionsStore((s) => s.create);
  const remove = useSessionsStore((s) => s.remove);
  const messages = useSessionsStore((s) => s.messages);
  const currentWorkspace = useCurrentWorkspaceId();

  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set([currentWorkspace]),
  );
  const [exportOpen, setExportOpen] = useState<string | null>(null);
  const [redactOnExport, setRedactOnExport] = useState(true);
  const [collapsedSections, setCollapsedSections] = useState<Set<Section>>(new Set());

  const sessionsByWs = useMemo(() => {
    const map = new Map<string, SessionMeta[]>();
    for (const w of workspaces) map.set(w.id, []);
    for (const s of allSessions) {
      const wsId = s.workspaceId ?? "default";
      if (!map.has(wsId)) map.set(wsId, []);
      map.get(wsId)!.push(s);
    }
    for (const list of map.values()) {
      list.sort((a, b) => b.updatedAt - a.updatedAt);
    }
    return map;
  }, [workspaces, allSessions]);

  const recentSessions = useMemo(() => {
    return [...allSessions]
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .slice(0, 20);
  }, [allSessions]);

  useEffect(() => {
    setExpanded((prev) => {
      if (prev.has(currentWorkspace)) return prev;
      const next = new Set(prev);
      next.add(currentWorkspace);
      return next;
    });
  }, [currentWorkspace]);

  const toggleSection = (s: Section) => {
    setCollapsedSections((prev) => {
      const next = new Set(prev);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      return next;
    });
  };

  const selectWorkspace = (wsId: string) => {
    if (wsId !== currentWorkspace) switchWorkspace(wsId);
    setExpanded((prev) => {
      if (prev.has(wsId)) return prev;
      const next = new Set(prev);
      next.add(wsId);
      return next;
    });
  };

  const selectSession = (wsId: string, sessionId: string) => {
    if (wsId !== currentWorkspace) switchWorkspace(wsId);
    setCurrent(sessionId);
    openTab(sessionId, wsId);
  };

  const handleNewChat = () => {
    const s = create();
    setCurrent(s.id);
    openTab(s.id, currentWorkspace);
  };

  const handleDelete = (id: string, title: string) => {
    if (!confirm(`删除会话「${title}」？`)) return;
    remove(id);
    closeTab(id);
  };

  const doExport = async (s: SessionMeta, fmt: ExportFormat) => {
    const msgs: PersistedMessage[] = messages[s.id] ?? [];
    try {
      const path = await exportSession(s, msgs, fmt, undefined, redactOnExport);
      setExportOpen(null);
      alert(
        `已导出到：${path}\n${redactOnExport ? "（已脱敏敏感数据）" : "（未脱敏）"}`,
      );
    } catch (e: unknown) {
      if (!String(e).includes("已取消")) {
        alert(`导出失败：${e}`);
      }
      setExportOpen(null);
    }
  };

  const automationCount = 18;
  const projectsHidden = collapsedSections.has("projects");
  const chatsHidden = collapsedSections.has("chats");

  return (
    <div className="project-tree">
      <div className="pt-rail">
        <button type="button" className="pt-rail-item" onClick={handleNewChat} title="新对话">
          <span className="pt-rail-icon">✏️</span>
          <span>新对话</span>
        </button>
        <button type="button" className="pt-rail-item" title="搜索">
          <span className="pt-rail-icon">🔍</span>
          <span>搜索</span>
        </button>
        <button type="button" className="pt-rail-item" title="插件">
          <span className="pt-rail-icon">🧩</span>
          <span>插件</span>
        </button>
        <button type="button" className="pt-rail-item" title="自动化">
          <span className="pt-rail-icon">⏱</span>
          <span>自动化</span>
          <span className="pt-rail-count">{automationCount}</span>
        </button>
      </div>

      <div className="pt-section-label">
        <button
          type="button"
          className="pt-section-toggle"
          onClick={() => toggleSection("projects")}
          aria-expanded={!projectsHidden}
        >
          <span className="pt-section-chevron">{projectsHidden ? "▸" : "▾"}</span>
          <span>项目</span>
        </button>
      </div>

      {!projectsHidden && (
        <div className="pt-list">
          {workspaces.map((w) => {
            const sessions = sessionsByWs.get(w.id) ?? [];
            const isExpanded = expanded.has(w.id);
            const isActiveWs = w.id === currentWorkspace;

            return (
              <div
                key={w.id}
                className={`pt-project ${isActiveWs ? "active-ws" : ""}`}
              >
                <div
                  className="pt-project-row"
                  onClick={() => selectWorkspace(w.id)}
                  title={w.folderPath || w.name}
                >
                  <span
                    className="pt-project-folder"
                    style={{ background: w.color || "#8b5cf6" }}
                    aria-hidden="true"
                  >
                    <svg viewBox="0 0 16 16" width="12" height="12">
                      <path
                        d="M1.5 3.5A1.5 1.5 0 0 1 3 2h3.5l1.5 1.5H13A1.5 1.5 0 0 1 14.5 5v7A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12V3.5z"
                        fill="white"
                        fillOpacity="0.92"
                      />
                    </svg>
                  </span>
                  <span className="pt-project-name">{w.name}</span>
                  {sessions.length > 0 && (
                    <span className="pt-project-count">{sessions.length}</span>
                  )}
                </div>

                {isExpanded && sessions.length > 0 && (
                  <div className="pt-sessions">
                    {sessions.map((s) => (
                      <div
                        key={s.id}
                        className={`pt-session-row ${s.id === currentId && isActiveWs ? "active" : ""}`}
                        onClick={() => selectSession(w.id, s.id)}
                        title={s.title}
                      >
                        <span className="pt-session-title">{s.title}</span>
                        <span className="pt-session-time">
                          {formatRelativeTime(s.updatedAt)}
                        </span>
                        <div
                          className="pt-session-actions"
                          onClick={(e) => e.stopPropagation()}
                        >
                          {encryptedSet.has(s.id) ? (
                            <button
                              type="button"
                              className="pt-session-action"
                              title="已加密 — 点击解锁"
                              onClick={() =>
                                onVaultPrompt({ sessionId: s.id, mode: "decrypt" })
                              }
                            >
                              🔒
                            </button>
                          ) : (
                            <button
                              type="button"
                              className="pt-session-action"
                              title="加密会话"
                              onClick={() =>
                                onVaultPrompt({ sessionId: s.id, mode: "encrypt" })
                              }
                            >
                              🔓
                            </button>
                          )}
                          <button
                            type="button"
                            className="pt-session-action"
                            title="导出"
                            onClick={() =>
                              setExportOpen(exportOpen === s.id ? null : s.id)
                            }
                          >
                            ⬇
                          </button>
                          <button
                            type="button"
                            className="pt-session-action danger"
                            title="删除"
                            onClick={() => handleDelete(s.id, s.title)}
                          >
                            ×
                          </button>
                        </div>
                        {exportOpen === s.id && (
                          <div className="pt-export-menu">
                            <label className="export-redact">
                              <input
                                type="checkbox"
                                checked={redactOnExport}
                                onChange={(e) => setRedactOnExport(e.target.checked)}
                              />
                              脱敏敏感数据
                            </label>
                            <button type="button" onClick={() => void doExport(s, "markdown")}>
                              📝 Markdown
                            </button>
                            <button type="button" onClick={() => void doExport(s, "html")}>
                              🌐 HTML
                            </button>
                            <button type="button" onClick={() => void doExport(s, "json")}>
                              📦 JSON
                            </button>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}

          <button type="button" className="pt-new-project" onClick={onNewProject}>
            <span className="pt-new-project-icon">＋</span>
            <span>新建项目</span>
          </button>
        </div>
      )}

      <div className="pt-section-label">
        <button
          type="button"
          className="pt-section-toggle"
          onClick={() => toggleSection("chats")}
          aria-expanded={!chatsHidden}
        >
          <span className="pt-section-chevron">{chatsHidden ? "▸" : "▾"}</span>
          <span>对话</span>
        </button>
      </div>

      {!chatsHidden && (
        <div className="pt-list">
          {recentSessions.length === 0 ? (
            <div className="pt-empty-hint">暂无对话</div>
          ) : (
            recentSessions.map((s) => {
              const wsId = s.workspaceId ?? "default";
              return (
                <div
                  key={s.id}
                  className={`pt-session-row flat ${s.id === currentId ? "active" : ""}`}
                  onClick={() => selectSession(wsId, s.id)}
                  title={s.title}
                >
                  <span className="pt-session-title">{s.title}</span>
                  <span className="pt-session-time">
                    {formatRelativeTime(s.updatedAt)}
                  </span>
                </div>
              );
            })
          )}
        </div>
      )}

      <div className="pt-bottom-spacer" />
    </div>
  );
}
