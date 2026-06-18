import { useEffect, useRef } from "react";
import { useSessionsStore, getSessionsState, type PersistedMessage } from "../stores/sessions";
import { MessageBubble } from "./MessageBubble";
import { useTranslation } from "../i18n";
import { useCurrentWorkspace } from "../stores/workspace";

const EMPTY_MESSAGES: PersistedMessage[] = [];

type Props = {
  sessionId: string | null;
};

const SUGGESTIONS = [
  { icon: "✨", text: "解释一下这段代码" },
  { icon: "🐛", text: "帮我 debug 一个 bug" },
  { icon: "✍️", text: "写一个 Python 工具脚本" },
  { icon: "💡", text: "给我一些点子" },
];

export function Thread({ sessionId }: Props) {
  const t = useTranslation();
  const currentWs = useCurrentWorkspace();
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
    const projectName = currentWs.name === "Default" ? "此项目" : currentWs.name;
    return (
      <div className="thread" ref={scrollRef}>
        <div className="thread-empty">
          <h2 className="thread-empty-heading">
            我们应该在 {projectName} 中构建什么？
          </h2>
          <p>我是 Codex gx — 你的 AI 编程 / 对话伙伴。</p>
          <div className="thread-empty-suggestions">
            {SUGGESTIONS.map((s, i) => (
              <button
                key={i}
                className="suggestion"
                onClick={() => {
                  const sess = create(t.newSession);
                  window.dispatchEvent(
                    new CustomEvent("agentshell:composer:fill", {
                      detail: s.text,
                    }),
                  );
                  getSessionsState().setCurrent(sess.id);
                }}
              >
                <span style={{ marginRight: 6 }}>{s.icon}</span>
                {s.text}
              </button>
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="thread" ref={scrollRef}>
      <div className="thread-inner">
        {messages.length === 0 && (
          <div className="thread-welcome thread-welcome-codex">
            <h2>
              我们应该在{" "}
              {currentWs.name === "Default" ? "此项目" : currentWs.name}{" "}
              中构建什么？
            </h2>
            <p style={{ color: "var(--text-muted)" }}>{t.placeholder}</p>
          </div>
        )}
        {messages.map((m) => (
          <MessageBubble key={m.id} msg={m} />
        ))}
      </div>
    </div>
  );
}
