import { useSessionsStore } from "../stores/sessions";

type Props = {
  sessionId: string | null;
};

export function StatusBar({ sessionId }: Props) {
  const session = useSessionsStore((s) =>
    sessionId ? s.sessions.find((x) => x.id === sessionId) : undefined
  );
  const messages = useSessionsStore((s) =>
    sessionId ? s.messages[sessionId] || [] : []
  );

  // 累计 token
  let totalIn = 0, totalOut = 0;
  for (const m of messages) {
    if (m.inputTokens) totalIn += m.inputTokens;
    if (m.outputTokens) totalOut += m.outputTokens;
  }

  return (
    <footer className="statusbar">
      <span className="status-item">
        ● <strong>Ready</strong>
      </span>
      <span className="status-divider">|</span>
      <span className="status-item">默认模型：MiniMax M3</span>
      {session && (
        <>
          <span className="status-divider">|</span>
          <span className="status-item">当前：{session.title}</span>
        </>
      )}
      <span className="status-divider">|</span>
      <span className="status-item">消息：{messages.length}</span>
      {(totalIn > 0 || totalOut > 0) && (
        <>
          <span className="status-divider">|</span>
          <span className="status-item" title="本会话累计 token">
            🪙 ↑{totalIn.toLocaleString()} ↓{totalOut.toLocaleString()}
          </span>
        </>
      )}
      <span className="status-spacer" />
      <span className="status-item status-muted">v0.2.0-alpha</span>
    </footer>
  );
}