import { useSessionsStore, type SessionMeta, type PersistedMessage } from "../stores/sessions";
import { exportSession, type ExportFormat } from "../lib/export";
import { useState, useEffect, useRef } from "react";
import { closeTab } from "../stores/tabs";
import { useCurrentWorkspaceId } from "../stores/workspace";
import { useCurrentUser, useUserList, switchUser } from "../stores/users";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { MarketplaceDialog } from "./MarketplaceDialog";
import { ThemeStudioDialog } from "./ThemeStudioDialog";
import { RoutingEditorDialog } from "./RoutingEditorDialog";
import { BugReportDialog } from "./BugReportDialog";
import { TeamPanel } from "./TeamPanel";
import { LocalModelDialog } from "./LocalModelDialog";
import { CodeReviewDialog } from "./CodeReviewDialog";
import { QueuePanel } from "./QueuePanel";
import { DevicesPanel } from "./DevicesPanel";
import { LearningPanel } from "./LearningPanel";
import { SkillsLibraryDialog } from "./SkillsLibraryDialog";
import { TtsPanel } from "./TtsPanel";
import { FlowGraphView } from "./FlowGraphView";
import { SyncPanel } from "./SyncPanel";
import { PluginPanel } from "./PluginPanel";
import { LicensePanel } from "./LicensePanel";
import { ApiKeysDialog } from "./ApiKeysDialog";
import { useThemeMode, type ThemeMode } from "../stores/theme";
import { useLocaleSwitcher, SUPPORTED_LOCALES, LOCALE_LABELS } from "../i18n";
import type { Locale } from "../i18n";

type UpdateInfo = {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  releaseUrl: string | null;
  releaseNotes: string | null;
};

type LicenseStatusKind =
  | { kind: "unactivated" }
  | { kind: "valid"; tier: string; remaining_days: number | null; activated_at: number; expires_at: number | null }
  | { kind: "expiring"; tier: string; days_left: number }
  | { kind: "expired"; tier: string; expired_at: number }
  | { kind: "offlinegrace"; days_offline: number }
  | { kind: "invalid"; reason: string };

type LicenseSummary = {
  status: LicenseStatusKind;
  last_validated_at: number;
  offline: boolean;
};

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
  const userList = useUserList();
  const [themeMode, setThemeMode] = useThemeMode();
  const { locale, setLocale } = useLocaleSwitcher();

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

  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const userMenuRef = useRef<HTMLDivElement | null>(null);

  const [license, setLicense] = useState<LicenseSummary | null>(null);
  const [apiKeysOpen, setApiKeysOpen] = useState(false);
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [marketplaceOpen, setMarketplaceOpen] = useState(false);
  const [themeStudioOpen, setThemeStudioOpen] = useState(false);
  const [routingOpen, setRoutingOpen] = useState(false);
  const [bugOpen, setBugOpen] = useState(false);
  const [teamOpen, setTeamOpen] = useState(false);
  const [localOpen, setLocalOpen] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [queueOpen, setQueueOpen] = useState(false);
  const [devicesOpen, setDevicesOpen] = useState(false);
  const [learningOpen, setLearningOpen] = useState(false);
  const [skillsOpen, setSkillsOpen] = useState(false);
  const [ttsOpen, setTtsOpen] = useState(false);
  const [flowOpen, setFlowOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [pluginOpen, setPluginOpen] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);

  const sessions = allSessions
    .filter((sess) => (sess.workspaceId ?? "default") === currentWorkspace)
    .slice()
    .sort((a, b) => b.updatedAt - a.updatedAt);

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

  const refreshLicense = async () => {
    try {
      const s = await invoke<LicenseSummary>("license_status");
      setLicense(s);
    } catch {
      setLicense(null);
    }
  };

  useEffect(() => {
    void refreshLicense();
    const unlistenP = listen("license:changed", () => void refreshLicense());
    return () => {
      void unlistenP.then((u) => u());
    };
  }, []);

  // 首次启动：未配置任何 Key 时引导填写
  useEffect(() => {
    void invoke<{
      minimax_configured: boolean;
      deepseek_configured: boolean;
      anthropic_configured: boolean;
      openai_configured: boolean;
    }>("api_keys_status")
      .then((s) => {
        const any =
          s.minimax_configured ||
          s.deepseek_configured ||
          s.anthropic_configured ||
          s.openai_configured;
        if (!any) setApiKeysOpen(true);
      })
      .catch(() => {});
    const onOpenApiKeys = () => setApiKeysOpen(true);
    window.addEventListener("open-api-keys", onOpenApiKeys);
    return () => window.removeEventListener("open-api-keys", onOpenApiKeys);
  }, []);

  // user menu outside click + esc
  useEffect(() => {
    if (!userMenuOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (!userMenuRef.current) return;
      if (!userMenuRef.current.contains(e.target as Node)) setUserMenuOpen(false);
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setUserMenuOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onEsc);
    };
  }, [userMenuOpen]);

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

  const cycleTheme = () => {
    const next: ThemeMode =
      themeMode === "light" ? "dark" : themeMode === "dark" ? "system" : "light";
    setThemeMode(next);
  };

  const checkUpdate = async () => {
    try {
      const info = await invoke<UpdateInfo>("check_update");
      setUpdateInfo(info);
    } catch (e) {
      alert(`检查更新失败：${e}`);
    }
  };

  const themeIcon = themeMode === "light" ? "☀️" : themeMode === "dark" ? "🌙" : "🖥️";
  const themeLabel = themeMode === "light" ? "白天" : themeMode === "dark" ? "夜晚" : "系统";

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

      <div className="sidebar-bottom" ref={userMenuRef}>
        <button
          className="sidebar-user"
          onClick={() => setUserMenuOpen((v) => !v)}
          title={`${currentUser.displayName} · 点击打开菜单`}
        >
          <div
            className="team-avatar small"
            style={{ background: currentUser.color }}
          >
            {currentUser.emoji}
          </div>
          <div style={{ flex: 1, minWidth: 0, textAlign: "left" }}>
            <div className="sidebar-user-name">{currentUser.displayName}</div>
            <div className="sidebar-user-role">
              {licenseBadgeText(license)}
            </div>
          </div>
          <span className="sidebar-user-chevron" aria-hidden="true">
            ▾
          </span>
        </button>

        {userMenuOpen && (
          <div className="user-menu user-menu-anchor" role="menu">
            <div className="user-menu-current">
              <div
                className="team-avatar"
                style={{ background: currentUser.color }}
              >
                {currentUser.emoji}
              </div>
              <div>
                <div style={{ fontWeight: 500 }}>{currentUser.displayName}</div>
                <div style={{ fontSize: 11, color: "var(--text-muted)" }}>
                  {currentUser.role}
                </div>
              </div>
            </div>

            <div className="user-menu-section">常用</div>
            <UserMenuItem
              icon="🔑"
              label="API Key 设置"
              onClick={() => {
                setUserMenuOpen(false);
                setApiKeysOpen(true);
              }}
            />
            <UserMenuItem
              icon="🔐"
              label="License 授权"
              onClick={() => {
                setUserMenuOpen(false);
                setLicenseOpen(true);
              }}
            />
            <UserMenuItem
              icon="🧩"
              label="插件市场"
              onClick={() => {
                setUserMenuOpen(false);
                setMarketplaceOpen(true);
              }}
            />
            <UserMenuItem
              icon="🎨"
              label="主题工作室"
              onClick={() => {
                setUserMenuOpen(false);
                setThemeStudioOpen(true);
              }}
            />
            <UserMenuItem
              icon="🧭"
              label="路由策略"
              onClick={() => {
                setUserMenuOpen(false);
                setRoutingOpen(true);
              }}
            />

            <div className="user-menu-section">开发</div>
            <UserMenuItem
              icon="🏠"
              label="本地 LLM"
              onClick={() => {
                setUserMenuOpen(false);
                setLocalOpen(true);
              }}
            />
            <UserMenuItem
              icon="🔍"
              label="代码 review"
              onClick={() => {
                setUserMenuOpen(false);
                setReviewOpen(true);
              }}
            />
            <UserMenuItem
              icon="📋"
              label="任务队列"
              onClick={() => {
                setUserMenuOpen(false);
                setQueueOpen(true);
              }}
            />
            <UserMenuItem
              icon="📡"
              label="设备协同 (P2P)"
              onClick={() => {
                setUserMenuOpen(false);
                setDevicesOpen(true);
              }}
            />
            <UserMenuItem
              icon="🧠"
              label="Agent 学习"
              onClick={() => {
                setUserMenuOpen(false);
                setLearningOpen(true);
              }}
            />
            <UserMenuItem
              icon="📚"
              label="Skills 库"
              onClick={() => {
                setUserMenuOpen(false);
                setSkillsOpen(true);
              }}
            />
            <UserMenuItem
              icon="🔊"
              label="语音输出 TTS"
              onClick={() => {
                setUserMenuOpen(false);
                setTtsOpen(true);
              }}
            />
            <UserMenuItem
              icon="🕸️"
              label="流程图"
              onClick={() => {
                setUserMenuOpen(false);
                setFlowOpen(true);
              }}
            />
            <UserMenuItem
              icon="☁️"
              label="Session 同步"
              onClick={() => {
                setUserMenuOpen(false);
                setSyncOpen(true);
              }}
            />
            <UserMenuItem
              icon="🧩"
              label="插件热加载"
              onClick={() => {
                setUserMenuOpen(false);
                setPluginOpen(true);
              }}
            />
            <UserMenuItem
              icon="🐞"
              label="Bug 报告"
              onClick={() => {
                setUserMenuOpen(false);
                setBugOpen(true);
              }}
            />
            <UserMenuItem
              icon="👥"
              label="团队 / 用户"
              onClick={() => {
                setUserMenuOpen(false);
                setTeamOpen(true);
              }}
            />

            <div className="user-menu-section">系统</div>
            <UserMenuItem
              icon="🆕"
              label="检查更新"
              onClick={() => {
                setUserMenuOpen(false);
                void checkUpdate();
              }}
            />
            <button
              className="user-menu-item"
              onClick={() => {
                cycleTheme();
              }}
              role="menuitem"
            >
              <span className="user-menu-icon">{themeIcon}</span>
              <span>主题</span>
              <span className="user-menu-extra">{themeLabel}</span>
            </button>
            <div className="user-menu-locale">
              <span className="user-menu-icon">🌐</span>
              <span style={{ flex: 1 }}>语言</span>
              <select
                className="topbar-select"
                value={locale}
                onChange={(e) => setLocale(e.target.value as Locale)}
                onClick={(e) => e.stopPropagation()}
              >
                {SUPPORTED_LOCALES.map((l) => (
                  <option key={l} value={l}>
                    {LOCALE_LABELS[l]}
                  </option>
                ))}
              </select>
            </div>

            <div className="user-menu-section">切换用户</div>
            {userList.map((u) => (
              <button
                key={u.id}
                className={`user-menu-item ${u.id === currentUser.id ? "active" : ""}`}
                onClick={() => {
                  switchUser(u.id);
                  setUserMenuOpen(false);
                }}
                role="menuitem"
              >
                <span
                  className="team-avatar small"
                  style={{ background: u.color }}
                >
                  {u.emoji}
                </span>
                <span>{u.displayName}</span>
                {u.id === currentUser.id && <span className="check">✓</span>}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Dialogs */}
      {apiKeysOpen && (
        <ApiKeysDialog onClose={() => setApiKeysOpen(false)} />
      )}
      {licenseOpen && (
        <LicensePanel onClose={() => setLicenseOpen(false)} />
      )}
      {marketplaceOpen && (
        <MarketplaceDialog onClose={() => setMarketplaceOpen(false)} />
      )}
      {themeStudioOpen && (
        <ThemeStudioDialog onClose={() => setThemeStudioOpen(false)} />
      )}
      {routingOpen && (
        <RoutingEditorDialog onClose={() => setRoutingOpen(false)} />
      )}
      {bugOpen && <BugReportDialog onClose={() => setBugOpen(false)} />}
      {teamOpen && <TeamPanel onClose={() => setTeamOpen(false)} />}
      {localOpen && (
        <LocalModelDialog
          onClose={() => setLocalOpen(false)}
          onUseModel={(id) => {
            navigator.clipboard.writeText(id).catch(() => {});
            alert(
              `模型 ID: ${id}\n\n已复制到剪贴板。\n\n在 Composer 模型下拉 / 路由策略里用此 ID。`,
            );
          }}
        />
      )}
      {reviewOpen && (
        <CodeReviewDialog onClose={() => setReviewOpen(false)} />
      )}
      {queueOpen && <QueuePanel onClose={() => setQueueOpen(false)} />}
      {devicesOpen && <DevicesPanel onClose={() => setDevicesOpen(false)} />}
      {learningOpen && (
        <LearningPanel onClose={() => setLearningOpen(false)} />
      )}
      {skillsOpen && (
        <SkillsLibraryDialog onClose={() => setSkillsOpen(false)} />
      )}
      {ttsOpen && <TtsPanel onClose={() => setTtsOpen(false)} />}
      {flowOpen && <FlowGraphView onClose={() => setFlowOpen(false)} />}
      {syncOpen && <SyncPanel onClose={() => setSyncOpen(false)} />}
      {pluginOpen && <PluginPanel onClose={() => setPluginOpen(false)} />}

      {updateInfo && (
        <div className="update-dialog-overlay" onClick={() => setUpdateInfo(null)}>
          <div className="update-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="update-dialog-header">
              <h3>
                {updateInfo.updateAvailable ? "🆕 有新版本可用" : "✓ 已是最新"}
              </h3>
              <button className="update-close" onClick={() => setUpdateInfo(null)}>×</button>
            </div>
            <div className="update-dialog-body">
              <p>当前版本: <code>{updateInfo.currentVersion}</code></p>
              {updateInfo.latestVersion && (
                <p>最新版本: <code>{updateInfo.latestVersion}</code></p>
              )}
              {updateInfo.releaseNotes && (
                <details>
                  <summary>更新说明</summary>
                  <pre>{updateInfo.releaseNotes.slice(0, 2000)}</pre>
                </details>
              )}
            </div>
            <div className="update-dialog-footer">
              <button className="update-cancel" onClick={() => setUpdateInfo(null)}>关闭</button>
              {updateInfo.updateAvailable && updateInfo.releaseUrl && (
                <button
                  className="update-go"
                  onClick={async () => {
                    if (updateInfo.releaseUrl) {
                      try {
                        await openUrl(updateInfo.releaseUrl);
                      } catch (e) {
                        alert(`打开失败：${e}`);
                      }
                    }
                  }}
                >
                  前往下载
                </button>
              )}
            </div>
          </div>
        </div>
      )}

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

function UserMenuItem({
  icon,
  label,
  onClick,
}: {
  icon: string;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className="user-menu-item" onClick={onClick} role="menuitem">
      <span className="user-menu-icon">{icon}</span>
      <span>{label}</span>
    </button>
  );
}

function licenseBadgeText(license: LicenseSummary | null): string {
  if (!license) return "检测中…";
  const s = license.status;
  switch (s.kind) {
    case "unactivated":
      return "未激活 · 点击设置";
    case "valid":
      if (s.remaining_days == null) return "已授权 · 终身";
      return `已授权 · 还剩 ${s.remaining_days} 天`;
    case "expiring":
      return `临期 ${s.days_left} 天 · 点击续费`;
    case "expired":
      return "已过期 · 点击续费";
    case "offlinegrace":
      return `离线 ${s.days_offline} 天 · 请联网`;
    case "invalid":
      return "授权异常 · 点击修复";
  }
}
