// v1.6：License 管理页（6.12）
// - 4 种状态卡：未激活 / 有效 / 临期 / 过期
// - 4 档 SKU 购买卡（月/季/年/终身）
// - 5 种错误态友好文案
// - 激活码输入
// - 离线时间提示
// - developer 隐藏入口：生成 demo code（仅 dev build 显示）

import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";

type LicenseTier = "monthly" | "quarterly" | "yearly" | "lifetime";

type LicenseStatus =
  | { kind: "unactivated" }
  | {
      kind: "valid";
      tier: LicenseTier;
      remaining_days: number | null;
      activated_at: number;
      expires_at: number | null;
    }
  | { kind: "expiring"; tier: LicenseTier; days_left: number }
  | { kind: "expired"; tier: LicenseTier; expired_at: number }
  | { kind: "offlinegrace"; days_offline: number }
  | { kind: "invalid"; reason: string };

type LicenseSummary = {
  status: LicenseStatus;
  last_validated_at: number;
  offline: boolean;
};

type TierInfo = {
  tier: string;
  displayName: string;
  durationDays: number | null;
  priceYuan: number;
  features: string[];
  recommended: boolean;
};

export function LicensePanel({ onClose }: { onClose: () => void }) {
  const [summary, setSummary] = useState<LicenseSummary | null>(null);
  const [tiers, setTiers] = useState<TierInfo[]>([]);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [devCode, setDevCode] = useState<string | null>(null);
  const [showDev, setShowDev] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [s, t] = await Promise.all([
        invoke<LicenseSummary>("license_status"),
        invoke<TierInfo[]>("license_tiers"),
      ]);
      setSummary(s);
      setTiers(t);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

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
      setDevCode(null);
    } catch (e) {
      setErr(humanizeError(String(e)));
    } finally {
      setBusy(false);
    }
  };

  const onRefresh = async () => {
    setBusy(true);
    setErr(null);
    try {
      await invoke("license_refresh");
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onDeactivate = async () => {
    if (!confirm("确定要清除 License 吗？清除后需要重新输入激活码。")) return;
    setBusy(true);
    setErr(null);
    try {
      await invoke("license_deactivate");
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onGenerateDev = async (tier: string) => {
    try {
      const c = await invoke<string>("license_demo_code", { tier });
      setDevCode(c);
      setCode(c);
      setShowDev(true);
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <div className="panel license-panel">
      <div className="panel-header">
        <h2>🔐 License 授权</h2>
        <button className="close-btn" onClick={onClose}>×</button>
      </div>

      {err && <div className="error-banner">{err}</div>}

      {summary && (
        <StatusCard
          status={summary.status}
          offline={summary.offline}
          lastValidatedAt={summary.last_validated_at}
        />
      )}

      {summary?.status.kind === "unactivated" ||
      summary?.status.kind === "expired" ? (
        <>
          <h3>购买 / 续费</h3>
          <div className="tier-grid">
            {tiers.map((t) => (
              <TierCard key={t.tier} tier={t} />
            ))}
          </div>

          <h3>已有激活码？</h3>
          <div className="activate-form">
            <textarea
              placeholder="把激活码粘到这里（Base64 字符串）"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              rows={3}
            />
            <div className="activate-actions">
              <button className="primary" disabled={busy} onClick={onActivate}>
                {busy ? "激活中..." : "立即激活"}
              </button>
              <button onClick={() => void openUrl("https://agentshell.io/buy")}>
                我要购买
              </button>
            </div>
            <p className="hint">
              💡 购买链接：<code>https://agentshell.io/buy</code>{" "}
              （v1.6 占位 — 真实环境会跳到 Lemon Squeezy / 微信小商店）
            </p>
          </div>

          {/* 内部 dev 工具：仅 dev build 显示 */}
          {import.meta.env.DEV && (
            <details className="dev-tool" open={showDev}>
              <summary onClick={(e) => { e.preventDefault(); setShowDev(!showDev); }}>
                🛠️ 内部工具（仅开发可见）
              </summary>
              <div className="dev-tool-body">
                <p>生成 demo 激活码（仅本地测试用，**生产环境禁止**）</p>
                <div className="dev-tool-buttons">
                  <button onClick={() => onGenerateDev("monthly")}>月卡</button>
                  <button onClick={() => onGenerateDev("quarterly")}>季卡</button>
                  <button onClick={() => onGenerateDev("yearly")}>年卡</button>
                  <button onClick={() => onGenerateDev("lifetime")}>终身</button>
                </div>
                {devCode && (
                  <pre className="dev-code">{devCode}</pre>
                )}
              </div>
            </details>
          )}
        </>
      ) : null}

      {summary &&
        summary.status.kind !== "unactivated" &&
        summary.status.kind !== "expired" && (
          <div className="active-actions">
            <button disabled={busy} onClick={() => void onRefresh()}>🔄 重新校验</button>
            <button onClick={onDeactivate}>清除 License</button>
          </div>
        )}

      <p className="eula-hint">
        激活码从首次输入时刻起算有效期；到期后软件进入只读模式，需输入新激活码恢复；
        新激活码从输入时刻重新累计，旧码剩余时间不合并。
      </p>
    </div>
  );
}

function StatusCard({
  status,
  offline,
  lastValidatedAt,
}: {
  status: LicenseStatus;
  offline: boolean;
  lastValidatedAt: number;
}) {
  if (status.kind === "unactivated") {
    return (
      <div className="status-card status-unactivated">
        <div className="status-icon">🔓</div>
        <div>
          <div className="status-title">未激活</div>
          <div className="status-sub">输入激活码解锁全部功能</div>
        </div>
      </div>
    );
  }
  if (status.kind === "valid") {
    return (
      <div className="status-card status-valid">
        <div className="status-icon">✅</div>
        <div>
          <div className="status-title">
            {tierDisplay(status.tier)} ·{" "}
            {status.remaining_days == null
              ? "终身"
              : `还剩 ${status.remaining_days} 天`}
          </div>
          <div className="status-sub">
            {offline ? "（离线模式）" : "已激活"} · 激活于{" "}
            {new Date(status.activated_at * 1000).toLocaleDateString()}
            {lastValidatedAt > 0 && (
              <> · 上次校验 {new Date(lastValidatedAt * 1000).toLocaleString()}</>
            )}
          </div>
        </div>
      </div>
    );
  }
  if (status.kind === "expiring") {
    return (
      <div className="status-card status-expiring">
        <div className="status-icon">⏰</div>
        <div>
          <div className="status-title">
            {tierDisplay(status.tier)} · 还剩 {status.days_left} 天到期
          </div>
          <div className="status-sub">建议尽快续费，避免进入只读模式</div>
        </div>
      </div>
    );
  }
  if (status.kind === "expired") {
    return (
      <div className="status-card status-expired">
        <div className="status-icon">❌</div>
        <div>
          <div className="status-title">
            {tierDisplay(status.tier)} · 已过期
          </div>
          <div className="status-sub">软件处于只读模式，请输入新激活码</div>
        </div>
      </div>
    );
  }
  if (status.kind === "offlinegrace") {
    return (
      <div className="status-card status-offline">
        <div className="status-icon">📡</div>
        <div>
          <div className="status-title">离线 {status.days_offline} 天</div>
          <div className="status-sub">已超过 7 天离线宽限 — 软件处于只读模式，请联网重新校验</div>
        </div>
      </div>
    );
  }
  // invalid
  return (
    <div className="status-card status-invalid">
      <div className="status-icon">⚠️</div>
      <div>
        <div className="status-title">License 异常</div>
        <div className="status-sub">{(status as any).reason}</div>
      </div>
    </div>
  );
}

function TierCard({ tier }: { tier: TierInfo }) {
  return (
    <div className={`tier-card ${tier.recommended ? "tier-recommended" : ""}`}>
      {tier.recommended && <div className="tier-badge">推荐</div>}
      <div className="tier-name">{tier.displayName}</div>
      <div className="tier-price">
        <span className="yuan">¥</span>
        <span className="amount">{tier.priceYuan.toFixed(1)}</span>
      </div>
      <div className="tier-duration">
        {tier.durationDays
          ? `${tier.durationDays} 天`
          : "永久免费升级 v1.x"}
      </div>
      <ul className="tier-features">
        {tier.features.map((f, i) => (
          <li key={i}>✓ {f}</li>
        ))}
      </ul>
    </div>
  );
}

function tierDisplay(tier: LicenseTier): string {
  switch (tier) {
    case "monthly":
      return "月卡 ¥9.9";
    case "quarterly":
      return "季卡 ¥29.9";
    case "yearly":
      return "年卡 ¥99";
    case "lifetime":
      return "终身 ¥299";
  }
}

function humanizeError(raw: string): string {
  if (raw.includes("BadSignature")) {
    return "激活码签名验证失败 — 码可能被篡改或不是 Codex gx 官方码";
  }
  if (raw.includes("DeviceMismatch")) {
    return "激活码绑定的设备与本机不一致 — 一个码只能在首次激活的设备上使用";
  }
  if (raw.includes("Expired")) {
    return "激活码已过期 — 请购买新码（v1.6 规则：不退款，到期即失效）";
  }
  if (raw.includes("base64") || raw.includes("json")) {
    return "激活码格式错误 — 请检查是否完整复制（码很长，需要整行粘）";
  }
  if (raw.includes("InvalidCode")) {
    return "激活码无效 — 请确认码来源（仅支持 Codex gx 官方码）";
  }
  return raw;
}
