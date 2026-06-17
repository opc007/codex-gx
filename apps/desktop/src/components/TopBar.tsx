import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ThemeMode } from "../stores/theme";
import { useLocaleSwitcher, SUPPORTED_LOCALES, LOCALE_LABELS } from "../i18n";
import type { Locale } from "../i18n";

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
          onClick={onLicenseClick}
          title="License"
        >
          🔑 {license?.active ? license.tierDisplay : "未激活"}
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
    </header>
  );
}