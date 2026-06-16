import { useSessionsStore } from "../stores/sessions";

export function Sidebar() {
  const sessions = useSessionsStore((s) => s.sessions);
  const currentId = useSessionsStore((s) => s.currentId);
  const setCurrent = useSessionsStore((s) => s.setCurrent);
  const create = useSessionsStore((s) => s.create);
  const remove = useSessionsStore((s) => s.remove);

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span>会话 ({sessions.length})</span>
        <button
          className="sidebar-new"
          onClick={() => create()}
          title="新建会话"
        >
          +
        </button>
      </div>
      <ul className="session-list">
        {sessions.length === 0 && (
          <li className="session-empty">还没有会话</li>
        )}
        {sessions.map((s) => (
          <li
            key={s.id}
            className={`session-item ${s.id === currentId ? "active" : ""}`}
            onClick={() => setCurrent(s.id)}
          >
            <span className="session-title">{s.title}</span>
            <button
              className="session-del"
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(`删除 "${s.title}"？`)) remove(s.id);
              }}
              title="删除"
            >
              ×
            </button>
          </li>
        ))}
      </ul>
    </aside>
  );
}