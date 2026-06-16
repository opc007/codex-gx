import type { SessionMeta } from "../stores/sessions";

type Props = {
  session?: SessionMeta;
};

export function StatusBar({ session }: Props) {
  return (
    <footer className="statusbar">
      <span className="status-item">
        ● <strong>Ready</strong>
      </span>
      <span className="status-divider">|</span>
      <span className="status-item">默认模型：MiniMax M3</span>
      <span className="status-divider">|</span>
      <span className="status-item">License：未激活</span>
      {session && (
        <>
          <span className="status-divider">|</span>
          <span className="status-item">当前：{session.title}</span>
        </>
      )}
      <span className="status-spacer" />
      <span className="status-item status-muted">v0.1.0-alpha</span>
    </footer>
  );
}