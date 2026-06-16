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
        {msg.streaming && <span className="bubble-streaming">● 正在输入...</span>}
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
        {msg.toolCalls && msg.toolCalls.length > 0 && (
          <details className="bubble-tools" open>
            <summary>🛠️ 工具调用 ({msg.toolCalls.length})</summary>
            <div className="bubble-tools-list">
              {msg.toolCalls.map((tc) => (
                <div key={tc.id} className={`tool-call ${tc.success === false ? "tool-error" : tc.success ? "tool-success" : "tool-running"}`}>
                  <div className="tool-call-header">
                    <span className="tool-icon">
                      {tc.success === false ? "❌" : tc.success ? "✅" : "⏳"}
                    </span>
                    <code className="tool-name">{tc.name}</code>
                    {tc.success && <span className="tool-done">已完成</span>}
                  </div>
                  <div className="tool-args">
                    <strong>参数：</strong>
                    <code>{JSON.stringify(tc.arguments, null, 2)}</code>
                  </div>
                  {tc.result !== undefined && (
                    <div className="tool-result">
                      <strong>结果：</strong>
                      <pre>{tc.result.slice(0, 2000)}{tc.result.length > 2000 ? "\n... [truncated]" : ""}</pre>
                    </div>
                  )}
                  {tc.error && (
                    <div className="tool-error-msg">
                      <strong>错误：</strong>
                      <pre>{tc.error}</pre>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </details>
        )}
        <div className="bubble-text">{msg.text}</div>
      </div>
    </div>
  );
}