// Codex 风格：未激活时显示居中激活卡片（3 天免费试用后必须激活）
// 试用期内只显示顶部倒计时条，不阻塞
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type LicenseStatus =
  | { kind: "unactivated" }
  | { kind: "trial"; remaining_days: number | null; started_at: number }
  | {
      kind: "valid";
      tier: string;
      remaining_days: number | null;
      activated_at: number;
      expires_at: number | null;
    }
  | { kind: "expiring"; tier: string; days_left: number }
  | { kind: "expired"; tier: string; expired_at: number }
  | { kind: "offlinegrace"; days_offline: number }
  | { kind: "invalid"; reason: string };

type LicenseSummary = {
  status: LicenseStatus;
  last_validated_at: number;
  offline: boolean;
};

type Props = {
  onActivated: () => void;
  onTrial: (days: number) => void;
};

export function ActivationGate({ onActivated, onTrial }: Props) {
  const [summary, setSummary] = useState<LicenseSummary | null>(null);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const s = await invoke<LicenseSummary>("license_status");
      setSummary(s);
      if (s.status.kind === "trial" && s.status.remaining_days !== null) {
        onTrial(s.status.remaining_days);
      }
    } catch (e) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const onActivate = async () => {
    if (!code.trim()) {
      setErr("请先粘贴激活码");
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const s = await invoke<LicenseSummary>("license_activate", { code: code.trim() });
      setSummary(s);
      setCode("");
      if (s.status.kind === "valid" || s.status.kind === "expiring") {
        onActivated();
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!summary) return null;

  // 已激活 / 试用期内 / 仅脱敏：全部不阻塞
  if (
    summary.status.kind === "valid" ||
    summary.status.kind === "expiring" ||
    summary.status.kind === "offlinegrace" ||
    (summary.status.kind === "trial" && summary.status.remaining_days !== null)
  ) {
    return null;
  }

  // 阻塞激活页
  const isUnactivated = summary.status.kind === "unactivated";
  return (
    <div className="activation-gate">
      <div className="activation-card">
        <div className="activation-logo" aria-hidden="true">✦</div>
        <h1>欢迎使用 Codex gx</h1>
        <p className="activation-sub">
          {isUnactivated
            ? "首次使用请输入激活码。未填激活码也能先用 3 天。"
            : "免费试用已结束。填入激活码继续使用。"}
        </p>

        <div className="activation-form">
          <input
            className="activation-input"
            type="text"
            placeholder="粘贴激活码（演示版可点下方填入测试码）"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !busy) void onActivate();
            }}
            autoFocus
            disabled={busy}
          />
          <button
            className="activation-submit"
            onClick={() => void onActivate()}
            disabled={busy || !code.trim()}
          >
            {busy ? "激活中…" : "激活"}
          </button>
        </div>

        {err && <div className="activation-err">{err}</div>}

        <div className="activation-actions">
          <button
            className="activation-link"
            onClick={async () => {
              try {
                const c = await invoke<string>("license_demo_code", { tier: "monthly" });
                setCode(c);
              } catch {
                /* dev only */
              }
            }}
            disabled={busy}
          >
            🧪 填入测试激活码
          </button>
        </div>

        <div className="activation-foot">
          没有激活码？访问
          <a href="https://github.com/opc007/codex-gx" target="_blank" rel="noreferrer">
            {" "}主页
          </a>
          {" "}了解更多
        </div>
      </div>
    </div>
  );
}
