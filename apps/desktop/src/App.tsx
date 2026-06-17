import { useEffect, useState, Component, type ReactNode, type ErrorInfo } from "react";
import { Sidebar } from "./components/Sidebar";
import { Thread } from "./components/Thread";
import { Composer } from "./components/Composer";
import { TopBar } from "./components/TopBar";
import { ApprovalDialog, type ApprovalRequest } from "./components/ApprovalDialog";
import PlanDialog, {
  type PlanRequest,
  respondPlan,
} from "./components/PlanDialog";
import { useThemeMode, type ThemeMode } from "./stores/theme";
import { useSessionsStore } from "./stores/sessions";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
      <div className="app-shell">
        <TopBar />
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