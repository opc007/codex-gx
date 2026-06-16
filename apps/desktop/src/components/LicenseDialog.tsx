// License 激活模态框
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type LicenseStatus = {
  active: boolean;
  tier: string;
  tierDisplay: string;
  activatedAt: string | null;
  expiresAt: string | null;
  remainingDays: number | null;
  deviceId: string | null;
};

type Props = {
  onClose: () => void;
  onChange: () => void;
};

export function LicenseDialog({ onClose, onChange }: Props) {
  const [status, setStatus] = useState<LicenseStatus | null>(null);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const s = await invoke<LicenseStatus>("get_license_status");
      setStatus(s);
    } catch (e) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const onActivate = async () => {
    if (!code.trim()) return;
    setBusy(true);
    setErr(null);
    try {
      const s = await invoke<LicenseStatus>("activate_license", { code: code.trim() });
      setStatus(s);
      setCode("");
      onChange();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onDeactivate = async () => {
    if (!confirm("确认清除当前 License？")) return;
    setBusy(true);
    try {
      await invoke("deactivate_license");
      await refresh();
      onChange();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>License 激活</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>
        <div className="modal-body">
          {status?.active && (
            <div className="license-active">
              <div className="license-status-pill">
                ✅ {status.tierDisplay}
              </div>
              <dl className="license-info">
                <dt>激活时间</dt>
                <dd>{status.activatedAt ? new Date(status.activatedAt).toLocaleString() : "-"}</dd>
                <dt>到期时间</dt>
                <dd>
                  {status.expiresAt ? (
                    <>
                      {new Date(status.expiresAt).toLocaleString()}
                      {status.remainingDays != null && (
                        <span className="muted"> (剩 {status.remainingDays} 天)</span>
                      )}
                    </>
                  ) : (
                    "永久"
                  )}
                </dd>
                <dt>设备</dt>
                <dd className="mono">{status.deviceId?.slice(0, 32)}...</dd>
              </dl>
              <button className="btn-danger" onClick={onDeactivate} disabled={busy}>
                清除 License
              </button>
            </div>
          )}
          {!status?.active && (
            <div className="license-empty">
              <p>当前未激活 License</p>
            </div>
          )}

          <div className="license-activate">
            <h3>输入激活码</h3>
            <textarea
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="粘贴你的 License Base64 字符串..."
              rows={4}
            />
            {err && <div className="license-err">❌ {err}</div>}
            <button className="btn-primary" onClick={onActivate} disabled={busy || !code.trim()}>
              {busy ? "激活中..." : "激活"}
            </button>
          </div>

          <div className="license-tiers">
            <h3>套餐</h3>
            <table>
              <thead>
                <tr><th>套餐</th><th>价格</th><th>有效期</th></tr>
              </thead>
              <tbody>
                <tr><td>月卡</td><td>¥9.9</td><td>30 天</td></tr>
                <tr><td>季卡</td><td>¥29.9</td><td>90 天</td></tr>
                <tr><td>年卡</td><td>¥99</td><td>365 天</td></tr>
                <tr><td>终身</td><td>¥299</td><td>永久（含 v1.x 升级）</td></tr>
              </tbody>
            </table>
            <p className="muted">
              一机一码，到期失效。终身卡含 v1.x 全部免费升级。
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}