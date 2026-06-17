// v1.3：团队 / 多用户面板
// - 列出所有 user
// - 切换当前 user（Top bar 头像下拉也调用同一组）
// - 新增 / 删除 / 重命名 user
// - 改 emoji / 颜色
// - 分享当前 session（生成 URL hash 链接）

import { useState } from "react";
import {
  type User,
  useUserList,
  useCurrentUser,
  useCurrentUserId,
  switchUser,
  createUser,
  deleteUser,
  updateUser,
} from "../stores/users";
import { useSessionsStore } from "../stores/sessions";

const COLOR_PALETTE = [
  "#0a84ff",
  "#34c759",
  "#ff9500",
  "#ff3b30",
  "#af52de",
  "#5ac8fa",
  "#ffcc00",
  "#ff2d92",
  "#8e8e93",
  "#a2845e",
];

const EMOJI_PALETTE = [
  "👤", "🦊", "🐱", "🐶", "🐼", "🦁", "🐯", "🐸",
  "🐵", "🦄", "🐧", "🐢", "🦉", "🐳", "🐝", "🐬",
  "🤖", "👨", "👩", "🧑", "👶", "👴", "👵", "🧔",
];

type Props = {
  onClose: () => void;
};

export function TeamPanel({ onClose }: Props) {
  const list = useUserList();
  const current = useCurrentUser();
  const currentId = useCurrentUserId();
  const sessionId = useSessionsStore((s) => s.currentId);
  const [shareUrl, setShareUrl] = useState<string | null>(null);

  const handleAdd = () => {
    const name = prompt("新用户名称：", `User ${list.length + 1}`);
    if (name === null) return;
    createUser(name);
  };

  const handleDelete = (u: User) => {
    if (u.role === "owner") {
      alert("owner 不能删除");
      return;
    }
    if (!confirm(`删除用户「${u.displayName}」？\n（其创建的 session 不会被删除）`)) return;
    deleteUser(u.id);
  };

  const handleGenerateShare = () => {
    if (!sessionId) {
      alert("请先选择要分享的 session");
      return;
    }
    // 简单 base64 编码 session id
    const encoded = btoa(sessionId);
    const url = `${window.location.origin}${window.location.pathname}#session=${encoded}`;
    setShareUrl(url);
    navigator.clipboard.writeText(url).catch(() => {
      // ignore
    });
  };

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 720, width: "92vw" }}
      >
        <div className="modal-head">
          <h3>👥 团队 / 用户管理（v1.3）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">
            ×
          </button>
        </div>

        <div className="modal-body theme-body">
          <div className="team-current">
            <span style={{ color: "var(--text-muted)", fontSize: 12 }}>
              当前用户
            </span>
            <div className="team-current-card">
              <span
                className="team-avatar"
                style={{ background: current.color }}
              >
                {current.emoji}
              </span>
              <div>
                <div style={{ fontWeight: 500 }}>{current.displayName}</div>
                <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
                  {current.role} · {current.id}
                </div>
              </div>
            </div>
          </div>

          <div className="team-list">
            <h4>
              📋 用户列表（{list.length}）
              <button
                className="btn small"
                onClick={handleAdd}
                style={{ marginLeft: 12 }}
              >
                ＋ 新增
              </button>
            </h4>
            {list.map((u) => (
              <div
                key={u.id}
                className={`team-user ${u.id === currentId ? "active" : ""}`}
              >
                <button
                  className="team-avatar-btn"
                  onClick={() => switchUser(u.id)}
                  title="切换到此用户"
                >
                  <span
                    className="team-avatar"
                    style={{ background: u.color }}
                  >
                    {u.emoji}
                  </span>
                </button>
                <input
                  className="vault-password-input"
                  value={u.displayName}
                  onChange={(e) =>
                    updateUser(u.id, { displayName: e.target.value })
                  }
                  disabled={u.role === "owner"}
                  style={{ flex: 1 }}
                />
                <select
                  className="topbar-select"
                  value={u.emoji}
                  onChange={(e) =>
                    updateUser(u.id, { emoji: e.target.value })
                  }
                >
                  {EMOJI_PALETTE.map((e) => (
                    <option key={e} value={e}>
                      {e}
                    </option>
                  ))}
                </select>
                <div className="team-colors">
                  {COLOR_PALETTE.map((c) => (
                    <button
                      key={c}
                      className={`team-color-dot ${u.color === c ? "active" : ""}`}
                      style={{ background: c }}
                      onClick={() => updateUser(u.id, { color: c })}
                      title={c}
                    />
                  ))}
                </div>
                <select
                  className="topbar-select"
                  value={u.role}
                  onChange={(e) =>
                    updateUser(u.id, { role: e.target.value as User["role"] })
                  }
                  disabled={u.role === "owner"}
                >
                  <option value="owner">owner</option>
                  <option value="admin">admin</option>
                  <option value="member">member</option>
                  <option value="guest">guest</option>
                </select>
                {u.role !== "owner" && (
                  <button
                    className="btn small danger"
                    onClick={() => handleDelete(u)}
                  >
                    ×
                  </button>
                )}
              </div>
            ))}
          </div>

          <div className="team-share">
            <h4>🔗 分享当前 session</h4>
            <p style={{ color: "var(--text-muted)", fontSize: 12 }}>
              生成本地链接（仅含 session id，不上传数据）。对方需在同一设备打开 Codex gx 才能加载。
            </p>
            <button
              className="btn primary"
              onClick={handleGenerateShare}
              disabled={!sessionId}
            >
              生成并复制分享链接
            </button>
            {shareUrl && (
              <div className="team-share-result">
                <code>{shareUrl}</code>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}