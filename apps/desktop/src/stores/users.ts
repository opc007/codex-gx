// v1.3：多用户 / 团队支持
//
// 设计：
// - User = { id, displayName, color, emoji, role }
// - 多个 user 共存于同一台设备（家庭/团队共享设备场景）
// - 每个 user 有自己的 sessions
// - session 创建时记录 owner
// - 支持 user 切换（Top bar 头像下拉）
// - 持久化到 localStorage

import { useSyncExternalStore } from "react";

const STORAGE_USERS = "codex_gx_users";
const STORAGE_CURRENT = "codex_gx_current_user";

export type UserRole = "owner" | "admin" | "member" | "guest";

export type User = {
  id: string;
  displayName: string;
  emoji: string;
  /// 颜色 token（hex）用于消息气泡 / 头像
  color: string;
  /// 角色
  role: UserRole;
  createdAt: number;
};

const DEFAULT_COLORS = [
  "#0a84ff",
  "#34c759",
  "#ff9500",
  "#ff3b30",
  "#af52de",
  "#5ac8fa",
  "#ffcc00",
  "#ff2d92",
];

const DEFAULT_EMOJIS = [
  "👤", "🦊", "🐱", "🐶", "🐼", "🦁", "🐯", "🐸",
  "🐵", "🦄", "🐧", "🐢", "🦉", "🐳", "🐝", "🐬",
];

function makeBuiltinUsers(): User[] {
  return [
    {
      id: "u_local_owner",
      displayName: "我",
      emoji: "👤",
      color: DEFAULT_COLORS[0],
      role: "owner",
      createdAt: 0,
    },
  ];
}

function loadUsers(): User[] {
  try {
    const raw = localStorage.getItem(STORAGE_USERS);
    if (raw) {
      const arr = JSON.parse(raw);
      if (Array.isArray(arr) && arr.length > 0) return arr;
    }
  } catch {
    // ignore
  }
  return makeBuiltinUsers();
}

function loadCurrent(users: User[]): string {
  try {
    const v = localStorage.getItem(STORAGE_CURRENT);
    if (v && users.find((u) => u.id === v)) return v;
  } catch {
    // ignore
  }
  return users[0].id;
}

let users: User[] = loadUsers();
let currentId: string = loadCurrent(users);
const listeners = new Set<() => void>();

function persist() {
  try {
    localStorage.setItem(STORAGE_USERS, JSON.stringify(users));
    localStorage.setItem(STORAGE_CURRENT, currentId);
  } catch {
    // ignore
  }
}

function notify() {
  persist();
  listeners.forEach((l) => l());
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

function getUsersSnapshot() {
  return users;
}
function getIdSnapshot() {
  return currentId;
}

export function switchUser(id: string) {
  if (id === currentId) return;
  if (!users.find((u) => u.id === id)) return;
  currentId = id;
  notify();
}

export function createUser(name: string): User {
  const id = `u_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
  const idx = users.length;
  const u: User = {
    id,
    displayName: name.trim() || `User ${idx + 1}`,
    emoji: DEFAULT_EMOJIS[idx % DEFAULT_EMOJIS.length],
    color: DEFAULT_COLORS[idx % DEFAULT_COLORS.length],
    role: "member",
    createdAt: Date.now(),
  };
  users = [...users, u];
  currentId = id;
  notify();
  return u;
}

export function deleteUser(id: string) {
  if (users.length <= 1) {
    throw new Error("至少保留一个用户");
  }
  const u = users.find((x) => x.id === id);
  if (u?.role === "owner") {
    throw new Error("owner 角色不能删除");
  }
  users = users.filter((x) => x.id !== id);
  if (currentId === id) currentId = users[0].id;
  notify();
}

export function updateUser(id: string, patch: Partial<User>) {
  users = users.map((u) => (u.id === id ? { ...u, ...patch } : u));
  notify();
}

export function getCurrentUser(): User {
  return users.find((u) => u.id === currentId) ?? users[0];
}

export function getCurrentUserId(): string {
  return currentId;
}

export function useCurrentUser(): User {
  const id = useSyncExternalStore(subscribe, getIdSnapshot, getIdSnapshot);
  const list = useSyncExternalStore(subscribe, getUsersSnapshot, getUsersSnapshot);
  return list.find((u) => u.id === id) ?? list[0];
}

export function useUserList(): User[] {
  return useSyncExternalStore(subscribe, getUsersSnapshot, getUsersSnapshot);
}

export function useCurrentUserId(): string {
  return useSyncExternalStore(subscribe, getIdSnapshot, getIdSnapshot);
}