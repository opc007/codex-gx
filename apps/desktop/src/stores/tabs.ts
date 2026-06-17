// v1.1：多 session 标签页
import { useSyncExternalStore } from "react";
import { setSessionsState, getSessionsState } from "./sessions";

const STORAGE_KEY = "codex_gx_open_tabs";

function load(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) return arr.filter((x) => typeof x === "string");
    }
  } catch {
    // ignore
  }
  return [];
}

let openTabs: string[] = load();
const listeners = new Set<() => void>();

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function getSnapshot() {
  return openTabs;
}

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(openTabs));
  } catch {
    // ignore
  }
}

function notify() {
  persist();
  listeners.forEach((cb) => cb());
}

export function openTab(id: string) {
  if (openTabs.includes(id)) return;
  openTabs = [...openTabs, id];
  notify();
}

export function closeTab(id: string) {
  openTabs = openTabs.filter((t) => t !== id);
  notify();
  // 如果关闭的是当前 session，切到第一个 tab 或 currentId=null
  const sessState = getSessionsState();
  if (sessState.currentId === id) {
    const next = openTabs[0] ?? null;
    setSessionsState({ currentId: next });
  }
}

export function closeOtherTabs(id: string) {
  openTabs = [id];
  notify();
}

export function reorderTabs(from: number, to: number) {
  if (from === to || from < 0 || to < 0 || from >= openTabs.length || to >= openTabs.length) return;
  const next = [...openTabs];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  openTabs = next;
  notify();
}

export function useOpenTabs(): string[] {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** 关闭所有 tab（保留 sessions 列表） */
export function closeAllTabs() {
  openTabs = [];
  notify();
  setSessionsState({ currentId: null });
}