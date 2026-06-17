import { useEffect, useRef } from "react";
import { useSessionsStore, type PersistedMessage } from "../stores/sessions";
import { MessageBubble } from "./MessageBubble";

const EMPTY_MESSAGES: PersistedMessage[] = [];

type Props = {
  sessionId: string | null;
};

export function Thread({ sessionId }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const messages = useSessionsStore((s) =>
    sessionId ? (s.messages[sessionId] ?? EMPTY_MESSAGES) : EMPTY_MESSAGES
  );
  const session = useSessionsStore((s) =>
    s.sessions.find((x) => x.id === sessionId)
  );

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages.length, sessionId]);

  if (!sessionId || !session) {
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