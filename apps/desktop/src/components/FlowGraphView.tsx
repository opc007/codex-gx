// v1.5：Agent 流程图可视化
// - 后端 graph_from_plan 返回 Graph { nodes, edges }
// - 简单 SVG 渲染（DAG，自上而下）
// - 节点按 kind 着色
// - 支持 Mermaid 导出
// - 监听 plan 事件：实时高亮

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type NodeKind =
  | "start"
  | "end"
  | "plan"
  | "message"
  | "tool"
  | "subagent"
  | "decision"
  | "queue"
  | "skill";

type Node = {
  id: string;
  kind: NodeKind;
  label: string;
  detail: string | null;
  status: string | null;
  meta: Record<string, unknown> | null;
};

type Edge = {
  from: string;
  to: string;
  label: string | null;
};

type Graph = {
  nodes: Node[];
  edges: Edge[];
  title: string | null;
};

type Props = {
  onClose: () => void;
};

const KIND_COLOR: Record<NodeKind, string> = {
  start: "#10b981",
  end: "#ef4444",
  plan: "#3b82f6",
  message: "#06b6d4",
  tool: "#a855f7",
  subagent: "#ec4899",
  decision: "#f59e0b",
  queue: "#6366f1",
  skill: "#14b8a6",
};

const KIND_ICON: Record<NodeKind, string> = {
  start: "🟢",
  end: "🔴",
  plan: "📋",
  message: "💬",
  tool: "🔧",
  subagent: "🤖",
  decision: "🔀",
  queue: "📋",
  skill: "🪄",
};

export function FlowGraphView({ onClose }: Props) {
  const [planText, setPlanText] = useState(
    "1. 读取项目目录结构\n2. 扫描 src/**/*.ts 文件\n3. 提取所有 export\n4. 生成依赖图\n5. 输出 Markdown 报告",
  );
  const [graph, setGraph] = useState<Graph | null>(null);
  const [busy, setBusy] = useState(false);
  const [mermaid, setMermaid] = useState("");
  const [tab, setTab] = useState<"svg" | "mermaid">("svg");
  const [selected, setSelected] = useState<Node | null>(null);
  const [status, setStatus] = useState("");

  const generate = async () => {
    if (!planText.trim()) return;
    setBusy(true);
    try {
      const g = await invoke<Graph>("graph_from_plan", {
        plan: planText,
        title: "Plan Flow",
      });
      setGraph(g);
      const m = await invoke<string>("graph_to_mermaid", { g });
      setMermaid(m);
      setStatus(`✅ 已生成：${g.nodes.length} 节点 / ${g.edges.length} 边`);
    } catch (e) {
      setStatus(`❌ ${e}`);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void generate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 900, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🕸️ Agent 流程图（v1.5）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose}>×</button>
        </div>

        <div className="modal-body theme-body">
          <div className="flow-toolbar">
            <textarea
              className="vault-password-input"
              rows={4}
              value={planText}
              onChange={(e) => setPlanText(e.target.value)}
              placeholder="输入 plan markdown（每行以 - 或 1. 开头）"
            />
            <button className="btn primary" onClick={generate} disabled={busy}>
              🔨 生成图
            </button>
            <button
              className="btn small"
              onClick={async () => {
                await navigator.clipboard.writeText(mermaid).catch(() => {});
                setStatus("📋 Mermaid 已复制到剪贴板");
              }}
              disabled={!mermaid}
            >
              📋 复制 Mermaid
            </button>
            <div className="flow-tabs">
              <button
                className={`btn small ${tab === "svg" ? "primary" : ""}`}
                onClick={() => setTab("svg")}
              >
                🎨 SVG
              </button>
              <button
                className={`btn small ${tab === "mermaid" ? "primary" : ""}`}
                onClick={() => setTab("mermaid")}
              >
                📝 Mermaid
              </button>
            </div>
          </div>

          {status && <p className="devices-status">{status}</p>}

          {graph && tab === "svg" && (
            <DagRenderer
              graph={graph}
              onSelect={(n) => setSelected(n)}
              selectedId={selected?.id ?? null}
            />
          )}

          {graph && tab === "mermaid" && (
            <pre className="flow-mermaid">{mermaid}</pre>
          )}

          {selected && (
            <div className="flow-detail">
              <h4>
                {KIND_ICON[selected.kind]} {selected.label}
              </h4>
              <p>
                <strong>id</strong>: <code>{selected.id}</code>
                {" · "}
                <strong>kind</strong>: {selected.kind}
                {selected.status && (
                  <>
                    {" · "}
                    <strong>status</strong>: {selected.status}
                  </>
                )}
              </p>
              {selected.detail && (
                <details open>
                  <summary>detail</summary>
                  <pre>{selected.detail}</pre>
                </details>
              )}
            </div>
          )}

          <div className="lint-help">
            <h4>💡 用法</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li>输入 markdown plan，按 - / 1. 起始的列表会自动识别为节点</li>
              <li>点击节点查看 detail</li>
              <li>切换 SVG / Mermaid，Mermaid 可粘贴到 GitHub / Notion 渲染</li>
              <li>节点按类型着色：📋 plan / 🔧 tool / 🤖 sub-agent / 💬 message</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}

// 简易 SVG DAG 渲染器（自上而下分层布局）
function DagRenderer({
  graph,
  onSelect,
  selectedId,
}: {
  graph: Graph;
  onSelect: (n: Node) => void;
  selectedId: string | null;
}) {
  const layout = useMemo(() => computeLayout(graph), [graph]);
  const width = Math.max(640, layout.width);
  const height = Math.max(360, layout.height);
  return (
    <div className="flow-svg-wrap">
      <svg width={width} height={height} style={{ background: "var(--bg)" }}>
        <defs>
          <marker
            id="arrow"
            viewBox="0 0 10 10"
            refX="10"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill="#888" />
          </marker>
        </defs>

        {graph.edges.map((e, i) => {
          const from = layout.positions.get(e.from);
          const to = layout.positions.get(e.to);
          if (!from || !to) return null;
          return (
            <g key={`e-${i}`}>
              <line
                x1={from.x + from.w / 2}
                y1={from.y + from.h}
                x2={to.x + to.w / 2}
                y2={to.y}
                stroke="#888"
                strokeWidth={1.5}
                markerEnd="url(#arrow)"
              />
              {e.label && (
                <text
                  x={(from.x + to.x) / 2 + from.w / 2}
                  y={(from.y + to.y) / 2 + to.h / 2}
                  fontSize={10}
                  fill="var(--text-muted)"
                  textAnchor="middle"
                >
                  {e.label}
                </text>
              )}
            </g>
          );
        })}

        {graph.nodes.map((n) => {
          const p = layout.positions.get(n.id);
          if (!p) return null;
          const isSel = n.id === selectedId;
          const fill = KIND_COLOR[n.kind];
          return (
            <g
              key={n.id}
              onClick={() => onSelect(n)}
              style={{ cursor: "pointer" }}
            >
              {n.kind === "start" || n.kind === "end" ? (
                <ellipse
                  cx={p.x + p.w / 2}
                  cy={p.y + p.h / 2}
                  rx={p.w / 2}
                  ry={p.h / 2}
                  fill={fill}
                  stroke={isSel ? "#fff" : "transparent"}
                  strokeWidth={2}
                />
              ) : n.kind === "decision" ? (
                <polygon
                  points={`
                    ${p.x + p.w / 2},${p.y}
                    ${p.x + p.w},${p.y + p.h / 2}
                    ${p.x + p.w / 2},${p.y + p.h}
                    ${p.x},${p.y + p.h / 2}
                  `}
                  fill={fill}
                  stroke={isSel ? "#fff" : "transparent"}
                  strokeWidth={2}
                />
              ) : (
                <rect
                  x={p.x}
                  y={p.y}
                  width={p.w}
                  height={p.h}
                  rx={6}
                  fill={fill}
                  stroke={isSel ? "#fff" : "transparent"}
                  strokeWidth={2}
                />
              )}
              <text
                x={p.x + p.w / 2}
                y={p.y + p.h / 2 - 4}
                fontSize={10}
                fill="white"
                textAnchor="middle"
                style={{ pointerEvents: "none" }}
              >
                {KIND_ICON[n.kind]}
              </text>
              <text
                x={p.x + p.w / 2}
                y={p.y + p.h / 2 + 10}
                fontSize={10}
                fill="white"
                textAnchor="middle"
                style={{ pointerEvents: "none" }}
              >
                {truncate(n.label, 14)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

function truncate(s: string, n: number) {
  if (s.length <= n) return s;
  return s.slice(0, n) + "…";
}

type Layout = {
  positions: Map<string, { x: number; y: number; w: number; h: number }>;
  width: number;
  height: number;
};

function computeLayout(g: Graph): Layout {
  // 简单分层：Kahn 拓扑分层
  const indeg = new Map<string, number>();
  const adj = new Map<string, string[]>();
  for (const n of g.nodes) {
    indeg.set(n.id, 0);
    adj.set(n.id, []);
  }
  for (const e of g.edges) {
    if (indeg.has(e.from) && indeg.has(e.to)) {
      indeg.set(e.to, (indeg.get(e.to) ?? 0) + 1);
      adj.get(e.from)!.push(e.to);
    }
  }
  const layers: string[][] = [];
  const remaining = new Set(g.nodes.map((n) => n.id));
  let frontier = g.nodes
    .filter((n) => (indeg.get(n.id) ?? 0) === 0)
    .map((n) => n.id);
  while (frontier.length > 0) {
    layers.push(frontier);
    const next: string[] = [];
    for (const id of frontier) {
      remaining.delete(id);
      for (const to of adj.get(id) ?? []) {
        const cur = (indeg.get(to) ?? 1) - 1;
        indeg.set(to, cur);
        if (cur === 0) next.push(to);
      }
    }
    frontier = next;
  }
  // 把未处理的（环）放到最后
  if (remaining.size > 0) {
    layers.push([...remaining]);
  }

  const NODE_W = 140;
  const NODE_H = 50;
  const GAP_X = 30;
  const GAP_Y = 70;
  const positions = new Map<
    string,
    { x: number; y: number; w: number; h: number }
  >();
  let maxW = 640;
  layers.forEach((layer, ly) => {
    layer.forEach((id, i) => {
      const x = 20 + i * (NODE_W + GAP_X);
      const y = 20 + ly * (NODE_H + GAP_Y);
      positions.set(id, { x, y, w: NODE_W, h: NODE_H });
      if (x + NODE_W + 20 > maxW) maxW = x + NODE_W + 20;
    });
  });
  const totalH = 40 + layers.length * (NODE_H + GAP_Y);
  return {
    positions,
    width: maxW,
    height: totalH,
  };
}
