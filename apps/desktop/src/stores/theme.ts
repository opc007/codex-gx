// 极简 store（无依赖，基于 useSyncExternalStore）
import { useSyncExternalStore } from "react";

export type ThemeMode = "light" | "dark" | "system";

export type ThemeStore = {
  mode: ThemeMode;
  setMode: (m: ThemeMode) => void;
};

const initial: ThemeMode =
  (typeof localStorage !== "undefined"
    ? (localStorage.getItem("agentshell.theme") as ThemeMode)
    : null) || "system";

let themeState: ThemeStore = {
  mode: initial,
  setMode: (m: ThemeMode) => {
    themeState = { ...themeState, mode: m };
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("agentshell.theme", m);
    }
    listeners.forEach((l) => l());
  },
};

const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): ThemeStore {
  return themeState;
}

export function useThemeStore<T>(selector: (s: ThemeStore) => T): T {
  return useSyncExternalStore(subscribe, () => selector(getSnapshot()));
}

// 便捷 hooks
export function useThemeMode(): [ThemeMode, (m: ThemeMode) => void] {
  const mode = useThemeStore((s) => s.mode);
  return [mode, themeState.setMode];
}