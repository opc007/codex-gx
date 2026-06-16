import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { Thread } from "./components/Thread";
import { Composer } from "./components/Composer";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { LicenseDialog } from "./components/LicenseDialog";
import { ApprovalDialog, type ApprovalRequest } from "./components/ApprovalDialog";
import { useThemeStore, type ThemeMode } from "./stores/theme";
import { useSessionsStore } from "./stores/sessions";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export default function App() {
  const [themeMode, setThemeMode] = useThemeStore((s) => [s.mode, s.setMode]);
  const [currentId] = useSessionsStore((s) => [s.currentId]);
  const [showLicense, setShowLicense] = useState(false);
  const [approvalReq, setApprovalReq] = useState<ApprovalRequest | null>(null);

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

  // v0.4：监听 approval_request
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
    </div>
  );
}

export type { ThemeMode };