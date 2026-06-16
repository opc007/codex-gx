import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { Thread } from "./components/Thread";
import { Composer } from "./components/Composer";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { useThemeStore, type ThemeMode } from "./stores/theme";
import { useSessionsStore } from "./stores/sessions";

export default function App() {
  const [themeMode, setThemeMode] = useThemeStore((s) => [s.mode, s.setMode]);
  const [sessions, currentId] = useSessionsStore((s) => [
    s.sessions,
    s.currentId,
  ]);

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

  const current = sessions.find((s) => s.id === currentId);

  return (
    <div className="app-shell">
      <TopBar themeMode={themeMode} setThemeMode={setThemeMode} />
      <div className="app-body">
        <Sidebar />
        <main className="main-pane">
          <Thread session={current} />
          <Composer sessionId={currentId} />
        </main>
      </div>
      <StatusBar session={current} />
    </div>
  );
}

// 兼容 ThemeMode 类型导出
export type { ThemeMode };