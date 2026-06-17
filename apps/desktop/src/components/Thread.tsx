import { useEffect, useRef } from "react";
import { useSessionsStore, type PersistedMessage } from "../stores/sessions";
import { MessageBubble } from "./MessageBubble";
import { useTranslation } from "../i18n";

const EMPTY_MESSAGES: PersistedMessage[] = [];

type Props = {
  sessionId: string | null;
};

export function Thread({ sessionId }: Props) {
  const t = useTranslation();
  const create = useSessionsStore((s) => s.create);
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
        <h2>{t.emptyHint}</h2>
        <p>{t.emptySubHint}</p>
        <p style={{ color: "var(--muted)", marginTop: 8 }}>
          点左侧 <kbd>+</kbd> 或下方按钮开始对话
        </p>
        <button
          className="btn primary"
          style={{ marginTop: 20 }}
          onClick={() => create(t.newSession)}
        >
          ＋ {t.newSession}
        </button>
      </div>
    );
  }

  return (
    <div className="thread" ref={scrollRef}>
      {messages.length === 0 && (
        <div className="thread-welcome">
          <h3>{session.title}</h3>
          <p style={{ color: "var(--muted)" }}>{t.placeholder}</p>
        </div>
      )}
      {messages.map((m) => (
        <MessageBubble key={m.id} msg={m} />
      ))}
    </div>
  );
}