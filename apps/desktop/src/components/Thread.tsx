import { useEffect, useRef } from "react";
import type { SessionMeta } from "../stores/sessions";
import type { ChatMessage } from "../lib/chat";
import { MessageBubble } from "./MessageBubble";

type Props = {
  session?: SessionMeta;
};

export function Thread({ session }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // v0.1 没有持久化 message，只在内存里
  const messages: ChatMessage[] = session ? getMessages(session.id) : [];

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages.length, session?.id]);

  if (!session) {
    return (
      <div className="thread-empty">
        <h2>👋 欢迎使用 AgentShell</h2>
        <p>Codex 全功能平替 · 默认 MiniMax M3 · 国产大模型驱动</p>
        <p style={{ color: "var(--muted)" }}>
          点左侧 <kbd>+</kbd> 创建会话，然后开始对话
        </p>
      </div>
    );
  }

  return (
    <div className="thread" ref={scrollRef}>
      {messages.length === 0 && (
        <div className="thread-welcome">
          <h3>{session.title}</h3>
          <p style={{ color: "var(--muted)" }}>开始输入，AgentShell 会用 M3 模型回应</p>
        </div>
      )}
      {messages.map((m) => (
        <MessageBubble key={m.id} msg={m} />
      ))}
    </div>
  );
}

// v0.1 内存消息存储（后续会用 tauri-store 持久化）
const messageMap = new Map<string, ChatMessage[]>();

export function appendMessage(sessionId: string, msg: ChatMessage) {
  const list = messageMap.get(sessionId) || [];
  list.push(msg);
  messageMap.set(sessionId, list);
}

export function updateMessage(sessionId: string, id: string, patch: Partial<ChatMessage>) {
  const list = messageMap.get(sessionId);
  if (!list) return;
  const idx = list.findIndex((m) => m.id === id);
  if (idx >= 0) list[idx] = { ...list[idx], ...patch };
}

function getMessages(sessionId: string): ChatMessage[] {
  return messageMap.get(sessionId) || [];
}