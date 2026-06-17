import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionsStore, getSessionsState, setSessionsState, type PersistedMessage } from "../stores/sessions";
import { sendChatStream } from "../lib/chat";
import { loadProviders, type ProviderInfo } from "../lib/providers";
import { StageTimeline, type Stage } from "./StageTimeline";

type Props = {
  sessionId: string | null;
};

export function Composer({ sessionId }: Props) {
  const appendMessage = useSessionsStore((s) => s.appendMessage);
  const setMessages = useSessionsStore((s) => s.setMessages);

  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [model, setModel] = useState("MiniMax-M3");
  const [requireApproval, setRequireApproval] = useState(true);
  // v0.6：plan mode
  const [planMode, setPlanMode] = useState(false);
  // v0.7：sub-agents
  const [subagents, setSubagents] = useState<Array<{
    id: string;
    role: string;
    status: string;
    task: string;
    result?: string | null;
    error?: string | null;
  }>>([]);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [stages, setStages] = useState<Stage[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    void loadProviders().then(setProviders);
  }, []);

  // busy 时初始化 / 重置 stages
  useEffect(() => {
    if (busy && stages.length === 0) {
      setStages([{ stage: "think", label: "正在启动...", detail: null, status: "running" }]);
    }
    if (!busy) {
      // 1.5s 后清空 stages
      const t = setTimeout(() => setStages([]), 1500);
      return () => clearTimeout(t);
    }
  }, [busy]);

  // v0.7：busy 开始时清空 subagents
  useEffect(() => {
    if (busy) {
      setSubagents([]);
    }
  }, [busy]);

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
        text: `📖 AgentShell v0.4 命令帮助：

通用：
/help      - 显示此帮助
/status    - 查看会话状态
/clear     - 清空当前会话
/usage     - Token 用量 + 费用估算

主题 & 界面：
/theme <light|dark|system> - 切主题

Git & IDE：
/ide       - 获取当前 IDE context（VSCode/Cursor）
/diff      - Git diff vs HEAD
/review    - AI 评审当前 diff（消耗 token）

🛠️ v0.4 工具调用：
M3 / Claude / GPT 会自动调用：
- bash（执行命令）
- read_file / write_file / edit_file
- list_dir
- web_search（需要 BRAVE_API_KEY）
- browser_navigate / browser_screenshot / browser_click / browser_type / browser_get_text（Computer Use，需要 Node + playwright）

🔐 v0.4 批准模式：
/approval on  - 启用手动批准（默认）
/approval off - 自动批准所有工具调用
/approval     - 查看当前模式

📋 v0.6 Plan Mode：
/plan on  - 启用（下次发送先看模型给的执行计划）
/plan off - 关闭
/plan     - 查看当前模式

🧠 v0.8 长期记忆：
/remember <内容> [#tag]  - 记住一条
/memories                  - 列出所有
/recall <查询>             - 检索相关
/forget <id前8位>          - 遗忘一条
（自动：每次新会话根据当前消息检索相关记忆注入 system prompt）

🛠️ v0.8 Skill：
/skills                    - 列出已加载的自定义 skill
/<name> <参数>             - 执行 ~/.agentshell/skills.json 定义的命令

💡 模型切换：Top bar 下拉
💡 所有会话和消息自动保存到本地`,
        createdAt: Date.now(),
      };
      appendMessage(sessionId, helpMsg);
      setText("");
      return;
    }
    if (trimmed === "/approval" || trimmed.startsWith("/approval ")) {
      const arg = trimmed.slice(10).trim();
      if (arg === "on") {
        setRequireApproval(true);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🔐 已开启手动批准模式。每个 tool call 会弹模态框让你确认。",
          createdAt: Date.now(),
        });
      } else if (arg === "off") {
        setRequireApproval(false);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "⚡ 已关闭手动批准模式。所有 tool call 自动执行（适合信任模型时）。",
          createdAt: Date.now(),
        });
      } else {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🔐 当前批准模式：**${requireApproval ? "手动（on）" : "自动（off）"}**\n\n切换：/approval on | /approval off`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    // v0.6：plan mode 开关
    if (trimmed === "/plan" || trimmed.startsWith("/plan ")) {
      const arg = trimmed.slice(5).trim();
      if (arg === "on") {
        setPlanMode(true);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "📋 已开启 plan mode。下次发送时会先让模型输出执行计划等你批准。",
          createdAt: Date.now(),
        });
      } else if (arg === "off") {
        setPlanMode(false);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "📋 已关闭 plan mode。",
          createdAt: Date.now(),
        });
      } else {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `📋 当前 plan mode：**${planMode ? "on" : "off"}**\n\n切换：/plan on | /plan off`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    // v0.7：测试模型路由
    if (trimmed.startsWith("/route ")) {
      const query = trimmed.slice(7).trim();
      if (!query) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🧭 用法：`/route <任务描述>`\n例如：`/route 写一个 Python 函数计算 fib`",
          createdAt: Date.now(),
        });
      } else {
        try {
          const routed = await invoke<string>("route_model_cmd", { message: query });
          const reason = routed.includes("deepseek")
            ? "检测到代码任务 → DeepSeek（便宜 + 代码强）"
            : routed.includes("claude")
            ? "检测到规划/分析任务 → Claude Sonnet 4.5"
            : routed.includes("MiniMax") || routed.includes("m3")
            ? "默认对话/创意任务 → MiniMax M3"
            : "默认 → MiniMax M3";
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🧭 路由结果：**${routed}**\n${reason}`,
            createdAt: Date.now(),
          });
        } catch (e) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🧭 路由失败：${e}`,
            createdAt: Date.now(),
          });
        }
      }
      setText("");
      return;
    }
    // v0.8：长期记忆命令
    if (trimmed.startsWith("/remember ") || trimmed === "/remember") {
      const content = trimmed.slice(9).trim();
      if (!content) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🧠 用法：`/remember <内容>`\n例如：`/remember 用户偏好 Rust 编程`",
          createdAt: Date.now(),
        });
      } else {
        try {
          const tags = content.match(/#[一-龥\w]+/g)?.map((t) => t.slice(1)) ?? [];
          const mem = await invoke<{ id: string; importance: number }>("remember_memory", {
            content: content.replace(/#[一-龥\w]+/g, "").trim(),
            tags,
            importance: 3,
            sessionId,
          });
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🧠 已记住 [#${mem.id.slice(0, 8)}] ${content}${tags.length > 0 ? `\n标签：${tags.join(", ")}` : ""}`,
            createdAt: Date.now(),
          });
        } catch (e) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🧠 记住失败：${e}`,
            createdAt: Date.now(),
          });
        }
      }
      setText("");
      return;
    }
    if (trimmed === "/memories" || trimmed.startsWith("/memories ")) {
      try {
        const all = await invoke<Array<{
          id: string;
          content: string;
          importance: number;
          tags: string[];
          accessedCount: number;
        }>>("list_memories");
        if (all.length === 0) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: "🧠 还没有任何长期记忆。用 `/remember <内容>` 添加。",
            createdAt: Date.now(),
          });
        } else {
          let msg = `🧠 长期记忆 (${all.length} 条)：\n\n`;
          for (const m of all.slice(-20)) {
            const tags = m.tags.length > 0 ? ` [${m.tags.join(", ")}]` : "";
            msg += `• [#${m.id.slice(0, 8)}] (重要度 ${m.importance}/5, 访问 ${m.accessedCount})${tags}\n  ${m.content}\n\n`;
          }
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: msg,
            createdAt: Date.now(),
          });
        }
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🧠 列表失败：${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    if (trimmed.startsWith("/recall ")) {
      const query = trimmed.slice(8).trim();
      if (!query) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🧠 用法：`/recall <查询>`",
          createdAt: Date.now(),
        });
      } else {
        try {
          const result = await invoke<string>("recall_memory", { query, k: 5 });
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: result || "🧠 没有找到相关记忆。",
            createdAt: Date.now(),
          });
        } catch (e) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🧠 检索失败：${e}`,
            createdAt: Date.now(),
          });
        }
      }
      setText("");
      return;
    }
    if (trimmed.startsWith("/forget ")) {
      const id = trimmed.slice(8).trim();
      if (!id) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🧠 用法：`/forget <记忆 id 前 8 位>`",
          createdAt: Date.now(),
        });
      } else {
        try {
          const all = await invoke<Array<{ id: string }>>("list_memories");
          const match = all.find((m) => m.id.startsWith(id));
          if (!match) {
            appendMessage(sessionId, {
              id: crypto.randomUUID(),
              role: "assistant",
              text: `🧠 找不到 id 以 "${id}" 开头的记忆。`,
              createdAt: Date.now(),
            });
          } else {
            await invoke("forget_memory", { id: match.id });
            appendMessage(sessionId, {
              id: crypto.randomUUID(),
              role: "assistant",
              text: `🧠 已遗忘 [${match.id.slice(0, 8)}]`,
              createdAt: Date.now(),
            });
          }
        } catch (e) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🧠 遗忘失败：${e}`,
            createdAt: Date.now(),
          });
        }
      }
      setText("");
      return;
    }
    if (trimmed === "/skills" || trimmed.startsWith("/skills ")) {
      try {
        const list = await invoke<Array<{ name: string; description: string }>>("list_skills");
        if (list.length === 0) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🛠️ 还没有自定义 skill。在 ~/.agentshell/skills.json 加：

示例:
{
  "skills": [
    {"name": "commit", "description": "Git 提交", "shell": "git add -A && git commit -m \"$ARG\""}
  ]
}

调用: /commit <消息>`,
            createdAt: Date.now(),
          });
        } else {
          let msg = `已加载 ${list.length} 个 skill：\n\n`;
          for (const s of list) {
            msg += `- /${s.name}: ${s.description}\n`;
          }
          msg += `\n调用方式: /<name> <参数>`;
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: msg,
            createdAt: Date.now(),
          });
        }
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🛠️ 加载失败：${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    // v0.8：动态 skill 调用（先看 list_skills，匹配则执行）
    if (trimmed.startsWith("/") && !trimmed.startsWith("//")) {
      const m = trimmed.match(/^\/(\S+)\s*(.*)$/);
      if (m) {
        const name = m[1];
        const arg = m[2];
        // 检查是否已知命令（避免误调用）
        const known = new Set([
          "help", "status", "approval", "plan", "route", "remember", "memories", "recall", "forget", "skills",
          "usage", "ide", "diff", "review",
        ]);
        if (!known.has(name)) {
          try {
            const r = await invoke<string>("run_skill", { name, arg });
            appendMessage(sessionId, {
              id: crypto.randomUUID(),
              role: "assistant",
              text: r,
              createdAt: Date.now(),
            });
            setText("");
            return;
          } catch (e) {
            // 不是 skill，继续常规处理
          }
        }
      }
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
        const assistantId = crypto.randomUUID();
        let acc = "", accThinking = "";
        let inT = 0, outT = 0;
        const { stream } = await sendChatStream({
          sessionId,
          userMessage: reviewPrompt,
          model,
          requireApproval,
          planMode,
        });
        for await (const evt of stream) {
          if (evt.kind === "content") acc += evt.delta;
          else if (evt.kind === "thinking") accThinking += evt.delta;
          else if (evt.kind === "done" && evt.usage) {
            inT = evt.usage.inputTokens;
            outT = evt.usage.outputTokens;
          } else if (evt.kind === "error") acc += `\n\n[错误] ${evt.delta}`;
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

    // 2. 普通消息 — v0.3 用 agent_run + 监听 agent:event
    const userMsg: PersistedMessage = {
      id: crypto.randomUUID(),
      role: "user",
      text: trimmed,
      createdAt: Date.now(),
    };
    appendMessage(sessionId, userMsg);
    setText("");
    setBusy(true);

    const assistantId = crypto.randomUUID();
    let acc = "";
    let accThinking = "";
    let inputTokens: number | undefined;
    let outputTokens: number | undefined;
    const toolCalls: NonNullable<PersistedMessage["toolCalls"]> = [];
    let streamingAssistantId: string | null = null;

    try {
      // 先写一个空的 streaming assistant 消息
      const placeholder: PersistedMessage = {
        id: assistantId,
        role: "assistant",
        text: "",
        createdAt: Date.now(),
        streaming: true,
      };
      appendMessage(sessionId, placeholder);
      streamingAssistantId = assistantId;

      const { stream } = await sendChatStream({
        sessionId,
        userMessage: trimmed,
        model,
        requireApproval,
        planMode,
      });

      for await (const evt of stream) {
        if (evt.kind === "content") {
          acc += evt.delta;
          if (streamingAssistantId) {
            updateAssistantMessage(sessionId, streamingAssistantId, { text: acc, thinking: accThinking, toolCalls: [...toolCalls] });
          }
        } else if (evt.kind === "thinking") {
          accThinking += evt.delta;
          if (streamingAssistantId) {
            updateAssistantMessage(sessionId, streamingAssistantId, { text: acc, thinking: accThinking, toolCalls: [...toolCalls] });
          }
        } else if (evt.kind === "tool_call_complete" && evt.toolCall) {
          // 新增 tool call
          toolCalls.push({
            id: evt.toolCall.id,
            name: evt.toolCall.name,
            arguments: evt.toolCall.arguments,
          });
          if (streamingAssistantId) {
            updateAssistantMessage(sessionId, streamingAssistantId, { text: acc, thinking: accThinking, toolCalls: [...toolCalls] });
          }
        } else if (evt.kind === "tool_result" && evt.toolResult) {
          // 回填到对应的 toolCall
          const tc = toolCalls.find((t) => t.id === evt.toolResult!.callId);
          if (tc) {
            tc.result = evt.toolResult.output;
            tc.success = evt.toolResult.success;
            tc.error = evt.toolResult.error || undefined;
          }
          if (streamingAssistantId) {
            updateAssistantMessage(sessionId, streamingAssistantId, { text: acc, thinking: accThinking, toolCalls: [...toolCalls] });
          }
        } else if (evt.kind === "stage" && evt.stage) {
          // v0.5：更新流程图
          setStages((prev) => {
            const stageKey = evt.stage!.stage;
            // 找到同 stage 的最后一项，更新 status
            const newStages = [...prev];
            for (let i = newStages.length - 1; i >= 0; i--) {
              if (newStages[i].stage === stageKey) {
                newStages[i] = {
                  ...newStages[i],
                  label: evt.stage!.label,
                  detail: evt.stage!.detail ?? newStages[i].detail,
                  status: "done",
                };
                break;
              }
            }
            // 如果是新 stage（approval 等），追加
            const lastStage = newStages[newStages.length - 1];
            if (!lastStage || lastStage.stage !== stageKey || stageKey === "approval" || stageKey === "act") {
              newStages.push({
                stage: stageKey,
                label: evt.stage!.label,
                detail: evt.stage!.detail ?? null,
                status: "running",
              });
            }
            return newStages;
          });
        } else if (evt.kind === "subagent" && evt.subagent) {
          // v0.7：更新 subagent 状态
          setSubagents((prev) => {
            const existing = prev.find((s) => s.id === evt.subagent!.subagentId);
            if (existing) {
              return prev.map((s) =>
                s.id === evt.subagent!.subagentId
                  ? {
                      ...s,
                      status: evt.subagent!.status,
                      result: evt.subagent!.result ?? s.result,
                      error: evt.subagent!.error ?? s.error,
                    }
                  : s
              );
            } else {
              return [
                ...prev,
                {
                  id: evt.subagent!.subagentId,
                  role: evt.subagent!.role,
                  status: evt.subagent!.status,
                  task: evt.subagent!.task,
                  result: evt.subagent!.result,
                  error: evt.subagent!.error,
                },
              ];
            }
          });
        } else if (evt.kind === "done") {
          if (evt.usage) {
            inputTokens = evt.usage.inputTokens;
            outputTokens = evt.usage.outputTokens;
          }
          // 标记所有 stage done
          setStages((prev) => prev.map((s) => ({ ...s, status: "done" as const })));
        } else if (evt.kind === "error") {
          acc += `\n\n[错误] ${evt.delta}`;
          setStages((prev) => prev.map((s) => (s.status === "running" ? { ...s, status: "error" as const } : s)));
        }
      }

      if (streamingAssistantId) {
        updateAssistantMessage(sessionId, streamingAssistantId, {
          text: acc || "(empty response)",
          thinking: accThinking || undefined,
          streaming: false,
          inputTokens,
          outputTokens,
          toolCalls: [...toolCalls],
        });
      }
    } catch (e) {
      if (streamingAssistantId) {
        updateAssistantMessage(sessionId, streamingAssistantId, {
          text: `[请求失败] ${String(e)}`,
          streaming: false,
          toolCalls: [...toolCalls],
        });
      } else {
        appendMessage(sessionId, {
          id: assistantId,
          role: "assistant",
          text: `[请求失败] ${String(e)}`,
          createdAt: Date.now(),
        });
      }
    } finally {
      setBusy(false);
    }
  };

  // helper: 更新 streaming assistant 消息
  function updateAssistantMessage(
    sid: string,
    mid: string,
    patch: Partial<PersistedMessage>
  ) {
    const cur = getSessionsState();
    const list = cur.messages[sid] || [];
    const idx = list.findIndex((m) => m.id === mid);
    if (idx < 0) return;
    const newList = [...list];
    newList[idx] = { ...newList[idx], ...patch };
    setSessionsState({
      messages: { ...cur.messages, [sid]: newList },
    });
  }

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
      {stages.length > 0 && <StageTimeline stages={stages} />}
      {subagents.length > 0 && (
        <div className="composer-subagents">
          {subagents.map((sa) => (
            <div key={sa.id} className={`subagent-card subagent-${sa.status}`}>
              <span className="subagent-icon">
                {sa.role === "researcher" ? "🔍" : sa.role === "coder" ? "💻" : sa.role === "reviewer" ? "👀" : "🤖"}
              </span>
              <span className="subagent-role">{sa.role}</span>
              <span className="subagent-task">{sa.task.slice(0, 60)}{sa.task.length > 60 ? "…" : ""}</span>
              <span className={`subagent-status subagent-status-${sa.status}`}>
                {sa.status === "started" ? "启动中" : sa.status === "running" ? "运行中" : sa.status === "done" ? "✓ 完成" : sa.status === "error" ? "✗ 错误" : sa.status}
              </span>
              {sa.result && (
                <details className="subagent-result">
                  <summary>查看结果</summary>
                  <pre>{sa.result.slice(0, 2000)}{sa.result.length > 2000 ? "\n... [truncated]" : ""}</pre>
                </details>
              )}
              {sa.error && (
                <div className="subagent-error">❌ {sa.error}</div>
              )}
            </div>
          ))}
        </div>
      )}
      <div className="composer-toolbar">
        <select
          className="composer-model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          disabled={busy}
        >
          {/* v0.7：auto 路由 */}
          <option value="auto">🧭 Auto 路由（按任务选 model）</option>
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
        <button className="composer-attachment" title="附件 (v0.4)" disabled>
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
          {busy ? "正在生成（可能含工具调用）..." : `${text.length} 字符`}
        </span>
        <span className={`composer-approval ${requireApproval ? "approval-on" : "approval-off"}`}
              title="工具调用批准模式"
              onClick={() => setRequireApproval(!requireApproval)}>
          {requireApproval ? "🔐 批准" : "⚡ 自动"}
        </span>
        {/* v0.6：plan mode 切换 */}
        <span className={`composer-plan ${planMode ? "plan-on" : "plan-off"}`}
              title="Plan Mode：先让模型输出执行计划再批准"
              onClick={() => setPlanMode(!planMode)}>
          {planMode ? "📋 Plan" : "📋"}
        </span>
        {busy ? (
          <button
            className="composer-cancel"
            onClick={async () => {
              try {
                await invoke("cancel_chat", { sessionId });
              } catch (e) {
                console.warn("cancel failed:", e);
              }
            }}
          >
            ⏹ 停止
          </button>
        ) : (
          <button
            className="composer-send"
            disabled={!sessionId || !text.trim()}
            onClick={onSend}
          >
            发送 ⏎
          </button>
        )}
      </div>
    </div>
  );
}