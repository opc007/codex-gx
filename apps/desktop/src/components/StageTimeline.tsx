import React from "react";

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

  const listRef = React.useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = React.useState(true);
  const MAX_VISIBLE = 5;
  const visibleStages = collapsed ? stages.slice(-MAX_VISIBLE) : stages;

  // 自动滚动到底部显示最新步骤
  React.useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [stages.length, collapsed]);

  return (
    <div className="stage-timeline">
      <div className="stage-header" onClick={() => setCollapsed(!collapsed)}>
        <span>⚡ 执行过程</span>
        <span className="stage-count">{stages.length} 步</span>
        <span className="stage-toggle">{collapsed ? "展开 ▾" : "收起 ▴"}</span>
      </div>

      <div className={`stage-list ${collapsed ? 'compact' : ''}`} ref={listRef}>
        {visibleStages.map((s, i) => {
          const icon = STAGE_ICONS[s.stage] || "▶";
          const label = STAGE_LABELS[s.stage] || s.stage;
          const globalIndex = collapsed ? stages.length - visibleStages.length + i : i;
          return (
            <div key={globalIndex} className={`stage-item stage-${s.status} stage-type-${s.stage}`}>
              <span className="stage-icon">{icon}</span>
              <div className="stage-body">
                <div className="stage-label">
                  <strong>{label}</strong>
                  <span className="stage-status">
                    {s.status === "running" ? "运行中" : s.status === "error" ? "失败" : "✓"}
                  </span>
                </div>
                <div className="stage-detail" title={s.label}>{s.label}</div>
                {s.detail && <div className="stage-extra" title={s.detail}>{s.detail}</div>}
              </div>
            </div>
          );
        })}
        {collapsed && stages.length > MAX_VISIBLE && (
          <div className="stage-more">⋯ 还有 {stages.length - MAX_VISIBLE} 步（点击展开查看全部）</div>
        )}
      </div>
    </div>
  );
}