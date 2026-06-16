import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionsStore, getSessionsState, type PersistedMessage } from "../stores/sessions";
import { sendChatStream } from "../lib/chat";
import { loadProviders, type ProviderInfo } from "../lib/providers";

type Props = {
  sessionId: string | null;
};

export function Composer({ sessionId }: Props) {
  const appendMessage = useSessionsStore((s) => s.appendMessage);
  const setMessages = useSessionsStore((s) => s.setMessages);

  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [model, setModel] = useState("MiniMax-M3");
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // 动态加载 provider 列表
  useEffect(() => {
    void loadProviders().then(setProviders);
  }, []);

  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = Math.min(ta.scrollHeight, 240) + "px";
  }, [text]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void onSend();
    }
    if (e.key === "Tab" && text.startsWith("/")) {
      e.preventDefault();
      const candidates = ["/help", "/status", "/clear", "/theme", "/model", "/usage", "/approval"];
      const match = candidates.find((c) => c.startsWith(text));
      if (match) setText(match);
    }
  };

  const onSend = async () => {
    if (!sessionId || !text.trim() || busy) return;

    // 1. 处理 slash 命令
    const trimmed = text.trim();
    if (trimmed === "/clear") {
      setMessages(sessionId, []);
      setText("");
      return;
    }
    if (trimmed.startsWith("/theme ")) {
      const mode = trimmed.slice(7).trim();
      const event = new CustomEvent("agentshell:theme", { detail: mode });
      window.dispatchEvent(event);
      setText("");
      return;
    }
    if (trimmed === "/help") {
      const helpMsg: PersistedMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        text: `📖 AgentShell v0.2 命令帮助：

通用：
/help    - 显示此帮助
/status  - 查看会话状态
/clear   - 清空当前会话
/usage   - Token 用量 + 费用估算

主题 & 界面：
/theme <light|dark|system> - 切主题

Git & IDE：
/ide     - 获取当前 IDE context（VSCode/Cursor）
/diff    - Git diff vs HEAD
/review  - AI 评审当前 diff（消耗 token）

💡 模型切换：Top bar 下拉
💡 工具调用：M3 / Claude 自动用 bash / read_file / write_file
💡 持久化：所有会话和消息自动保存到本地`,
        createdAt: Date.now(),
      };
      appendMessage(sessionId, helpMsg);
      setText("");
      return;
    }
    if (trimmed === "/status") {
      const statusMsg: PersistedMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        text: `📊 当前状态：

模型: ${model}
会话 ID: ${sessionId.slice(0, 8)}...

💡 完整 token 用量请用 /usage`,
        createdAt: Date.now(),
      };
      appendMessage(sessionId, statusMsg);
      setText("");
      return;
    }
    if (trimmed === "/usage") {
      // 从 store 取本会话累计
      const msgs = getSessionsState().messages[sessionId] || [];
      let totalIn = 0, totalOut = 0;
      const modelMsgs: Record<string, { in: number; out: number; count: number }> = {};
      for (const m of msgs) {
        if (m.role === "assistant" && (m.inputTokens || m.outputTokens)) {
          totalIn += m.inputTokens || 0;
          totalOut += m.outputTokens || 0;
          const k = model;
          if (!modelMsgs[k]) modelMsgs[k] = { in: 0, out: 0, count: 0 };
          modelMsgs[k].in += m.inputTokens || 0;
          modelMsgs[k].out += m.outputTokens || 0;
          modelMsgs[k].count += 1;
        }
      }
      // 估算费用
      const PRICE_PER_M: Record<string, { in: number; out: number }> = {
        "MiniMax-M3": { in: 0.60, out: 2.40 },
        "claude-opus-4-8": { in: 15, out: 75 },
        "claude-sonnet-4-5": { in: 3, out: 15 },
        "deepseek-v4-pro": { in: 0.27, out: 1.10 },
        "gpt-5.5": { in: 5, out: 15 },
      };
      const price = PRICE_PER_M[model] || { in: 0, out: 0 };
      const costUSD = (totalIn / 1_000_000) * price.in + (totalOut / 1_000_000) * price.out;
      const costCNY = costUSD * 7.2;
      const usageMsg: PersistedMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        text: `📊 本会话用量统计：

模型：${model}
轮次：${modelMsgs[model]?.count || 0}

输入 token：${totalIn.toLocaleString()}
输出 token：${totalOut.toLocaleString()}
合计：${(totalIn + totalOut).toLocaleString()}

单价（${model}）：
- 输入：$${price.in}/M
- 输出：$${price.out}/M

💰 估算费用：
- USD：$${costUSD.toFixed(4)}
- CNY：¥${costCNY.toFixed(4)}

💡 实际扣费以 M3 / OpenAI 后台账单为准。`,
        createdAt: Date.now(),
      };
      appendMessage(sessionId, usageMsg);
      setText("");
      return;
    }
    if (trimmed === "/ide") {
      try {
        const ctx = await invoke<{
          ide: string;
          currentFile: string | null;
          selection: string | null;
          cursorLine: number | null;
          cursorColumn: number | null;
        }>("get_ide_context");
        const ideMsg: PersistedMessage = {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🔌 IDE Context：

IDE: ${ctx.ide}${ctx.ide === "none" ? "（未检测到 — 请确认在 Cursor/VSCode 终端里运行）" : ""}
${ctx.currentFile ? `当前文件: ${ctx.currentFile}` : ""}
${ctx.cursorLine ? `光标位置: line ${ctx.cursorLine}, col ${ctx.cursorColumn}` : ""}
${ctx.selection ? `\n选中内容:\n\`\`\`\n${ctx.selection.slice(0, 500)}${ctx.selection.length > 500 ? "..." : ""}\n\`\`\`` : ""}`,
          createdAt: Date.now(),
        };
        appendMessage(sessionId, ideMsg);
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ 获取 IDE context 失败: ${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    if (trimmed === "/diff" || trimmed.startsWith("/diff ")) {
      try {
        const diff = await invoke<{ stat: string; diff: string; truncated: boolean }>("get_git_diff");
        const diffMsg: PersistedMessage = {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `📝 Git Diff（vs HEAD）：

${diff.stat || "(no changes)"}

\`\`\`diff
${diff.diff.slice(0, 5000)}${diff.truncated ? "\n... [truncated, view full in git]" : ""}
\`\`\``,
          createdAt: Date.now(),
        };
        appendMessage(sessionId, diffMsg);
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ 获取 git diff 失败: ${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    if (trimmed === "/review") {
      try {
        const diff = await invoke<{ stat: string; diff: string; truncated: boolean }>("get_git_diff");
        const reviewPrompt = `请 review 以下 git diff，重点关注：\n1. 代码 bug / 边界条件\n2. 安全漏洞\n3. 性能问题\n4. 可读性 / 命名\n\nDiff:\n\`\`\`\n${diff.diff.slice(0, 30000)}\n\`\`\``;
        const userMsg: PersistedMessage = {
          id: crypto.randomUUID(),
          role: "user",
          text: "/review",
          createdAt: Date.now(),
        };
        appendMessage(sessionId, userMsg);
        setText("");
        setBusy(true);
        // 复用流式 chat
        const assistantId = crypto.randomUUID();
        let acc = "", accThinking = "";
        let inT = 0, outT = 0;
        const stream = await sendChatStream({
          sessionId,
          userMessage: reviewPrompt,
          model,
        });
        for await (const chunk of stream) {
          if (chunk.kind === "content") acc += chunk.delta;
          else if (chunk.kind === "thinking") accThinking += chunk.delta;
          else if (chunk.kind === "done" && chunk.usage) {
            inT = chunk.usage.inputTokens;
            outT = chunk.usage.outputTokens;
          } else if (chunk.kind === "error") acc += `\n\n[错误] ${chunk.delta}`;
        }
        appendMessage(sessionId, {
          id: assistantId,
          role: "assistant",
          text: acc,
          thinking: accThinking || undefined,
          inputTokens: inT,
          outputTokens: outT,
          createdAt: Date.now(),
        });
        setBusy(false);
      } catch (e) {
        setBusy(false);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ review 失败: ${e}`,
          createdAt: Date.now(),
        });
      }
      return;
    }

    // 2. 普通消息 — 写入 user
    const userMsg: PersistedMessage = {
      id: crypto.randomUUID(),
      role: "user",
      text: trimmed,
      createdAt: Date.now(),
    };
    appendMessage(sessionId, userMsg);
    setText("");
    setBusy(true);

    // 3. 流式生成 assistant 响应（内存累积，最后一次性写入）
    const assistantId = crypto.randomUUID();
    let acc = "";
    let accThinking = "";
    let inputTokens: number | undefined;
    let outputTokens: number | undefined;

    try {
      const stream = await sendChatStream({
        sessionId,
        userMessage: trimmed,
        model,
      });

      for await (const chunk of stream) {
        if (chunk.kind === "content") {
          acc += chunk.delta;
        } else if (chunk.kind === "thinking") {
          accThinking += chunk.delta;
        } else if (chunk.kind === "done") {
          if (chunk.usage) {
            inputTokens = chunk.usage.inputTokens;
            outputTokens = chunk.usage.outputTokens;
          }
        } else if (chunk.kind === "error") {
          acc += `\n\n[错误] ${chunk.delta}`;
        }
      }

      // 一次性写入最终 assistant 消息
      const assistantMsg: PersistedMessage = {
        id: assistantId,
        role: "assistant",
        text: acc || "(empty response)",
        thinking: accThinking || undefined,
        createdAt: Date.now(),
        inputTokens,
        outputTokens,
      };
      appendMessage(sessionId, assistantMsg);
    } catch (e) {
      appendMessage(sessionId, {
        id: assistantId,
        role: "assistant",
        text: `[请求失败] ${String(e)}`,
        createdAt: Date.now(),
      });
    } finally {
      setBusy(false);
    }
  };

  const slashMenu = text.startsWith("/") && (
    <div className="slash-menu">
      {[
        { cmd: "/help", desc: "命令帮助" },
        { cmd: "/status", desc: "会话状态" },
        { cmd: "/clear", desc: "清空当前会话" },
        { cmd: "/theme dark", desc: "切到夜晚主题" },
        { cmd: "/theme light", desc: "切到白天主题" },
        { cmd: "/theme system", desc: "跟随系统主题" },
        { cmd: "/usage", desc: "Token 用量 + 费用估算" },
        { cmd: "/ide", desc: "IDE context（Cursor/VSCode）" },
        { cmd: "/diff", desc: "Git diff vs HEAD" },
        { cmd: "/review", desc: "AI 评审当前 diff" },
      ]
        .filter((c) => c.cmd.startsWith(text) || text === "/")
        .slice(0, 6)
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
          {providers.length === 0 && (
            <option value="MiniMax-M3">MiniMax M3 · $0.60/M</option>
          )}
          {providers.flatMap((p) =>
            p.models.map((m) => (
              <option key={m} value={m}>
                {m} · {p.name}
              </option>
            ))
          )}
        </select>
        <button className="composer-attachment" title="附件 (v0.3)" disabled>
          📎
        </button>
        <button
          className="composer-attachment"
          title="Computer Use (浏览器层)"
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