import type { PersistedMessage } from "../stores/sessions";

type Props = {
  msg: PersistedMessage;
};

export function MessageBubble({ msg }: Props) {
  return (
    <div className={`bubble bubble-${msg.role}`}>
      <div className="bubble-meta">
        <span className="bubble-role">
          {msg.role === "user" ? "你" : msg.role === "assistant" ? "AgentShell" : msg.role}
        </span>
        <span className="bubble-time">
          {new Date(msg.createdAt).toLocaleTimeString()}
        </span>
        {(msg.inputTokens || msg.outputTokens) && (
          <span className="bubble-tokens">
            🪙 ↑{msg.inputTokens || 0} ↓{msg.outputTokens || 0}
          </span>
        )}
      </div>
      <div className="bubble-content">
        {msg.thinking && (
          <details className="bubble-thinking" open={msg.thinking.length < 200}>
            <summary>💭 思考过程</summary>
            <pre>{msg.thinking}</pre>
          </details>
        )}
        <div className="bubble-text">{msg.text}</div>
      </div>
    </div>
  );
}