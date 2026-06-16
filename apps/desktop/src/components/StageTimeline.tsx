// v0.5 流程图：think → plan → act → verify
export type Stage = {
  stage: string; // "think" | "act" | "verify" | "approval" | "done"
  label: string;
  detail?: string | null;
  status: "running" | "done" | "error";
};

const STAGE_ICONS: Record<string, string> = {
  think: "💭",
  plan: "📋",
  act: "⚡",
  verify: "✅",
  approval: "🔐",
  done: "🏁",
};

const STAGE_LABELS: Record<string, string> = {
  think: "思考",
  plan: "规划",
  act: "执行",
  verify: "验证",
  approval: "等待批准",
  done: "完成",
};

type Props = {
  stages: Stage[];
};

export function StageTimeline({ stages }: Props) {
  if (stages.length === 0) return null;

  return (
    <div className="stage-timeline">
      {stages.map((s, i) => {
        const icon = STAGE_ICONS[s.stage] || "▶";
        const label = STAGE_LABELS[s.stage] || s.stage;
        return (
          <div key={i} className={`stage-item stage-${s.status} stage-type-${s.stage}`}>
            <span className="stage-icon">{icon}</span>
            <div className="stage-body">
              <div className="stage-label">
                <strong>{label}</strong>
                <span className="stage-status">
                  {s.status === "running" ? "运行中..." : s.status === "error" ? "错误" : "完成"}
                </span>
              </div>
              <div className="stage-detail">{s.label}</div>
              {s.detail && <div className="stage-extra">{s.detail}</div>}
            </div>
          </div>
        );
      })}
    </div>
  );
}