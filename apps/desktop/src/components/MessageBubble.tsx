import type { ChatMessage } from "../lib/chat";

type Props = {
  msg: ChatMessage;
};

export function MessageBubble({ msg }: Props) {
  const role = msg.role;
  return (
    <div className={`bubble bubble-${role}`}>
      <div className="bubble-meta">
        <span className="bubble-role">
          {role === "user" ? "你" : role === "assistant" ? "AgentShell" : role}
        </span>
        <span className="bubble-time">
          {new Date(msg.createdAt).toLocaleTimeString()}
        </span>
      </div>
      <div className="bubble-content">
        {msg.thinking && (
          <details className="bubble-thinking">
            <summary>💭 思考过程</summary>
            <pre>{msg.thinking}</pre>
          </details>
        )}
        <div className="bubble-text">{msg.text || (msg.streaming ? "..." : "")}</div>
        {msg.toolCalls && msg.toolCalls.length > 0 && (
          <div className="bubble-tools">
            {msg.toolCalls.map((t) => (
              <div key={t.id} className="bubble-tool">
                <strong>{t.name}</strong>
                <code>{JSON.stringify(t.arguments)}</code>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}