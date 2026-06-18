// v1.9.x：Workspace = 项目组（绑定本地文件夹，session 自动按文件夹隔离）
//
// 设计：
// - 工作区是「项目组」概念，绑定一个本地文件夹路径
// - Session 仍持久化在 ~/.agentshell/store.json（plugin-store）
// - SessionMeta 加 workspaceId 字段；切换 workspace = UI 按 workspaceId 过滤
// - 每个 workspace 可绑定 folderPath，Composer 注入 system prompt 帮助 M3 理解项目
// - 后端 agent_run 接收 projectFolder 字段，并尝试读 README/AGENTS.md 摘要

import { useSyncExternalStore } from "react";

const STORAGE_KEY = "codex_gx_current_workspace";
const WS_LIST_KEY = "codex_gx_workspaces";

export type WorkspaceMeta = {
  id: string;
  name: string;
  createdAt: number;
  /** 绑定的本地文件夹绝对路径（v1.9.x：项目根） */
  folderPath?: string;
  /** v1.9.x：项目简介（用户在新建时填，可选） */
  description?: string;
  /** 颜色色标（便于 Sidebar 区分） */
  color?: string;
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
      if (Array.isArray(arr)) {
        // 迁移：给老的 default 加 folderPath 占位
        return arr.map((w) =>
          w.id === "default" && !w.folderPath
            ? { ...w, folderPath: undefined }
            : w,
        );
      }
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

export function createWorkspace(
  name: string,
  opts: { folderPath?: string; description?: string; color?: string } = {},
): WorkspaceMeta {
  const id = `ws_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  const meta: WorkspaceMeta = {
    id,
    name: name.trim() || `Workspace ${workspaces.length + 1}`,
    createdAt: Date.now(),
    folderPath: opts.folderPath,
    description: opts.description,
    color: opts.color,
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

export function renameWorkspace(
  id: string,
  name: string,
  opts: { folderPath?: string; description?: string; color?: string } = {},
) {
  workspaces = workspaces.map((w) =>
    w.id === id
      ? {
          ...w,
          name: name.trim() || w.name,
          ...(opts.folderPath !== undefined ? { folderPath: opts.folderPath } : {}),
          ...(opts.description !== undefined ? { description: opts.description } : {}),
          ...(opts.color !== undefined ? { color: opts.color } : {}),
        }
      : w,
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

/** 拿到当前 workspace 的完整元信息（含 folderPath） */
export function getCurrentWorkspace(): WorkspaceMeta {
  return workspaces.find((w) => w.id === currentId) ?? workspaces[0];
}

export function useWorkspaceList(): WorkspaceMeta[] {
  return useSyncExternalStore(subscribe, getListSnapshot, getListSnapshot);
}

export function useCurrentWorkspace(): WorkspaceMeta {
  return useSyncExternalStore(subscribe, getListSnapshot, getListSnapshot)
    .find((w) => w.id === currentId) ?? { id: "default", name: "Default", createdAt: 0 };
}
