// 与 Rust 后端的聊天桥（真流式 — Tauri event）
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ChatMessage = {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  text: string;
  thinking?: string;
  toolCalls?: Array<{ id: string; name: string; arguments: unknown }>;
  createdAt: number;
  streaming?: boolean;
  /** v0.2 token usage */
  inputTokens?: number;
  outputTokens?: number;
};

export type ChatChunk =
  | { kind: "content"; delta: string }
  | { kind: "thinking"; delta: string }
  | { kind: "tool_call_delta"; delta: string }
  | { kind: "done"; usage?: { inputTokens: number; outputTokens: number } }
  | { kind: "error"; delta: string };

export type SendChatParams = {
  sessionId: string;
  userMessage: string;
  model: string;
};

const CHAT_EVENT_PREFIX = "chat-chunk:";

/**
 * 发送消息并流式返回响应 chunks
 *
 * 通过 tauri command `chat_stream` 调用 Rust 后端
 * 后端在 tokio::spawn 里跑 stream，每个 chunk 通过 emit 推到前端
 * 前端 listen "chat-chunk:{session_id}" 收
 */
export async function sendChatStream(
  params: SendChatParams
): Promise<AsyncIterable<ChatChunk>> {
  // 注册 listener
  const eventName = CHAT_EVENT_PREFIX + params.sessionId;
  let unlisten: UnlistenFn | null = null;
  let resolveDone: (() => void) | null = null;
  const donePromise = new Promise<void>((resolve) => {
    resolveDone = resolve;
  });

  // 用 generator yield chunks
  async function* gen(): AsyncIterable<ChatChunk> {
    // 设置 listener
    unlisten = await listen<{
      kind: string;
      delta: string;
      usage: { inputTokens: number; outputTokens: number } | null;
      done: boolean;
    }>(eventName, (event) => {
      const payload = event.payload;
      // 这里不能直接 yield（listener 是另一个回调）
      // 改为：把 chunk 推入队列，generator 异步拉
      queue.push(payload);
      if (payload.done && resolveDone) {
        resolveDone();
      }
    });

    // 触发后端
    await invoke("chat_stream", {
      req: {
        model: params.model,
        message: params.userMessage,
        sessionId: params.sessionId,
      },
    });

    // 拉队列
    while (true) {
      if (queue.length === 0) {
        // 等 done 信号或新 chunk
        await Promise.race([
          donePromise,
          new Promise((r) => setTimeout(r, 30)),
        ]);
        if (queue.length === 0) {
          // 没有更多 + done 已发
          break;
        }
      }
      const payload = queue.shift()!;
      if (payload.kind === "content") {
        yield { kind: "content", delta: payload.delta };
      } else if (payload.kind === "thinking") {
        yield { kind: "thinking", delta: payload.delta };
      } else if (payload.kind === "tool_call_delta") {
        yield { kind: "tool_call_delta", delta: payload.delta };
      } else if (payload.kind === "usage") {
        if (payload.usage) {
          yield {
            kind: "done",
            usage: {
              inputTokens: payload.usage.inputTokens,
              outputTokens: payload.usage.outputTokens,
            },
          };
        }
      } else if (payload.kind === "done") {
        if (payload.usage) {
          yield {
            kind: "done",
            usage: {
              inputTokens: payload.usage.inputTokens,
              outputTokens: payload.usage.outputTokens,
            },
          };
        }
        break;
      } else if (payload.kind === "error") {
        yield { kind: "error", delta: payload.delta };
        break;
      }
    }

    if (unlisten) unlisten();
  }

  const queue: Array<{
    kind: string;
    delta: string;
    usage: { inputTokens: number; outputTokens: number } | null;
    done: boolean;
  }> = [];

  return gen();
}