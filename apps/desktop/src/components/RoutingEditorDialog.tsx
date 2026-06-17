// v1.3：路由策略编辑器
// - 列出当前策略的所有规则
// - 增删改规则
// - 测试决策（输入消息，看命中哪条规则）
// - 保存 / 重置

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type RouteTarget = { provider: string; model: string };

export type MatchCondition = {
  task_types: string[];
  keywords: string[];
  file_exts: string[];
  min_length: number | null;
  max_length: number | null;
};

export type RoutingRule = {
  id: string;
  name: string;
  priority: number;
  match_condition: MatchCondition;
  primary: RouteTarget;
  fallbacks: RouteTarget[];
};

export type RoutingStrategy = {
  version: number;
  default: RouteTarget;
  default_fallbacks: RouteTarget[];
  rules: RoutingRule[];
};

const TASK_TYPES = [
  "code",
  "reason",
  "summary",
  "translate",
  "chat",
  "vision",
  "long",
  "quick",
  "generic",
] as const;

const PROVIDERS = [
  { id: "MiniMax", label: "MiniMax M3" },
  { id: "deepseek", label: "DeepSeek" },
  { id: "anthropic", label: "Anthropic Claude" },
  { id: "openai", label: "OpenAI" },
];

const MODEL_PRESETS: Record<string, string[]> = {
  MiniMax: ["MiniMax-M3"],
  deepseek: ["deepseek-chat", "deepseek-coder"],
  anthropic: ["claude-sonnet-4-5", "claude-opus-4-5"],
  openai: ["gpt-4o", "gpt-4o-mini", "o1-preview"],
};

type Props = {
  onClose: () => void;
};

export function RoutingEditorDialog({ onClose }: Props) {
  const [strategy, setStrategy] = useState<RoutingStrategy | null>(null);
  const [busy, setBusy] = useState(false);
  const [testMsg, setTestMsg] = useState("请帮我写一个 Rust 函数解析 JSON");
  const [testHint, setTestHint] = useState<string>("code");
  const [testResult, setTestResult] = useState<{
    primary_provider: string;
    primary_model: string;
    fallbacks: RouteTarget[];
    reason: string;
    rule_id: string | null;
  } | null>(null);
  const [editingIdx, setEditingIdx] = useState<number | null>(null);

  const load = async () => {
    try {
      const s = await invoke<RoutingStrategy>("routing_get_strategy");
      setStrategy(s);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const handleSave = async () => {
    if (!strategy) return;
    setBusy(true);
    try {
      await invoke("routing_set_strategy", { strategy });
      alert("✅ 路由策略已保存");
    } catch (e) {
      alert(`❌ 保存失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleReset = async () => {
    if (!confirm("重置为内置默认策略？现有规则将丢失。")) return;
    setBusy(true);
    try {
      const s = await invoke<RoutingStrategy>("routing_reset_to_default");
      setStrategy(s);
    } catch (e) {
      alert(`❌ 重置失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleTest = async () => {
    setBusy(true);
    try {
      const r = await invoke<{
        primary_provider: string;
        primary_model: string;
        fallbacks: RouteTarget[];
        reason: string;
        rule_id: string | null;
      }>("routing_decide", {
        args: { message: testMsg, task_type: testHint || null },
      });
      setTestResult(r);
    } catch (e) {
      alert(`❌ 决策失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  const updateRule = (idx: number, patch: Partial<RoutingRule>) => {
    if (!strategy) return;
    const rules = strategy.rules.map((r, i) =>
      i === idx ? { ...r, ...patch } : r,
    );
    setStrategy({ ...strategy, rules });
  };

  const addRule = () => {
    if (!strategy) return;
    const r: RoutingRule = {
      id: `rule_${Date.now()}`,
      name: "新规则",
      priority: 50,
      match_condition: {
        task_types: [],
        keywords: [],
        file_exts: [],
        min_length: null,
        max_length: null,
      },
      primary: { provider: "MiniMax", model: "MiniMax-M3" },
      fallbacks: [],
    };
    setStrategy({ ...strategy, rules: [...strategy.rules, r] });
    setEditingIdx(strategy.rules.length);
  };

  const removeRule = (idx: number) => {
    if (!strategy) return;
    if (!confirm("删除此规则？")) return;
    setStrategy({
      ...strategy,
      rules: strategy.rules.filter((_, i) => i !== idx),
    });
  };

  if (!strategy) {
    return (
      <div className="modal-mask" onClick={onClose}>
        <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
          <p>加载路由策略中…</p>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 880, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🧭 路由策略（v1.3）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">
            ×
          </button>
        </div>

        <div className="modal-body theme-body">
          {/* 测试区 */}
          <div className="routing-test">
            <h4>🔍 测试决策</h4>
            <div className="routing-test-row">
              <textarea
                className="vault-password-input"
                style={{ minHeight: 60, flex: 1 }}
                value={testMsg}
                onChange={(e) => setTestMsg(e.target.value)}
                placeholder="输入测试消息…"
              />
              <select
                className="topbar-select"
                value={testHint}
                onChange={(e) => setTestHint(e.target.value)}
              >
                <option value="">无 hint</option>
                {TASK_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
              <button className="btn primary" onClick={handleTest} disabled={busy}>
                决策
              </button>
            </div>
            {testResult && (
              <div className="routing-test-result">
                <div>
                  <strong>{testResult.primary_provider}/{testResult.primary_model}</strong>
                </div>
                <div style={{ color: "var(--text-muted)", fontSize: 12 }}>
                  {testResult.reason}
                  {testResult.rule_id && ` · 规则: ${testResult.rule_id}`}
                </div>
                {testResult.fallbacks.length > 0 && (
                  <div style={{ fontSize: 12 }}>
                    兜底：
                    {testResult.fallbacks
                      .map((f) => `${f.provider}/${f.model}`)
                      .join(" → ")}
                  </div>
                )}
              </div>
            )}
          </div>

          {/* 默认目标 */}
          <div className="routing-default">
            <h4>🎯 默认目标（无规则命中时使用）</h4>
            <div className="routing-target-row">
              <select
                className="topbar-select"
                value={strategy.default.provider}
                onChange={(e) =>
                  setStrategy({
                    ...strategy,
                    default: { ...strategy.default, provider: e.target.value },
                  })
                }
              >
                {PROVIDERS.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
              <select
                className="topbar-select"
                value={strategy.default.model}
                onChange={(e) =>
                  setStrategy({
                    ...strategy,
                    default: { ...strategy.default, model: e.target.value },
                  })
                }
              >
                {(MODEL_PRESETS[strategy.default.provider] ?? []).map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* 规则列表 */}
          <div className="routing-rules">
            <h4>
              📜 规则（{strategy.rules.length}）
              <button
                className="btn small"
                onClick={addRule}
                style={{ marginLeft: 12 }}
              >
                ＋ 新规则
              </button>
            </h4>
            {strategy.rules
              .slice()
              .sort((a, b) => a.priority - b.priority)
              .map((r) => {
                const realIdx = strategy.rules.findIndex((x) => x.id === r.id);
                const isOpen = editingIdx === realIdx;
                return (
                  <div
                    key={r.id}
                    className={`routing-rule ${isOpen ? "open" : ""}`}
                  >
                    <div
                      className="routing-rule-head"
                      onClick={() => setEditingIdx(isOpen ? null : realIdx)}
                    >
                      <span className="routing-rule-priority">P{r.priority}</span>
                      <span className="routing-rule-name">{r.name}</span>
                      <span className="routing-rule-target">
                        → {r.primary.provider}/{r.primary.model}
                      </span>
                      <span className="routing-rule-toggle">{isOpen ? "▾" : "▸"}</span>
                    </div>
                    {isOpen && (
                      <div className="routing-rule-body">
                        <div className="routing-rule-row">
                          <label>名称</label>
                          <input
                            className="vault-password-input"
                            value={r.name}
                            onChange={(e) =>
                              updateRule(realIdx, { name: e.target.value })
                            }
                          />
                          <label>优先级</label>
                          <input
                            className="vault-password-input"
                            type="number"
                            value={r.priority}
                            onChange={(e) =>
                              updateRule(realIdx, {
                                priority: Number(e.target.value) || 0,
                              })
                            }
                            style={{ width: 80 }}
                          />
                        </div>
                        <div className="routing-rule-row">
                          <label>任务类型</label>
                          <div className="routing-task-types">
                            {TASK_TYPES.map((t) => (
                              <label key={t} className="routing-check">
                                <input
                                  type="checkbox"
                                  checked={r.match_condition.task_types.includes(t)}
                                  onChange={(e) => {
                                    const cur = r.match_condition.task_types;
                                    const next = e.target.checked
                                      ? [...cur, t]
                                      : cur.filter((x) => x !== t);
                                    updateRule(realIdx, {
                                      match_condition: {
                                        ...r.match_condition,
                                        task_types: next,
                                      },
                                    });
                                  }}
                                />
                                {t}
                              </label>
                            ))}
                          </div>
                        </div>
                        <div className="routing-rule-row">
                          <label>关键词</label>
                          <input
                            className="vault-password-input"
                            value={r.match_condition.keywords.join(", ")}
                            onChange={(e) =>
                              updateRule(realIdx, {
                                match_condition: {
                                  ...r.match_condition,
                                  keywords: e.target.value
                                    .split(/[,，]/)
                                    .map((s) => s.trim())
                                    .filter(Boolean),
                                },
                              })
                            }
                            placeholder="逗号分隔"
                          />
                        </div>
                        <div className="routing-rule-row">
                          <label>文件后缀</label>
                          <input
                            className="vault-password-input"
                            value={r.match_condition.file_exts.join(", ")}
                            onChange={(e) =>
                              updateRule(realIdx, {
                                match_condition: {
                                  ...r.match_condition,
                                  file_exts: e.target.value
                                    .split(/[,，]/)
                                    .map((s) => s.trim())
                                    .filter(Boolean),
                                },
                              })
                            }
                            placeholder="rs, py, ts, …"
                          />
                        </div>
                        <div className="routing-rule-row">
                          <label>长度范围</label>
                          <input
                            className="vault-password-input"
                            type="number"
                            placeholder="min"
                            value={r.match_condition.min_length ?? ""}
                            onChange={(e) =>
                              updateRule(realIdx, {
                                match_condition: {
                                  ...r.match_condition,
                                  min_length: e.target.value
                                    ? Number(e.target.value)
                                    : null,
                                },
                              })
                            }
                            style={{ width: 100 }}
                          />
                          <span>—</span>
                          <input
                            className="vault-password-input"
                            type="number"
                            placeholder="max"
                            value={r.match_condition.max_length ?? ""}
                            onChange={(e) =>
                              updateRule(realIdx, {
                                match_condition: {
                                  ...r.match_condition,
                                  max_length: e.target.value
                                    ? Number(e.target.value)
                                    : null,
                                },
                              })
                            }
                            style={{ width: 100 }}
                          />
                        </div>
                        <div className="routing-rule-row">
                          <label>主目标</label>
                          <select
                            className="topbar-select"
                            value={r.primary.provider}
                            onChange={(e) =>
                              updateRule(realIdx, {
                                primary: {
                                  ...r.primary,
                                  provider: e.target.value,
                                  model: MODEL_PRESETS[e.target.value]?.[0] ?? "",
                                },
                              })
                            }
                          >
                            {PROVIDERS.map((p) => (
                              <option key={p.id} value={p.id}>
                                {p.label}
                              </option>
                            ))}
                          </select>
                          <select
                            className="topbar-select"
                            value={r.primary.model}
                            onChange={(e) =>
                              updateRule(realIdx, {
                                primary: { ...r.primary, model: e.target.value },
                              })
                            }
                          >
                            {(MODEL_PRESETS[r.primary.provider] ?? []).map((m) => (
                              <option key={m} value={m}>
                                {m}
                              </option>
                            ))}
                          </select>
                        </div>
                        <div className="routing-rule-row">
                          <label>兜底链</label>
                          <div className="routing-fallbacks">
                            {r.fallbacks.map((f, fi) => (
                              <span key={fi} className="routing-fallback-chip">
                                {f.provider}/{f.model}
                                <button
                                  className="routing-chip-x"
                                  onClick={() =>
                                    updateRule(realIdx, {
                                      fallbacks: r.fallbacks.filter(
                                        (_, i) => i !== fi,
                                      ),
                                    })
                                  }
                                >
                                  ×
                                </button>
                              </span>
                            ))}
                            <button
                              className="btn small"
                              onClick={() =>
                                updateRule(realIdx, {
                                  fallbacks: [
                                    ...r.fallbacks,
                                    { provider: "MiniMax", model: "MiniMax-M3" },
                                  ],
                                })
                              }
                            >
                              ＋
                            </button>
                          </div>
                        </div>
                        <div className="routing-rule-row">
                          <button
                            className="btn small danger"
                            onClick={() => removeRule(realIdx)}
                          >
                            删除规则
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
          </div>
        </div>

        <div className="modal-foot theme-foot">
          <span style={{ color: "var(--text-muted)", fontSize: 12 }}>
            规则按 priority 升序匹配；首条命中即生效。
          </span>
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn" onClick={handleReset} disabled={busy}>
              重置为默认
            </button>
            <button className="btn primary" onClick={handleSave} disabled={busy}>
              保存
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}