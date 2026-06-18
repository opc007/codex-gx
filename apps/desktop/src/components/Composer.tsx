import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useSessionsStore, getSessionsState, setSessionsState, type PersistedMessage } from "../stores/sessions";
import { useTranslation, setLocale as i18nSetLocale } from "../i18n";
import { redactSimple, detectTypes } from "../lib/redact";
import { sendChatStream } from "../lib/chat";
import { buildChatHistory } from "../lib/chatHistory";
import { loadProviders, type ProviderInfo } from "../lib/providers";
import { useCurrentWorkspace, getCurrentWorkspace } from "../stores/workspace";
import { StageTimeline, type Stage } from "./StageTimeline";
import { BUILTIN_NAME_SET, searchSlashCommands, type SlashCommand } from "../lib/slashCommands";

type Props = {
  sessionId: string | null;
};

type ApiKeysStatus = {
  minimax_configured: boolean;
  anthropic_configured: boolean;
  deepseek_configured: boolean;
  openai_configured: boolean;
};

const MODEL_STORAGE_KEY = "codex-gx-model";

function providerConfigured(id: string, status: ApiKeysStatus): boolean {
  switch (id) {
    case "minimax":
      return status.minimax_configured;
    case "deepseek":
      return status.deepseek_configured;
    case "anthropic":
      return status.anthropic_configured;
    case "openai":
      return status.openai_configured;
    default:
      return false;
  }
}

export function Composer({ sessionId }: Props) {
  const t = useTranslation();
  const currentWs = useCurrentWorkspace();
  const projectContext = {
    workspaceId: currentWs.id,
    name: currentWs.name,
    folderPath: currentWs.folderPath,
    description: currentWs.description,
  };
  const appendMessage = useSessionsStore((s) => s.appendMessage);
  const setMessages = useSessionsStore((s) => s.setMessages);

  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  // v1.9.6：排队提示（busy 时 Enter 入队）
  const [queuedPrompts, setQueuedPrompts] = useState<string[]>([]);
  const queuedPromptsRef = useRef<string[]>([]);
  const [model, setModel] = useState(() => {
    try {
      return localStorage.getItem(MODEL_STORAGE_KEY) || "MiniMax-M3";
    } catch {
      return "MiniMax-M3";
    }
  });
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
  const [keyStatus, setKeyStatus] = useState<ApiKeysStatus | null>(null);
  const [stages, setStages] = useState<Stage[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const refreshModelOptions = async () => {
    const [p, s] = await Promise.all([
      loadProviders(),
      invoke<ApiKeysStatus>("api_keys_status"),
    ]);
    setProviders(p);
    setKeyStatus(s);
  };

  const availableModels = useMemo(() => {
    const opts: Array<{ value: string; label: string; group: string }> = [
      { value: "auto", label: "Auto 路由", group: "智能" },
    ];
    if (keyStatus) {
      for (const p of providers) {
        if (!providerConfigured(p.id, keyStatus)) continue;
        for (const m of p.models) {
          opts.push({ value: m, label: m, group: p.name });
        }
      }
    }
    if (model !== "auto" && !opts.some((o) => o.value === model)) {
      opts.push({ value: model, label: `${model}（需配置 Key）`, group: "当前" });
    }
    return opts;
  }, [providers, keyStatus, model]);

  const modelReady =
    model === "auto" ||
    (keyStatus !== null &&
      providers.some(
        (p) => providerConfigured(p.id, keyStatus) && p.models.includes(model),
      ));

  useEffect(() => {
    void refreshModelOptions();
    const onKeys = () => void refreshModelOptions();
    window.addEventListener("api-keys:changed", onKeys);
    return () => window.removeEventListener("api-keys:changed", onKeys);
  }, []);

  // v1.2：voice input 状态
  const [recording, setRecording] = useState(false);
  const [voiceBusy, setVoiceBusy] = useState(false);
  const [voiceHint, setVoiceHint] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const recordedChunksRef = useRef<Blob[]>([]);

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
    if (e.key === "Tab" && (text.startsWith("/") || text.startsWith("$") || text.startsWith("@"))) {
      e.preventDefault();
      const trigger = text[0];
      const list = searchSlashCommands(text, dynamicSkills, []);
      const target = list.find((c) => c.name.toLowerCase().startsWith(text.slice(1).toLowerCase()));
      if (target) {
        setText(trigger === "$" ? "$" + target.name + " " : trigger === "@" ? "@" + target.name + " " : "/" + target.template);
      }
    }
  };

  // v1.9.6：Cmd+M 全局快捷键触发 voice input（Codex App 风格）
  useEffect(() => {
    const onToggle = () => {
      if (recording) stopRecording();
      else void startRecording();
    };
    window.addEventListener("agentshell:toggle-voice", onToggle);
    return () => window.removeEventListener("agentshell:toggle-voice", onToggle);
  }, [recording]);

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
    if (!sessionId || !text.trim()) return;

    // 1. 处理 slash 命令
    const trimmed = text.trim();

    // v1.9.6：busy 时按 Enter → 排队下轮（Codex App 风格）
    if (busy) {
      setQueuedPrompts((p) => {
        queuedPromptsRef.current = [...p, trimmed];
        return queuedPromptsRef.current;
      });
      setText("");
      return;
    }
    // 如果是从队列自动触发的，text 已经被 setText(next) 填好了
    // 正常路径：清空
    // setText("") 将在底部成功处

    // 1.0 v1.9.6：$ 显式调用 skill（Codex App 风格）
    if (trimmed.startsWith("$") && !trimmed.startsWith("$$")) {
      const m = trimmed.match(/^\$(\S+)\s*([\s\S]*)$/);
      if (m) {
        const name = m[1];
        const arg = m[2];
        try {
          const r = await invoke<string>("run_skill", { name, arg });
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `💲 $${name}\n\n${r}`,
            createdAt: Date.now(),
          });
        } catch (e) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: `❌ $${name} 失败：${e}`,
            createdAt: Date.now(),
          });
        }
        setText("");
        return;
      }
    }

    // 1.0b v1.9.6：@ 引用（实验性 — 当前仅把 @xxx 视作 skill 引用）
    if (trimmed.startsWith("@") && !trimmed.startsWith("@@")) {
      const m = trimmed.match(/^@(\S+)\s*([\s\S]*)$/);
      if (m) {
        const name = m[1];
        const arg = m[2];
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `📎 @${name}（实验性：v1.9.7 计划支持文件/插件/技能补全）\n\n当前按 skill 调用。`,
          createdAt: Date.now(),
        });
        try {
          const r = await invoke<string>("run_skill", { name, arg });
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            text: r,
            createdAt: Date.now(),
          });
        } catch {
          /* keep the explainer */
        }
        setText("");
        return;
      }
    }

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
    // v1.9.6：/init 生成 AGENTS.md（Codex App 风格）
    if (trimmed === "/init" || trimmed.startsWith("/init ")) {
      const force = /\s--force$/.test(trimmed);
      const wsFolder = getCurrentWorkspace()?.folderPath;
      if (!wsFolder) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: "❌ /init 需要先在左下角项目组选择器里绑定一个文件夹（没绑定文件夹的会话不知道把 AGENTS.md 写到哪里）。",
          createdAt: Date.now(),
        });
        setText("");
        return;
      }
      try {
        const r = await invoke<string>("init_agents_md", {
          args: { folder: wsFolder, project_name: null, force },
        });
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: r,
          createdAt: Date.now(),
        });
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          text: `❌ /init 失败：${e}`,
          createdAt: Date.now(),
        });
      }
      setText("");
      return;
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
    if (trimmed === "/apikey" || trimmed === "/key") {
      window.dispatchEvent(new CustomEvent("open-api-keys"));
      setText("");
      return;
    }
    if (trimmed === "/help") {
      const helpMsg: PersistedMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        text: `📖 Codex gx v1.9.6 命令帮助：

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
- 自动跟踪模型 / 工具 / 命令 / 提示长度 / 语言

🎨 v1.9.6 多模态生图 (MiniMax):
/image <提示词>                     - 文生图（图灵 / image-01）
  --model <modelId>                - 指定模型（image-01 / image-02）
  --w 1024 --h 1024                - 尺寸
  --n 1                            - 生成张数
  --ref <url1,url2>                - 参考图（图生图）
  示例: /image a cute cat, anime style --w 768 --h 768

🎬 v1.9.6 多模态生视频 (MiniMax-Hailuo):
/video <提示词>                    - 文生视频（最长 6s / 768P）
  --model MiniMax-Hailuo-2.3       - 模型（Hailuo-2.3 / 2）
  --duration 6|10                  - 时长（秒）
  --resolution 768P|1080P          - 分辨率（API 仅支持 768P / 1080P）
  --first <url>                    - 首帧图（图生视频）
  --ref <url1,url2>                - 主体参考
  --wait 240                       - 等待上限（秒）
  示例: /video a cat walking in rain --duration 6
  ⚠️ 视频生成通常 60-180s，前端会自动等结果`,
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

    // v1.9.5: /mobile server (HTTP server + tunnel + devices)
    if (trimmed === "/mobile server" || trimmed.startsWith("/mobile server ")) {
      const arg = trimmed.slice(13).trim();
      try {
        if (arg === "" || arg === "start") {
          const enable_tunnel = true;
          const info = await invoke<{ status: string; bind: string; port: number; tunnelStatus: string; publicUrl: string | null; requestsHandled: number }>("mobile_server_start", { args: { port: 8788, enableTunnel: enable_tunnel, bind: "0.0.0.0" } });
          const qr = await invoke<string>("mobile_qr_payload", { token: "see-mobile-status", publicUrl: info.publicUrl });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🟢 Mobile HTTP server 已启动：\n\n  bind:        ${info.bind}:${info.port}\n  tunnel:      ${info.tunnelStatus}\n  public URL:  ${info.publicUrl ?? "—"}\n  handled:     ${info.requestsHandled}\n\nQR 内容 (base64): \n  ${qr.slice(0, 80)}${qr.length > 80 ? "..." : ""}\n\n测试：\n  curl http://${info.bind}:${info.port}/health\n  curl -X POST http://${info.bind}:${info.port}/send \\\n    -H 'Authorization: Bearer <token>' \\\n    -H 'Content-Type: application/json' \\\n    -d '{"device_id":"dev1","command":"ping","payload":{}}'\n\n查看 token：/mobile status` });
        } else if (arg === "stop") {
          const info = await invoke<{ status: string; bind: string; port: number }>("mobile_server_stop");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🔴 Mobile HTTP server 已停止 (was on ${info.bind}:${info.port})` });
        } else if (arg === "status") {
          const info = await invoke<{ status: string; bind: string; port: number; tunnelStatus: string; publicUrl: string | null; requestsHandled: number; lastRequestAt: number | null; lastError: string | null; devices: Array<{ deviceId: string; lastSeen: number; pendingCommands: number; status: string }>; pendingCommands: number }>("mobile_server_status");
          const devs = await invoke<Array<{ deviceId: string; lastSeen: number; pendingCommands: number; status: string }>>("mobile_server_devices");
          const notifs = await invoke<Array<{ timestamp: number; level: string; message: string; deviceId: string | null }>>("mobile_server_notifications");
          const cmds = await invoke<Array<{ id: string; deviceId: string; command: string; timestamp: number; status: string }>>("mobile_server_commands");
          const devTxt = devs.length === 0 ? "  (无)" : devs.map((d) => `  ${d.status === "online" ? "🟢" : "⚫"} ${d.deviceId}  pending=${d.pendingCommands}  last=${new Date(d.lastSeen * 1000).toLocaleTimeString()}`).join("\n");
          const notifTxt = notifs.length === 0 ? "  (无)" : notifs.slice(-5).map((n) => `  [${n.level}] ${n.message}`).join("\n");
          const cmdTxt = cmds.length === 0 ? "  (无)" : cmds.slice(-5).map((c) => `  ${c.status === "queued" ? "🟡" : "✅"} ${c.id} → ${c.deviceId} ${c.command}`).join("\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📱 Mobile HTTP Server 状态：\n\n  status:        ${info.status}\n  bind:          ${info.bind}:${info.port}\n  tunnel:        ${info.tunnelStatus}\n  public URL:    ${info.publicUrl ?? "—"}\n  handled:       ${info.requestsHandled}\n  pending cmds:  ${info.pendingCommands}\n  last req:      ${info.lastRequestAt ? new Date(info.lastRequestAt * 1000).toLocaleString() : "—"}\n  last err:      ${info.lastError ?? "—"}\n\n设备 (${devs.length})：\n${devTxt}\n\n命令队列 (${cmds.length})：\n${cmdTxt}\n\n最近通知 (${notifs.length})：\n${notifTxt}` });
        } else if (arg.startsWith("start ")) {
          const port = parseInt(arg.slice(6).trim(), 10) || 8788;
          const info = await invoke<{ status: string; bind: string; port: number; tunnelStatus: string; publicUrl: string | null }>("mobile_server_start", { args: { port, enableTunnel: true, bind: "0.0.0.0" } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🟢 Server 启动：${info.bind ?? "0.0.0.0"}:${info.port}\n  tunnel=${info.tunnelStatus} url=${info.publicUrl ?? "—"}` });
        } else if (arg === "devices") {
          const devs = await invoke<Array<{ deviceId: string; lastSeen: number; pendingCommands: number; status: string }>>("mobile_server_devices");
          const txt = devs.length === 0 ? "(无设备)" : devs.map((d) => `  ${d.status === "online" ? "🟢" : "⚫"} **${d.deviceId}**\n    status: ${d.status}\n    pending: ${d.pendingCommands}\n    last seen: ${new Date(d.lastSeen * 1000).toLocaleString()}`).join("\n\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📱 配对设备 (${devs.length})：\n\n${txt}` });
        } else if (arg === "qr") {
          const tok = await invoke<{ token: string }>("mobile_get_token");
          const info = await invoke<{ publicUrl: string | null }>("mobile_server_status");
          const qr = await invoke<string>("mobile_qr_payload", { token: tok.token, publicUrl: info.publicUrl });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📱 QR Payload (base64)：\n\n  \`${qr}\`\n\n解码：agentshell://mobile?token=...&url=${info.publicUrl ?? ""}\n（用任何 QR 生成器渲染）` });
        } else {
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /mobile server start [port] | stop | status | devices | qr" });
        }
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /mobile server 失败: ${e}` });
      }
      return;
    }

    // v1.9.4: /vision (多模态：图像/OCR/标注)
    if (trimmed === "/vision" || trimmed.startsWith("/vision ")) {
      const arg = trimmed.slice(7).trim();
      try {
        if (arg === "" || arg === "status") {
          const st = await invoke<{ version: string; capabilities: string[]; promptExcerpt: string }>("vision_status");
          const fmts = await invoke<Array<{ name: string; label: string; mime: string }>>("vision_formats");
          const fmtTxt = fmts.map((f) => `  ${f.label.padEnd(6)} ${f.mime}`).join("\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📷 Vision 状态：\n\n  version:  ${st.version}\n  caps:     ${st.capabilities.join(", ")}\n\n支持格式：\n${fmtTxt}\n\n命令：/vision meta <path> | caption <path> | ocr <path> | annotate <path> | formats | prompt` });
        } else if (arg === "formats") {
          const fmts = await invoke<Array<{ name: string; label: string; mime: string }>>("vision_formats");
          const fmtTxt = fmts.map((f) => `  ${f.label.padEnd(6)} ${f.mime}`).join("\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📷 支持的图像格式：\n\n${fmtTxt}` });
        } else if (arg === "prompt") {
          const p = await invoke<string>("vision_protocol_prompt");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: p });
        } else if (arg.startsWith("meta ")) {
          const path = arg.slice(5).trim();
          const m = await invoke<{ format: string; mime: string; sizeBytes: number; width: number | null; height: number | null; aspectRatio: number | null; mode: string | null; source: string }>("vision_meta", { args: { path } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📷 图像元数据：\n\n  format:  ${m.format}\n  mime:    ${m.mime}\n  size:    ${m.sizeBytes} bytes\n  width:   ${m.width ?? "—"}\n  height:  ${m.height ?? "—"}\n  aspect:  ${m.aspectRatio?.toFixed(2) ?? "—"}\n  mode:    ${m.mode ?? "—"}\n  source:  ${m.source}` });
        } else if (arg.startsWith("caption ")) {
          const path = arg.slice(8).trim();
          const c = await invoke<{ short: string; detailed: string; tags: string[]; colors: string[]; mood: string | null }>("vision_caption", { args: { path } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📷 图像描述：\n\n**Short**: ${c.short}\n\n**Detailed**: ${c.detailed}\n\n**Tags**: ${c.tags.join(", ")}\n**Colors**: ${c.colors.join(", ")}\n**Mood**: ${c.mood ?? "—"}` });
        } else if (arg.startsWith("ocr ")) {
          const path = arg.slice(4).trim();
          const r = await invoke<{ text: string; lines: Array<{ text: string; bbox: [number, number, number, number]; confidence: number }>; confidence: number; language: string }>("vision_ocr", { args: { path } });
          const linesTxt = r.lines.map((l) => `  [${(l.bbox[0] * 100).toFixed(0)},${(l.bbox[1] * 100).toFixed(0)}] ${l.text} (${(l.confidence * 100).toFixed(0)}%)`).join("\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📷 OCR 结果（${r.language}）：\n\n**Text**:\n${r.text}\n\n**Lines**:\n${linesTxt || "  (无)"}` });
        } else if (arg.startsWith("annotate ")) {
          // annotate <path> + demo box
          const path = arg.slice(9).trim();
          const demoBoxes = [
            { id: "btn-1", label: "Submit button", x: 0.5, y: 0.6, w: 0.1, h: 0.05, confidence: 0.95, description: "blue submit button at center-bottom" },
            { id: "txt-1", label: "Title text", x: 0.05, y: 0.05, w: 0.9, h: 0.08, confidence: 0.88, description: "large heading text" },
          ];
          const txt = await invoke<string>("vision_annotate", { args: { path, boxes: demoBoxes } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📷 Screenshot 标注（demo 2 boxes）：\n\n${txt}\n（真实检测需要 VLM API）` });
        } else {
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /vision status | formats | prompt | meta <path> | caption <path> | ocr <path> | annotate <path>" });
        }
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /vision 失败: ${e}` });
      }
      return;
    }

    // v1.9.6: /image （MiniMax 文/图生图）
    if (trimmed === "/image" || trimmed.startsWith("/image ")) {
      const arg = trimmed.slice(6).trim();
      if (!arg) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          createdAt: Date.now(),
          text: `🎨 用法: \`/image <提示词>\`\n\n可选项: \`--model <modelId>\` · \`--w 1024 --h 1024\` · \`--n 1\` · \`--ref <url1,url2>\`\n\n示例: \n  \`/image a cute cat, anime style\`\n  \`/image a mountain --model image-02 --w 1280 --h 720\`\n  \`/image a logo --ref https://example.com/style.png\``,
        });
        setText("");
        return;
      }
      // 解析 flag
      const tokens = arg.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
      const flags: string[] = [];
      const posArgs: string[] = [];
      for (let i = 0; i < tokens.length; i++) {
        const t = tokens[i];
        if (t.startsWith("--")) {
          flags.push(t);
        } else if (
          (t === "--ref" || t === "--model" || t === "--w" || t === "--h" || t === "--n") &&
          tokens[i + 1]
        ) {
          flags.push(t, tokens[i + 1]);
          i++;
        } else {
          posArgs.push(t);
        }
      }
      const getFlag = (name: string) => {
        const idx = flags.indexOf(name);
        return idx >= 0 ? flags[idx + 1] : undefined;
      };
      const prompt = posArgs.join(" ").trim();
      if (!prompt) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          createdAt: Date.now(),
          text: "❌ 缺少提示词，例如 `/image a cute cat`",
        });
        setText("");
        return;
      }
      const refStr = getFlag("--ref");
      const refUrls = refStr
        ? refStr.split(",").map((u) => u.trim()).filter(Boolean)
        : null;
      const args: {
        prompt: string;
        model?: string;
        width?: number;
        height?: number;
        n?: number;
        image_urls?: string[];
      } = { prompt };
      const m = getFlag("--model");
      if (m) args.model = m;
      const w = getFlag("--w");
      if (w) args.width = parseInt(w, 10);
      const h = getFlag("--h");
      if (h) args.height = parseInt(h, 10);
      const n = getFlag("--n");
      if (n) args.n = parseInt(n, 10);
      if (refUrls && refUrls.length > 0) args.image_urls = refUrls;
      // 把用户原始命令也显示
      appendMessage(sessionId, {
        id: crypto.randomUUID(),
        role: "user",
        text: `/image ${prompt}`,
        createdAt: Date.now(),
      });
      const startId = `media-image-${crypto.randomUUID()}`;
      appendMessage(sessionId, {
        id: startId,
        role: "assistant",
        createdAt: Date.now(),
        text: `🎨 正在生成图像…\n\n  prompt: ${prompt}\n  model: ${args.model || "image-01 (默认)"}\n  size:  ${args.width || 1024}×${args.height || 1024}${refUrls ? `\n  ref:   ${refUrls.length} 张` : ""}`,
      });
      setText("");
      try {
        const r = await invoke<{
          id: string;
          image_urls: string[];
          success_count: string;
          failed_count: string;
        }>("media_generate_image", { args });
        // 把生成结果用 media-gallery 标记的消息保存
        const galId = `gallery-${crypto.randomUUID()}`;
        appendMessage(sessionId, {
          id: galId,
          role: "assistant",
          createdAt: Date.now(),
          text: `✅ 图像生成成功 · id \`${r.id.slice(0, 16)}\`\n\n${r.image_urls.map((u, i) => `![generated-${i}](${u})`).join("\n\n")}\n\n💡 右键图片 → 复制 / 在浏览器中打开。\n⚠️ 链接 24h 内有效，建议右键保存到本地。`,
          mediaGallery: r.image_urls,
        });
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          createdAt: Date.now(),
          text: `❌ 图像生成失败：${e}`,
        });
      }
      return;
    }

    // v1.9.6: /video （MiniMax 文生视频）
    if (trimmed === "/video" || trimmed.startsWith("/video ")) {
      const arg = trimmed.slice(6).trim();
      if (!arg) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          createdAt: Date.now(),
          text: `🎬 用法: \`/video <提示词>\`\n\n可选项: \`--model MiniMax-Hailuo-2.3\` · \`--duration 6|10\` · \`--resolution 720P|1080P\` · \`--first <url>\` · \`--ref <url1,url2>\` · \`--wait 240\`\n\n示例: \n  \`/video a cat walking in the rain\`\n  \`/video a city skyline --duration 10 --resolution 1080P\`\n  \`/video a dancer --first https://example.com/pose.png\`\n\n⚠️ 视频生成通常 1-3 分钟，会自动等待结果。`,
        });
        setText("");
        return;
      }
      const tokens = arg.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
      const flags: string[] = [];
      const posArgs: string[] = [];
      for (let i = 0; i < tokens.length; i++) {
        const t = tokens[i];
        if (t.startsWith("--")) {
          flags.push(t);
        } else if (
          (t === "--ref" ||
            t === "--model" ||
            t === "--duration" ||
            t === "--resolution" ||
            t === "--first" ||
            t === "--wait") &&
          tokens[i + 1]
        ) {
          flags.push(t, tokens[i + 1]);
          i++;
        } else {
          posArgs.push(t);
        }
      }
      const getFlag = (name: string) => {
        const idx = flags.indexOf(name);
        return idx >= 0 ? flags[idx + 1] : undefined;
      };
      const prompt = posArgs.join(" ").trim();
      if (!prompt) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          createdAt: Date.now(),
          text: "❌ 缺少提示词，例如 `/video a cat walking`",
        });
        setText("");
        return;
      }
      const args: {
        prompt: string;
        model?: string;
        duration?: number;
        resolution?: string;
        first_frame_image?: string;
        subject_reference?: string[];
        wait_secs?: number;
      } = { prompt };
      const m = getFlag("--model");
      if (m) args.model = m;
      const dur = getFlag("--duration");
      if (dur) args.duration = parseInt(dur, 10);
      const res = getFlag("--resolution");
      if (res) args.resolution = res;
      const first = getFlag("--first");
      if (first) args.first_frame_image = first;
      const refStr = getFlag("--ref");
      if (refStr) {
        args.subject_reference = refStr
          .split(",")
          .map((u) => u.trim())
          .filter(Boolean);
      }
      const wait = getFlag("--wait");
      if (wait) args.wait_secs = parseInt(wait, 10);
      appendMessage(sessionId, {
        id: crypto.randomUUID(),
        role: "user",
        text: `/video ${prompt}`,
        createdAt: Date.now(),
      });
      appendMessage(sessionId, {
        id: crypto.randomUUID(),
        role: "assistant",
        createdAt: Date.now(),
        text: `🎬 视频生成中…\n\n  prompt:  ${prompt}\n  model:   ${args.model || "MiniMax-Hailuo-2.3 (默认)"}\n  duration:${args.duration || 6}s\n  resol.:  ${args.resolution || "720P"}${args.first_frame_image ? `\n  first:   ${args.first_frame_image}` : ""}${args.subject_reference ? `\n  ref:     ${args.subject_reference.length} 张` : ""}\n\n⏳ 通常 60-180 秒，最多等 ${args.wait_secs || 240}s`,
      });
      setText("");
      try {
        const r = await invoke<{
          task_id: string;
          status: string;
          video_url: string | null;
          file_id: string | null;
          message: string | null;
          elapsed_secs: number;
        }>("media_generate_video", { args });
        if (r.video_url) {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            createdAt: Date.now(),
            text: `✅ 视频生成完成 · 用时 ${r.elapsed_secs}s\n\n  task_id:  \`${r.task_id}\`\n  file_id:  ${r.file_id || "—"}\n  status:   ${r.status}\n  🎥 [下载视频](${r.video_url})\n\n💡 右键视频链接 → 链接另存为 / 在浏览器中打开。\n⚠️ 链接会过期，建议立即下载。`,
            mediaVideo: r.video_url,
          });
        } else {
          appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "assistant",
            createdAt: Date.now(),
            text: `⚠️ 视频任务结束但无 URL\n\n  status:  ${r.status}\n  message: ${r.message || "—"}\n  task_id: \`${r.task_id}\``,
          });
        }
      } catch (e) {
        appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: "assistant",
          createdAt: Date.now(),
          text: `❌ 视频生成失败：${e}`,
        });
      }
      return;
    }

    // v1.9.2: /pocket
    if (trimmed === "/pocket" || trimmed.startsWith("/pocket ")) {
      const arg = trimmed.slice(7).trim();
      try {
        if (arg === "" || arg === "status") {
          const st = await invoke<{ sourceCount: number; pairingCount: number; enabledPairings: number; sources: Array<{ name: string; label: string; paired: boolean }>; configPath: string }>("pocket_status");
          const url = await invoke<string>("pocket_webhook_url");
          const src = st.sources.map((s) => `  ${s.paired ? "✅" : "○"} ${s.label}`).join("\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📨 Pocket 状态：\n\n  sources:        ${st.sourceCount}\n  pairings:       ${st.pairingCount} (${st.enabledPairings} enabled)\n  config:         ${st.configPath}\n  webhook URL:    ${url}\n\n**Sources**：\n${src}\n\n命令：/pocket pair <source> <user_id> <user_name> <chat_id> | /pocket list | /pocket unpair <id> | /pocket sign <key> <body> | /pocket test <pairing_id>` });
        } else if (arg === "list") {
          const list = await invoke<Array<{ id: string; source: string; userId: string; userName: string; chatId: string; chatType: string; signatureKey: string; pairedAt: number; enabled: boolean }>>("pocket_list_pairings");
          if (list.length === 0) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "📨 还没配对任何 source" }); return; }
          const txt = list.map((p) => `  ${p.enabled ? "🟢" : "🔴"} **${p.source}** — ${p.userName} (${p.userId})\n    chat: ${p.chatType}/${p.chatId}\n    key: \`${p.signatureKey.slice(0, 16)}...\`\n    id: ${p.id}`).join("\n\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📨 配对列表 (${list.length})：\n\n${txt}` });
        } else if (arg.startsWith("pair ")) {
          // pair <source> <user_id> <user_name> <chat_id>
          const m = arg.slice(5).match(/^(\S+)\s+(\S+)\s+(\S+)\s+(\S+)$/);
          if (!m) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /pocket pair <source> <user_id> <user_name> <chat_id>" }); return; }
          const p = await invoke<{ id: string; source: string; signatureKey: string }>("pocket_add_pairing", { args: { source: m[1], userId: m[2], userName: m[3], chatId: m[4] } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `✅ 配对创建：\n\n  source: ${p.source}\n  id:     ${p.id}\n  key:    \`${p.signatureKey}\`\n\n⚠️ 保存 key 到消息 App 的 webhook 配置（HMAC 共享密钥）。` });
        } else if (arg.startsWith("unpair ")) {
          const id = arg.slice(7).trim();
          const ok = await invoke<boolean>("pocket_remove_pairing", { id });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: ok ? `🗑️ 配对已删除：${id}` : `❌ 没找到：${id}` });
        } else if (arg.startsWith("sign ")) {
          // sign <key> <body>
          const m = arg.slice(5).match(/^(\S+)\s+(.+)$/);
          if (!m) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /pocket sign <key> <body>" }); return; }
          const sig = await invoke<string>("pocket_sign", { args: { key: m[1], body: m[2] } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🔏 HMAC-SHA256 签名：\n\n  body: ${m[2]}\n  sig:  \`${sig}\`` });
        } else if (arg.startsWith("server ")) {
          // server <start|stop|status> [port]
          const parts = arg.slice(7).trim().split(/\s+/);
          const cmd = parts[0];
          const port = parts[1] ? parseInt(parts[1], 10) : 8787;
          if (cmd === "start") {
            try {
              const info = await invoke<{ status: string; bind: string; port: number; requestsHandled: number; lastError: string | null }>("pocket_server_start", { args: { bind: "127.0.0.1", port } });
              appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🟢 Pocket HTTP server 启动：\n\n  bind:  ${info.bind}\n  port:  ${info.port}\n  status: ${info.status}\n\n测试：\n  curl http://${info.bind}:${info.port}/agentshell/health\n  curl -X POST http://${info.bind}:${info.port}/agentshell/pocket -H 'Content-Type: application/json' -d '{"source":"feishu","user_id":"u1","user_name":"U","chat_id":"c1","chat_type":"direct","text":"hi"}'` });
            } catch (e) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ 启动失败: ${e}` }); }
          } else if (cmd === "stop") {
            try {
              const info = await invoke<{ status: string; bind: string; port: number }>("pocket_server_stop");
              appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🔴 Pocket HTTP server 已停止 (was on ${info.bind}:${info.port})` });
            } catch (e) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ 停止失败: ${e}` }); }
          } else if (cmd === "status") {
            const info = await invoke<{ status: string; bind: string; port: number; startedAt: number; requestsHandled: number; lastRequestAt: number | null; lastError: string | null }>("pocket_server_status");
            const log = await invoke<Array<{ timestamp: number; source: string; userId: string; chatId: string; text: string; signatureOk: boolean; threadId: string; status: string }>>("pocket_inbound_log", { args: { limit: 5 } });
            const inboundTxt = log.length === 0 ? "  (无)" : log.map((l) => `  [${new Date(l.timestamp * 1000).toLocaleTimeString()}] ${l.source}/${l.userId} ${l.signatureOk ? "✅" : "❌"} → ${l.status}\n    "${l.text.slice(0, 60)}"\n    thread: ${l.threadId || "—"}`).join("\n");
            appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📨 Pocket Server 状态：\n\n  status:    ${info.status}\n  bind:      ${info.bind}:${info.port}\n  started:   ${info.startedAt > 0 ? new Date(info.startedAt * 1000).toLocaleString() : "—"}\n  handled:   ${info.requestsHandled}\n  last req:  ${info.lastRequestAt ? new Date(info.lastRequestAt * 1000).toLocaleString() : "—"}\n  last err:  ${info.lastError || "—"}\n\n最近入站 (${log.length})：\n${inboundTxt}` });
          } else {
            appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /pocket server start [port] | stop | status" });
          }
        } else if (arg.startsWith("test ")) {
          // 模拟 webhook 调用
          const id = arg.slice(5).trim();
          const list = await invoke<Array<{ id: string; source: string; userId: string; chatId: string; signatureKey: string }>>("pocket_list_pairings");
          const p = list.find((x) => x.id === id);
          if (!p) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ 没找到配对：${id}` }); return; }
          const body = `{"user_id":"${p.userId}","chat_id":"${p.chatId}","text":"test from slash cmd"}`;
          const sig = await invoke<string>("pocket_sign", { args: { key: p.signatureKey, body } });
          const r = await invoke<{ status: string; threadId: string; message: string }>("pocket_handle_request", { req: { source: p.source, userId: p.userId, userName: "Test", chatId: p.chatId, chatType: "direct", text: "test from slash cmd", signature: sig } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🧪 模拟 webhook 调用：\n\n  status:   ${r.status}\n  thread:   ${r.threadId || "—"}\n  message:  ${r.message}` });
        } else {
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /pocket status | list | pair <source> <uid> <name> <cid> | unpair <id> | sign <key> <body> | server <start|stop|status> [port] | test <id>" });
        }
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /pocket 失败: ${e}` });
      }
      return;
    }

    // v1.9.1: /mobile (Mobile Remote 管理)
    if (trimmed === "/mobile" || trimmed.startsWith("/mobile ")) {
      const arg = trimmed.slice(7).trim();
      try {
        if (arg === "" || arg === "status") {
          const info = await invoke<{ token: string; createdAt: number; lastUsedAt: number | null; deviceCount: number; tokenPath: string }>("mobile_get_token");
          const lastUsed = info.lastUsedAt ? new Date(info.lastUsedAt * 1000).toISOString() : "never";
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📱 Mobile Remote 状态：\n\n  token:   \`${info.token.slice(0, 20)}...\`\n  created: ${new Date(info.createdAt * 1000).toISOString()}\n  used:    ${lastUsed}\n  devices: ${info.deviceCount}\n  path:    ${info.tokenPath}\n\n命令：/mobile regen | pair <name> <ios|android> | unpair <id> | list | call <action>` });
        } else if (arg === "regen") {
          const info = await invoke<{ token: string }>("mobile_regen_token");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `🔄 Token 已重新生成：\n\n  \`${info.token}\`\n\n⚠️ 老 token 立即失效。请更新移动 App 配置。` });
        } else if (arg.startsWith("pair ")) {
          const m = arg.slice(5).match(/^(\S+)\s+(ios|android)$/);
          if (!m) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /mobile pair <name> <ios|android>" }); return; }
          await invoke("mobile_pair_device", { args: { name: m[1], platform: m[2] } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📱 配对成功：**${m[1]}** (${m[2]})` });
        } else if (arg.startsWith("unpair ")) {
          const id = arg.slice(7).trim();
          const ok = await invoke<boolean>("mobile_unpair_device", { id });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: ok ? `📱 解除配对：${id}` : `❌ 没找到：${id}` });
        } else if (arg === "list") {
          const devs = await invoke<Array<{ id: string; name: string; platform: string; pairedAt: number; lastSeenAt: number | null }>>("mobile_list_devices");
          if (devs.length === 0) { appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "📱 还没配对任何设备" }); return; }
          const txt = devs.map((d) => `  ${d.platform === "ios" ? "🍎" : "🤖"} **${d.name}** — ${d.id}\n    paired: ${new Date(d.pairedAt * 1000).toISOString()}`).join("\n");
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📱 配对设备 (${devs.length})：\n\n${txt}` });
        } else if (arg.startsWith("call ")) {
          const action = arg.slice(5).trim();
          const info = await invoke<{ token: string }>("mobile_get_token");
          const r = await invoke<{ status: string; data: any }>("mobile_call", { req: { action, token: info.token } });
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `📱 API 调用 [${action}]：\n\n  status: ${r.status}\n  data:   \`\`\`json\n${JSON.stringify(r.data, null, 2)}\n\`\`\`` });
        } else {
          appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: "❌ 用法: /mobile status | regen | pair <name> <ios|android> | unpair <id> | list | call <action>" });
        }
      } catch (e) {
        appendMessage(sessionId, { id: crypto.randomUUID(), role: "assistant", createdAt: Date.now(), text: `❌ /mobile 失败: ${e}` });
      }
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
        // 检查是否已知命令（避免误调用）— 从集中注册表读
        if (!BUILTIN_NAME_SET.has(name)) {
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
        projectContext,
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
      const errText = String(e);
      if (/API Key|未配置/i.test(errText)) {
        window.dispatchEvent(new CustomEvent("open-api-keys"));
      }
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
      // v1.9.6：turn 完成后，自动从队列取下一条发出去（Codex App 风格 Tab/Enter 排队）
      const next = queuedPromptsRef.current[0];
      if (next) {
        queuedPromptsRef.current = queuedPromptsRef.current.slice(1);
        setQueuedPrompts(queuedPromptsRef.current);
        setText(next);
        // 异步触发下一轮（避免在 setState 阶段直接调 onSend）
        setTimeout(() => {
          void onSend();
        }, 50);
      }
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

  // v1.9.6：动态 slash menu（Codex App 风格：built-in + skills + plugins 分组）
  const [dynamicSkills, setDynamicSkills] = useState<Array<{ name: string; description: string }>>([]);

  // 拉取用户自定义 skills（best-effort，失败忽略）
  useEffect(() => {
    let alive = true;
    invoke<Array<{ name: string; description: string }>>("list_skills")
      .then((list) => {
        if (alive) setDynamicSkills(list);
      })
      .catch(() => {
        /* ignore */
      });
    return () => {
      alive = false;
    };
  }, []);

  const allMatches = searchSlashCommands(text, dynamicSkills, []);
  const grouped: Record<string, SlashCommand[]> = {};
  for (const c of allMatches) {
    (grouped[c.group] ??= []).push(c);
  }

  const slashMenu = (text.startsWith("/") || text.startsWith("$") || text.startsWith("@")) && (
    <div className="slash-menu">
      {text.startsWith("$") && (
        <div className="slash-menu-hint">
          $ 显式调用 skill
        </div>
      )}
      {text.startsWith("@") && (
        <div className="slash-menu-hint">
          @ 引用文件 / 插件 / skill（v1.9.6 实验性：暂列当前项目组内置项）
        </div>
      )}
      {Object.keys(grouped).length === 0 && (
        <div className="slash-menu-empty">没有匹配的命令</div>
      )}
      {Object.entries(grouped)
        .slice(0, 6) // 最多 6 个分组
        .map(([group, items]) => (
          <div key={group} className="slash-menu-group">
            <div className="slash-menu-group-label">{group}</div>
            {items.slice(0, 5).map((c) => (
              <div
                key={c.name}
                className="slash-item"
                onClick={() => {
                  setText("/" + c.template);
                  textareaRef.current?.focus();
                }}
              >
                <code>/{c.name}</code>
                <span>{c.description}</span>
              </div>
            ))}
          </div>
        ))}
    </div>
  );

  return (
    <div className="composer">
      <div className="composer-inner">
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
        <div className="composer-box">
          <div className="composer-input-row">
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
                const items = e.dataTransfer?.files;
                if (!items) return;
                for (let i = 0; i < items.length; i++) {
                  const f = items[i] as File & { path?: string };
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
          </div>
          <div className="composer-bottom-row">
            <label
              className={`composer-model-chip${modelReady ? "" : " model-missing"}`}
              title={modelReady ? "切换模型" : "当前模型未配置 Key，请点击 ⋯ → API Key 设置"}
            >
              <span style={{ fontSize: 12 }}>🧠</span>
              <select
                value={model}
                disabled={busy}
                onChange={(e) => {
                  const v = e.target.value;
                  setModel(v);
                  try {
                    localStorage.setItem(MODEL_STORAGE_KEY, v);
                  } catch {
                    /* ignore */
                  }
                }}
              >
                {availableModels.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.group}: {o.label}
                  </option>
                ))}
              </select>
            </label>
            {/* v1.9.6: Codex 风格 context 指示器 */}
            <ContextIndicator model={model} />
            {queuedPrompts.length > 0 && (
              <span
                className="composer-queue-chip"
                title={`${queuedPrompts.length} 条排队中，模型空闲后自动发送`}
              >
                📋 {queuedPrompts.length} 排队
                <button
                  className="queue-chip-clear"
                  onClick={() => {
                    setQueuedPrompts([]);
                    queuedPromptsRef.current = [];
                  }}
                  title="清空队列"
                >
                  ×
                </button>
              </span>
            )}
            <button
              className="composer-icon-btn"
              title="添加图片"
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
            >
              📎
            </button>
            {/* v1.9.6: Appshots 按钮（Codex App 风格：截屏 + 一键发到 chat） */}
            <button
              className="composer-icon-btn"
              title="Appshots：截取主屏幕并附到当前会话"
              onClick={async () => {
                if (busy) return;
                try {
                  const result = await invoke<string | { data?: string; path?: string }>(
                    "screen_screenshot",
                    { args: { display: "primary", return_base64: true } },
                  );
                  let dataUrl: string | null = null;
                  let note = "Appshot of primary display";
                  if (typeof result === "string") {
                    dataUrl = result.startsWith("data:")
                      ? result
                      : `data:image/png;base64,${result}`;
                  } else if (result?.data) {
                    dataUrl = result.data.startsWith("data:")
                      ? result.data
                      : `data:image/png;base64,${result.data}`;
                    if (result.path) note += ` (${result.path})`;
                  }
                  if (dataUrl && sessionId) {
                    appendMessage(sessionId, {
                      id: crypto.randomUUID(),
                      role: "user",
                      text: `📸 ${note}\n\n(data URL: ${dataUrl.slice(0, 60)}...)`,
                      createdAt: Date.now(),
                    });
                  }
                } catch (e) {
                  appendMessage(sessionId!, {
                    id: crypto.randomUUID(),
                    role: "assistant",
                    text: `❌ Appshots 失败：${e}`,
                    createdAt: Date.now(),
                  });
                }
              }}
            >
              📸
            </button>
            <button
              className={`composer-icon-btn ${recording ? "recording" : ""} ${voiceBusy ? "busy" : ""}`}
              title={
                recording
                  ? "点击停止录音"
                  : voiceBusy
                    ? "转写中…"
                    : "语音输入（本地 Whisper）"
              }
              disabled={voiceBusy}
              onClick={() => {
                if (recording) stopRecording();
                else void startRecording();
              }}
            >
              {recording ? "⏹" : voiceBusy ? "⏳" : "🎙"}
            </button>
            {busy ? (
              <button
                className="composer-cancel-btn"
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
                className="composer-send-btn"
                disabled={!sessionId || !text.trim()}
                onClick={onSend}
                title="发送 (Enter)"
                aria-label="发送"
              >
                ↑
              </button>
            )}
          </div>
        </div>
        <div className="composer-hint">
          {busy
            ? "正在生成（可能含工具调用）…"
            : sessionId
              ? `${text.length} 字符 · Enter 发送 · Shift+Enter 换行`
              : "请先在左侧创建或选择会话"}
        </div>
      </div>
    </div>
  );
}

// v1.9.6：Context 指示器（Codex App 风格：当前会话已用 token / 模型上下文窗口）
const MODEL_CONTEXT_WINDOW: Record<string, number> = {
  "MiniMax-M3": 1_000_000,
  "minimax-m3": 1_000_000,
  "claude-opus-4-8": 200_000,
  "claude-sonnet-4-5": 200_000,
  "deepseek-v4-pro": 128_000,
  "deepseek-chat": 64_000,
  "deepseek-reasoner": 64_000,
  "gpt-5.5": 128_000,
  "gpt-5": 128_000,
  "gpt-5-mini": 128_000,
};

function ContextIndicator({ model }: { model: string }) {
  const sessionId = useSessionsStore((s) => s.currentId);
  const messages = useSessionsStore((s) => (sessionId ? s.messages[sessionId] : undefined)) ?? [];
  const totalTokens = messages.reduce(
    (sum, m) => sum + (m.inputTokens || 0) + (m.outputTokens || 0),
    0,
  );
  const window = MODEL_CONTEXT_WINDOW[model] ?? 128_000;
  const pct = Math.min(100, (totalTokens / window) * 100);
  const color =
    pct < 50 ? "var(--success, #1a7f37)" : pct < 80 ? "var(--warn, #d4a72c)" : "var(--error, #cf222e)";
  return (
    <span
      className="composer-context-indicator"
      title={`本会话已用 ${totalTokens.toLocaleString()} / ${window.toLocaleString()} tokens (${pct.toFixed(1)}%)`}
    >
      <span
        className="context-bar"
        style={{ ["--pct" as any]: `${pct}%`, ["--bar-color" as any]: color }}
      />
      <span className="context-text">
        {totalTokens >= 1000 ? `${(totalTokens / 1000).toFixed(1)}k` : totalTokens}
        <span className="context-pct" style={{ color }}>
          {" "}{pct.toFixed(0)}%
        </span>
      </span>
    </span>
  );
}