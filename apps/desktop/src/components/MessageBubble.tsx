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
        {msg.role === "assistant" && msg.modelUsed && !msg.streaming && (
          <span className="bubble-model" title="本条回复实际调用的模型">
            {msg.modelUsed}
          </span>
        )}
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
        <div className="bubble-text"><RichText text={msg.text} /></div>
        {msg.imageUrl && (
          <a href={msg.imageUrl} target="_blank" rel="noreferrer" title="点击查看原图">
            <img
              src={msg.imageUrl}
              alt="screenshot"
              className="bubble-inline-image"
              loading="lazy"
            />
          </a>
        )}
        {/* v1.9.6: 多模态生图 */}
        {msg.mediaGallery && msg.mediaGallery.length > 0 && (
          <div className="media-gallery">
            {msg.mediaGallery.map((u, i) => (
              <a key={i} href={u} target="_blank" rel="noreferrer" title="点击查看原图 / 右键保存">
                <img src={u} alt={`generated-${i}`} loading="lazy" />
              </a>
            ))}
          </div>
        )}
        {/* v1.9.6: 多模态生视频 */}
        {msg.mediaVideo && (
          <div className="media-video">
            <video src={msg.mediaVideo} controls preload="metadata" />
            <div className="media-video-actions">
              <a href={msg.mediaVideo} target="_blank" rel="noreferrer" download>
                ⬇️ 下载视频
              </a>
            </div>
          </div>
        )}
      </div>
      <ReplayDialog record={replayTarget} onClose={() => setReplayTarget(null)} />
    </div>
  );
}

// v1.9.6：Rich text renderer（Codex App 风格：图片 / 视频 URL inline 渲染）
// - 支持 ![alt](url) 标准 markdown
// - 裸 URL（https://...png|jpg|jpeg|gif|webp|mp4|webm）自动转 <img> / <video>
const IMAGE_RE = /(https?:\/\/[^\s)]+\.(?:png|jpg|jpeg|gif|webp|svg))(\?[^\s)]*)?/gi;
const VIDEO_RE = /(https?:\/\/[^\s)]+\.(?:mp4|webm|mov))(\?[^\s)]*)?/gi;
const MD_IMAGE_RE = /!\[([^\]]*)\]\((https?:\/\/[^\s)]+)\)/g;

function RichText({ text }: { text: string }) {
  if (!text) return null;
  // 先抽出 markdown 图片 ![alt](url) —— 替换为占位
  const placeholders: Array<{ kind: "img" | "video"; url: string; alt?: string }> = [];
  let src = text.replace(MD_IMAGE_RE, (_m, alt: string, url: string) => {
    placeholders.push({ kind: "img", url, alt });
    return `\u0000IMG${placeholders.length - 1}\u0000`;
  });
  src = src.replace(VIDEO_RE, (m) => {
    placeholders.push({ kind: "video", url: m });
    return `\u0000VID${placeholders.length - 1}\u0000`;
  });
  src = src.replace(IMAGE_RE, (m) => {
    placeholders.push({ kind: "img", url: m });
    return `\u0000IMG${placeholders.length - 1}\u0000`;
  });

  // 简易：保留换行；不替换其他 markdown（避免破坏现有体验）
  const parts = src.split(/(\u0000(?:IMG|VID)\d+\u0000)/g);
  return (
    <>
      {parts.map((part, i) => {
        const m = part.match(/^\u0000(?:IMG|VID)(\d+)\u0000$/);
        if (!m) {
          return <span key={i} style={{ whiteSpace: "pre-wrap" }}>{part}</span>;
        }
        const idx = Number(m[1]);
        const item = placeholders[idx];
        if (!item) return null;
        if (item.kind === "video") {
          return (
            <div key={i} className="media-video inline">
              <video src={item.url} controls preload="metadata" />
              <a href={item.url} target="_blank" rel="noreferrer" download>
                ⬇️ 下载
              </a>
            </div>
          );
        }
        return (
          <a
            key={i}
            href={item.url}
            target="_blank"
            rel="noreferrer"
            title="点击查看原图 / 右键保存"
            style={{ display: "block", margin: "6px 0" }}
          >
            <img
              src={item.url}
              alt={item.alt ?? "image"}
              loading="lazy"
              style={{ maxWidth: "100%", borderRadius: 6, border: "1px solid var(--border)" }}
            />
          </a>
        );
      })}
    </>
  );
}
