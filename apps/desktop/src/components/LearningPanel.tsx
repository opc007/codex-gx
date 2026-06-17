// v1.4：Agent 学习 / 个性化面板

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Learning = {
  signals: {
    total_chats: number;
    total_tool_calls: number;
    total_messages: number;
    model_usage: Record<string, number>;
    tool_usage: Record<string, number>;
    slash_usage: Record<string, number>;
    prompt_length_buckets: Record<string, number>;
    positive_feedback: number;
    negative_feedback: number;
    languages: Record<string, number>;
    frequent_tokens: Record<string, number>;
    hours: Record<string, number>;
  };
  preferences: {
    default_model: string | null;
    favorite_tools: string[];
    favorite_slash_commands: string[];
    preferred_language: string | null;
    typical_prompt_length: string | null;
    active_hours: number[];
    confidence: number;
  };
  updated_at: number;
  created_at: number;
};

type Props = {
  onClose: () => void;
};

export function LearningPanel({ onClose }: Props) {
  const [data, setData] = useState<Learning | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    setBusy(true);
    try {
      const l = await invoke<Learning>("learning_get");
      setData(l);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const handleReset = async () => {
    if (!confirm("确定重置所有学习数据？此操作不可撤销。")) return;
    await invoke("learning_reset");
    await refresh();
  };

  const handleFeedback = async (positive: boolean) => {
    await invoke("learning_record_feedback", { positive });
    await refresh();
  };

  const sortedByValue = (m: Record<string, number>) =>
    Object.entries(m).sort((a, b) => b[1] - a[1]);

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 820, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🧠 Agent 学习 / 个性化（v1.4）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">×</button>
        </div>

        <div className="modal-body theme-body">
          {data ? (
            <>
              {/* 偏好摘要 */}
              <div className="learning-section">
                <h4>📊 当前推断的偏好</h4>
                <div className="learning-prefs">
                  <div className="learning-pref">
                    <span className="learning-pref-label">默认模型</span>
                    <code>{data.preferences.default_model ?? "—"}</code>
                  </div>
                  <div className="learning-pref">
                    <span className="learning-pref-label">偏好语言</span>
                    <code>{data.preferences.preferred_language ?? "—"}</code>
                  </div>
                  <div className="learning-pref">
                    <span className="learning-pref-label">典型提示长度</span>
                    <code>{data.preferences.typical_prompt_length ?? "—"}</code>
                  </div>
                  <div className="learning-pref">
                    <span className="learning-pref-label">活跃时段</span>
                    <code>
                      {data.preferences.active_hours.length > 0
                        ? data.preferences.active_hours
                            .map((h) => `${h}:00`)
                            .join(", ")
                        : "—"}
                    </code>
                  </div>
                  <div className="learning-pref">
                    <span className="learning-pref-label">置信度</span>
                    <div className="learning-bar">
                      <div
                        className="learning-bar-fill"
                        style={{
                          width: `${data.preferences.confidence * 100}%`,
                        }}
                      />
                      <span>{Math.round(data.preferences.confidence * 100)}%</span>
                    </div>
                  </div>
                </div>
              </div>

              {/* 统计 */}
              <div className="learning-section">
                <h4>📈 行为统计</h4>
                <div className="learning-stats">
                  <div className="learning-stat">
                    <span className="learning-stat-n">{data.signals.total_chats}</span>
                    <span className="learning-stat-l">总 chat 数</span>
                  </div>
                  <div className="learning-stat">
                    <span className="learning-stat-n">{data.signals.total_messages}</span>
                    <span className="learning-stat-l">总消息数</span>
                  </div>
                  <div className="learning-stat">
                    <span className="learning-stat-n">{data.signals.total_tool_calls}</span>
                    <span className="learning-stat-l">工具调用</span>
                  </div>
                  <div className="learning-stat">
                    <span className="learning-stat-n">👍 {data.signals.positive_feedback}</span>
                    <span className="learning-stat-l">正面反馈</span>
                  </div>
                  <div className="learning-stat">
                    <span className="learning-stat-n">👎 {data.signals.negative_feedback}</span>
                    <span className="learning-stat-l">负面反馈</span>
                  </div>
                </div>
              </div>

              {/* 详细使用 */}
              <div className="learning-section">
                <h4>🔧 工具使用 Top 10</h4>
                <table className="learning-table">
                  <tbody>
                    {sortedByValue(data.signals.tool_usage)
                      .slice(0, 10)
                      .map(([k, v]) => (
                        <tr key={k}>
                          <td>
                            <code>{k}</code>
                          </td>
                          <td>
                            <div className="learning-bar">
                              <div
                                className="learning-bar-fill"
                                style={{
                                  width: `${Math.min(100, v * 10)}%`,
                                }}
                              />
                              <span>{v}</span>
                            </div>
                          </td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </div>

              <div className="learning-section">
                <h4>⌨️ 命令使用</h4>
                <table className="learning-table">
                  <tbody>
                    {sortedByValue(data.signals.slash_usage).map(([k, v]) => (
                      <tr key={k}>
                        <td>
                          <code>{k}</code>
                        </td>
                        <td>{v} 次</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              <div className="learning-section">
                <h4>🌐 模型使用</h4>
                <table className="learning-table">
                  <tbody>
                    {sortedByValue(data.signals.model_usage).map(([k, v]) => (
                      <tr key={k}>
                        <td>
                          <code>{k}</code>
                        </td>
                        <td>{v} 次</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              <div className="learning-section">
                <h4>📝 常用词 Top 20</h4>
                <div className="learning-tokens">
                  {sortedByValue(data.signals.frequent_tokens)
                    .slice(0, 20)
                    .map(([k, v]) => (
                      <span key={k} className="learning-token" title={`出现 ${v} 次`}>
                        {k} <small>{v}</small>
                      </span>
                    ))}
                </div>
              </div>

              <div className="learning-actions">
                <button className="btn" onClick={() => handleFeedback(true)}>
                  👍 这次回答不错
                </button>
                <button className="btn" onClick={() => handleFeedback(false)}>
                  👎 这次回答不好
                </button>
                <button className="btn small" onClick={refresh} disabled={busy}>
                  🔄 刷新
                </button>
                <div style={{ flex: 1 }} />
                <button className="btn danger" onClick={handleReset}>
                  🗑 重置
                </button>
              </div>
            </>
          ) : (
            <p>加载中…</p>
          )}

          <div className="lint-help">
            <h4>💡 工作方式</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li>每次 chat / 工具调用 / slash 命令都会记录到 ~/.agentshell/learning.json</li>
              <li>每次 chat 完成后会自动重算偏好</li>
              <li>推断的偏好会作为"用户偏好"附加到 system prompt 里</li>
              <li>完整透明 — 随时可看、可重置、可点 👍/👎 反馈</li>
              <li>所有数据存在本地，不上传任何服务器</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}