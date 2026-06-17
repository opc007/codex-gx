import { useSessionsStore, type SessionMeta, type PersistedMessage } from "../stores/sessions";
import { exportSession, type ExportFormat } from "../lib/export";
import { useState, useEffect } from "react";
import { useOpenTabs, openTab, closeTab, closeOtherTabs, closeAllTabs } from "../stores/tabs";
import { useCurrentWorkspaceId } from "../stores/workspace";
import { invoke } from "@tauri-apps/api/core";

export function Sidebar() {
  const allSessions = useSessionsStore((s) => s.sessions);
  const currentId = useSessionsStore((s) => s.currentId);
  const setCurrent = useSessionsStore((s) => s.setCurrent);
  const create = useSessionsStore((s) => s.create);
  const remove = useSessionsStore((s) => s.remove);
  const messages = useSessionsStore((s) => s.messages);
  const setMessages = useSessionsStore((s) => s.setMessages);
  const openTabs = useOpenTabs();
  const currentWorkspace = useCurrentWorkspaceId();
  // v1.3：按 workspace 过滤
  const sessions = allSessions.filter(
    (sess) => (sess.workspaceId ?? "default") === currentWorkspace,
  );

  const [exportOpen, setExportOpen] = useState<string | null>(null);
  const [tabMenuOpen, setTabMenuOpen] = useState(false);
  const [redactOnExport, setRedactOnExport] = useState(true);

  // v1.2：vault 加密
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
    } catch (e) {
      // 后端未启动等
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

  const tabSessions = openTabs
    .map((id) => sessions.find((s) => s.id === id))
    .filter((s): s is SessionMeta => Boolean(s));

  return (
    <aside className="sidebar">
      {/* v1.1：标签栏 */}
      {tabSessions.length > 0 && (
        <div className="tab-bar">
          <div className="tab-list">
            {tabSessions.map((s) => (
              <div
                key={s.id}
                className={`tab ${s.id === currentId ? "active" : ""}`}
                onClick={() => setCurrent(s.id)}
                title={s.title}
              >
                <span className="tab-title">
                  {s.title.length > 12 ? s.title.slice(0, 12) + "…" : s.title}
                </span>
                <button
                  className="tab-close"
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTab(s.id);
                  }}
                  title="关闭标签"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
          <div className="tab-menu-wrap">
            <button
              className="tab-menu-btn"
              onClick={() => setTabMenuOpen(!tabMenuOpen)}
              title="标签管理"
            >
              ⋯
            </button>
            {tabMenuOpen && (
              <div className="tab-menu" onClick={() => setTabMenuOpen(false)}>
                {currentId && (
                  <button onClick={() => closeOtherTabs(currentId)}>
                    关闭其他
                  </button>
                )}
                <button onClick={closeAllTabs}>关闭所有</button>
              </div>
            )}
          </div>
        </div>
      )}

      <div className="sidebar-header">
        <span>会话 ({sessions.length})</span>
        <button
          className="sidebar-new"
          onClick={() => {
            const s = create();
            openTab(s.id);
          }}
          title="新建会话 (并打开标签)"
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
            className={`session-item ${s.id === currentId ? "active" : ""} ${
              openTabs.includes(s.id) ? "tabbed" : ""
            }`}
            onClick={() => {
              setCurrent(s.id);
              openTab(s.id); // 点击侧边栏自动 pin 到 tab
            }}
          >
            <span className="session-title">{s.title}</span>
            <div className="session-actions">
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
                  if (confirm(`删除 "${s.title}"？`)) {
                    remove(s.id);
                    closeTab(s.id);
                  }
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
          </li>
        ))}
      </ul>

      {vaultPrompt && (
        <div className="update-dialog-overlay" onClick={() => setVaultPrompt(null)}>
          <div className="update-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="update-dialog-header">
              <h2>
                {vaultPrompt.mode === "encrypt" ? "🔒 加密 Session" : "🔓 解锁 Session"}
              </h2>
              <button className="update-cancel" onClick={() => setVaultPrompt(null)}>×</button>
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
                      // 解密 — 把解密后的 JSON 写回 messages
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