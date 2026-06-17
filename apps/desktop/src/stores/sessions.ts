// 会话持久化 store（基于 tauri-plugin-store）
import { useSyncExternalStore } from "react";
import { LazyStore } from "@tauri-apps/plugin-store";

export type SessionMeta = {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
};

export type PersistedMessage = {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  text: string;
  thinking?: string;
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: unknown;
    result?: string;
    success?: boolean;
    error?: string;
  }>;
  streaming?: boolean;
  createdAt: number;
  inputTokens?: number;
  outputTokens?: number;
};

export type SessionsStore = {
  sessions: SessionMeta[];
  currentId: string | null;
  /** v0.2 新增：按 sessionId 索引的消息 */
  messages: Record<string, PersistedMessage[]>;
  setCurrent: (id: string | null) => void;
  create: (title?: string) => SessionMeta;
  remove: (id: string) => void;
  rename: (id: string, title: string) => void;
  /** v0.2 新增：追加消息到 session */
  appendMessage: (sessionId: string, msg: PersistedMessage) => void;
  /** v0.2 新增：替换整 session 消息列表（用于加载历史） */
  setMessages: (sessionId: string, msgs: PersistedMessage[]) => void;
};

function uid(): string {
  // 用 crypto.randomUUID 优先，fallback 到时间戳
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

// Tauri Store（懒加载）
let storeInstance: LazyStore | null = null;
function getStore(): LazyStore | null {
  if (storeInstance) return storeInstance;
  try {
    storeInstance = new LazyStore("agentshell.sessions.json");
    return storeInstance;
  } catch {
    return null;
  }
}

// v1.1：debounce + 增量同步
let persistTimer: number | null = null;
let pendingDirty = new Set<string>(); // session ids whose messages changed
let pendingSessionsList = false;
let pendingCurrentId: string | null | undefined = undefined;

function schedulePersist(
  sessions: SessionMeta[],
  currentId: string | null,
  dirtySessionIds: string[] | "all",
) {
  if (dirtySessionIds === "all") {
    const snap = getSnapshot();
    pendingDirty = new Set(Object.keys(snap.messages));
    pendingSessionsList = true;
  } else {
    for (const id of dirtySessionIds) pendingDirty.add(id);
  }
  if (currentId !== undefined) pendingCurrentId = currentId;
  pendingSessionsList = pendingSessionsList || true;

  if (persistTimer !== null) return;
  persistTimer = window.setTimeout(() => {
    void flushPersist(sessions, currentId);
  }, 800);
}

async function flushPersist(
  sessionsRef: SessionMeta[],
  _currentIdRef: string | null,
) {
  persistTimer = null;
  const s = getStore();
  if (!s) return;
  try {
    if (pendingSessionsList) {
      await s.set("sessions", sessionsRef);
      pendingSessionsList = false;
    }
    if (pendingCurrentId !== undefined) {
      await s.set("currentId", pendingCurrentId);
      pendingCurrentId = undefined;
    }
    if (pendingDirty.size > 0) {
      // v1.1：只写改动的 session 的 messages
      const snap = getSnapshot();
      for (const id of pendingDirty) {
        const list = snap.messages[id] ?? [];
        await s.set(`msg:${id}`, list);
      }
      pendingDirty = new Set();
    }
    await s.save();
  } catch (e) {
    console.warn("persist sessions failed:", e);
  }
}

async function persist(
  sessions: SessionMeta[],
  currentId: string | null,
  messages: Record<string, PersistedMessage[]>,
  dirty: string[] | "all" = "all",
) {
  // 兼容老 API：立即全量
  const s = getStore();
  if (!s) return;
  if (dirty === "all") {
    try {
      await s.set("sessions", sessions);
      await s.set("currentId", currentId);
      await s.set("messages", messages);
      await s.save();
    } catch (e) {
      console.warn("persist sessions failed:", e);
    }
  } else {
    schedulePersist(sessions, currentId, dirty);
  }
}

function getSnapshot(): SessionsStore {
  return state;
}

async function load(): Promise<{
  sessions: SessionMeta[];
  currentId: string | null;
  messages: Record<string, PersistedMessage[]>;
}> {
  const s = getStore();
  if (!s) return { sessions: [], currentId: null, messages: {} };
  try {
    const sessions = (await s.get<SessionMeta[]>("sessions")) || [];
    const currentId = (await s.get<string | null>("currentId")) || null;
    const messages: Record<string, PersistedMessage[]> = {};
    // v1.1：优先读分片 msg:<id>，没有再 fallback 老的 messages 单 key
    for (const sess of sessions) {
      const shard = await s.get<PersistedMessage[]>(`msg:${sess.id}`);
      if (shard) {
        messages[sess.id] = shard;
      }
    }
    if (Object.keys(messages).length === 0) {
      // 兼容老格式
      const old = (await s.get<Record<string, PersistedMessage[]>>("messages")) || {};
      Object.assign(messages, old);
    }
    return { sessions, currentId, messages };
  } catch {
    return { sessions: [], currentId: null, messages: {} };
  }
}

// 同步初始值（避免 SSR/首屏闪烁）
const initial = { sessions: [] as SessionMeta[], currentId: null as string | null, messages: {} as Record<string, PersistedMessage[]> };

// 异步加载 — 完成后通知 listeners
let state: SessionsStore = {
  sessions: initial.sessions,
  currentId: initial.currentId,
  messages: initial.messages,
  setCurrent: (id) => {
    state = { ...state, currentId: id };
    pendingCurrentId = id;
    schedulePersist(state.sessions, state.currentId, []);
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
    pendingSessionsList = true;
    pendingCurrentId = s.id;
    schedulePersist(state.sessions, state.currentId, []);
    listeners.forEach((l) => l());
    return s;
  },
  remove: (id) => {
    const sessions = state.sessions.filter((s) => s.id !== id);
    const currentId = state.currentId === id ? null : state.currentId;
    const { [id]: _, ...messages } = state.messages;
    state = { ...state, sessions, currentId, messages };
    pendingSessionsList = true;
    pendingDirty.delete(id);
    pendingCurrentId = currentId;
    schedulePersist(sessions, currentId, []);
    // v1.1：清掉 store key
    const st = getStore();
    if (st) void st.delete(`msg:${id}`);
    listeners.forEach((l) => l());
  },
  rename: (id, title) => {
    const sessions = state.sessions.map((s) =>
      s.id === id ? { ...s, title, updatedAt: Date.now() } : s
    );
    state = { ...state, sessions };
    pendingSessionsList = true;
    schedulePersist(sessions, state.currentId, []);
    listeners.forEach((l) => l());
  },
  appendMessage: (sessionId, msg) => {
    const list = state.messages[sessionId] || [];
    const next = [...list, msg];
    const messages = { ...state.messages, [sessionId]: next };
    const sessions = state.sessions.map((s) =>
      s.id === sessionId ? { ...s, updatedAt: Date.now() } : s
    );
    state = { ...state, messages, sessions };
    // v1.1：增量 + debounce
    pendingDirty.add(sessionId);
    pendingSessionsList = true;
    schedulePersist(sessions, state.currentId, [sessionId]);
    listeners.forEach((l) => l());
  },
  setMessages: (sessionId, msgs) => {
    const messages = { ...state.messages, [sessionId]: msgs };
    state = { ...state, messages };
    // v1.1：增量
    pendingDirty.add(sessionId);
    schedulePersist(state.sessions, state.currentId, [sessionId]);
    listeners.forEach((l) => l());
  },
};

// 启动时异步加载
void load().then((loaded) => {
  state = { ...state, ...loaded };
  listeners.forEach((l) => l());
});

// v1.1：关闭前立即 flush
if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", () => {
    if (persistTimer !== null) {
      clearTimeout(persistTimer);
      persistTimer = null;
      void flushPersist(state.sessions, state.currentId);
    }
  });
}

const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useSessionsStore<T>(selector: (s: SessionsStore) => T): T {
  return useSyncExternalStore(subscribe, () => selector(getSnapshot()));
}

/// 非 hook 版：直接读当前状态
export function getSessionsState(): SessionsStore {
  return getSnapshot();
}

/// 非 hook 版：直接写（用 patch 函数），触发 listeners
export function setSessionsState(patch: Partial<SessionsStore> | ((s: SessionsStore) => Partial<SessionsStore>)) {
  const current = getSnapshot();
  const p = typeof patch === "function" ? patch(current) : patch;
  state = { ...current, ...p };
  void persist(state.sessions, state.currentId, state.messages);
  listeners.forEach((l) => l());
}