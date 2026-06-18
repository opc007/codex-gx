// v1.1：多 session 标签页（v1.9.7：按项目组隔离 tabs，切换时同步 current session）
import { useSyncExternalStore } from "react";
import { setSessionsState, getSessionsState } from "./sessions";
import { getCurrentWorkspaceId } from "./workspace";

const STORAGE_KEY = "codex_gx_open_tabs_v2";
const LEGACY_KEY = "codex_gx_open_tabs";

function load(): Record<string, string[]> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const obj = JSON.parse(raw) as unknown;
      if (obj && typeof obj === "object" && !Array.isArray(obj)) {
        const out: Record<string, string[]> = {};
        for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
          if (Array.isArray(v)) {
            out[k] = v.filter((x) => typeof x === "string");
          }
        }
        return out;
      }
    }
  } catch {
    // ignore
  }
  try {
    const legacy = localStorage.getItem(LEGACY_KEY);
    if (legacy) {
      const arr = JSON.parse(legacy);
      if (Array.isArray(arr)) {
        localStorage.removeItem(LEGACY_KEY);
        return {
          default: arr.filter((x) => typeof x === "string"),
        };
      }
    }
  } catch {
    // ignore
  }
  return {};
}

let tabsByWorkspace: Record<string, string[]> = load();
const listeners = new Set<() => void>();

function tabsForWorkspace(wsId?: string): string[] {
  const id = wsId ?? getCurrentWorkspaceId();
  return tabsByWorkspace[id] ?? [];
}

function setTabsForWorkspace(wsId: string, tabs: string[]) {
  tabsByWorkspace = { ...tabsByWorkspace, [wsId]: tabs };
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function getSnapshot() {
  return tabsForWorkspace();
}

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(tabsByWorkspace));
  } catch {
    // ignore
  }
}

function notify() {
  persist();
  listeners.forEach((cb) => cb());
}

function sessionsInWorkspace(wsId: string) {
  return getSessionsState()
    .sessions.filter((s) => (s.workspaceId ?? "default") === wsId)
    .sort((a, b) => b.updatedAt - a.updatedAt);
}

/** 切换项目组后：恢复该组的 tabs，并确保 current session 属于当前组 */
export function syncTabsForWorkspace(wsId: string) {
  const sessState = getSessionsState();
  const wsSessions = sessionsInWorkspace(wsId);
  const validIds = new Set(wsSessions.map((s) => s.id));

  let tabs = tabsForWorkspace(wsId).filter((id) => validIds.has(id));
  let currentId = sessState.currentId;
  const currentInWs = currentId != null && validIds.has(currentId);

  if (!currentInWs) {
    currentId = tabs[0] ?? wsSessions[0]?.id ?? null;
    if (currentId) {
      sessState.setCurrent(currentId);
    }
  }

  if (currentId && !tabs.includes(currentId)) {
    tabs = [...tabs, currentId];
  }

  setTabsForWorkspace(wsId, tabs);
  notify();
}

if (typeof window !== "undefined") {
  window.addEventListener("codex_gx:workspace-changed", (e) => {
    const wsId = (e as CustomEvent<string>).detail;
    if (typeof wsId === "string") syncTabsForWorkspace(wsId);
  });
}

export function openTab(id: string, wsId?: string) {
  const ws = wsId ?? getCurrentWorkspaceId();
  const tabs = tabsForWorkspace(ws);
  if (tabs.includes(id)) return;
  setTabsForWorkspace(ws, [...tabs, id]);
  notify();
}

export function closeTab(id: string) {
  const ws = getCurrentWorkspaceId();
  const tabs = tabsForWorkspace(ws).filter((t) => t !== id);
  setTabsForWorkspace(ws, tabs);
  notify();
  const sessState = getSessionsState();
  if (sessState.currentId === id) {
    const next = tabs[0] ?? null;
    setSessionsState({ currentId: next });
  }
}

export function closeOtherTabs(id: string) {
  const ws = getCurrentWorkspaceId();
  setTabsForWorkspace(ws, [id]);
  notify();
}

export function reorderTabs(from: number, to: number) {
  const ws = getCurrentWorkspaceId();
  const tabs = [...tabsForWorkspace(ws)];
  if (from === to || from < 0 || to < 0 || from >= tabs.length || to >= tabs.length) return;
  const [item] = tabs.splice(from, 1);
  tabs.splice(to, 0, item);
  setTabsForWorkspace(ws, tabs);
  notify();
}

export function useOpenTabs(): string[] {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** 关闭当前项目组的所有 tab（保留 sessions 列表） */
export function closeAllTabs() {
  const ws = getCurrentWorkspaceId();
  setTabsForWorkspace(ws, []);
  notify();
  setSessionsState({ currentId: null });
}