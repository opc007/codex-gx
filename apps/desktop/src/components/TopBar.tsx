import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ThemeMode } from "../stores/theme";
import { useLocaleSwitcher, SUPPORTED_LOCALES, LOCALE_LABELS } from "../i18n";
import type { Locale } from "../i18n";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { MarketplaceDialog } from "./MarketplaceDialog";
import { ThemeStudioDialog } from "./ThemeStudioDialog";
import {
  useCurrentWorkspaceId,
  useWorkspaceList,
  createWorkspace,
  switchWorkspace,
  deleteWorkspace,
  renameWorkspace,
} from "../stores/workspace";

type UpdateInfo = {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  releaseUrl: string | null;
  releaseNotes: string | null;
};

type LicenseStatus = {
  active: boolean;
  tier: string;
  tierDisplay: string;
  remainingDays: number | null;
};

type Props = {
  themeMode: ThemeMode;
  setThemeMode: (m: ThemeMode) => void;
  onLicenseClick: () => void;
};

export function TopBar({ themeMode, setThemeMode, onLicenseClick }: Props) {
  const [busy, setBusy] = useState(false);
  const [license, setLicense] = useState<LicenseStatus | null>(null);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [marketplaceOpen, setMarketplaceOpen] = useState(false);
  const [themeStudioOpen, setThemeStudioOpen] = useState(false);
  const { locale, setLocale } = useLocaleSwitcher();

  const refreshLicense = async () => {
    try {
      const s = await invoke<LicenseStatus>("get_license_status");
      setLicense(s);
    } catch {
      setLicense(null);
    }
  };

  useEffect(() => {
    void refreshLicense();
    // 监听 license 变更
    const unlistenP = listen("license:changed", () => void refreshLicense());
    return () => {
      void unlistenP.then((u) => u());
    };
  }, []);

  const cycleTheme = () => {
    const next: ThemeMode =
      themeMode === "light" ? "dark" : themeMode === "dark" ? "system" : "light";
    setThemeMode(next);
  };

  const pingBackend = async () => {
    setBusy(true);
    try {
      const v = await invoke<string>("ping");
      alert(`Rust 后端回应：${v}`);
    } catch (e) {
      alert(`错误: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <header className="topbar">
      <div className="topbar-left">
        <strong>AgentShell</strong>
        <span className="topbar-version">v1.2.1</span>
        <WorkspaceSelector />
      </div>
      <div className="topbar-right">
        <button
          className="topbar-btn"
          onClick={async () => {
            setUpdateBusy(true);
            try {
              const info = await invoke<UpdateInfo>("check_update");
              setUpdateInfo(info);
            } catch (e) {
              alert(`检查更新失败：${e}`);
            } finally {
              setUpdateBusy(false);
            }
          }}
          disabled={updateBusy}
          title="检查更新"
        >
          {updateBusy ? "..." : updateInfo?.updateAvailable ? "🆕" : "🔄"}
        </button>
        <button
          className="topbar-btn"
          onClick={onLicenseClick}
          title="License"
        >
          🔑 {license?.active ? license.tierDisplay : "未激活"}
        </button>
        <button
          className="topbar-btn"
          onClick={() => setMarketplaceOpen(true)}
          title="Plugin marketplace (v1.2)"
        >
          🧩
        </button>
        <button
          className="topbar-btn"
          onClick={() => setThemeStudioOpen(true)}
          title="主题市场 (v1.3)"
        >
          🎨
        </button>
        <button className="topbar-btn" onClick={pingBackend} disabled={busy}>
          {busy ? "..." : "Ping"}
        </button>
        <button className="topbar-btn" onClick={cycleTheme}>
          {themeMode === "light" ? "☀️" : themeMode === "dark" ? "🌙" : "🖥️"}
          <span style={{ marginLeft: 6 }}>
            {themeMode === "system" ? "跟随" : themeMode === "light" ? "白天" : "夜晚"}
          </span>
        </button>
        <select
          className="topbar-select"
          value={locale}
          onChange={(e) => setLocale(e.target.value as Locale)}
          title="Language"
        >
          {SUPPORTED_LOCALES.map((l) => (
            <option key={l} value={l}>{LOCALE_LABELS[l]}</option>
          ))}
        </select>
      </div>
      {updateInfo && (
        <div className="update-dialog-overlay" onClick={() => setUpdateInfo(null)}>
          <div className="update-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="update-dialog-header">
              <h3>{updateInfo.updateAvailable ? "🆕 有新版本可用" : "✓ 已是最新"}</h3>
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

      {/* v1.2: Plugin marketplace 弹窗 */}
      {marketplaceOpen && (
        <MarketplaceDialog onClose={() => setMarketplaceOpen(false)} />
      )}

      {/* v1.3: Theme Studio 弹窗 */}
      {themeStudioOpen && (
        <ThemeStudioDialog onClose={() => setThemeStudioOpen(false)} />
      )}
    </header>
  );
}

// ------------------ v1.3：Workspace selector ------------------

function WorkspaceSelector() {
  const currentId = useCurrentWorkspaceId();
  const list = useWorkspaceList();
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);

  const handleCreate = () => {
    const name = prompt("新建工作区名称：", `Workspace ${list.length + 1}`);
    if (name === null) return;
    const meta = createWorkspace(name);
    // 触发 session 列表刷新：广播事件
    void invoke("workspace_changed_broadcast", { workspaceId: meta.id });
  };

  const handleDelete = (id: string, name: string) => {
    if (id === "default") {
      alert("默认工作区不能删除");
      return;
    }
    if (!confirm(`确认删除工作区「${name}」？\n（其中的 session 不会被删除，可在切换回 default 时看到）`)) return;
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
          void invoke("workspace_changed_broadcast", {
            workspaceId: e.target.value,
          });
        }}
        title="当前工作区"
      >
        {list.map((w) => (
          <option key={w.id} value={w.id}>
            📁 {w.name}
          </option>
        ))}
      </select>
      <button
        className="topbar-btn small"
        onClick={handleCreate}
        title="新建工作区"
      >
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
              <button className="btn" onClick={() => setEditing(null)}>
                取消
              </button>
              <button className="btn primary" onClick={commitRename}>
                保存
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}