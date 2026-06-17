// v1.4：P2P 设备协同面板

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

type DeviceStatus =
  | "discovered"
  | "pairing"
  | "paired"
  | "connecting"
  | "connected"
  | "rejected"
  | "lost";

type PeerDevice = {
  info: {
    device_id: string;
    name: string;
    version: string;
    platform: string;
  };
  address: string;
  status: DeviceStatus;
  last_seen: number;
  paired_token: string | null;
};

type Props = {
  onClose: () => void;
};

export function DevicesPanel({ onClose }: Props) {
  const [peers, setPeers] = useState<PeerDevice[]>([]);
  const [hosting, setHosting] = useState(false);
  const [pairingCode, setPairingCode] = useState<string | null>(null);
  const [pendingPair, setPendingPair] = useState<{
    device_id: string;
    code: string;
    name: string;
  } | null>(null);
  const [connectAddr, setConnectAddr] = useState("");
  const [connectCode, setConnectCode] = useState("");
  const [status, setStatus] = useState("");

  const refresh = async () => {
    try {
      const list = await invoke<PeerDevice[]>("p2p_list_peers");
      setPeers(list);
    } catch (e) {
      // ignore
    }
  };

  useEffect(() => {
    void refresh();
    let unlisten: UnlistenFn | null = null;
    listen<{
      kind: string;
      [k: string]: unknown;
    }>("p2p:event", (e) => {
      if (e.payload.kind === "pairing_requested") {
        setPendingPair({
          device_id: e.payload.device_id as string,
          code: e.payload.code as string,
          name: e.payload.name as string,
        });
      } else {
        void refresh();
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const startHost = async () => {
    setStatus("启动中…");
    try {
      const r = await invoke<string>("p2p_start_host", { port: 9876 });
      setHosting(true);
      setStatus(r);
      const code = await invoke<string>("p2p_generate_pairing");
      setPairingCode(code);
    } catch (e) {
      setStatus(`❌ ${e}`);
    }
  };

  const stopHost = async () => {
    await invoke("p2p_stop_host");
    setHosting(false);
    setPairingCode(null);
    setStatus("已停止");
  };

  const refreshCode = async () => {
    const code = await invoke<string>("p2p_generate_pairing");
    setPairingCode(code);
  };

  const acceptPair = async () => {
    if (!pendingPair) return;
    try {
      await invoke("p2p_accept_pairing", { deviceId: pendingPair.device_id });
      setStatus(`✅ 已接受配对：${pendingPair.name}`);
    } catch (e) {
      setStatus(`❌ ${e}`);
    }
    setPendingPair(null);
    void refresh();
  };

  const rejectPair = async () => {
    if (!pendingPair) return;
    await invoke("p2p_reject_pairing", { deviceId: pendingPair.device_id });
    setPendingPair(null);
    setStatus("已拒绝");
  };

  const connect = async () => {
    if (!connectAddr || !connectCode) return;
    setStatus("连接中…");
    try {
      const token = await invoke<string>("p2p_connect", {
        address: connectAddr,
        code: connectCode,
      });
      setStatus(`✅ 已连接，token: ${token.slice(0, 8)}…`);
      setConnectAddr("");
      setConnectCode("");
    } catch (e) {
      setStatus(`❌ ${e}`);
    }
  };

  const statusIcon = (s: DeviceStatus) =>
    s === "connected" ? "🟢" :
    s === "paired" ? "🔗" :
    s === "pairing" ? "⏳" :
    s === "discovered" ? "📡" :
    s === "rejected" ? "🚫" :
    s === "lost" ? "❌" : "❓";

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 720, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>📡 P2P 设备协同（v1.4）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">×</button>
        </div>

        <div className="modal-body theme-body">
          {/* 主机控制 */}
          <div className="devices-section">
            <h4>🏠 主机模式（让别人连我）</h4>
            {!hosting ? (
              <button className="btn primary" onClick={startHost}>
                ▶ 启动主机 (port 9876)
              </button>
            ) : (
              <>
                <div className="pairing-code-box">
                  <span>配对码：</span>
                  <code className="pairing-code">{pairingCode}</code>
                  <button className="btn small" onClick={refreshCode}>
                    🔄 重新生成
                  </button>
                  <button className="btn small" onClick={stopHost}>
                    ⏹ 停止
                  </button>
                </div>
                <p style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 6 }}>
                  对方在客户端连接时输入此 6 位码。配对码仅在主机运行时有效。
                </p>
              </>
            )}
          </div>

          {/* 待处理配对请求 */}
          {pendingPair && (
            <div className="pairing-request">
              <h4>🤝 配对请求</h4>
              <p>
                <strong>{pendingPair.name}</strong> 想连接你（代码：
                <code>{pendingPair.code}</code>）
              </p>
              <div style={{ display: "flex", gap: 8 }}>
                <button className="btn primary" onClick={acceptPair}>
                  ✅ 接受
                </button>
                <button className="btn" onClick={rejectPair}>
                  ❌ 拒绝
                </button>
              </div>
            </div>
          )}

          {/* 客户端连接 */}
          <div className="devices-section">
            <h4>🔌 客户端模式（连别人）</h4>
            <div className="connect-row">
              <input
                className="vault-password-input"
                placeholder="IP:port (e.g. 192.168.1.10:9876)"
                value={connectAddr}
                onChange={(e) => setConnectAddr(e.target.value)}
              />
              <input
                className="vault-password-input pairing-input"
                placeholder="6 位配对码"
                value={connectCode}
                onChange={(e) => setConnectCode(e.target.value.toUpperCase())}
                maxLength={6}
              />
              <button className="btn primary" onClick={connect}>
                连接
              </button>
            </div>
          </div>

          {/* 已发现 / 已配对设备列表 */}
          <div className="devices-section">
            <h4>📱 已知设备 ({peers.length})</h4>
            {peers.length === 0 ? (
              <p style={{ fontSize: 12, color: "var(--text-muted)" }}>
                启动主机或连接别人后这里会显示
              </p>
            ) : (
              <div className="peer-list">
                {peers.map((p) => (
                  <div key={p.info.device_id} className="peer-item">
                    <span>{statusIcon(p.status)}</span>
                    <span className="peer-name">{p.info.name}</span>
                    <span className="peer-platform">{p.info.platform}</span>
                    <span className="peer-addr">{p.address}</span>
                    <span className="peer-status">{p.status}</span>
                  </div>
                ))}
              </div>
            )}
          </div>

          {status && <p className="devices-status">{status}</p>}

          <div className="lint-help">
            <h4>💡 提示</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li>需要两台在同一局域网（同一 WiFi）上的 macOS / Windows 设备</li>
              <li>mDNS 自动发现（macOS / Linux）；Windows 上需手动输入 IP</li>
              <li>本机防火墙需放行 TCP 9876</li>
              <li>当前版本只同步 session 列表（不含消息内容）— 消息拉取（SessionPull / SessionData）已实现，前端 UI 待补</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}