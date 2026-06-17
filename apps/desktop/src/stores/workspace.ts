// v1.3：Workspace store — 多工作区切换（最小方案）
//
// 设计：
// - 工作区是前端逻辑层概念
// - Session 仍持久化在 ~/.agentshell/store.json（plugin-store）
// - SessionMeta 加 workspaceId 字段
// - 切换 workspace = 改 currentWorkspaceId，UI 按 workspaceId 过滤
// - 支持新建、删除、切换、持久化当前 workspace id
//
// 这样避免重写 sessions store，零迁移成本。

import { useSyncExternalStore } from "react";

const STORAGE_KEY = "codex_gx_current_workspace";
const WS_LIST_KEY = "codex_gx_workspaces";

export type WorkspaceMeta = {
  id: string;
  name: string;
  createdAt: number;
};

let currentId: string = loadInitial();
let workspaces: WorkspaceMeta[] = loadList();
const listeners = new Set<() => void>();

function loadInitial(): string {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v) return v;
  } catch {
    // ignore
  }
  return "default";
}

function loadList(): WorkspaceMeta[] {
  try {
    const raw = localStorage.getItem(WS_LIST_KEY);
    if (raw) {
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) return arr;
    }
  } catch {
    // ignore
  }
  return [{ id: "default", name: "Default", createdAt: 0 }];
}

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, currentId);
    localStorage.setItem(WS_LIST_KEY, JSON.stringify(workspaces));
  } catch {
    // ignore
  }
}

function notify() {
  persist();
  listeners.forEach((cb) => cb());
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

function getIdSnapshot() {
  return currentId;
}
function getListSnapshot() {
  return workspaces;
}

export function switchWorkspace(id: string) {
  if (id === currentId) return;
  currentId = id;
  notify();
}

export function createWorkspace(name: string): WorkspaceMeta {
  const id = `ws_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  const meta: WorkspaceMeta = {
    id,
    name: name.trim() || `Workspace ${workspaces.length + 1}`,
    createdAt: Date.now(),
  };
  workspaces = [...workspaces, meta];
  currentId = id;
  notify();
  return meta;
}

export function deleteWorkspace(id: string) {
  if (id === "default") {
    throw new Error("不能删除默认工作区");
  }
  workspaces = workspaces.filter((w) => w.id !== id);
  if (currentId === id) {
    currentId = "default";
  }
  notify();
}

export function renameWorkspace(id: string, name: string) {
  workspaces = workspaces.map((w) =>
    w.id === id ? { ...w, name: name.trim() || w.name } : w,
  );
  notify();
}

export function useCurrentWorkspaceId(): string {
  return useSyncExternalStore(subscribe, getIdSnapshot, getIdSnapshot);
}

/** 非 hook 版：sessions store 在 mutation 时调用，拿到当前 workspaceId 给新 session 打标 */
export function getCurrentWorkspaceId(): string {
  return currentId;
}

export function useWorkspaceList(): WorkspaceMeta[] {
  return useSyncExternalStore(subscribe, getListSnapshot, getListSnapshot);
}