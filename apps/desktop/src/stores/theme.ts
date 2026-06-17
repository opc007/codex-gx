// 主题 store（含 v1.3：自定义主题 + 主题市场）
//
// - mode: light / dark / system（沿用旧 API）
// - customTheme: 当前激活的"用户自定义主题"（覆盖默认颜色变量）
//   - 内置预设：Default / Solarized Dark / Nord / Dracula / Gruvbox / Monokai
//   - 用户可在 ThemeStudio 中编辑
//   - 持久化到 localStorage

import { useSyncExternalStore } from "react";

export type ThemeMode = "light" | "dark" | "system";

export type ThemeColors = {
  /// 背景
  bg: string;
  bgSecondary: string;
  bgHover: string;
  /// 文字
  text: string;
  textMuted: string;
  /// 边框
  border: string;
  /// 主色 / 强调
  primary: string;
  primaryText: string;
  /// 危险 / 警告 / 成功
  danger: string;
  warning: string;
  success: string;
  /// 代码
  codeBg: string;
};

export type CustomTheme = {
  id: string;
  name: string;
  base: ThemeMode;
  colors: ThemeColors;
};

export const DEFAULT_DARK_COLORS: ThemeColors = {
  bg: "#1e1e1e",
  bgSecondary: "#252526",
  bgHover: "#2d2d2e",
  text: "#e6e6e6",
  textMuted: "#9b9b9b",
  border: "#3c3c3c",
  primary: "#0a84ff",
  primaryText: "#ffffff",
  danger: "#ff453a",
  warning: "#ffb000",
  success: "#32d74b",
  codeBg: "#1a1a1a",
};

export const DEFAULT_LIGHT_COLORS: ThemeColors = {
  bg: "#ffffff",
  bgSecondary: "#f7f7f8",
  bgHover: "#ececef",
  text: "#1c1c1e",
  textMuted: "#6b6b70",
  border: "#d6d6d8",
  primary: "#0a84ff",
  primaryText: "#ffffff",
  danger: "#d70015",
  warning: "#b25000",
  success: "#0c8500",
  codeBg: "#f4f4f6",
};

export const BUILTIN_THEMES: CustomTheme[] = [
  {
    id: "default-dark",
    name: "Default Dark",
    base: "dark",
    colors: DEFAULT_DARK_COLORS,
  },
  {
    id: "default-light",
    name: "Default Light",
    base: "light",
    colors: DEFAULT_LIGHT_COLORS,
  },
  {
    id: "solarized-dark",
    name: "Solarized Dark",
    base: "dark",
    colors: {
      bg: "#002b36",
      bgSecondary: "#073642",
      bgHover: "#0a4451",
      text: "#93a1a1",
      textMuted: "#657b83",
      border: "#0a4451",
      primary: "#268bd2",
      primaryText: "#fdf6e3",
      danger: "#dc322f",
      warning: "#b58900",
      success: "#859900",
      codeBg: "#001f27",
    },
  },
  {
    id: "nord",
    name: "Nord",
    base: "dark",
    colors: {
      bg: "#2e3440",
      bgSecondary: "#3b4252",
      bgHover: "#434c5e",
      text: "#eceff4",
      textMuted: "#d8dee9",
      border: "#4c566a",
      primary: "#88c0d0",
      primaryText: "#2e3440",
      danger: "#bf616a",
      warning: "#ebcb8b",
      success: "#a3be8c",
      codeBg: "#292e39",
    },
  },
  {
    id: "dracula",
    name: "Dracula",
    base: "dark",
    colors: {
      bg: "#282a36",
      bgSecondary: "#44475a",
      bgHover: "#4a4d61",
      text: "#f8f8f2",
      textMuted: "#6272a4",
      border: "#44475a",
      primary: "#bd93f9",
      primaryText: "#282a36",
      danger: "#ff5555",
      warning: "#f1fa8c",
      success: "#50fa7b",
      codeBg: "#21222c",
    },
  },
  {
    id: "gruvbox",
    name: "Gruvbox Dark",
    base: "dark",
    colors: {
      bg: "#282828",
      bgSecondary: "#3c3836",
      bgHover: "#504945",
      text: "#ebdbb2",
      textMuted: "#a89984",
      border: "#504945",
      primary: "#83a598",
      primaryText: "#282828",
      danger: "#fb4934",
      warning: "#fabd2f",
      success: "#b8bb26",
      codeBg: "#1d2021",
    },
  },
  {
    id: "monokai",
    name: "Monokai",
    base: "dark",
    colors: {
      bg: "#272822",
      bgSecondary: "#3e3d32",
      bgHover: "#49483e",
      text: "#f8f8f2",
      textMuted: "#75715e",
      border: "#3e3d32",
      primary: "#66d9ef",
      primaryText: "#272822",
      danger: "#f92672",
      warning: "#e6db74",
      success: "#a6e22e",
      codeBg: "#1e1f1c",
    },
  },
];

const STORAGE_MODE = "agentshell.theme";
const STORAGE_THEME_ID = "agentshell.theme.id";

type ThemeStore = {
  mode: ThemeMode;
  setMode: (m: ThemeMode) => void;
  /// v1.3：当前激活的 custom theme id（可来自 BUILTIN_THEMES 或用户自己保存的）
  activeThemeId: string;
  setActiveThemeId: (id: string) => void;
  /// v1.3：所有可用主题
  themes: CustomTheme[];
  /// v1.3：用户保存的自定义主题
  saveCustomTheme: (theme: CustomTheme) => void;
  deleteCustomTheme: (id: string) => void;
};

function loadInitial(): { mode: ThemeMode; themeId: string } {
  let mode: ThemeMode = "system";
  let themeId = "default-dark";
  try {
    const m = localStorage.getItem(STORAGE_MODE) as ThemeMode | null;
    if (m === "light" || m === "dark" || m === "system") mode = m;
    const t = localStorage.getItem(STORAGE_THEME_ID);
    if (t) themeId = t;
  } catch {
    // ignore
  }
  return { mode, themeId };
}

let loaded = loadInitial();
let state: ThemeStore = {
  mode: loaded.mode,
  setMode: (m) => {
    state = { ...state, mode: m };
    try {
      localStorage.setItem(STORAGE_MODE, m);
    } catch {
      // ignore
    }
    listeners.forEach((l) => l());
    applyToDom(state);
  },
  activeThemeId: loaded.themeId,
  setActiveThemeId: (id) => {
    state = { ...state, activeThemeId: id };
    try {
      localStorage.setItem(STORAGE_THEME_ID, id);
    } catch {
      // ignore
    }
    listeners.forEach((l) => l());
    applyToDom(state);
  },
  themes: BUILTIN_THEMES,
  saveCustomTheme: (theme) => {
    const existing = state.themes.findIndex((t) => t.id === theme.id);
    let themes: CustomTheme[];
    if (existing >= 0) {
      themes = state.themes.map((t, i) => (i === existing ? theme : t));
    } else {
      themes = [...state.themes, theme];
    }
    state = { ...state, themes };
    listeners.forEach((l) => l());
  },
  deleteCustomTheme: (id) => {
    state = { ...state, themes: state.themes.filter((t) => t.id !== id) };
    if (state.activeThemeId === id) {
      state = { ...state, activeThemeId: "default-dark" };
      try {
        localStorage.setItem(STORAGE_THEME_ID, "default-dark");
      } catch {
        // ignore
      }
    }
    listeners.forEach((l) => l());
  },
};

const listeners = new Set<() => void>();

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function getSnapshot(): ThemeStore {
  return state;
}

export function useThemeStore<T>(selector: (s: ThemeStore) => T): T {
  return useSyncExternalStore(subscribe, () => selector(getSnapshot()));
}

export function useThemeMode(): [ThemeMode, (m: ThemeMode) => void] {
  const mode = useThemeStore((s) => s.mode);
  return [mode, state.setMode];
}

// ---------------- v1.3: apply theme to DOM ----------------

const CSS_VAR_MAP: Array<[keyof ThemeColors, string]> = [
  ["bg", "--bg"],
  ["bgSecondary", "--bg-secondary"],
  ["bgHover", "--bg-hover"],
  ["text", "--text"],
  ["textMuted", "--text-muted"],
  ["border", "--border"],
  ["primary", "--primary"],
  ["primaryText", "--primary-text"],
  ["danger", "--danger"],
  ["warning", "--warning"],
  ["success", "--success"],
  ["codeBg", "--code-bg"],
];

function applyToDom(s: ThemeStore) {
  if (typeof document === "undefined") return;
  const theme = s.themes.find((t) => t.id === s.activeThemeId) ?? BUILTIN_THEMES[0];
  const root = document.documentElement;
  // 系统模式：根据系统偏好选 light / dark 主题
  let effective: ThemeMode = s.mode;
  if (s.mode === "system") {
    effective = window.matchMedia?.("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  // 切换 root 的 data-theme（给 css hook 用）
  root.setAttribute("data-theme", effective);
  // 写入 CSS 变量（用户自定义主题的颜色优先）
  for (const [k, varName] of CSS_VAR_MAP) {
    const v = theme.colors[k];
    if (v) root.style.setProperty(varName, v);
  }
  root.style.setProperty("--theme-name", theme.name);
}

// 启动时应用一次
if (typeof document !== "undefined") {
  // 延后到下一个 tick，等 css 加载完
  setTimeout(() => applyToDom(state), 0);
  // 监听系统切换（仅 system 模式）
  if (window.matchMedia) {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    mql.addEventListener?.("change", () => {
      if (state.mode === "system") applyToDom(state);
    });
  }
}

/// 重新应用（用于外部导入主题后）
export function reapplyTheme() {
  applyToDom(state);
}