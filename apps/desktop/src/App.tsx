import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { Thread } from "./components/Thread";
import { Composer } from "./components/Composer";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { LicenseDialog } from "./components/LicenseDialog";
import { useThemeStore, type ThemeMode } from "./stores/theme";
import { useSessionsStore } from "./stores/sessions";

export default function App() {
  const [themeMode, setThemeMode] = useThemeStore((s) => [s.mode, s.setMode]);
  const [currentId] = useSessionsStore((s) => [s.currentId]);
  const [showLicense, setShowLicense] = useState(false);

  // 跟随系统
  useEffect(() => {
    if (themeMode !== "system") return;
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = mql.matches ? "dark" : "light";
    };
    apply();
    mql.addEventListener("change", apply);
    return () => mql.removeEventListener("change", apply);
  }, [themeMode]);

  // 显式模式
  useEffect(() => {
    if (themeMode === "system") return;
    document.documentElement.dataset.theme = themeMode;
  }, [themeMode]);

  // 监听 /theme slash 命令
  useEffect(() => {
    const handler = (e: Event) => {
      const mode = (e as CustomEvent).detail as string;
      if (["light", "dark", "system"].includes(mode)) {
        setThemeMode(mode as ThemeMode);
      }
    };
    window.addEventListener("agentshell:theme", handler);
    return () => window.removeEventListener("agentshell:theme", handler);
  }, [setThemeMode]);

  return (
    <div className="app-shell">
      <TopBar
        themeMode={themeMode}
        setThemeMode={setThemeMode}
        onLicenseClick={() => setShowLicense(true)}
      />
      <div className="app-body">
        <Sidebar />
        <main className="main-pane">
          <Thread sessionId={currentId} />
          <Composer sessionId={currentId} />
        </main>
      </div>
      <StatusBar sessionId={currentId} />
      {showLicense && (
        <LicenseDialog
          onClose={() => setShowLicense(false)}
          onChange={() => {
            // StatusBar / TopBar 会通过 listen("license:changed") 自动刷新
          }}
        />
      )}
    </div>
  );
}

// 兼容 ThemeMode 类型导出
export type { ThemeMode };