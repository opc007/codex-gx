import { useSessionsStore, type SessionMeta, type PersistedMessage } from "../stores/sessions";
import { exportSession, type ExportFormat } from "../lib/export";
import { useState, useEffect } from "react";
import { closeTab } from "../stores/tabs";
import { useCurrentWorkspaceId } from "../stores/workspace";
import { useCurrentUser } from "../stores/users";
import { invoke } from "@tauri-apps/api/core";

export function Sidebar() {
  const allSessions = useSessionsStore((s) => s.sessions);
  const currentId = useSessionsStore((s) => s.currentId);
  const setCurrent = useSessionsStore((s) => s.setCurrent);
  const create = useSessionsStore((s) => s.create);
  const remove = useSessionsStore((s) => s.remove);
  const messages = useSessionsStore((s) => s.messages);
  const setMessages = useSessionsStore((s) => s.setMessages);
  const currentWorkspace = useCurrentWorkspaceId();
  const currentUser = useCurrentUser();

  const sessions = allSessions
    .filter((sess) => (sess.workspaceId ?? "default") === currentWorkspace)
    .slice()
    .sort((a, b) => b.updatedAt - a.updatedAt);

  const [exportOpen, setExportOpen] = useState<string | null>(null);
  const [redactOnExport, setRedactOnExport] = useState(true);

  const [encryptedSet, setEncryptedSet] = useState<Set<string>>(new Set());
  const [vaultPrompt, setVaultPrompt] = useState<null | {
    sessionId: string;
    mode: "encrypt" | "decrypt";
  }>(null);
  const [vaultPassword, setVaultPassword] = useState("");
  const [vaultError, setVaultError] = useState<string | null>(null);
  const [vaultBusy, setVaultBusy] = useState(false);

  const refreshEncrypted = async () => {
    try {
      const list = await invoke<{ session_id: string }[]>("vault_list_encrypted");
      setEncryptedSet(new Set(list.map((l) => l.session_id)));
    } catch {
      // backend not ready
    }
  };

  useEffect(() => {
    void refreshEncrypted();
  }, []);

  const doExport = async (s: SessionMeta, fmt: ExportFormat) => {
    const msgs: PersistedMessage[] = messages[s.id] ?? [];
    try {
      const path = await exportSession(s, msgs, fmt, undefined, redactOnExport);
      setExportOpen(null);
      alert(
        `已导出到：${path}\n${redactOnExport ? "（已脱敏敏感数据）" : "（未脱敏）"}`,
      );
    } catch (e: any) {
      if (!String(e).includes("已取消")) {
        alert(`导出失败：${e}`);
      }
      setExportOpen(null);
    }
  };

  const handleNewChat = () => {
    const s = create();
    setCurrent(s.id);
  };

  const handleDelete = (id: string, title: string) => {
    if (!confirm(`删除会话 "${title}"？`)) return;
    remove(id);
    closeTab(id);
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-top">
        <button
          className="sidebar-new-chat"
          onClick={handleNewChat}
          title="新建会话"
        >
          <span className="new-chat-icon">＋</span>
          <span>New chat</span>
        </button>
      </div>

      <div className="sidebar-section-label">
        <span>Chats</span>
        <span className="count">{sessions.length}</span>
      </div>

      <div className="sidebar-list">
        {sessions.length === 0 && (
          <div className="sidebar-empty">还没有会话</div>
        )}
        {sessions.map((s) => (
          <div
            key={s.id}
            className={`session-item ${s.id === currentId ? "active" : ""}`}
            onClick={() => setCurrent(s.id)}
            title={s.title}
          >
            <span className="session-item-icon">
              {s.side ? "💬" : s.parentId ? "↳" : "💭"}
            </span>
            <span className="session-item-title">{s.title}</span>
            <div className="session-item-actions">
              {encryptedSet.has(s.id) ? (
                <button
                  className="session-vault-locked"
                  title="已加密 — 点击解锁"
                  onClick={(e) => {
                    e.stopPropagation();
                    setVaultError(null);
                    setVaultPassword("");
                    setVaultPrompt({ sessionId: s.id, mode: "decrypt" });
                  }}
                >
                  🔒
                </button>
              ) : (
                <button
                  className="session-vault"
                  title="标记为敏感（加密）"
                  onClick={(e) => {
                    e.stopPropagation();
                    setVaultError(null);
                    setVaultPassword("");
                    setVaultPrompt({ sessionId: s.id, mode: "encrypt" });
                  }}
                >
                  🔓
                </button>
              )}
              <button
                className="session-export"
                onClick={(e) => {
                  e.stopPropagation();
                  setExportOpen(exportOpen === s.id ? null : s.id);
                }}
                title="导出"
              >
                ⬇
              </button>
              <button
                className="session-del"
                onClick={(e) => {
                  e.stopPropagation();
                  handleDelete(s.id, s.title);
                }}
                title="删除"
              >
                ×
              </button>
            </div>
            {exportOpen === s.id && (
              <div className="export-menu" onClick={(e) => e.stopPropagation()}>
                <label className="export-redact">
                  <input
                    type="checkbox"
                    checked={redactOnExport}
                    onChange={(e) => setRedactOnExport(e.target.checked)}
                  />
                  脱敏敏感数据
                </label>
                <button onClick={() => doExport(s, "markdown")}>📝 Markdown</button>
                <button onClick={() => doExport(s, "html")}>🌐 HTML</button>
                <button onClick={() => doExport(s, "json")}>📦 JSON</button>
              </div>
            )}
          </div>
        ))}
      </div>

      <div className="sidebar-bottom">
        <div
          className="sidebar-user"
          onClick={() => {
            window.dispatchEvent(new CustomEvent("open-user-menu"));
          }}
          title={`${currentUser.displayName} · ${currentUser.role}`}
        >
          <div
            className="team-avatar small"
            style={{ background: currentUser.color }}
          >
            {currentUser.emoji}
          </div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="sidebar-user-name">{currentUser.displayName}</div>
            <div className="sidebar-user-role">{currentUser.role}</div>
          </div>
        </div>
      </div>

      {vaultPrompt && (
        <div className="update-dialog-overlay" onClick={() => setVaultPrompt(null)}>
          <div className="update-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="update-dialog-header">
              <h2>
                {vaultPrompt.mode === "encrypt" ? "🔒 加密 Session" : "🔓 解锁 Session"}
              </h2>
              <button className="update-close" onClick={() => setVaultPrompt(null)}>×</button>
            </div>
            <div className="update-dialog-body">
              <p style={{ fontSize: 13, color: "var(--text-muted)" }}>
                {vaultPrompt.mode === "encrypt"
                  ? "此 session 的内容将用 AES-256-GCM 加密存储。请设置一个密码。忘记密码无法恢复。"
                  : "请输入密码以解锁此 session。"}
              </p>
              <input
                type="password"
                value={vaultPassword}
                onChange={(e) => setVaultPassword(e.target.value)}
                placeholder="密码"
                className="vault-password-input"
                autoFocus
              />
              {vaultError && (
                <div className="mp-error">❌ {vaultError}</div>
              )}
            </div>
            <div className="update-dialog-footer">
              <button className="update-cancel" onClick={() => setVaultPrompt(null)}>取消</button>
              <button
                className="update-go"
                disabled={vaultBusy || !vaultPassword}
                onClick={async () => {
                  if (!vaultPrompt) return;
                  setVaultBusy(true);
                  setVaultError(null);
                  try {
                    if (vaultPrompt.mode === "encrypt") {
                      const msgs = messages[vaultPrompt.sessionId] ?? [];
                      const plain = JSON.stringify(msgs);
                      await invoke("vault_encrypt_session", {
                        args: {
                          session_id: vaultPrompt.sessionId,
                          plaintext: plain,
                          password: vaultPassword,
                        },
                      });
                      await refreshEncrypted();
                      setVaultPrompt(null);
                      setVaultPassword("");
                    } else {
                      const text = await invoke<string>("vault_decrypt_session", {
                        args: {
                          session_id: vaultPrompt.sessionId,
                          password: vaultPassword,
                        },
                      });
                      const restored = JSON.parse(text) as PersistedMessage[];
                      setMessages(vaultPrompt.sessionId, restored);
                      setVaultPrompt(null);
                      setVaultPassword("");
                    }
                  } catch (e: any) {
                    setVaultError(String(e));
                  } finally {
                    setVaultBusy(false);
                  }
                }}
              >
                {vaultBusy ? "..." : vaultPrompt.mode === "encrypt" ? "加密" : "解锁"}
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}
