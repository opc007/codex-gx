// v1.0：会话导出
import type { PersistedMessage, SessionMeta } from "../stores/sessions";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { save } from "@tauri-apps/plugin-dialog";
import { redactSimple } from "./redact";

export type ExportFormat = "markdown" | "json" | "html";

/** 导出单个 session */
export async function exportSession(
  session: SessionMeta,
  messages: PersistedMessage[],
  format: ExportFormat,
  targetPath?: string,
  redactSecrets: boolean = false,
): Promise<string> {
  const content = renderSession(session, messages, format, redactSecrets);
  // 没指定路径就弹保存对话框
  const path =
    targetPath ??
    (await save({
      title: "导出会话",
      defaultPath: defaultFileName(session.title, format),
      filters: [
        format === "markdown"
          ? { name: "Markdown", extensions: ["md"] }
          : format === "html"
            ? { name: "HTML", extensions: ["html"] }
            : { name: "JSON", extensions: ["json"] },
      ],
    }));
  if (!path) throw new Error("已取消");
  await writeTextFile(path, content);
  return path;
}

function defaultFileName(title: string, format: ExportFormat): string {
  const safe = (title || "session")
    .replace(/[\\/:*?"<>|]/g, "_")
    .slice(0, 50);
  const stamp = new Date().toISOString().slice(0, 10);
  const ext = format === "markdown" ? "md" : format;
  return `${safe}-${stamp}.${ext}`;
}

function renderSession(
  session: SessionMeta,
  messages: PersistedMessage[],
  format: ExportFormat,
  redactSecrets: boolean = false,
): string {
  if (format === "json") return renderJson(session, messages, redactSecrets);
  if (format === "html") return renderHtml(session, messages, redactSecrets);
  return renderMarkdown(session, messages, redactSecrets);
}

/** 统一脱敏入口 */
function r(s: string, redactSecrets: boolean): string {
  return redactSecrets ? redactSimple(s) : s;
}

function renderMarkdown(session: SessionMeta, messages: PersistedMessage[], redactSecrets: boolean = false): string {
  const lines: string[] = [];
  lines.push(`# ${session.title}`);
  lines.push("");
  lines.push(`- **Session ID**: \`${session.id}\``);
  lines.push(`- **创建**: ${new Date(session.createdAt).toLocaleString()}`);
  lines.push(`- **最后更新**: ${new Date(session.updatedAt).toLocaleString()}`);
  lines.push(`- **消息数**: ${messages.length}`);
  lines.push("");
  lines.push("---");
  lines.push("");

  let totalIn = 0;
  let totalOut = 0;
  for (const m of messages) {
    const role = m.role;
    const time = new Date(m.createdAt).toLocaleString();
    lines.push(`## ${roleEmoji(role)} ${capitalize(role)} · ${time}`);
    lines.push("");
    if (m.thinking) {
      const t = r(m.thinking, redactSecrets);
      lines.push("> 💭 思考：");
      lines.push("> " + t.replace(/\n/g, "\n> "));
      lines.push("");
    }
    if (m.text) {
      lines.push(r(m.text, redactSecrets));
      lines.push("");
    }
    if (m.toolCalls && m.toolCalls.length > 0) {
      lines.push("### 🔧 工具调用");
      lines.push("");
      for (const tc of m.toolCalls) {
        lines.push(`- **${tc.name}**` + (tc.success === false ? " ❌" : ""));
        if (tc.arguments !== undefined) {
          const argStr = r(JSON.stringify(tc.arguments, null, 2), redactSecrets);
          lines.push(
            "  ```json",
            argStr.split("\n").map((l) => "  " + l).join("\n"),
            "  ```",
          );
        }
        if (tc.result) {
          const resStr = r(tc.result, redactSecrets);
          lines.push("  - 结果：");
          lines.push("    ```");
          lines.push(
            resStr.split("\n").map((l) => "    " + l).join("\n"),
            "    ```",
          );
        }
        if (tc.error) {
          lines.push(`  - ❌ ${r(tc.error, redactSecrets)}`);
        }
      }
      lines.push("");
    }
    if (m.inputTokens) totalIn += m.inputTokens;
    if (m.outputTokens) totalOut += m.outputTokens;
  }

  if (totalIn || totalOut) {
    lines.push("---");
    lines.push("");
    lines.push(`📊 **Token**: in=${totalIn} out=${totalOut} total=${totalIn + totalOut}`);
  }
  return lines.join("\n");
}

function renderJson(session: SessionMeta, messages: PersistedMessage[], redactSecrets: boolean = false): string {
  const data = {
    version: 1,
    session,
    messages: redactSecrets
      ? messages.map((m) => ({
          ...m,
          text: m.text ? r(m.text, true) : m.text,
          thinking: m.thinking ? r(m.thinking, true) : m.thinking,
          toolCalls: m.toolCalls?.map((tc) => ({
            ...tc,
            result: tc.result ? r(tc.result, true) : tc.result,
            error: tc.error ? r(tc.error, true) : tc.error,
            arguments:
              typeof tc.arguments === "string"
                ? r(tc.arguments, true)
                : tc.arguments !== undefined
                  ? JSON.parse(r(JSON.stringify(tc.arguments), true))
                  : tc.arguments,
          })),
        }))
      : messages,
    exportedAt: new Date().toISOString(),
  };
  return JSON.stringify(data, null, 2);
}

function renderHtml(session: SessionMeta, messages: PersistedMessage[], redactSecrets: boolean = false): string {
  const esc = (s: string) =>
    s.replace(/[&<>"']/g, (c) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    }[c]!));
  const s = (v: string) => esc(r(v, redactSecrets));
  const body = messages
    .map((m) => {
      const time = new Date(m.createdAt).toLocaleString();
      const tcHtml = (m.toolCalls ?? [])
        .map(
          (tc) => `<div class="tool"><b>${esc(tc.name)}</b>${tc.success === false ? " ❌" : ""}<pre>${s(
            JSON.stringify(tc.arguments, null, 2),
          )}</pre>${tc.result ? `<pre class="result">${s(tc.result)}</pre>` : ""}${tc.error ? `<div class="error">${s(tc.error)}</div>` : ""}</div>`,
        )
        .join("\n");
      return `<div class="msg ${esc(m.role)}">
  <div class="meta"><b>${esc(m.role)}</b> · ${esc(time)}</div>
  ${m.thinking ? `<blockquote>💭 ${s(m.thinking)}</blockquote>` : ""}
  <div class="text">${s(m.text).replace(/\n/g, "<br/>")}</div>
  ${tcHtml}
</div>`;
    })
    .join("\n");
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>${esc(session.title)}</title>
<style>
  body{font-family:-apple-system,sans-serif;max-width:780px;margin:24px auto;padding:0 16px;background:#fff;color:#222}
  .msg{border-left:3px solid #ddd;padding:8px 12px;margin:12px 0;background:#fafafa}
  .msg.user{border-color:#3b82f6}
  .msg.assistant{border-color:#10b981}
  .msg.tool{border-color:#f59e0b}
  .meta{font-size:12px;color:#666;margin-bottom:6px}
  .text{white-space:pre-wrap;line-height:1.6}
  .tool{background:#fff8e1;padding:6px 10px;margin-top:6px;border-radius:4px;font-size:13px}
  pre{background:#f4f4f4;padding:6px;border-radius:4px;overflow-x:auto;font-size:12px}
  pre.result{background:#e8f5e9}
  .error{color:#c62828;font-size:12px}
  blockquote{border-left:3px solid #ddd;color:#666;padding:4px 10px;margin:6px 0;background:#f5f5f5}
</style>
</head><body>
<h1>${esc(session.title)}</h1>
<p><small>Session <code>${esc(session.id)}</code> · ${messages.length} messages · 导出于 ${new Date().toLocaleString()}</small></p>
<hr/>
${body}
</body></html>`;
}

function roleEmoji(role: string): string {
  switch (role) {
    case "user": return "👤";
    case "assistant": return "🤖";
    case "tool": return "🔧";
    case "system": return "⚙️";
    default: return "•";
  }
}

function capitalize(s: string) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}