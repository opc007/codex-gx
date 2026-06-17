import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ThemeMode } from "../stores/theme";
import { useLocaleSwitcher, SUPPORTED_LOCALES, LOCALE_LABELS } from "../i18n";
import type { Locale } from "../i18n";
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
import {
  useCurrentWorkspaceId,
  useWorkspaceList,
  createWorkspace,
  switchWorkspace,
  deleteWorkspace,
  renameWorkspace,
} from "../stores/workspace";
import {
  useCurrentUser,
  useUserList,
  switchUser,
} from "../stores/users";
import { useSessionsStore } from "../stores/sessions";

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

type Props = {
  themeMode: ThemeMode;
  setThemeMode: (m: ThemeMode) => void;
};

export function TopBar({ themeMode, setThemeMode }: Props) {
  const [license, setLicense] = useState<LicenseSummary | null>(null);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [marketplaceOpen, setMarketplaceOpen] = useState(false);
  const [themeStudioOpen, setThemeStudioOpen] = useState(false);
  const [routingOpen, setRoutingOpen] = useState(false);
  const [bugOpen, setBugOpen] = useState(false);
  const [teamOpen, setTeamOpen] = useState(false);
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
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
  const [licenseOpen, setLicenseOpen] = useState(false);
  const { locale, setLocale } = useLocaleSwitcher();

  const settingsRef = useRef<HTMLDivElement | null>(null);

  const refreshLicense = async () => {
    try {
      const s = await invoke<LicenseSummary>("license_status");
      setLicense(s);
      // v1.6 联动：过期 / 离线宽限才自动弹 License（未激活只显示角标，不挡着用）
      if (
          s.status.kind === "expired" ||
          s.status.kind === "offlinegrace") {
        setLicenseOpen(true);
      }
      // v1.6 联动：只读模式广播
      const readonly =
        s.status.kind === "expired" ||
        s.status.kind === "offlinegrace" ||
        s.status.kind === "invalid";
      window.dispatchEvent(
        new CustomEvent("agentshell:license-readonly", {
          detail: { readonly, status: s.status },
        })
      );
    } catch {
      setLicense(null);
    }
  };

  useEffect(() => {
    void refreshLicense();
    // 监听 license 变更
    const unlistenP = listen("license:changed", () => void refreshLicense());
    return () => {
      void unlistenP.then((u) => u());
    };
  }, []);

  // v1.8：settings 菜单外部点击关闭
  useEffect(() => {
    if (!settingsOpen) return;
    const onDocClick = (e: MouseEvent) => {
      if (!settingsRef.current) return;
      if (!settingsRef.current.contains(e.target as Node)) {
        setSettingsOpen(false);
      }
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSettingsOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onEsc);
    };
  }, [settingsOpen]);

  // v1.8：Cycle theme helper (used in settings menu)
  const cycleTheme = () => {
    const next: ThemeMode =
      themeMode === "light" ? "dark" : themeMode === "dark" ? "system" : "light";
    setThemeMode(next);
  };

  // v1.8：Ping backend (used in settings menu)
  const pingBackend = async () => {
    try {
      const v = await invoke<string>("ping");
      alert(`Rust 后端回应：${v}`);
    } catch (e) {
      alert(`错误: ${e}`);
    }
  };

  // v1.8: Check for updates
  const checkUpdate = async () => {
    setUpdateBusy(true);
    try {
      const info = await invoke<UpdateInfo>("check_update");
      setUpdateInfo(info);
    } catch (e) {
      alert(`检查更新失败：${e}`);
    } finally {
      setUpdateBusy(false);
    }
  };

  const currentSession = useSessionsStore((s) =>
    s.sessions.find((x) => x.id === s.currentId)
  );

  return (
    <>
    <header className="topbar">
      <div className="topbar-left">
        <span className="topbar-logo" aria-hidden="true">✦</span>
        <span className="topbar-title">Codex gx</span>
        <WorkspaceSelector />
      </div>
      <div className="topbar-center">
        {currentSession ? currentSession.title : "Codex"}
      </div>
      <div className="topbar-right">
        <button
          className="topbar-btn license-badge"
          onClick={() => setLicenseOpen(true)}
          title="License 授权"
        >
          🔑 <span className="license-badge-text">{formatLicenseBadge(license)}</span>
        </button>
        <div className="settings-menu-wrap" ref={settingsRef}>
          <button
            className="topbar-btn icon-btn"
            onClick={() => setSettingsOpen((v) => !v)}
            title="设置"
            aria-haspopup="menu"
            aria-expanded={settingsOpen}
          >
            ⋯
          </button>
          {settingsOpen && (
            <SettingsMenu
              locale={locale}
              onChangeLocale={(l) => setLocale(l)}
              themeMode={themeMode}
              onCycleTheme={cycleTheme}
              onCheckUpdate={checkUpdate}
              updateBusy={updateBusy}
              onClose={() => setSettingsOpen(false)}
              onOpenMarketplace={() => { setSettingsOpen(false); setMarketplaceOpen(true); }}
              onOpenThemeStudio={() => { setSettingsOpen(false); setThemeStudioOpen(true); }}
              onOpenRouting={() => { setSettingsOpen(false); setRoutingOpen(true); }}
              onOpenBug={() => { setSettingsOpen(false); setBugOpen(true); }}
              onOpenTeam={() => { setSettingsOpen(false); setTeamOpen(true); }}
              onOpenLocal={() => { setSettingsOpen(false); setLocalOpen(true); }}
              onOpenReview={() => { setSettingsOpen(false); setReviewOpen(true); }}
              onOpenQueue={() => { setSettingsOpen(false); setQueueOpen(true); }}
              onOpenDevices={() => { setSettingsOpen(false); setDevicesOpen(true); }}
              onOpenLearning={() => { setSettingsOpen(false); setLearningOpen(true); }}
              onOpenSkills={() => { setSettingsOpen(false); setSkillsOpen(true); }}
              onOpenTts={() => { setSettingsOpen(false); setTtsOpen(true); }}
              onOpenFlow={() => { setSettingsOpen(false); setFlowOpen(true); }}
              onOpenSync={() => { setSettingsOpen(false); setSyncOpen(true); }}
              onOpenPlugins={() => { setSettingsOpen(false); setPluginOpen(true); }}
              onOpenLicense={() => { setSettingsOpen(false); setLicenseOpen(true); }}
              onPing={() => { setSettingsOpen(false); void pingBackend(); }}
            />
          )}
        </div>
        <UserMenu
          open={userMenuOpen}
          onToggle={() => setUserMenuOpen(!userMenuOpen)}
          onClose={() => setUserMenuOpen(false)}
          onOpenTeam={() => {
            setUserMenuOpen(false);
            setTeamOpen(true);
          }}
        />
      </div>
    </header>

      {updateInfo && (
        <div className="update-dialog-overlay" onClick={() => setUpdateInfo(null)}>
          <div className="update-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="update-dialog-header">
              <h3>{updateInfo.updateAvailable ? "🆕 有新版本可用" : "✓ 已是最新"}</h3>
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

      {/* v1.2: Plugin marketplace 弹窗 */}
      {marketplaceOpen && (
        <MarketplaceDialog onClose={() => setMarketplaceOpen(false)} />
      )}

      {/* v1.3: Theme Studio 弹窗 */}
      {themeStudioOpen && (
        <ThemeStudioDialog onClose={() => setThemeStudioOpen(false)} />
      )}

      {/* v1.3: Routing 弹窗 */}
      {routingOpen && (
        <RoutingEditorDialog onClose={() => setRoutingOpen(false)} />
      )}

      {/* v1.3: Bug Report 弹窗 */}
      {bugOpen && <BugReportDialog onClose={() => setBugOpen(false)} />}

      {/* v1.3: Team Panel 弹窗 */}
      {teamOpen && <TeamPanel onClose={() => setTeamOpen(false)} />}

      {/* v1.4: Local Model 弹窗 */}
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

      {/* v1.4: Code Review 弹窗 */}
      {reviewOpen && <CodeReviewDialog onClose={() => setReviewOpen(false)} />}

      {/* v1.4: Queue Panel 弹窗 */}
      {queueOpen && <QueuePanel onClose={() => setQueueOpen(false)} />}

      {/* v1.4: Devices Panel 弹窗 */}
      {devicesOpen && <DevicesPanel onClose={() => setDevicesOpen(false)} />}

      {/* v1.4: Learning Panel 弹窗 */}
      {learningOpen && <LearningPanel onClose={() => setLearningOpen(false)} />}

      {/* v1.5: Skills Library 弹窗 */}
      {skillsOpen && <SkillsLibraryDialog onClose={() => setSkillsOpen(false)} />}

      {/* v1.5: TTS 弹窗 */}
      {ttsOpen && <TtsPanel onClose={() => setTtsOpen(false)} />}

      {/* v1.5: Flow Graph 弹窗 */}
      {flowOpen && <FlowGraphView onClose={() => setFlowOpen(false)} />}

      {/* v1.5: Sync Panel 弹窗 */}
      {syncOpen && <SyncPanel onClose={() => setSyncOpen(false)} />}

      {/* v1.5: Plugin Panel 弹窗 */}
      {pluginOpen && <PluginPanel onClose={() => setPluginOpen(false)} />}
      {/* v1.6: License Panel 弹窗 */}
      {licenseOpen && <LicensePanel onClose={() => setLicenseOpen(false)} />}
    </>
  );
}

// v1.6: License 状态 → TopBar 角标
function formatLicenseBadge(license: LicenseSummary | null): string {
  if (!license) return "检测中…";
  const s = license.status;
  switch (s.kind) {
    case "unactivated":
      return "未激活";
    case "valid":
      if (s.remaining_days == null) return "终身";
      return `还剩 ${s.remaining_days} 天`;
    case "expiring":
      return `临期 ${s.days_left} 天`;
    case "expired":
      return "已过期";
    case "offlinegrace":
      return `离线 ${s.days_offline} 天`;
    case "invalid":
      return "异常";
  }
}

// ------------------ v1.8: Settings menu (Codex style) ------------------

type SettingsMenuProps = {
  locale: Locale;
  onChangeLocale: (l: Locale) => void;
  themeMode: ThemeMode;
  onCycleTheme: () => void;
  onCheckUpdate: () => void;
  updateBusy: boolean;
  onClose: () => void;
  onOpenMarketplace: () => void;
  onOpenThemeStudio: () => void;
  onOpenRouting: () => void;
  onOpenBug: () => void;
  onOpenTeam: () => void;
  onOpenLocal: () => void;
  onOpenReview: () => void;
  onOpenQueue: () => void;
  onOpenDevices: () => void;
  onOpenLearning: () => void;
  onOpenSkills: () => void;
  onOpenTts: () => void;
  onOpenFlow: () => void;
  onOpenSync: () => void;
  onOpenPlugins: () => void;
  onOpenLicense: () => void;
  onPing: () => void;
};

function SettingsMenu(props: SettingsMenuProps) {
  const themeLabel =
    props.themeMode === "system" ? "跟随系统" : props.themeMode === "light" ? "白天" : "夜晚";
  const themeIcon = props.themeMode === "light" ? "☀️" : props.themeMode === "dark" ? "🌙" : "🖥️";

  return (
    <div className="settings-menu" role="menu">
      <div className="settings-menu-section">能力</div>
      <MenuItem icon="🧩" label="插件市场" onClick={props.onOpenMarketplace} />
      <MenuItem icon="🎨" label="主题工作室" onClick={props.onOpenThemeStudio} />
      <MenuItem icon="🧭" label="路由策略" onClick={props.onOpenRouting} />
      <MenuItem icon="👥" label="团队 / 用户" onClick={props.onOpenTeam} />

      <div className="settings-menu-section">开发</div>
      <MenuItem icon="🏠" label="本地 LLM" onClick={props.onOpenLocal} />
      <MenuItem icon="🔍" label="代码 review" onClick={props.onOpenReview} />
      <MenuItem icon="📋" label="任务队列" onClick={props.onOpenQueue} />
      <MenuItem icon="📡" label="设备协同 (P2P)" onClick={props.onOpenDevices} />
      <MenuItem icon="🧠" label="Agent 学习" onClick={props.onOpenLearning} />
      <MenuItem icon="📚" label="Skills 库" onClick={props.onOpenSkills} />
      <MenuItem icon="🔊" label="语音输出 TTS" onClick={props.onOpenTts} />
      <MenuItem icon="🕸️" label="流程图" onClick={props.onOpenFlow} />
      <MenuItem icon="☁️" label="Session 同步" onClick={props.onOpenSync} />
      <MenuItem icon="🧩" label="插件热加载" onClick={props.onOpenPlugins} />
      <MenuItem icon="🐞" label="Bug 报告" onClick={props.onOpenBug} />

      <div className="settings-menu-section">系统</div>
      <MenuItem icon="🔐" label="License 授权" onClick={props.onOpenLicense} />
      <MenuItem
        icon="🆕"
        label={props.updateBusy ? "检查更新…" : "检查更新"}
        onClick={props.onCheckUpdate}
      />
      <button
        className="settings-menu-item"
        onClick={props.onCycleTheme}
        role="menuitem"
      >
        <span className="settings-menu-icon">{themeIcon}</span>
        <span>主题</span>
        <span className="settings-menu-extra">{themeLabel}</span>
      </button>
      <div className="settings-menu-locale">
        <span className="settings-menu-icon">🌐</span>
        <span style={{ flex: 1 }}>语言</span>
        <select
          className="topbar-select"
          value={props.locale}
          onChange={(e) => props.onChangeLocale(e.target.value as Locale)}
          onClick={(e) => e.stopPropagation()}
        >
          {SUPPORTED_LOCALES.map((l) => (
            <option key={l} value={l}>{LOCALE_LABELS[l]}</option>
          ))}
        </select>
      </div>
      {import.meta.env.DEV && (
        <MenuItem icon="🛠" label="Ping 后端 (dev)" onClick={props.onPing} />
      )}
    </div>
  );
}

function MenuItem({
  icon,
  label,
  onClick,
}: {
  icon: string;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className="settings-menu-item" onClick={onClick} role="menuitem">
      <span className="settings-menu-icon">{icon}</span>
      <span>{label}</span>
    </button>
  );
}

// ------------------ v1.3：Workspace selector ------------------

function WorkspaceSelector() {
  const currentId = useCurrentWorkspaceId();
  const list = useWorkspaceList();
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);

  const handleCreate = () => {
    const name = prompt("新建工作区名称：", `Workspace ${list.length + 1}`);
    if (name === null) return;
    const meta = createWorkspace(name);
    // 触发 session 列表刷新：广播事件
    void invoke("workspace_changed_broadcast", { workspaceId: meta.id });
  };

  const handleDelete = (id: string, name: string) => {
    if (id === "default") {
      alert("默认工作区不能删除");
      return;
    }
    if (!confirm(`确认删除工作区「${name}」？\n（其中的 session 不会被删除，可在切换回 default 时看到）`)) return;
    deleteWorkspace(id);
    void invoke("workspace_changed_broadcast", { workspaceId: "default" });
  };

  const handleRename = (id: string, name: string) => {
    setEditing({ id, name });
  };

  const commitRename = () => {
    if (editing) {
      renameWorkspace(editing.id, editing.name);
      setEditing(null);
    }
  };

  const current = list.find((w) => w.id === currentId) ?? list[0];

  return (
    <div className="workspace-selector">
      <select
        className="topbar-select workspace-select"
        value={currentId}
        onChange={(e) => {
          switchWorkspace(e.target.value);
          void invoke("workspace_changed_broadcast", {
            workspaceId: e.target.value,
          });
        }}
        title="当前工作区"
      >
        {list.map((w) => (
          <option key={w.id} value={w.id}>
            📁 {w.name}
          </option>
        ))}
      </select>
      <button
        className="topbar-btn small"
        onClick={handleCreate}
        title="新建工作区"
      >
        ＋
      </button>
      {current && current.id !== "default" && (
        <>
          <button
            className="topbar-btn small"
            onClick={() => handleRename(current.id, current.name)}
            title="重命名"
          >
            ✎
          </button>
          <button
            className="topbar-btn small danger"
            onClick={() => handleDelete(current.id, current.name)}
            title="删除工作区"
          >
            ×
          </button>
        </>
      )}

      {editing && (
        <div className="modal-mask" onClick={() => setEditing(null)}>
          <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>重命名工作区</h3>
            <input
              className="vault-password-input"
              value={editing.name}
              onChange={(e) => setEditing({ ...editing, name: e.target.value })}
              autoFocus
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename();
                if (e.key === "Escape") setEditing(null);
              }}
            />
            <div className="modal-actions">
              <button className="btn" onClick={() => setEditing(null)}>
                取消
              </button>
              <button className="btn primary" onClick={commitRename}>
                保存
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ------------------ v1.3: User Menu ------------------

type UserMenuProps = {
  open: boolean;
  onToggle: () => void;
  onClose: () => void;
  onOpenTeam: () => void;
};

function UserMenu({ open, onToggle, onClose, onOpenTeam }: UserMenuProps) {
  const current = useCurrentUser();
  const list = useUserList();
  return (
    <div className="user-menu-wrap">
      <button
        className="user-avatar-btn"
        onClick={onToggle}
        title={`当前用户: ${current.displayName}`}
        style={{ background: current.color }}
      >
        <span>{current.emoji}</span>
      </button>
      {open && (
        <>
          <div className="user-menu-mask" onClick={onClose} />
          <div className="user-menu">
            <div className="user-menu-current">
              <div
                className="team-avatar"
                style={{ background: current.color }}
              >
                {current.emoji}
              </div>
              <div>
                <div style={{ fontWeight: 500 }}>{current.displayName}</div>
                <div style={{ fontSize: 11, color: "var(--text-muted)" }}>
                  {current.role}
                </div>
              </div>
            </div>
            <div className="user-menu-section">切换用户</div>
            {list.map((u) => (
              <button
                key={u.id}
                className={`user-menu-item ${u.id === current.id ? "active" : ""}`}
                onClick={() => {
                  switchUser(u.id);
                  onClose();
                }}
              >
                <span
                  className="team-avatar small"
                  style={{ background: u.color }}
                >
                  {u.emoji}
                </span>
                <span>{u.displayName}</span>
                {u.id === current.id && <span className="check">✓</span>}
              </button>
            ))}
            <div className="user-menu-section" />
            <button
              className="user-menu-item"
              onClick={() => {
                onClose();
                onOpenTeam();
              }}
            >
              ⚙️ 团队管理...
            </button>
          </div>
        </>
      )}
    </div>
  );
}
