import { useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
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

  // v1.9.13：thread 切换由 App.tsx 的全局快捷键 Cmd+Shift+{ / } 接管

  return (
    <header className="topbar">
      {/* v1.9.13：Codex 极简 — 顶栏只放 1 个返回 + 1 个 thread 标题。thread 切换用 Cmd+Shift+{ / } 快捷键（见 App.tsx） */}
      <div className="topbar-left">
        <button
          type="button"
          className="topbar-back-btn"
          onClick={() => {
            // 折叠侧栏（Codex 顶栏返回按钮效果）
            try {
              const next = localStorage.getItem("codex_gx_sidebar_collapsed") !== "1";
              localStorage.setItem("codex_gx_sidebar_collapsed", next ? "1" : "0");
              window.dispatchEvent(new CustomEvent("codex_gx:sidebar-toggle"));
            } catch {
              /* ignore */
            }
          }}
          title="折叠/展开侧栏 (Cmd+B)"
          aria-label="返回"
        >
          ‹
        </button>
      </div>
      <div className="topbar-center">
        <div className="topbar-thread-title" title={currentSession?.title || "Codex"}>
          {currentSession ? currentSession.title : "Codex"}
        </div>
      </div>
      <div className="topbar-right" />

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
