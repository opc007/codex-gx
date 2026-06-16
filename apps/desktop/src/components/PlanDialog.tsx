import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type PlanRequest = {
  sessionId: string;
  plan: string;
  planId: string;
};

type Props = {
  request: PlanRequest | null;
  onApprove: () => void;
  onDeny: (reason: string) => void;
  onEdit: (edited: string) => void;
};

export default function PlanDialog({ request, onApprove, onDeny, onEdit }: Props) {
  const [editing, setEditing] = useState(false);
  const [edited, setEdited] = useState("");
  const [denying, setDenying] = useState(false);
  const [reason, setReason] = useState("");

  useEffect(() => {
    if (request) {
      setEditing(false);
      setEdited(request.plan);
      setDenying(false);
      setReason("");
    }
  }, [request?.planId]);

  if (!request) return null;

  return (
    <div className="modal-overlay">
      <div className="modal plan-dialog">
        <div className="plan-header">
          <h2>📋 模型给出了执行计划</h2>
          <p className="plan-subtitle">
            请审阅下面的计划。批准后将按此执行；可编辑后再批准，或直接拒绝。
          </p>
        </div>

        {!editing && !denying && (
          <div className="plan-body">
            <pre className="plan-content">{request.plan}</pre>
          </div>
        )}

        {editing && (
          <div className="plan-body">
            <textarea
              className="plan-textarea"
              value={edited}
              onChange={(e) => setEdited(e.target.value)}
              rows={14}
              spellCheck={false}
            />
          </div>
        )}

        {denying && (
          <div className="plan-body">
            <p style={{ color: "var(--text-muted)", fontSize: 13, marginBottom: 8 }}>
              请说明拒绝原因（将记录到对话日志）：
            </p>
            <textarea
              className="plan-textarea"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              rows={4}
              placeholder="例如：想用不同的实现方式"
              autoFocus
            />
          </div>
        )}

        <div className="plan-footer">
          {!editing && !denying && (
            <>
              <button className="btn-secondary" onClick={() => setDenying(true)}>
                拒绝
              </button>
              <button className="btn-secondary" onClick={() => setEditing(true)}>
                ✏️ 编辑
              </button>
              <button className="btn-primary" onClick={onApprove}>
                ✓ 批准并执行
              </button>
            </>
          )}
          {editing && (
            <>
              <button className="btn-secondary" onClick={() => setEditing(false)}>
                ← 返回
              </button>
              <button
                className="btn-primary"
                onClick={() => onEdit(edited)}
                disabled={!edited.trim()}
              >
                ✓ 用编辑后的计划执行
              </button>
            </>
          )}
          {denying && (
            <>
              <button className="btn-secondary" onClick={() => setDenying(false)}>
                ← 返回
              </button>
              <button
                className="btn-danger"
                onClick={() => onDeny(reason || "user denied")}
              >
                确认拒绝
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

// Tauri command invoker — exported helper
export async function respondPlan(
  sessionId: string,
  action: "approve" | "deny" | "edit",
  opts: { reason?: string; editedPlan?: string } = {}
): Promise<void> {
  await invoke("respond_plan", {
    sessionId,
    action,
    reason: opts.reason ?? null,
    editedPlan: opts.editedPlan ?? null,
  });
}
