import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { Thread } from "./components/Thread";
import { Composer } from "./components/Composer";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { LicenseDialog } from "./components/LicenseDialog";
import { ApprovalDialog, type ApprovalRequest } from "./components/ApprovalDialog";
import PlanDialog, {
  type PlanRequest,
  respondPlan,
} from "./components/PlanDialog";
import { useThemeStore, type ThemeMode } from "./stores/theme";
import { useSessionsStore } from "./stores/sessions";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export default function App() {
  const [themeMode, setThemeMode] = useThemeStore((s) => [s.mode, s.setMode]);
  const [currentId] = useSessionsStore((s) => [s.currentId]);
  const [showLicense, setShowLicense] = useState(false);
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
    <div className="app-shell">
      <TopBar
        themeMode={themeMode}
        setThemeMode={setThemeMode}
        onLicenseClick={() => setShowLicense(true)}
      />
      <div className="app-body">
        <Sidebar />
        <main className="main-pane">
          <Thread sessionId={currentId} />
          <Composer sessionId={currentId} />
        </main>
      </div>
      <StatusBar sessionId={currentId} />
      {showLicense && (
        <LicenseDialog
          onClose={() => setShowLicense(false)}
          onChange={() => {}}
        />
      )}
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
  );
}

export type { ThemeMode };