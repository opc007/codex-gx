import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  useCurrentWorkspaceId,
  useWorkspaceList,
  createWorkspace,
  switchWorkspace,
  deleteWorkspace,
  renameWorkspace,
  type WorkspaceMeta,
} from "../stores/workspace";
import { useSessionsStore } from "../stores/sessions";

type UpdateInfo = {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  releaseUrl: string | null;
  releaseNotes: string | null;
};

export function TopBar() {
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const currentSession = useSessionsStore((s) =>
    s.sessions.find((x) => x.id === s.currentId)
  );

  // 设置入口（检查更新、API Key、License、主题…）全部从左下角用户头像进入
  // TopBar 仅展示 logo / 工作区 / 当前会话标题，避免右上角拥挤

  return (
    <header className="topbar">
      <div className="topbar-left">
        <span className="topbar-logo" aria-hidden="true">✦</span>
        <span className="topbar-title">Codex gx</span>
        <WorkspaceSelector />
      </div>
      <div className="topbar-center">
        {currentSession ? currentSession.title : "Codex"}
      </div>
      <div className="topbar-right">{/* Codex gx：设置入口收在左下角用户头像 */}</div>

      {updateInfo && (
        <div className="update-dialog-overlay" onClick={() => setUpdateInfo(null)}>
          <div className="update-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="update-dialog-header">
              <h3>
                {updateInfo.updateAvailable ? "🆕 有新版本可用" : "✓ 已是最新"}
              </h3>
              <button className="update-close" onClick={() => setUpdateInfo(null)}>×</button>
            </div>
            <div className="update-dialog-body">
              <p>当前版本: <code>{updateInfo.currentVersion}</code></p>
              {updateInfo.latestVersion && (
                <p>最新版本: <code>{updateInfo.latestVersion}</code></p>
              )}
              {updateInfo.releaseNotes && (
                <details>
                  <summary>更新说明</summary>
                  <pre>{updateInfo.releaseNotes.slice(0, 2000)}</pre>
                </details>
              )}
            </div>
            <div className="update-dialog-footer">
              <button className="update-cancel" onClick={() => setUpdateInfo(null)}>关闭</button>
              {updateInfo.updateAvailable && updateInfo.releaseUrl && (
                <button
                  className="update-go"
                  onClick={async () => {
                    if (updateInfo.releaseUrl) {
                      try {
                        await openUrl(updateInfo.releaseUrl);
                      } catch (e) {
                        alert(`打开失败：${e}`);
                      }
                    }
                  }}
                >
                  前往下载
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </header>
  );
}

type EditState = {
  id: string;
  name: string;
  folderPath: string;
  description: string;
  color: string;
};

type NewState = {
  name: string;
  folderPath: string;
  description: string;
  color: string;
};

const COLORS = ["#2ea043", "#3b82f6", "#a855f7", "#f59e0b", "#ec4899", "#06b6d4"];

function WorkspaceSelector() {
  const currentId = useCurrentWorkspaceId();
  const list = useWorkspaceList();
  const [editing, setEditing] = useState<EditState | null>(null);
  const [creating, setCreating] = useState<NewState | null>(null);

  const current = list.find((w) => w.id === currentId) ?? list[0];

  const handleCreate = () => {
    setCreating({ name: "", folderPath: "", description: "", color: COLORS[list.length % COLORS.length] });
  };

  const commitCreate = () => {
    if (!creating) return;
    const meta = createWorkspace(creating.name || `项目组 ${list.length + 1}`, {
      folderPath: creating.folderPath || undefined,
      description: creating.description || undefined,
      color: creating.color || undefined,
    });
    void invoke("workspace_changed_broadcast", { workspaceId: meta.id });
    setCreating(null);
  };

  const handleDelete = (id: string, name: string) => {
    if (id === "default") {
      alert("默认工作区不能删除");
      return;
    }
    if (
      !confirm(
        `确认删除项目组「${name}」？\n（其中的 session 不会被删除，可在切换回 default 时看到）`,
      )
    ) {
      return;
    }
    deleteWorkspace(id);
    void invoke("workspace_changed_broadcast", { workspaceId: "default" });
  };

  const handleEdit = (w: WorkspaceMeta) => {
    setEditing({
      id: w.id,
      name: w.name,
      folderPath: w.folderPath ?? "",
      description: w.description ?? "",
      color: w.color ?? COLORS[0],
    });
  };

  const commitEdit = () => {
    if (!editing) return;
    renameWorkspace(editing.id, editing.name, {
      folderPath: editing.folderPath || undefined,
      description: editing.description || undefined,
      color: editing.color || undefined,
    });
    setEditing(null);
  };

  const pickFolder = async (set: (p: string) => void, currentPath: string) => {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "选择项目文件夹",
        defaultPath: currentPath || undefined,
      });
      if (typeof selected === "string") {
        set(selected);
      }
    } catch {
      // ignore
    }
  };

  return (
    <div className="workspace-selector">
      <select
        className="topbar-select workspace-select"
        value={currentId}
        onChange={(e) => {
          switchWorkspace(e.target.value);
          void invoke("workspace_changed_broadcast", { workspaceId: e.target.value });
        }}
        title={current?.folderPath ? `📁 ${current.folderPath}` : "当前项目组"}
      >
        {list.map((w) => (
          <option key={w.id} value={w.id}>
            {w.folderPath ? "📂" : "📁"} {w.name}
          </option>
        ))}
      </select>
      <button className="topbar-btn small" onClick={handleCreate} title="新建项目组">
        ＋
      </button>
      {current && current.id !== "default" && (
        <>
          <button
            className="topbar-btn small"
            onClick={() => handleEdit(current)}
            title="编辑项目组"
          >
            ✎
          </button>
          <button
            className="topbar-btn small danger"
            onClick={() => handleDelete(current.id, current.name)}
            title="删除项目组"
          >
            ×
          </button>
        </>
      )}

      {current?.folderPath && (
        <span className="workspace-folder-hint" title={current.folderPath}>
          {current.folderPath.split("/").pop() || current.folderPath}
        </span>
      )}

      {creating && (
        <WorkspaceDialog
          title="新建项目组"
          state={creating}
          setState={setCreating}
          onClose={() => setCreating(null)}
          onCommit={commitCreate}
          onPickFolder={() =>
            void pickFolder(
              (v) => setCreating({ ...creating, folderPath: v }),
              creating.folderPath,
            )
          }
        />
      )}

      {editing && (
        <WorkspaceDialog
          title="编辑项目组"
          state={editing}
          setState={setEditing}
          onClose={() => setEditing(null)}
          onCommit={commitEdit}
          onPickFolder={() =>
            void pickFolder(
              (v) => setEditing({ ...editing, folderPath: v }),
              editing.folderPath,
            )
          }
        />
      )}
    </div>
  );
}

function WorkspaceDialog({
  title,
  state,
  setState,
  onClose,
  onCommit,
  onPickFolder,
}: {
  title: string;
  state: NewState | EditState;
  setState: (s: any) => void;
  onClose: () => void;
  onCommit: () => void;
  onPickFolder: () => void;
}) {
  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal-dialog workspace-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>{title}</h3>

        <label className="ws-field-label">项目组名称</label>
        <input
          className="vault-password-input"
          placeholder="例如：M3 桌面版 / 个人博客"
          value={state.name}
          onChange={(e) => setState({ ...state, name: e.target.value })}
          autoFocus
        />

        <label className="ws-field-label">绑定文件夹（可选）</label>
        <div className="ws-folder-row">
          <input
            className="vault-password-input"
            placeholder="选择一个本地项目根目录"
            value={state.folderPath}
            onChange={(e) => setState({ ...state, folderPath: e.target.value })}
          />
          <button className="btn" onClick={onPickFolder} type="button">
            📁 选择
          </button>
        </div>
        <p className="ws-hint">
          绑定后，M3 会自动获取文件夹根路径与 README/AGENTS.md 摘要作为上下文。
        </p>

        <label className="ws-field-label">项目简介（可选）</label>
        <textarea
          className="vault-password-input ws-description"
          placeholder="简短描述项目目标，让 M3 更快进入状态"
          value={state.description}
          onChange={(e) => setState({ ...state, description: e.target.value })}
          rows={3}
        />

        <label className="ws-field-label">颜色</label>
        <div className="ws-color-row">
          {COLORS.map((c) => (
            <button
              key={c}
              type="button"
              className={`ws-color-dot ${state.color === c ? "active" : ""}`}
              style={{ background: c }}
              onClick={() => setState({ ...state, color: c })}
            />
          ))}
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={onClose}>取消</button>
          <button className="btn primary" onClick={onCommit}>保存</button>
        </div>
      </div>
    </div>
  );
}
