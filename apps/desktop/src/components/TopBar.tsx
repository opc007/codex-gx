import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ThemeMode } from "../stores/theme";

type Props = {
  themeMode: ThemeMode;
  setThemeMode: (m: ThemeMode) => void;
};

export function TopBar({ themeMode, setThemeMode }: Props) {
  const [busy, setBusy] = useState(false);

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
        <span className="topbar-version">v0.1.0-alpha</span>
      </div>
      <div className="topbar-right">
        <button className="topbar-btn" onClick={pingBackend} disabled={busy}>
          {busy ? "..." : "Ping"}
        </button>
        <button className="topbar-btn" onClick={cycleTheme}>
          {themeMode === "light" ? "☀️" : themeMode === "dark" ? "🌙" : "🖥️"}
          <span style={{ marginLeft: 6 }}>
            {themeMode === "system" ? "跟随" : themeMode === "light" ? "白天" : "夜晚"}
          </span>
        </button>
      </div>
    </header>
  );
}