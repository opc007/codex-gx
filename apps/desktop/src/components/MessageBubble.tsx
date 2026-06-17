import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PersistedMessage } from "../stores/sessions";
import ReplayDialog, { type ToolCallRecord } from "./ReplayDialog";

type Props = {
  msg: PersistedMessage;
};

export function MessageBubble({ msg }: Props) {
  // v0.6：tool call 回放
  const [replayTarget, setReplayTarget] = useState<ToolCallRecord | null>(null);
  // v1.5：TTS 朗读
  const [speaking, setSpeaking] = useState(false);

  const handleSpeak = async () => {
    if (speaking) return;
    setSpeaking(true);
    try {
      await invoke("tts_speak", { text: msg.text });
    } catch (e) {
      console.warn("[tts] speak failed", e);
    } finally {
      setTimeout(() => setSpeaking(false), 2000);
    }
  };

  return (
    <div className={`bubble bubble-${msg.role}`}>
      <div className="bubble-meta">
        <span className="bubble-role">
          {msg.role === "user" ? "你" : msg.role === "assistant" ? "Codex gx" : msg.role}
        </span>
        <span className="bubble-time">
          {new Date(msg.createdAt).toLocaleTimeString()}
        </span>
        {msg.role === "assistant" && msg.text && !msg.streaming && (
          <button
            className="msg-speak"
            onClick={handleSpeak}
            disabled={speaking}
            title="朗读（v1.5 TTS）"
          >
            {speaking ? "🔊…" : "🔊"}
          </button>
        )}
        {msg.streaming && <span className="bubble-streaming">● 正在输入...</span>}
        {(msg.inputTokens || msg.outputTokens) && (
          <span className="bubble-tokens">
            🪙 ↑{msg.inputTokens || 0} ↓{msg.outputTokens || 0}
          </span>
        )}
      </div>
      <div className="bubble-content">
        {msg.thinking && (
          <details className="bubble-thinking" open={msg.thinking.length < 200}>
            <summary>💭 思考过程</summary>
            <pre>{msg.thinking}</pre>
          </details>
        )}
        {msg.toolCalls && msg.toolCalls.length > 0 && (
          <details className="bubble-tools" open>
            <summary>🛠️ 工具调用 ({msg.toolCalls.length})</summary>
            <div className="bubble-tools-list">
              {msg.toolCalls.map((tc) => (
                <div key={tc.id} className={`tool-call ${tc.success === false ? "tool-error" : tc.success ? "tool-success" : "tool-running"}`}>
                  <div className="tool-call-header">
                    <span className="tool-icon">
                      {tc.success === false ? "❌" : tc.success ? "✅" : "⏳"}
                    </span>
                    <code className="tool-name">{tc.name}</code>
                    {tc.success && <span className="tool-done">已完成</span>}
                    {/* v0.6：回放按钮（仅已完成或失败的） */}
                    {tc.success !== undefined && (
                      <button
                        className="tool-replay-btn"
                        title="重新执行（可修改参数）"
                        onClick={() =>
                          setReplayTarget({
                            id: tc.id,
                            name: tc.name,
                            arguments: tc.arguments,
                            result: tc.result,
                            success: tc.success,
                            error: tc.error,
                          })
                        }
                      >
                        🔁 回放
                      </button>
                    )}
                  </div>
                  <div className="tool-args">
                    <strong>参数：</strong>
                    <code>{JSON.stringify(tc.arguments, null, 2)}</code>
                  </div>
                  {tc.result !== undefined && (
                    <div className="tool-result">
                      <strong>结果：</strong>
                      <pre>{tc.result.slice(0, 2000)}{tc.result.length > 2000 ? "\n... [truncated]" : ""}</pre>
                    </div>
                  )}
                  {tc.error && (
                    <div className="tool-error-msg">
                      <strong>错误：</strong>
                      <pre>{tc.error}</pre>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </details>
        )}
        <div className="bubble-text">{msg.text}</div>
      </div>
      <ReplayDialog record={replayTarget} onClose={() => setReplayTarget(null)} />
    </div>
  );
}
