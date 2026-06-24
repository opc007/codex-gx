//! ActivationGate — v2.0 永久免费版
//! 后端已改为永远返回有效状态，此组件不再渲染任何阻塞界面
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type LicenseStatus =
  | { kind: "unactivated" }
  | { kind: "trial"; remaining_days: number | null }
  | { kind: "valid"; tier: string }
  | { kind: "expiring"; tier: string; days_left: number }
  | { kind: "expired"; tier: string }
  | { kind: "offlinegrace"; days_offline: number }
  | { kind: "invalid"; reason: string };

type Props = {
  onActivated: () => void;
  onTrial: (days: number) => void;
};

export function ActivationGate(_props: Props) {
  // v2.0: 永久免费，后端已返回 community 有效状态，不阻塞
  // 这里只是做个兼容性检查，实际上不会渲染任何内容
  const [status, setStatus] = useState<LicenseStatus | null>(null);

  useEffect(() => {
    invoke<{ status: LicenseStatus }>("license_status").then((s) => {
      setStatus(s.status);
      if (s.status.kind === "trial" && s.status.remaining_days !== null) {
        _props.onTrial(s.status.remaining_days);
      }
    });
  }, []);

  if (!status) return null;

  // 所有状态都不阻塞（v2.0 免费版）
  return null;
}
