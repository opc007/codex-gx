import { useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { useSessionsStore } from "../stores/sessions";
import { useCurrentWorkspace } from "../stores/workspace";

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
  const currentWs = useCurrentWorkspace();

  return (
    <header className="topbar">
      <div className="topbar-left">
        <span className="topbar-logo" aria-hidden="true">✦</span>
        <span className="topbar-title">Codex gx</span>
        <span className="topbar-project-badge" title={currentWs.folderPath || currentWs.name}>
          {currentWs.name === "Default" ? "默认项目" : currentWs.name}
        </span>
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
