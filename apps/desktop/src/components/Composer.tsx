import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useSessionsStore, getSessionsState, setSessionsState, type PersistedMessage } from "../stores/sessions";
import { useTranslation, setLocale as i18nSetLocale } from "../i18n";
import { redactSimple, detectTypes } from "../lib/redact";
import { sendChatStream } from "../lib/chat";
import { buildChatHistory } from "../lib/chatHistory";
import { loadProviders, type ProviderInfo } from "../lib/providers";
import { StageTimeline, type Stage } from "./StageTimeline";

type Props = {
  sessionId: string | null;
};

export function Composer({ sessionId }: Props) {
  const t = useTranslation();
  const appendMessage = useSessionsStore((s) => s.appendMessage);
  const setMessages = useSessionsStore((s) => s.setMessages);

  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [model, setModel] = useState("MiniMax-M3");
  const [requireApproval, setRequireApproval] = useState(true);
  // v0.9：附件图片
  const [attachedImages, setAttachedImages] = useState<Array<{ path: string; mime: string; name: string }>>([]);
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

  // v1.2：voice input 状态
  const [recording, setRecording] = useState(false);
  const [voiceBusy, setVoiceBusy] = useState(false);
  const [voiceHint, setVoiceHint] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const recordedChunksRef = useRef<Blob[]>([]);

  useEffect(() => {
    void loadProviders().then(setProviders);
  }, []);

  // v1.2：监听 voice 模型下载进度
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ model: string; pct: number; downloaded: number; total: number; done: boolean; error: string | null }>(
      "voice:download_progress",
      (e) => {
        const p = e.payload;
        if (p.error) {
          setVoiceHint(`下载失败: ${p.error}`);
        } else if (p.done) {
          setVoiceHint(`✅ 模型 ${p.model} 下载完成 (${(p.downloaded / 1024 / 1024).toFixed(1)} MB)`);
          if (sessionId) {
            appendMessage(sessionId, {
              id: crypto.randomUUID(),
              role: "assistant",
              text: `✅ 模型 ${p.model} 下载完成`,
              createdAt: Date.now(),
            });
          }
        } else {
          setVoiceHint(`⏬ 下载 ${p.model}: ${(p.pct * 100).toFixed(1)}% (${(p.downloaded / 1024 / 1024).toFixed(1)}/${(p.total / 1024 / 1024).toFixed(1)} MB)`);
        }
      },
    ).then((u) => (unlisten = u));
    return () => {
      unlisten?.();
    };
  }, [sessionId, appendMessage]);

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

  // v1.2：voice input 处理
  const startRecording = async () => {
    if (recording) return;
    setVoiceHint(null);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      // 优先选 webm/opus；whisper-cli 接受 wav，但我们输出 webm 也行——浏览器只支持 webm/ogg
      const mime = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
        ? "audio/webm;codecs=opus"
        : "audio/webm";
      const mr = new MediaRecorder(stream, { mimeType: mime });
      recordedChunksRef.current = [];
      mr.ondataavailable = (e) => {
        if (e.data.size > 0) recordedChunksRef.current.push(e.data);
      };
      mr.onstop = async () => {
        stream.getTracks().forEach((t) => t.stop());
        setRecording(false);
        const blob = new Blob(recordedChunksRef.current, { type: mime });
        await transcribeBlob(blob);
      };
      mr.start();
      mediaRecorderRef.current = mr;
      setRecording(true);
    } catch (e: any) {
      setVoiceHint(`麦克风访问失败: ${e?.message || e}`);
    }
  };

  const stopRecording = () => {
    if (mediaRecorderRef.current && mediaRecorderRef.current.state === "recording") {
      mediaRecorderRef.current.stop();
    }
  };

  const transcribeBlob = async (blob: Blob) => {
    setVoiceBusy(true);
    setVoiceHint("转写中…");
    try {
      const buf = await blob.arrayBuffer();
      const bytes = new Uint8Array(buf);
      // 转 base64（分块避免调用栈溢出）
      let bin = "";
      const chunk = 0x8000;
      for (let i = 0; i < bytes.length; i += chunk) {
        bin += String.fromCharCode.apply(
          null,
          Array.from(bytes.subarray(i, i + chunk)),
        );
      }
      const base64 = btoa(bin);
      const r = await invoke<{ text: string; model: string; elapsed_ms: number }>(
        "voice_transcribe",
        { args: { base64, filename: "rec.webm", model: null } },
      );
      setVoiceHint(null);
      if (r.text.trim()) {
        // 追加到现有文本
        setText((prev) => (prev ? prev + " " + r.text : r.text));
      } else {
        setVoiceHint("未识别到语音");
      }
    } catch (e: any) {
      setVoiceHint(`转写失败: ${e}`);
    } finally {
      setVoiceBusy(false);
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
    // v1.4：学习面板
    if (trimmed === "/learn" || trimmed.startsWith("/learn ")) {
      const args = trimmed.slice(6).trim();
      try {
        if (args === "reset") {
          await invoke("learning_reset");
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: "🧠 学习数据已重置。",
            createdAt: Date.now(),
          });
        } else if (args === "feedback" || args.startsWith("feedback ")) {
          const sub = args.slice("feedback".length).trim();
          const positive = !/bad|down|negative/i.test(sub);
          await invoke("learning_record_feedback", { positive });
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `🧠 反馈已记录：${positive ? "👍 正面" : "👎 负面"}`,
            createdAt: Date.now(),
          });
        } else {
          const l = await invoke<{
            signals: { total_chats: number; total_tool_calls: number; positive_feedback: number; negative_feedback: number };
            preferences: { default_model: string | null; confidence: number; favorite_tools: string[] };
          }>("learning_get");
          let text = `🧠 **学习数据**\n\n`;
          text += `- 总 chat: ${l.signals.total_chats} | 工具调用: ${l.signals.total_tool_calls}\n`;
          text += `- 👍 ${l.signals.positive_feedback} / 👎 ${l.signals.negative_feedback}\n`;
          text += `- 默认模型: ${l.preferences.default_model ?? "—"}\n`;
          text += `- 常用工具: ${l.preferences.favorite_tools.join(", ") || "—"}\n`;
          text += `- 置信度: ${Math.round(l.preferences.confidence * 100)}%\n\n`;
          text += `💡 用法: \n  - \`/learn\` 看统计\n  - \`/learn feedback\` / \`/learn feedback bad\` 反馈\n  - \`/learn reset\` 重置\n  - Top bar 🧠 打开完整面板`;
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text,
            createdAt: Date.now(),
          });
        }
        setText("");
        return;
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ 失败: ${e}`,
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
    }
    // v1.5：插件
    if (trimmed === "/plugin" || trimmed === "/plugins") {
      window.dispatchEvent(new CustomEvent("open-plugin-panel"));
      appendMessage(sessionId, {
        id: crypto.randomUUID(),
        role: "assistant",
        text: "🧩 已触发：打开插件面板（请点 TopBar 🧩）",
        createdAt: Date.now(),
      });
      return;
    }

    // v1.5：同步当前 session
    if (trimmed === "/sync") {
      const sid = sessionId;
      try {
        const cur = getSessionsState();
        const all = cur.sessions;
        const msgs = cur.messages;
        const sess = all.find((s) => s.id === sid);
        if (!sess) {
          appendMessage(sid, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: "⚠️ 没有当前 session",
            createdAt: Date.now(),
          });
          return;
        }
        const version = await invoke<number>("sync_schema_version");
        const bundle = {
          schema_version: version,
          session_id: sess.id,
          title: sess.title,
          created_at: sess.createdAt,
          updated_at: sess.updatedAt,
          owner_id: sess.ownerId ?? null,
          workspace_id: sess.workspaceId ?? null,
          messages: msgs[sess.id] || [],
          source_device: navigator.platform || "device",
          synced_at: Date.now(),
        };
        await invoke("sync_publish", { bundle });
        const list = await invoke<{
          cached: number;
          total_size: number;
          sessions: Array<{ title: string; size: number }>;
        }>("sync_list");
        appendMessage(sid, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `☁️ 同步完成：${list.cached} 个 session · ${(list.total_size / 1024).toFixed(1)} KB\n最新：${sess.title}`,
          createdAt: Date.now(),
        });
      } catch (e) {
        appendMessage(sid, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ ${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }

    // v1.5：流程图
    if (trimmed === "/flow") {
      window.dispatchEvent(new CustomEvent("open-flow-panel"));
      appendMessage(sessionId, {
        id: crypto.randomUUID(),
        role: "assistant",
        text: "🕸️ 已触发：打开流程图（请点 TopBar 🕸️）",
        createdAt: Date.now(),
      });
      return;
    }

    // v1.5：TTS 设置
    if (trimmed === "/tts") {
      window.dispatchEvent(new CustomEvent("open-tts-panel"));
      appendMessage(sessionId, {
        id: crypto.randomUUID(),
        role: "assistant",
        text: "🔊 已触发：打开 TTS 面板（请点 TopBar 🔊）",
        createdAt: Date.now(),
      });
      return;
    }

    // v1.5：朗读
    if (trimmed.startsWith("/speak ") || trimmed.startsWith("/say ")) {
      const args = trimmed.startsWith("/speak ")
        ? trimmed.slice(7).trim()
        : trimmed.slice(5).trim();
      try {
        const cfg = await invoke<{ enabled: boolean }>("tts_get_config");
        if (!cfg.enabled) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: "⚠️ TTS 未启用，先用 /tts 打开设置（TopBar 🔊）",
            createdAt: Date.now(),
          });
          return;
        }
        await invoke("tts_speak", { text: args || "（空）" });
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `▶ 朗读中：${args.slice(0, 40)}${args.length > 40 ? "…" : ""}`,
          createdAt: Date.now(),
        });
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ ${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }

    // v1.4：本地 LLM 探测
    if (trimmed.startsWith("/local ") || trimmed === "/local") {
      const args = trimmed.slice(6).trim();
      try {
        const ollamaUrl = args || "http://127.0.0.1:11434";
        const d = await invoke<{
          ollama_models: Array<{ name: string; size: number | null }>;
          llamacpp_models: Array<{ id: string }>;
          ollama_error: string | null;
          llamacpp_error: string | null;
        }>("local_discover", {
          ollamaUrl,
          llamacppUrl: null,
        });
        let text = `🏠 **本地模型**\n\n🦙 Ollama (${ollamaUrl}): ${d.ollama_models.length} 个模型`;
        if (d.ollama_error) text += `\n  ⚠️ ${d.ollama_error}`;
        for (const m of d.ollama_models.slice(0, 20)) {
          const size = m.size ? `${(m.size / 1e9).toFixed(2)}GB` : "?";
          text += `\n  - \`ollama:${m.name}\` (${size})`;
        }
        text += `\n\n🐑 llama.cpp: ${d.llamacpp_models.length} 个模型`;
        for (const m of d.llamacpp_models.slice(0, 20)) {
          text += `\n  - \`llamacpp:${m.id}\``;
        }
        text += `\n\n💡 也可点击 Top bar 🏠 打开完整 UI`;
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text,
          createdAt: Date.now(),
        });
        setText("");
        return;
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ 探测失败: ${e}`,
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
    }
    // v1.4：代码 review
    if (trimmed.startsWith("/lint ") || trimmed === "/lint") {
      const args = trimmed.slice(5).trim() || ".";
      try {
        const s = await invoke<{
          total_errors: number;
          total_warnings: number;
          total_infos: number;
          total_ms: number;
          reports: Array<{
            checker: string;
            summary: string;
            duration_ms: number;
            skipped_reason: string | null;
            issues: Array<{
              file: string;
              line: number | null;
              severity: "error" | "warning" | "info";
              code: string | null;
              message: string;
            }>;
          }>;
        }>("lint_run_summary", { path: args });
        let text = `🔍 **代码 review (${args})**\n\n`;
        text += `❌ error: ${s.total_errors}  ⚠️ warning: ${s.total_warnings}  ℹ️ info: ${s.total_infos}  (${(s.total_ms / 1000).toFixed(1)}s)\n\n`;
        for (const r of s.reports) {
          text += `**${r.checker}** — ${r.summary}`;
          if (r.skipped_reason) text += ` ⏭️ ${r.skipped_reason}`;
          text += `\n`;
          for (const i of r.issues.slice(0, 8)) {
            const icon = i.severity === "error" ? "❌" : i.severity === "warning" ? "⚠️" : "ℹ️";
            const loc = i.line ? `:${i.line}` : "";
            text += `  ${icon} ${i.file.split("/").slice(-2).join("/")}${loc} ${i.code ?? ""} ${i.message.slice(0, 60)}\n`;
          }
          if (r.issues.length > 8) {
            text += `  …还有 ${r.issues.length - 8} 项\n`;
          }
          text += `\n`;
        }
        text += `💡 Top bar 🔍 打开完整 UI`;
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text,
          createdAt: Date.now(),
        });
        setText("");
        return;
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ 失败: ${e}`,
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
    }
    // v1.4：任务队列
    if (trimmed.startsWith("/queue ") || trimmed === "/queue") {
      const args = trimmed.slice(6).trim();
      try {
        if (args) {
          // 入队命令
          const id = await invoke<string>("queue_enqueue", {
            args: {
              kind: "command",
              title: args.slice(0, 40),
              input: { cmd: args },
              session_id: sessionId,
              description: null,
            },
          });
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `📋 任务已入队: \`${id.slice(0, 8)}\`\n\n$ ${args}\n\n💡 Top bar 📋 打开队列面板看进度`,
            createdAt: Date.now(),
          });
          setText("");
          return;
        } else {
          // 列出
          const list = await invoke<Array<{
            id: string;
            status: string;
            kind: string;
            title: string;
            progress: number;
          }>>("queue_list");
          let text = `📋 **任务队列** (${list.length})\n\n`;
          for (const t of list.slice(-15)) {
            const icon = t.status === "completed" ? "✅" : t.status === "running" ? "▶️" : t.status === "failed" ? "❌" : t.status === "cancelled" ? "🚫" : "⏳";
            text += `${icon} \`${t.id.slice(0, 8)}\` ${t.kind} — ${t.title} (${Math.round(t.progress * 100)}%)\n`;
          }
          if (list.length === 0) text += `（空）\n`;
          text += `\n💡 用法: \`/queue <shell 命令>\`  — 后台跑命令不阻塞当前 chat`;
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text,
            createdAt: Date.now(),
          });
          setText("");
          return;
        }
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ 失败: ${e}`,
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
    }
    // v1.3：路由测试
    if (trimmed.startsWith("/route ") || trimmed === "/route") {
      const args = trimmed.slice(6).trim();
      if (!args) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🧭 用法:\n  /route <消息文本>  - 看会被路由到哪个 model\n\n或在 Top bar 点击 🧭 打开策略编辑器",
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
      try {
        const r = await invoke<{
          primary_provider: string;
          primary_model: string;
          fallbacks: Array<{ provider: string; model: string }>;
          reason: string;
          rule_id: string | null;
        }>("routing_decide", {
          args: { message: args, task_type: null },
        });
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🧭 **路由决策**\n\n主目标: **${r.primary_provider}/${r.primary_model}**\n\n${r.reason}${
            r.rule_id ? `\n命中规则: \`${r.rule_id}\`` : ""
          }${
            r.fallbacks.length > 0
              ? `\n\n兜底链: ${r.fallbacks
                  .map((f) => `${f.provider}/${f.model}`)
                  .join(" → ")}`
              : ""
          }`,
          createdAt: Date.now(),
        });
        setText("");
        return;
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ 路由失败: ${e}`,
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
    }
    // v1.1：脱敏测试
    if (trimmed.startsWith("/redact ") || trimmed === "/redact") {
      const input = trimmed.slice(8).trim();
      if (!input) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🔒 用法: `/redact <文本>` — 测试脱敏。\n\n会检测：api key、email、IPv4、JWT、bearer token、PEM 私钥、64 字符 hex",
          createdAt: Date.now(),
        });
      } else {
        const types = detectTypes(input);
        const out = redactSimple(input);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🔒 脱敏结果：\n\n检测到: ${types.length > 0 ? types.join(", ") : "无"}\n\n输入:\n${input}\n\n输出:\n${out}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    // v1.2：voice 状态 + 下载模型
    if (trimmed === "/voice" || trimmed.startsWith("/voice ")) {
      const arg = trimmed.slice(6).trim();
      if (!arg || arg === "status") {
        try {
          const status = await invoke<any>("voice_check");
          const lines = [
            `🎙 Voice (v1.2)`,
            `whisper-cli: ${status.cli_available ? "✅ " + status.cli_path : "❌ 未找到"}`,
            ...(status.cli_hint ? [`💡 ${status.cli_hint}`] : []),
            `默认模型: ${status.default_model ?? "(无)"}`,
            `已下载模型:`,
            ...status.models.map(
              (m: any) =>
                `  ${m.downloaded ? "✅" : "  "} ${m.name}  ${m.display_name}  (${m.description})`,
            ),
            "",
            "用法: /voice download <模型名>    下载模型",
            "     /voice cleanup             清理临时文件",
            "     /voice status              查看状态",
          ];
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: lines.join("\n"),
            createdAt: Date.now(),
          });
        } catch (e: any) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `❌ ${e}`,
            createdAt: Date.now(),
          });
        }
        setText("");
        return;
      }
      if (arg.startsWith("download ")) {
        const model = arg.slice(9).trim();
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `⏬ 开始下载模型 ${model}…（窗口下方会显示进度）`,
          createdAt: Date.now(),
        });
        // 不 await，让下载在后台跑，进度通过 voice:download_progress 事件显示
        invoke("voice_download_model", { args: { model } }).catch((e) => {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `❌ 下载失败: ${e}`,
            createdAt: Date.now(),
          });
        });
        setText("");
        return;
      }
      if (arg === "cleanup") {
        await invoke("voice_cleanup");
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🧹 临时文件已清理",
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
    }
    // v1.0：长会话压缩
    if (trimmed === "/compress" || trimmed.startsWith("/compress ")) {
      const arg = trimmed.slice(9).trim();
      const keepRecent = arg ? parseInt(arg, 10) || 6 : 6;
      const all = (getSessionsState().messages[sessionId] ?? [])
        .filter((m) => m.role === "user" || m.role === "assistant")
        .filter((m) => m.text);
      if (all.length <= keepRecent + 2) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🗜 消息数 (${all.length}) 太少，无需压缩（需要 > ${keepRecent + 2}）。\n\n语法：/compress [保留最近 N 条]`,
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
      try {
        const result = await invoke<{
          summary: string;
          originalCount: number;
          summaryCount: number;
          newMessages: Array<{ role: string; content: string }>;
        }>("compress_session", {
          req: {
            model: model === "auto" ? "MiniMax-M3" : model,
            messages: all.map((m) => ({ role: m.role, content: m.text })),
            keepRecent,
          },
        });
        // 用 summary + 最近 N 条替换当前 session 消息
        // 但 history 还包含 [之前摘要] system + keepRecent user/assistant
        // 我们重建：从 store 拿全 messages，把 keepRecent 之前的都丢掉，前缀加一条 [summary] user msg
        const fullList = getSessionsState().messages[sessionId] ?? [];
        const cutoff = fullList.length - keepRecent;
        const kept = fullList.slice(cutoff);
        const summaryMsg: PersistedMessage = {
          id: crypto.randomUUID(),
          role: "system",
          text: `🗜 [之前对话摘要]\n${result.summary}\n\n---\n\n（以下 ${keepRecent} 条为最近对话）`,
          createdAt: Date.now(),
        };
        setMessages(sessionId, [summaryMsg, ...kept]);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🗜 压缩完成：${result.originalCount} → ${kept.length + 1} 条\n\n摘要：\n${result.summary}`,
          createdAt: Date.now(),
        });
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `🗜 压缩失败：${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
    }
    // v0.9：i18n 切换
    if (trimmed.startsWith("/lang ") || trimmed === "/lang") {
      const arg = trimmed.slice(5).trim();
      if (arg === "zh" || arg === "en") {
        i18nSetLocale(arg);
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: arg === "zh" ? "🌐 已切换到中文" : "🌐 Switched to English",
          createdAt: Date.now(),
        });
      } else {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "🌐 用法: /lang zh | /lang en  (或在 Top bar 切换)",
          createdAt: Date.now(),
        });
      }
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
        text: `📖 Codex gx v1.4 命令帮助：

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

🛠️ 工具调用：
M3 / Claude / GPT 会自动调用：
- bash（执行命令）
- read_file / write_file / edit_file
- list_dir
- web_search（需要 BRAVE_API_KEY）
- browser_navigate / browser_screenshot / browser_click / browser_type / browser_get_text（Computer Use，需要 Node + playwright）
- desktop_list_windows / desktop_focus_window / desktop_get_app_tree / desktop_click_at / desktop_type_text / desktop_key_combo / desktop_screenshot（v1.2 Desktop CUA）
- mcp__<server>__<tool>（v0.5 MCP）
- subagent_<role>（v0.7 子代理）

🔐 批准模式：
/approval on  - 启用手动批准（默认）
/approval off - 自动批准所有工具调用
/approval     - 查看当前模式

📋 Plan Mode：
/plan on  - 启用（下次发送先看模型给的执行计划）
/plan off - 关闭
/plan     - 查看当前模式

🧠 长期记忆：
/remember <内容> [#tag]  - 记住一条
/memories                  - 列出所有
/recall <查询>             - 检索相关
/forget <id前8位>          - 遗忘一条
（自动：每次新会话根据当前消息检索相关记忆注入 system prompt）

🛠️ Skill：
/skills                    - 列出已加载的自定义 skill
/<name> <参数>             - 执行 ~/.agentshell/skills.json 定义的命令

🗜 长会话压缩：
/compress [保留最近N条]    - 用 LLM 摘要压缩当前会话（默认保留 6 条）

🔒 脱敏测试：
/redact <文本>             - 测试脱敏（api key/email/IPv4/JWT/PEM...）

🎙 语音输入（本地 Whisper）：
/voice status              - 查看 whisper-cli 状态和模型
/voice download <模型名>   - 下载 Whisper 模型（tiny/base/small/medium）
/voice cleanup             - 清理临时音频
按钮: 🎙 - 录音；⏹ - 结束；转写文本自动填到输入框

📁 v1.3 工作区（Workspace）：
- Top bar 左侧下拉切换 / 新建 / 重命名 / 删除工作区
- 每个工作区拥有独立 sessions 列表
- 默认 "Default"，可创建任意多个
- 删除工作区不会删除其中的 session（可切回 Default 查看）

🧩 插件市场：Top bar 🧩 按钮
🔐 会话加密：侧栏 session 旁 🔒 / 🔓 按钮
📊 Token 用量：/usage
💡 模型切换：Top bar 下拉
💡 所有会话和消息自动保存到本地

🏠 v1.4 本地 LLM：
- /local [ollama_url]   - 探测本机 Ollama 模型
- /tts                  - 打开 TTS 语音输出设置
- /speak <text>         - 朗读一段文本（需先在 TopBar 🔊 启用 TTS）
- /flow                 - 打开 Agent 流程图（v1.5）
- /sync                 - 同步当前 session 到本地缓存（v1.5）
- /plugin               - 打开插件热加载面板（v1.5）
- Top bar 🏠 打开模型管理 UI
- 模型 ID 格式：ollama:<name> / llamacpp:<name>
- 自动 discover Ollama (http://127.0.0.1:11434) 和 llama.cpp server

🔍 v1.4 代码 review：
- /lint [path]          - 运行 clippy / tsc / TODO 扫描
- Top bar 🔍 打开完整 UI（按 file/severity 分组，可筛选）
- 自动跳过 node_modules / target / dist / .git

📋 v1.4 任务队列：
- /queue                - 列出所有任务
- /queue <cmd>          - 后台跑 shell 命令不阻塞 chat
- Top bar 📋 打开队列面板
- 支持 cancel / clear finished / 实时进度事件

📡 v1.4 P2P 设备协同：
- Top bar 📡 打开设备面板
- 启动主机生成 6 位配对码
- 客户端输入 IP:port + 配对码连接
- 自动 mDNS 广播（macOS / Linux）

🧠 v1.4 Agent 学习：
- /learn                - 查看学习统计
- /learn feedback       - 👍 正面反馈
- /learn feedback bad   - 👎 负面反馈
- /learn reset          - 重置所有学习数据
- Top bar 🧠 打开学习面板
- 自动跟踪模型 / 工具 / 命令 / 提示长度 / 语言`,
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
    // v1.4：本地 LLM 探测
    // v1.4：代码 review
    // v1.4：任务队列
    // v1.4：学习面板 — see /learn above
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
    // v1.8: /ps (背景进程列表)
    if (trimmed === "/ps" || trimmed.startsWith("/ps ")) {
      const arg = trimmed.slice(3).trim();
      try {
        if (arg === "") {
          const list = await invoke<Array<{ id: string; label: string; command: string; statusLabel: string; pid: number; logPath: string | null }>>("bg_list");
          if (list.length === 0) {
            appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "🟢 没有后台进程" });
          } else {
            const txt = list.map((b) => `  ${b.statusLabel} **${b.label}** (pid ${b.pid}) — ${b.command.slice(0, 50)}\n    id: ${b.id.slice(0, 12)}  log: ${b.logPath ?? "—"}`).join("\n\n");
            appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📋 后台进程 (${list.length})：\n\n${txt}\n\n命令：/stop <id> | /ps <id>` });
          }
        } else {
          const b = await invoke<{ id: string; label: string; command: string; statusLabel: string; pid: number; logPath: string | null; tail: string } | null>("bg_get", { id: arg });
          if (b) {
            const tailTrim = b.tail.length > 2000 ? b.tail.slice(-2000) : b.tail;
            appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📋 **${b.label}** (${b.statusLabel})\n\n  pid:    ${b.pid}\n  cmd:    ${b.command}\n  log:    ${b.logPath ?? "—"}\n  id:     ${b.id}\n\n--- 最近输出 ---\n\`\`\`\n${tailTrim || "(无输出)"}\n\`\`\`` });
          } else {
            appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ 没找到 id：${arg}` });
          }
        }
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /ps 失败: ${e}` });
      }
      return;
    }

    // v1.8: /stop
    if (trimmed === "/stop" || trimmed.startsWith("/stop ")) {
      const arg = trimmed.slice(5).trim();
      try {
        if (arg === "" || arg === "all") {
          const n = await invoke<number>("bg_stop_all");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🛑 停了 ${n} 个后台进程` });
        } else {
          const ok = await invoke<boolean>("bg_stop", { id: arg });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: ok ? `🛑 Stopped ${arg}` : `❌ 没找到：${arg}` });
        }
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /stop 失败: ${e}` });
      }
      return;
    }

    // v1.8: /bg (启动后台进程)
    if (trimmed.startsWith("/bg ") || trimmed.startsWith("/background ")) {
      const raw = trimmed.startsWith("/bg ") ? trimmed.slice(4).trim() : trimmed.slice(12).trim();
      const m = raw.match(/^"([^"]+)"\s+(.+)$/);
      let label: string, rest: string;
      if (m) { label = m[1]; rest = m[2]; }
      else {
        const parts = raw.split(/\s+/);
        label = parts[0];
        rest = parts.slice(1).join(" ");
      }
      const tokens = rest.split(/\s+/);
      if (tokens.length === 0) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /bg <label> <command> [args...]" });
        return;
      }
      const cmd = tokens[0];
      const args = tokens.slice(1);
      try {
        const b = await invoke<{ id: string; pid: number; logPath: string | null }>("bg_spawn", { args: { label, command: cmd, args } });
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🚀 后台启动：**${label}**\n\n  pid: ${b.pid}\n  id:  ${b.id}\n  log: ${b.logPath ?? "—"}\n\n命令：/ps 看状态、/stop ${b.id.slice(0, 12)} 关掉` });
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /bg 失败: ${e}` });
      }
      return;
    }

    // v1.8: /fork
    if (trimmed === "/fork" || trimmed.startsWith("/fork ")) {
      const label = trimmed.slice(5).trim();
      const s = getSessionsState().fork(label || undefined);
      if (s) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🍴 Fork 创建：${s.title}\n\n  id:       ${s.id}\n  parentId: ${s.parentId}\n  point:    ${s.forkPointMessageId ?? "—"}\n\n已切换到 fork session。原 session 历史保留。` });
      } else {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ /fork 失败：没有当前 session" });
      }
      return;
    }

    // v1.8: /side (旁问)
    if (trimmed.startsWith("/side ") || trimmed === "/side") {
      const question = trimmed.slice(5).trim();
      if (!question) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /side <问题>（临时旁问，24h 后过期）" });
        return;
      }
      const s = getSessionsState().side(question);
      appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `💬 Side 旁问创建："${question.slice(0, 40)}${question.length > 40 ? "…" : ""}"\n\n  id: ${s.id}\n  expires: 24h 后\n\n已切换到 side session。主 thread 历史保持干净。` });
      return;
    }

    // v1.8: /voice (流式 TTS)
    if (trimmed.startsWith("/voice ") || trimmed === "/voice") {
      const text = trimmed.slice(6).trim();
      if (!text) {
        const st = await invoke<{ currentSession: number; maxConcurrent: number; supportedVoices: string[] }>("voice_duplex_status");
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🔊 Voice duplex 状态：\n\n  current: ${st.currentSession}\n  max:     ${st.maxConcurrent}\n  voices:  ${st.supportedVoices.join(", ")}\n\n用法：/voice <文本> | /voice --voice nova <文本>` });
        return;
      }
      let voice: string | undefined;
      const voiceMatch = text.match(/--voice\s+(\w+)/);
      if (voiceMatch) voice = voiceMatch[1];
      const cleanText = text.replace(/--voice\s+\w+/, "").trim();
      const sid = await invoke<number>("voice_duplex_start", { args: { text: cleanText, voice } });
      appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🔊 Voice duplex 启动 — session ${sid}\n\n  text:  ${cleanText.slice(0, 80)}${cleanText.length > 80 ? "…" : ""}\n  voice: ${voice ?? "alloy"}\n  监听 \`voice:duplex:event\` 事件拿 chunk` });
      return;
    }

    // v1.9: /screenshot
    if (trimmed === "/screenshot" || trimmed === "/ss" || trimmed.startsWith("/screenshot ")) {
      try {
        const meta = await invoke<{ width: number; height: number; scale: number; displayId: string; dataBase64: string; format: string }>("screen_screenshot");
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📸 截图完成：\n\n  display: ${meta.displayId}\n  size:    ${meta.width}×${meta.height} (scale ${meta.scale})\n  format:  ${meta.format}\n  data:    ${meta.dataBase64.length} bytes (base64)\n\n（小提示：M3 用 0.0-1.0 相对坐标，不是 0-1000 整数。AgentShell 已自动换算）` });
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /screenshot 失败: ${e}` });
      }
      return;
    }

    // v1.9: /coord (相对坐标换算)
    if (trimmed.startsWith("/coord ") || trimmed === "/coord") {
      const arg = trimmed.slice(6).trim();
      if (!arg) {
        const screens = await invoke<Array<{ displayId: string; physicalWidth: number; physicalHeight: number; scale: number }>>("screen_list");
        const txt = screens.map((s) => `  **${s.displayId}** ${s.physicalWidth}×${s.physicalHeight} (scale ${s.scale})`).join("\n");
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🖥️ Screens：\n\n${txt || "（无）"}\n\n用法：/coord <x> <y>  (0.0-1.0 相对坐标)` });
        return;
      }
      const m = arg.match(/^([\d.]+)\s+([\d.]+)$/);
      if (!m) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /coord <x> <y>  (0.0-1.0)" });
        return;
      }
      try {
        const abs = await invoke<{ logicalX: number; logicalY: number; physicalX: number; physicalY: number; displayId: string }>("screen_to_absolute", { args: { x: parseFloat(m[1]), y: parseFloat(m[2]) } });
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🎯 (${m[1]}, ${m[2]}) →\n\n  display:   ${abs.displayId}\n  physical:  (${abs.physicalX}, ${abs.physicalY})\n  logical:   (${abs.logicalX}, ${abs.logicalY})` });
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /coord 失败: ${e}` });
      }
      return;
    }

    // v1.9: /perm (权限管理)
    if (trimmed === "/perm" || trimmed.startsWith("/perm ")) {
      const arg = trimmed.slice(5).trim();
      try {
        if (arg === "" || arg === "list") {
          const l = await invoke<{ alwaysAllow: string[]; alwaysAsk: string[]; denied: string[]; usageCount: Record<string, number> }>("perm_get_list");
          const aa = l.alwaysAllow.map((a) => `  ✅ ${a}${l.usageCount[a] ? ` (${l.usageCount[a]}次)` : ""}`).join("\n");
          const ask = l.alwaysAsk.map((a) => `  ❓ ${a}`).join("\n");
          const d = l.denied.map((a) => `  ⛔ ${a}`).join("\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🔐 Desktop 权限列表：\n\n**Always allow (${l.alwaysAllow.length})**:\n${aa || "  (无)"}\n\n**Always ask (${l.alwaysAsk.length})**:\n${ask || "  (无)"}\n\n**Denied (${l.denied.length})**:\n${d || "  (无)"}\n\n**强制黑名单**：银行 / 支付 / 证券 / 密码管理 / 2FA（不可移除）\n\n命令：/perm allow <name> | /perm deny <name> | /perm clear` });
        } else if (arg === "clear") {
          await invoke("perm_clear_allow");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "🧹 Always allow 列表已清空" });
        } else if (arg.startsWith("allow ")) {
          const key = arg.slice(6).trim();
          await invoke("perm_add_allow", { key });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `✅ **${key}** 已加入 always_allow` });
        } else if (arg.startsWith("deny ")) {
          const key = arg.slice(5).trim();
          await invoke("perm_add_deny", { key });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `⛔ **${key}** 已加入 denied` });
        } else if (arg === "protocol") {
          const prompt = await invoke<string>("screen_protocol_prompt");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📋 M3 Computer Use 协议（注入 system prompt）：\n\n\`\`\`\n${prompt}\n\`\`\`` });
        } else {
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /perm list | allow <name> | deny <name> | clear | protocol" });
        }
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /perm 失败: ${e}` });
      }
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
          // v1.8
          "ps", "stop", "bg", "background", "fork", "side", "voice",
          // v1.9
          "screenshot", "ss", "coord", "perm",
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
          history: buildChatHistory(sessionId, [userMsg.id]),
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
    // v1.4：记录学习信号（不阻塞 UI）
    void invoke("learning_record_chat", {
      model: model,
      userMsg: trimmed,
    }).catch(() => {});
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

      // v0.9：附件随本次消息发送
      const imagesToSend = attachedImages.map((img) => ({ path: img.path, mime: img.mime }));
      setAttachedImages([]);

      const { stream } = await sendChatStream({
        sessionId,
        userMessage: trimmed,
        model,
        requireApproval,
        planMode,
        images: imagesToSend,
        history: buildChatHistory(sessionId, [userMsg.id, assistantId]),
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
          className={`composer-attachment composer-voice ${recording ? "recording" : ""} ${voiceBusy ? "busy" : ""}`}
          title={
            recording
              ? "点击停止录音"
              : voiceBusy
                ? "转写中…"
                : "语音输入（v1.2：本地 Whisper）"
          }
          disabled={voiceBusy}
          onClick={() => {
            if (recording) stopRecording();
            else void startRecording();
          }}
        >
          {recording ? "⏹" : voiceBusy ? "⏳" : "🎙"}
        </button>
      </div>
      {(recording || voiceHint) && (
        <div className="composer-voice-status">
          {recording && "🔴 正在录音… 再次点击结束"}
          {voiceHint && !recording && `🔊 ${voiceHint}`}
        </div>
      )}
      {attachedImages.length > 0 && (
        <div className="composer-attachments">
          {attachedImages.map((img, i) => (
            <span key={i} className="composer-attachment-chip" title={img.path}>
              🖼 {img.name}
              <button
                className="chip-remove"
                onClick={() => setAttachedImages((prev) => prev.filter((_, j) => j !== i))}
              >×</button>
            </span>
          ))}
        </div>
      )}
      <textarea
        ref={textareaRef}
        className="composer-input"
        placeholder={sessionId ? t.placeholder : t.noSessionPlaceholder}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
        disabled={!sessionId || busy}
        rows={1}
        onDragOver={(e) => { e.preventDefault(); }}
        onDrop={async (e) => {
          e.preventDefault();
          // Tauri 2 webview drop 通常给的是 file path
          // @ts-ignore — DataTransferItem 在 webview 中带 path
          const items = e.dataTransfer?.files;
          if (!items) return;
          for (let i = 0; i < items.length; i++) {
            const f = items[i] as File & { path?: string };
            // Tauri 提供 .path 字段（绝对路径）
            const p = (f as any).path ?? "";
            if (!p) continue;
            const lower = p.toLowerCase();
            const mime = lower.endsWith(".png") ? "image/png"
              : lower.endsWith(".jpg") || lower.endsWith(".jpeg") ? "image/jpeg"
              : lower.endsWith(".gif") ? "image/gif"
              : lower.endsWith(".webp") ? "image/webp" : "image/png";
            const name = p.split("/").pop() || p;
            setAttachedImages((prev) => [...prev, { path: p, mime, name }]);
          }
        }}
      />
      <div className="composer-footer">
        <span className="composer-hint">
          {busy ? "正在生成（可能含工具调用）..." : `${text.length} 字符`}
        </span>
        <span className={`composer-approval ${requireApproval ? "approval-on" : "approval-off"}`}
              title="工具调用审批模式（点击切换）"
              onClick={() => setRequireApproval(!requireApproval)}>
          {requireApproval ? t.approvalOn : t.approvalOff}
        </span>
        {/* v0.6：plan mode 切换 */}
        <span className={`composer-plan ${planMode ? "plan-on" : "plan-off"}`}
              title={t.planOn}
              onClick={() => setPlanMode(!planMode)}>
          {planMode ? "📋 " + t.planOn : "📋"}
        </span>
        {/* v0.9：附件按钮 */}
        <span
          className="composer-attach"
          title="添加图片 (也支持拖放)"
          onClick={async () => {
            try {
              const selected = await openDialog({
                multiple: true,
                filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
              });
              if (Array.isArray(selected)) {
                for (const p of selected) {
                  const lower = p.toLowerCase();
                  const mime = lower.endsWith(".png") ? "image/png"
                    : lower.endsWith(".jpg") || lower.endsWith(".jpeg") ? "image/jpeg"
                    : lower.endsWith(".gif") ? "image/gif"
                    : lower.endsWith(".webp") ? "image/webp" : "image/png";
                  const name = p.split("/").pop() || p;
                  setAttachedImages((prev) => [...prev, { path: p, mime, name }]);
                }
              }
            } catch (e) {
              console.warn("attach failed:", e);
            }
          }}
        >📎 {attachedImages.length > 0 ? `${attachedImages.length}` : ""}</span>
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