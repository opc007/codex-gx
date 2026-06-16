import { useEffect, useRef, useState } from "react";
import { appendMessage, updateMessage } from "./Thread";
import type { ChatMessage } from "../lib/chat";
import { sendChatStream } from "../lib/chat";

type Props = {
  sessionId: string | null;
};

export function Composer({ sessionId }: Props) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [model, setModel] = useState("MiniMax-M3");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // 自动 resize
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = Math.min(ta.scrollHeight, 240) + "px";
  }, [text]);

  // slash 命令快捷
  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void onSend();
    }
    if (e.key === "Tab" && text.startsWith("/")) {
      e.preventDefault();
      // 简单补全
      const candidates = ["/help", "/status", "/clear", "/theme", "/model", "/usage"];
      const match = candidates.find((c) => c.startsWith(text));
      if (match) setText(match);
    }
  };

  const onSend = async () => {
    if (!sessionId || !text.trim() || busy) return;
    const msg: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      text: text.trim(),
      createdAt: Date.now(),
    };
    appendMessage(sessionId, msg);
    setText("");
    setBusy(true);

    // 准备 assistant 消息占位
    const assistantId = crypto.randomUUID();
    const assistant: ChatMessage = {
      id: assistantId,
      role: "assistant",
      text: "",
      thinking: "",
      createdAt: Date.now(),
      streaming: true,
    };
    appendMessage(sessionId, assistant);

    try {
      const stream = await sendChatStream({
        sessionId,
        userMessage: msg.text,
        model,
      });

      let acc = "";
      let accThinking = "";
      for await (const chunk of stream) {
        if (chunk.kind === "content") {
          acc += chunk.delta;
          updateMessage(sessionId, assistantId, { text: acc });
        } else if (chunk.kind === "thinking") {
          accThinking += chunk.delta;
          updateMessage(sessionId, assistantId, { thinking: accThinking });
        } else if (chunk.kind === "done") {
          updateMessage(sessionId, assistantId, { streaming: false });
        } else if (chunk.kind === "error") {
          updateMessage(sessionId, assistantId, {
            text: acc + "\n\n[错误] " + chunk.delta,
            streaming: false,
          });
        }
      }
    } catch (e) {
      updateMessage(sessionId, assistantId, {
        text: `[请求失败] ${String(e)}`,
        streaming: false,
      });
    } finally {
      setBusy(false);
    }
  };

  // slash 命令快捷
  const slashMenu = text.startsWith("/") && (
    <div className="slash-menu">
      {[
        { cmd: "/help", desc: "帮助" },
        { cmd: "/status", desc: "查看状态" },
        { cmd: "/clear", desc: "清空当前会话" },
        { cmd: "/theme", desc: "切换主题" },
        { cmd: "/model MiniMax-M3", desc: "切到 M3" },
        { cmd: "/model claude-opus-4-8", desc: "切到 Claude" },
        { cmd: "/usage", desc: "查看用量" },
      ]
        .filter((c) => c.cmd.startsWith(text) || text === "/")
        .slice(0, 5)
        .map((c) => (
          <div
            key={c.cmd}
            className="slash-item"
            onClick={() => {
              setText(c.cmd);
              textareaRef.current?.focus();
            }}
          >
            <code>{c.cmd}</code>
            <span>{c.desc}</span>
          </div>
        ))}
    </div>
  );

  return (
    <div className="composer">
      {slashMenu}
      <div className="composer-toolbar">
        <select
          className="composer-model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          disabled={busy}
        >
          <option value="MiniMax-M3">MiniMax M3 · $0.60/M · 1M ctx</option>
          <option value="claude-opus-4-8">Claude Opus 4.8 · $15/M</option>
          <option value="claude-sonnet-4-5">Claude Sonnet 4.5 · $3/M</option>
          <option value="deepseek-v4-pro">DeepSeek V4 Pro · $0.27/M</option>
          <option value="gpt-5.5">GPT-5.5 · $5/M</option>
        </select>
        <button className="composer-attachment" title="附件 (v0.2)">
          📎
        </button>
        <button
          className="composer-attachment"
          title="Computer Use (v0.1)"
          disabled
        >
          🖥️
        </button>
      </div>
      <textarea
        ref={textareaRef}
        className="composer-input"
        placeholder={
          sessionId
            ? "输入消息，Enter 发送，Shift+Enter 换行，/ 命令..."
            : "请先创建会话"
        }
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
        disabled={!sessionId || busy}
        rows={1}
      />
      <div className="composer-footer">
        <span className="composer-hint">
          {busy ? "正在生成..." : `${text.length} 字符`}
        </span>
        <button
          className="composer-send"
          disabled={!sessionId || busy || !text.trim()}
          onClick={onSend}
        >
          发送 ⏎
        </button>
      </div>
    </div>
  );
}