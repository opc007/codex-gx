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
  type WorkspaceMeta,
} from "../stores/workspace";
import { closeTab, openTab } from "../stores/tabs";
import { formatRelativeTime } from "../lib/formatRelativeTime";
import { exportSession, type ExportFormat } from "../lib/export";
import { ProjectContextMenu } from "./ProjectContextMenu";

type Props = {
  encryptedSet: Set<string>;
  onVaultPrompt: (p: { sessionId: string; mode: "encrypt" | "decrypt" }) => void;
  /** v1.9.14：重命名项目（由父组件弹 WorkspaceDialog edit 模式） */
  onRenameProject: (ws: WorkspaceMeta) => void;
};

export function ProjectTree({
  encryptedSet,
  onVaultPrompt,
  onRenameProject,
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
  // v1.9.14：项目右键菜单
  const [contextMenu, setContextMenu] = useState<{
    ws: WorkspaceMeta;
    x: number;
    y: number;
  } | null>(null);
  // v1.9.13：section 不可折叠，移除 collapsedSections

  const sessionsByWs = useMemo(() => {
    const map = new Map<string, SessionMeta[]>();
    for (const w of workspaces) map.set(w.id, []);
    for (const s of allSessions) {
      if (s.archived) continue; // v1.9.14：归档 session 不再属于 sidebar 任何项目
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
      .filter((s) => !s.archived) // v1.9.14：归档 session 不进最近对话
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .slice(0, 20);
  }, [allSessions]);

  // v1.9.14：项目排序 — 置顶在前，其余按 createdAt
  const sortedWorkspaces = useMemo(() => {
    return [...workspaces].sort((a, b) => {
      if ((b.pinned ? 1 : 0) !== (a.pinned ? 1 : 0)) {
        return (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0);
      }
      return a.createdAt - b.createdAt;
    });
  }, [workspaces]);

  useEffect(() => {
    setExpanded((prev) => {
      if (prev.has(currentWorkspace)) return prev;
      const next = new Set(prev);
      next.add(currentWorkspace);
      return next;
    });
  }, [currentWorkspace]);

  // v1.9.13：toggleSection 已移除，section 不可折叠

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

  return (
    <div className="project-tree">
      {/* v1.9.14：去掉 4 个 rail item 中无功能的 3 个（搜索/插件/自动化）和硬编码的 18 计数。
          只保留「快速对话」入口——新建项目入口收在 Composer 下方 ProjectPicker。 */}
      <div className="pt-rail">
        <button type="button" className="pt-rail-item" onClick={handleNewChat} title="快速对话 (Cmd+N)">
          <span className="pt-rail-icon">✏️</span>
          <span>快速对话</span>
        </button>
      </div>

      {/* v1.9.13：Codex 真实 — section header 不可折叠，直接展开 */}
      <div className="pt-section-label">
        <span>项目</span>
      </div>

      <div className="pt-list">
          {sortedWorkspaces.map((w) => {
            const sessions = sessionsByWs.get(w.id) ?? [];
            const isExpanded = expanded.has(w.id);
            const isActiveWs = w.id === currentWorkspace;

            return (
              <div
                key={w.id}
                className={`pt-project ${isActiveWs ? "active-ws" : ""} ${w.pinned ? "pinned" : ""}`}
              >
                <div
                  className="pt-project-row"
                  onClick={() => selectWorkspace(w.id)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    setContextMenu({ ws: w, x: e.clientX, y: e.clientY });
                  }}
                  title={w.folderPath || w.name}
                >
                  <span
                    className="pt-project-folder"
                    style={{ background: w.color || "#8b5cf6" }}
                    aria-hidden="true"
                  >
                    {/* v1.9.13：Codex 真实文件柜图标（4 行 3 列抽屉） */}
                    <svg viewBox="0 0 16 16" width="14" height="14">
                      <rect x="2" y="2" width="12" height="12" rx="1.2" fill="white" fillOpacity="0.95" />
                      <line x1="2" y1="5.5" x2="14" y2="5.5" stroke="currentColor" strokeOpacity="0.45" strokeWidth="0.6" />
                      <line x1="2" y1="8.5" x2="14" y2="8.5" stroke="currentColor" strokeOpacity="0.45" strokeWidth="0.6" />
                      <line x1="2" y1="11.5" x2="14" y2="11.5" stroke="currentColor" strokeOpacity="0.45" strokeWidth="0.6" />
                      <circle cx="11.5" cy="3.7" r="0.55" fill="currentColor" fillOpacity="0.6" />
                      <circle cx="11.5" cy="6.7" r="0.55" fill="currentColor" fillOpacity="0.6" />
                      <circle cx="11.5" cy="9.7" r="0.55" fill="currentColor" fillOpacity="0.6" />
                    </svg>
                  </span>
                  <span className="pt-project-name">{w.name}</span>
                  {w.pinned && <span className="pt-project-pin" title="已置顶">📌</span>}
                  {sessions.length > 0 && (
                    <span className="pt-project-count">{sessions.length}</span>
                  )}
                  {/* v1.9.14：Codex 真实三点菜单按钮 */}
                  <button
                    type="button"
                    className="pt-project-menu-btn"
                    title="项目操作"
                    onClick={(e) => {
                      e.stopPropagation();
                      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                      setContextMenu({ ws: w, x: r.right - 220, y: r.bottom + 4 });
                    }}
                  >
                    ⋯
                  </button>
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

          {/* v1.9.14：删除「+ 新建项目」按钮 — 用户已经在 Composer 下方 ProjectPicker 弹窗里
              拿到「添加新项目」入口，两处重复。Codex 实际也只有一处入口。 */}
      </div>

      <div className="pt-section-label">
        <span>对话</span>
      </div>

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

      <div className="pt-bottom-spacer" />

      {/* v1.9.14：项目右键菜单 — 仿 Codex Desktop 真实交互 */}
      {contextMenu && (
        <ProjectContextMenu
          workspace={contextMenu.ws}
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onRename={(ws) => onRenameProject(ws)}
        />
      )}
    </div>
  );
}
