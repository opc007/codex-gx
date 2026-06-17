import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import {
  useCurrentWorkspaceId,
  useWorkspaceList,
  createWorkspace,
  switchWorkspace,
  deleteWorkspace,
  renameWorkspace,
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
  const [updateBusy, setUpdateBusy] = useState(false);
  const currentSession = useSessionsStore((s) =>
    s.sessions.find((x) => x.id === s.currentId)
  );

  const checkUpdate = async () => {
    setUpdateBusy(true);
    try {
      const info = await invoke<UpdateInfo>("check_update");
      setUpdateInfo(info);
    } catch (e) {
      alert(`检查更新失败：${e}`);
    } finally {
      setUpdateBusy(false);
    }
  };

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
      <div className="topbar-right">
        <button
          className="topbar-update-pill"
          onClick={() => void checkUpdate()}
          disabled={updateBusy}
          title="检查更新"
          aria-label="检查更新"
        >
          <span aria-hidden="true">🆕</span>
          <span>{updateBusy ? "检查中" : "更新"}</span>
        </button>
      </div>

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

function WorkspaceSelector() {
  const currentId = useCurrentWorkspaceId();
  const list = useWorkspaceList();
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);

  const handleCreate = () => {
    const name = prompt("新建工作区名称：", `Workspace ${list.length + 1}`);
    if (name === null) return;
    const meta = createWorkspace(name);
    void invoke("workspace_changed_broadcast", { workspaceId: meta.id });
  };

  const handleDelete = (id: string, name: string) => {
    if (id === "default") {
      alert("默认工作区不能删除");
      return;
    }
    if (
      !confirm(
        `确认删除工作区「${name}」？\n（其中的 session 不会被删除，可在切换回 default 时看到）`,
      )
    ) {
      return;
    }
    deleteWorkspace(id);
    void invoke("workspace_changed_broadcast", { workspaceId: "default" });
  };

  const handleRename = (id: string, name: string) => {
    setEditing({ id, name });
  };

  const commitRename = () => {
    if (editing) {
      renameWorkspace(editing.id, editing.name);
      setEditing(null);
    }
  };

  const current = list.find((w) => w.id === currentId) ?? list[0];

  return (
    <div className="workspace-selector">
      <select
        className="topbar-select workspace-select"
        value={currentId}
        onChange={(e) => {
          switchWorkspace(e.target.value);
          void invoke("workspace_changed_broadcast", { workspaceId: e.target.value });
        }}
        title="当前工作区"
      >
        {list.map((w) => (
          <option key={w.id} value={w.id}>
            📁 {w.name}
          </option>
        ))}
      </select>
      <button className="topbar-btn small" onClick={handleCreate} title="新建工作区">
        ＋
      </button>
      {current && current.id !== "default" && (
        <>
          <button
            className="topbar-btn small"
            onClick={() => handleRename(current.id, current.name)}
            title="重命名"
          >
            ✎
          </button>
          <button
            className="topbar-btn small danger"
            onClick={() => handleDelete(current.id, current.name)}
            title="删除工作区"
          >
            ×
          </button>
        </>
      )}

      {editing && (
        <div className="modal-mask" onClick={() => setEditing(null)}>
          <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>重命名工作区</h3>
            <input
              className="vault-password-input"
              value={editing.name}
              onChange={(e) => setEditing({ ...editing, name: e.target.value })}
              autoFocus
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename();
                if (e.key === "Escape") setEditing(null);
              }}
            />
            <div className="modal-actions">
              <button className="btn" onClick={() => setEditing(null)}>取消</button>
              <button className="btn primary" onClick={commitRename}>保存</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
