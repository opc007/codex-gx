import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ThemeMode } from "../stores/theme";
import { useLocaleSwitcher, SUPPORTED_LOCALES, LOCALE_LABELS } from "../i18n";
import type { Locale } from "../i18n";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { MarketplaceDialog } from "./MarketplaceDialog";

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
        <span className="topbar-version">v0.2.0-alpha</span>
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
    </header>
  );
}