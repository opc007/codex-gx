// 会话持久化 store（基于 tauri-plugin-store）
import { useSyncExternalStore } from "react";
import { LazyStore } from "@tauri-apps/plugin-store";
import { getCurrentWorkspaceId } from "./workspace";
import { getCurrentUserId } from "./users";

export type SessionMeta = {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  /// v1.3：所属 workspace id（默认 "default"）
  workspaceId?: string;
  /// v1.3：所属 user id（默认当前用户）
  ownerId?: string;
  /// v1.8：分支父 session id（fork 时填，side 旁问为空）
  parentId?: string;
  /// v1.8：分支点 message id（fork 时填，side 旁问为空）
  forkPointMessageId?: string;
  /// v1.8：旁问标记（side 旁问 24h 后自动清理）
  side?: boolean;
  /// v1.8：旁问过期时间
  expiresAt?: number;
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
  /// v1.9.6：多模态生图结果（图片 URL 数组）
  mediaGallery?: string[];
  /// v1.9.6：多模态生视频结果（视频 URL）
  mediaVideo?: string;
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
  /** v1.8: Fork 当前 session — 复制所有 messages 到新 session，标记 parentId/forkPointMessageId */
  fork: (label?: string) => SessionMeta | null;
  /** v1.8: Side 旁问 — 临时 session 24h 后过期 */
  side: (question: string) => SessionMeta;
  /** v1.8: 列所有 forks of a parent */
  forksOf: (parentId: string) => SessionMeta[];
  /** v1.8: 清理过期 side sessions */
  gcExpiredSides: () => number;
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
    const rawSessions = (await s.get<SessionMeta[]>("sessions")) || [];
    // v1.3 迁移：老 session 没有 workspaceId / ownerId 字段 → 标为 "default" / 当前 user
    const currentUser = getCurrentUserId();
    const sessions: SessionMeta[] = rawSessions.map((sess) => ({
      ...sess,
      workspaceId: sess.workspaceId ?? "default",
      ownerId: sess.ownerId ?? currentUser,
    }));
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
      workspaceId: getCurrentWorkspaceId(),
      ownerId: getCurrentUserId(),
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
  // v1.8: Fork — 复制所有 messages 到新 session
  fork: (label) => {
    if (!state.currentId) return null;
    const parent = state.sessions.find((s) => s.id === state.currentId);
    if (!parent) return null;
    const now = Date.now();
    const parentMessages = state.messages[parent.id] || [];
    const lastMsg = parentMessages[parentMessages.length - 1];
    const forkPoint = lastMsg?.id;
    const newSession: SessionMeta = {
      id: uid(),
      title: label || `↳ ${parent.title} (fork)`,
      createdAt: now,
      updatedAt: now,
      workspaceId: parent.workspaceId,
      ownerId: parent.ownerId,
      parentId: parent.id,
      forkPointMessageId: forkPoint,
    };
    // 复制 messages (深 clone, 重新分配 id 防止冲突)
    const clonedMessages: PersistedMessage[] = parentMessages.map((m) => ({
      ...m,
      id: uid(),
      toolCalls: m.toolCalls?.map((tc) => ({ ...tc, id: uid() })),
    }));
    state = {
      ...state,
      sessions: [newSession, ...state.sessions],
      currentId: newSession.id,
      messages: { ...state.messages, [newSession.id]: clonedMessages },
    };
    pendingSessionsList = true;
    pendingCurrentId = newSession.id;
    pendingDirty.add(newSession.id);
    schedulePersist(state.sessions, state.currentId, [newSession.id]);
    listeners.forEach((l) => l());
    return newSession;
  },
  // v1.8: Side 旁问 — 临时 session 24h 后过期
  side: (question) => {
    const now = Date.now();
    const title = question.length > 30 ? question.slice(0, 30) + "…" : question;
    const s: SessionMeta = {
      id: uid(),
      title: `💬 ${title}`,
      createdAt: now,
      updatedAt: now,
      workspaceId: getCurrentWorkspaceId(),
      ownerId: getCurrentUserId(),
      side: true,
      expiresAt: now + 24 * 60 * 60 * 1000, // 24h
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
  // v1.8: 列所有 forks of a parent
  forksOf: (parentId) => {
    return state.sessions.filter((s) => s.parentId === parentId);
  },
  // v1.8: 清理过期 side sessions
  gcExpiredSides: () => {
    const now = Date.now();
    const expired = state.sessions.filter((s) => s.side && s.expiresAt && s.expiresAt < now);
    if (expired.length === 0) return 0;
    const expiredIds = new Set(expired.map((s) => s.id));
    const newMessages = { ...state.messages };
    for (const id of expiredIds) delete newMessages[id];
    state = {
      ...state,
      sessions: state.sessions.filter((s) => !expiredIds.has(s.id)),
      messages: newMessages,
    };
    pendingSessionsList = true;
    schedulePersist(state.sessions, state.currentId, "all");
    listeners.forEach((l) => l());
    return expired.length;
  },
};

// 启动时异步加载
void load().then((loaded) => {
  state = { ...state, ...loaded };
  // 首启或数据为空：自动建一个会话，避免输入框禁用、占位符显示「取消」
  if (state.sessions.length === 0) {
    state.create("新会话");
  } else if (!state.currentId) {
    state.setCurrent(state.sessions[0].id);
  }
  listeners.forEach((l) => l());
});

// v1.3：监听 workspace 切换事件
// 当前实现：所有 workspace 共用同一个 store，session 按 workspaceId 字段过滤
// 切换 workspace 不需要重新 load，只需让 UI 重新订阅（state 不变）
// 这里只是占位监听（打日志）— 未来若做多 store 物理隔离时启用
if (typeof window !== "undefined") {
  void import("@tauri-apps/api/event").then(({ listen }) => {
    listen("workspace:changed", (e) => {
      tracing_warn("workspace changed to", e.payload);
      // 不重 load；UI 按 workspaceId 过滤即可
    });
  });
}

function tracing_warn(..._args: unknown[]) {
  // console-only log
  // eslint-disable-next-line no-console
  console.debug("[v1.3 workspace]", ..._args);
}

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