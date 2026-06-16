// 与 Rust 后端的聊天桥
import { invoke } from "@tauri-apps/api/core";

export type ChatMessage = {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  text: string;
  thinking?: string;
  toolCalls?: Array<{ id: string; name: string; arguments: unknown }>;
  createdAt: number;
  streaming?: boolean;
};

export type ChatChunk =
  | { kind: "content"; delta: string }
  | { kind: "thinking"; delta: string }
  | { kind: "done" }
  | { kind: "error"; delta: string };

export type SendChatParams = {
  sessionId: string;
  userMessage: string;
  model: string;
};

/**
 * 发送消息并流式返回响应 chunks
 *
 * 通过 tauri command `chat_stream` 调用 Rust 后端
 * 后端用 provider crate 调用 M3 / Claude / GPT / DeepSeek
 */
export async function sendChatStream(
  params: SendChatParams
): Promise<AsyncIterable<ChatChunk>> {
  // v0.1 简化：直接 invoke 整个 chat，返回完整响应（流式在 v0.2）
  // 后续会用 tauri 的事件系统做真正的流式
  const result = await invoke<{ content: string; thinking: string }>("chat", {
    req: {
      model: params.model,
      message: params.userMessage,
      sessionId: params.sessionId,
    },
  });

  async function* gen(): AsyncIterable<ChatChunk> {
    if (result.thinking) {
      yield { kind: "thinking", delta: result.thinking };
    }
    if (result.content) {
      // 模拟流式（每 20ms 一个 chunk）
      const chunks = chunkString(result.content, 4);
      for (const c of chunks) {
        yield { kind: "content", delta: c };
        await delay(15);
      }
    }
    yield { kind: "done" };
  }
  return gen();
}

function chunkString(s: string, size: number): string[] {
  const out: string[] = [];
  for (let i = 0; i < s.length; i += size) {
    out.push(s.slice(i, i + size));
  }
  return out;
}

function delay(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}