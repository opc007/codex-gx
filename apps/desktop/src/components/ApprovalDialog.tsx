// Tool call 批准 / 拒绝 模态框
import { useEffect, useState } from "react";

export type ApprovalRequest = {
  sessionId: string;
  toolCallId: string;
  name: string;
  arguments: unknown;
};

type Props = {
  request: ApprovalRequest | null;
  onApprove: () => void;
  onDeny: (reason: string) => void;
};

// 风险评估
function riskLevel(name: string, args: unknown): { level: "safe" | "warning" | "danger"; reason: string } {
  if (name === "bash") {
    const cmd = (args as { command?: string })?.command || "";
    if (/rm\s+-rf|sudo|dd\s+if|chmod\s+777|mkfs|format/i.test(cmd)) {
      return { level: "danger", reason: "包含危险操作（删除/格式化/权限修改）" };
    }
    if (/curl|wget|nc\s|bash\s+-c/i.test(cmd)) {
      return { level: "warning", reason: "下载/远程执行命令" };
    }
    return { level: "safe", reason: "普通 shell 命令" };
  }
  if (name === "write_file" || name === "edit_file") {
    return { level: "warning", reason: "写入/修改文件" };
  }
  if (name === "web_search") {
    return { level: "safe", reason: "联网搜索（只读）" };
  }
  if (name.startsWith("browser_")) {
    return { level: "warning", reason: "浏览器自动化操作" };
  }
  return { level: "safe", reason: "只读操作" };
}

export function ApprovalDialog({ request, onApprove, onDeny }: Props) {
  const [denyReason, setDenyReason] = useState("");
  const [showDenyInput, setShowDenyInput] = useState(false);
  const [countdown, setCountdown] = useState(0);

  // 60s 后自动拒绝
  useEffect(() => {
    if (!request) return;
    setCountdown(60);
    const timer = setInterval(() => {
      setCountdown((c) => {
        if (c <= 1) {
          clearInterval(timer);
          onDeny("timeout");
          return 0;
        }
        return c - 1;
      });
    }, 1000);
    return () => clearInterval(timer);
  }, [request?.toolCallId]);

  if (!request) return null;

  const risk = riskLevel(request.name, request.arguments);
  const argsJson = JSON.stringify(request.arguments, null, 2);

  return (
    <div className="modal-overlay" onClick={() => { /* require explicit choice */ }}>
      <div className="modal approval-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>
            {risk.level === "danger" ? "🚨 危险操作" : risk.level === "warning" ? "⚠️ 需要确认" : "🔧 工具调用"}
          </h2>
          <span className="approval-countdown">{countdown}s</span>
        </div>
        <div className="modal-body">
          <div className={`approval-risk approval-${risk.level}`}>
            <strong>风险等级：{risk.level === "danger" ? "高危" : risk.level === "warning" ? "中等" : "安全"}</strong>
            <p>{risk.reason}</p>
          </div>

          <div className="approval-tool">
            <h3>工具</h3>
            <code className="approval-tool-name">{request.name}</code>
          </div>

          <div className="approval-args">
            <h3>参数</h3>
            <pre>{argsJson}</pre>
          </div>

          {showDenyInput && (
            <div className="approval-deny-input">
              <h3>拒绝原因（可选）</h3>
              <input
                value={denyReason}
                onChange={(e) => setDenyReason(e.target.value)}
                placeholder="告诉模型为什么不批准..."
                autoFocus
              />
            </div>
          )}

          <div className="approval-actions">
            {!showDenyInput ? (
              <>
                <button className="btn-danger" onClick={() => setShowDenyInput(true)}>
                  ❌ 拒绝
                </button>
                <button className="btn-primary" onClick={onApprove}>
                  ✅ 批准
                </button>
              </>
            ) : (
              <>
                <button className="btn-secondary" onClick={() => setShowDenyInput(false)}>
                  ← 返回
                </button>
                <button className="btn-danger" onClick={() => onDeny(denyReason || "denied")}>
                  确认拒绝
                </button>
              </>
            )}
          </div>

          <p className="approval-hint muted">
            💡 60 秒未操作将自动拒绝。可在 Composer 用 <code>/approval off</code> 切到自动批准模式。
          </p>
        </div>
      </div>
    </div>
  );
}