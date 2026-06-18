import { useEffect, useState, Component, type ReactNode, type ErrorInfo } from "react";
import { Sidebar } from "./components/Sidebar";
import { Thread } from "./components/Thread";
import { Composer } from "./components/Composer";
import { TopBar } from "./components/TopBar";
import { ActivationGate } from "./components/ActivationGate";
import { ApprovalDialog, type ApprovalRequest } from "./components/ApprovalDialog";
import PlanDialog, {
  type PlanRequest,
  respondPlan,
} from "./components/PlanDialog";
import { useThemeMode, type ThemeMode } from "./stores/theme";
import { useSessionsStore, getSessionsState } from "./stores/sessions";
import { getCurrentWorkspaceId } from "./stores/workspace";
import { useOpenTabs, closeTab } from "./stores/tabs";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type LicenseStatusKind =
  | { kind: "unactivated" }
  | { kind: "trial"; remaining_days: number | null; started_at: number }
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

// v1.3：全局错误上报
function reportError(source: string, severity: string, message: string, stack?: string) {
  try {
    void invoke("bug_report_record", {
      args: {
        source,
        severity,
        message,
        stack: stack ?? null,
        session_id: null,
        model: null,
        context: null,
        user_note: null,
      },
    });
  } catch (e) {
    console.warn("report error failed:", e);
  }
}

if (typeof window !== "undefined") {
  window.addEventListener("error", (e) => {
    reportError("frontend", "error", String(e.message), e.error?.stack);
  });
  window.addEventListener("unhandledrejection", (e) => {
    const reasonStr =
      e.reason instanceof Error ? e.reason.message : String(e.reason);
    const reasonStack = e.reason instanceof Error ? e.reason.stack : undefined;
    reportError("promise", "error", reasonStr, reasonStack);
  });
}

// v1.3：React error boundary
class AppErrorBoundary extends Component<
  { children: ReactNode },
  { error: Error | null }
> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  componentDidCatch(error: Error, info: ErrorInfo) {
    reportError(
      "react",
      "fatal",
      `React render error: ${error.message}`,
      `${error.stack ?? ""}\n\nComponent stack:\n${info.componentStack ?? ""}`,
    );
  }
  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: 32,
            color: "var(--text)",
            background: "var(--bg)",
            minHeight: "100vh",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <h1 style={{ color: "var(--danger)" }}>💥 应用出错了</h1>
          <p style={{ color: "var(--text-muted)", maxWidth: 600, textAlign: "center" }}>
            错误已被记录到本地 crash log。可点击 🐞 按钮查看并提交 Issue。
          </p>
          <pre
            style={{
              background: "var(--bg-secondary)",
              padding: 16,
              borderRadius: 6,
              maxWidth: 800,
              maxHeight: 300,
              overflow: "auto",
              fontSize: 12,
              marginTop: 16,
            }}
          >
            {this.state.error.stack ?? this.state.error.message}
          </pre>
          <button
            className="btn primary"
            style={{ marginTop: 16 }}
            onClick={() => {
              this.setState({ error: null });
              window.location.reload();
            }}
          >
            重新加载
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

export default function App() {
  const [themeMode, setThemeMode] = useThemeMode();
  const currentId = useSessionsStore((s) => s.currentId);
  const [approvalReq, setApprovalReq] = useState<ApprovalRequest | null>(null);
  // v0.6：plan mode
  const [planReq, setPlanReq] = useState<PlanRequest | null>(null);
  // v1.9.x：未激活 / 试用已结束 → 显示激活门
  const [licenseStatus, setLicenseStatus] = useState<LicenseStatusKind | null>(null);

  useEffect(() => {
    const refreshLicense = async () => {
      try {
        const s = await invoke<LicenseSummary>("license_status");
        setLicenseStatus(s.status);
      } catch {
        setLicenseStatus(null);
      }
    };
    void refreshLicense();
    const unlistenP = listen("license:changed", () => void refreshLicense());
    return () => {
      void unlistenP.then((u) => u());
    };
  }, []);

  const isBlocking =
    licenseStatus !== null &&
    (licenseStatus.kind === "unactivated" ||
      (licenseStatus.kind === "trial" && licenseStatus.remaining_days === null));

  // v1.9.6：侧栏折叠状态（Codex 风格 Cmd+B）
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  // v1.9.6：键盘快捷键（Codex App 风格）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      // 输入框内不响应（除了 Cmd+K 强制聚焦）
      const target = e.target as HTMLElement | null;
      const inInput =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);
      switch (e.key.toLowerCase()) {
        case "b":
          if (!inInput) {
            e.preventDefault();
            setSidebarCollapsed((v) => !v);
          }
          break;
        case "k":
          // Cmd+K 总是聚焦 composer
          e.preventDefault();
          document
            .querySelector<HTMLTextAreaElement>(".composer-input")
            ?.focus();
          break;
        case "n":
          if (!inInput) {
            e.preventDefault();
            const s = getSessionsState();
            const fresh = s.create();
            s.setCurrent(fresh.id);
          }
          break;
        case "m":
          if (e.metaKey || e.ctrlKey) {
            e.preventDefault();
            // v1.9.6：Codex 风格 Cmd+M 触发语音输入（开始 / 停止）
            window.dispatchEvent(new CustomEvent("agentshell:toggle-voice"));
          }
          break;
        case "j":
          if (!inInput) {
            e.preventDefault();
            window.dispatchEvent(new CustomEvent("agentshell:toggle-terminal"));
          }
          break;
        case "[":
        case "]":
          if (!inInput) {
            e.preventDefault();
            const st = getSessionsState();
            const wsId = getCurrentWorkspaceId();
            const order = st.sessions
              .filter((sess) => (sess.workspaceId ?? "default") === wsId)
              .sort((a, b) => b.updatedAt - a.updatedAt);
            const idx = order.findIndex((sess) => sess.id === st.currentId);
            if (order.length === 0) return;
            const next =
              e.key === "]"
                ? order[(idx + 1) % order.length]
                : order[(idx - 1 + order.length) % order.length];
            if (next) st.setCurrent(next.id);
          }
          break;
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  // 跟随系统
  useEffect(() => {
    if (themeMode !== "system") return;
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = mql.matches ? "dark" : "light";
    };
    apply();
    mql.addEventListener("change", apply);
    return () => mql.removeEventListener("change", apply);
  }, [themeMode]);

  useEffect(() => {
    if (themeMode === "system") return;
    document.documentElement.dataset.theme = themeMode;
  }, [themeMode]);

  useEffect(() => {
    const handler = (e: Event) => {
      const mode = (e as CustomEvent).detail as string;
      if (["light", "dark", "system"].includes(mode)) {
        setThemeMode(mode as ThemeMode);
      }
    };
    window.addEventListener("agentshell:theme", handler);
    return () => window.removeEventListener("agentshell:theme", handler);
  }, [setThemeMode]);

  // v0.4 + v0.6：监听 approval_request / plan
  useEffect(() => {
    const unlistenP = listen<{
      sessionId: string;
      kind: string;
      toolCall: {
        id: string;
        name: string;
        arguments: unknown;
        sessionId: string;
      } | null;
      plan: {
        plan: string;
        planId: string;
      } | null;
    }>("agent:event", (event) => {
      const p = event.payload;
      if (p.kind === "approval_request" && p.toolCall) {
        setApprovalReq({
          sessionId: p.toolCall.sessionId,
          toolCallId: p.toolCall.id,
          name: p.toolCall.name,
          arguments: p.toolCall.arguments,
        });
      }
      // v0.6：plan 事件
      if (p.kind === "plan" && p.plan) {
        setPlanReq({
          sessionId: p.sessionId,
          plan: p.plan.plan,
          planId: p.plan.planId,
        });
      }
    });
    return () => {
      void unlistenP.then((u) => u());
    };
  }, []);

  const onApprove = async () => {
    if (!approvalReq) return;
    try {
      await invoke("respond_approval", {
        sessionId: approvalReq.sessionId,
        approve: true,
      });
    } catch (e) {
      console.warn("approve failed:", e);
    }
    setApprovalReq(null);
  };

  const onDeny = async (reason: string) => {
    if (!approvalReq) return;
    try {
      await invoke("respond_approval", {
        sessionId: approvalReq.sessionId,
        approve: false,
        reason,
      });
    } catch (e) {
      console.warn("deny failed:", e);
    }
    setApprovalReq(null);
  };

  // v0.6：plan 回调
  const onPlanApprove = async () => {
    if (!planReq) return;
    try {
      await respondPlan(planReq.sessionId, "approve");
    } catch (e) {
      console.warn("plan approve failed:", e);
    }
    setPlanReq(null);
  };

  const onPlanDeny = async (reason: string) => {
    if (!planReq) return;
    try {
      await respondPlan(planReq.sessionId, "deny", { reason });
    } catch (e) {
      console.warn("plan deny failed:", e);
    }
    setPlanReq(null);
  };

  const onPlanEdit = async (edited: string) => {
    if (!planReq) return;
    try {
      await respondPlan(planReq.sessionId, "edit", { editedPlan: edited });
    } catch (e) {
      console.warn("plan edit failed:", e);
    }
    setPlanReq(null);
  };

  return (
    <AppErrorBoundary>
      <ActivationGate
        onActivated={async () => {
          try {
            const s = await invoke<LicenseSummary>("license_status");
            setLicenseStatus(s.status);
          } catch {
            /* ignore */
          }
        }}
        onTrial={() => {
          /* 试用期内不阻塞；trial 状态已在 useEffect 中读取 */
        }}
      />
      <div className={`app-shell ${isBlocking ? "app-shell-locked" : ""} ${sidebarCollapsed ? "app-shell-sidebar-collapsed" : ""}`}>
        <TopBar />
        <ThreadTabs />
        <div className="app-body">
          <Sidebar />
          <main className="main-pane">
            <Thread sessionId={currentId} />
            <Composer sessionId={currentId} />
          </main>
        </div>
        <ApprovalDialog
          request={approvalReq}
          onApprove={onApprove}
          onDeny={onDeny}
        />
        <PlanDialog
          request={planReq}
          onApprove={onPlanApprove}
          onDeny={onPlanDeny}
          onEdit={onPlanEdit}
        />
      </div>
    </AppErrorBoundary>
  );
}

export type { ThemeMode } from "./stores/theme";

// ============================================================
// v1.9.6：Thread tabs 栏（Codex App 风格：顶部多 thread）
// ============================================================
function ThreadTabs() {
  const tabs = useOpenTabs();
  const sessions = useSessionsStore((s) => s.sessions);
  const currentId = useSessionsStore((s) => s.currentId);
  const setCurrent = useSessionsStore((s) => s.setCurrent);
  if (tabs.length === 0) return null;
  return (
    <div className="thread-tabs">
      {tabs.map((id) => {
        const sess = sessions.find((s) => s.id === id);
        if (!sess) return null;
        return (
          <div
            key={id}
            className={`thread-tab ${id === currentId ? "active" : ""}`}
            onClick={() => setCurrent(id)}
            title={sess.title}
          >
            <span className="thread-tab-title">{sess.title}</span>
            <button
              className="thread-tab-close"
              title="关闭 tab"
              onClick={(e) => {
                e.stopPropagation();
                closeTab(id);
              }}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}