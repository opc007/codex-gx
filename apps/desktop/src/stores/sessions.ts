// 会话 store
import { useSyncExternalStore } from "react";

export type SessionMeta = {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
};

export type SessionsStore = {
  sessions: SessionMeta[];
  currentId: string | null;
  setCurrent: (id: string | null) => void;
  create: (title?: string) => SessionMeta;
  remove: (id: string) => void;
  rename: (id: string, title: string) => void;
};

function uid(): string {
  return Math.random().toString(36).slice(2, 10);
}

function persist(sessions: SessionMeta[], currentId: string | null) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem("agentshell.sessions", JSON.stringify({ sessions, currentId }));
}

function load(): { sessions: SessionMeta[]; currentId: string | null } {
  if (typeof localStorage === "undefined") return { sessions: [], currentId: null };
  try {
    const raw = localStorage.getItem("agentshell.sessions");
    if (raw) return JSON.parse(raw);
  } catch {}
  return { sessions: [], currentId: null };
}

const initial = load();

let state: SessionsStore = {
  sessions: initial.sessions,
  currentId: initial.currentId,
  setCurrent: (id) => {
    state = { ...state, currentId: id };
    persist(state.sessions, state.currentId);
    listeners.forEach((l) => l());
  },
  create: (title) => {
    const now = Date.now();
    const s: SessionMeta = {
      id: uid(),
      title: title || `New session ${state.sessions.length + 1}`,
      createdAt: now,
      updatedAt: now,
    };
    state = {
      ...state,
      sessions: [s, ...state.sessions],
      currentId: s.id,
    };
    persist(state.sessions, state.currentId);
    listeners.forEach((l) => l());
    return s;
  },
  remove: (id) => {
    const sessions = state.sessions.filter((s) => s.id !== id);
    const currentId = state.currentId === id ? null : state.currentId;
    state = { ...state, sessions, currentId };
    persist(sessions, currentId);
    listeners.forEach((l) => l());
  },
  rename: (id, title) => {
    const sessions = state.sessions.map((s) =>
      s.id === id ? { ...s, title, updatedAt: Date.now() } : s
    );
    state = { ...state, sessions };
    persist(sessions, state.currentId);
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

function getSnapshot(): SessionsStore {
  return state;
}

export function useSessionsStore<T>(selector: (s: SessionsStore) => T): T {
  return useSyncExternalStore(subscribe, () => selector(getSnapshot()));
}