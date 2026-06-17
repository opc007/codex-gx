// 与 Rust 后端的聊天桥（v0.3 — listen "agent:event" 统一事件流）
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ChatMessage = {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  text: string;
  thinking?: string;
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: unknown;
    result?: string;
    success?: boolean;
    error?: string;
  }>;
  createdAt: number;
  streaming?: boolean;
  inputTokens?: number;
  outputTokens?: number;
};

export type AgentEvent = {
  sessionId: string;
  kind:
    | "content"
    | "thinking"
    | "tool_call_delta"
    | "tool_call_complete"
    | "tool_result"
    | "assistant_turn"
    | "stage"
    | "approval_request"
    | "plan"        // v0.6
    | "subagent"    // v0.7
    | "usage"
    | "done"
    | "error";
  delta: string;
  usage: { inputTokens: number; outputTokens: number } | null;
  done: boolean;
  toolCall: {
    id: string;
    name: string;
    arguments: unknown;
    sessionId: string;
  } | null;
  toolResult: {
    callId: string;
    success: boolean;
    output: string;
    error: string | null;
    sessionId: string;
  } | null;
  stage: {
    stage: string;
    label: string;
    detail: string | null;
  } | null;
  plan: {
    plan: string;
    planId: string;
  } | null; // v0.6
  subagent: {
    subagentId: string;
    role: string;
    status: "started" | "running" | "done" | "error";
    task: string;
    result: string | null;
    error: string | null;
    content?: string; // v0.7：增量内容
    step?: number;
  } | null; // v0.7
};

export type SendChatParams = {
  sessionId: string;
  userMessage: string;
  model: string;
  /** 历史消息（不含 tool_calls 的简化版） */
  history?: Array<{ role: string; content: string; toolCallId?: string }>;
  /** v0.4：是否需要用户批准 tool call */
  requireApproval?: boolean;
  /** v0.6：plan mode — 先输出 plan 等用户批准 */
  planMode?: boolean;
};

/**
 * v0.3：调用 agent_run 命令 + 监听 "agent:event" 事件流
 *
 * Generator 通过队列 + Promise 解耦 listener 回调
 */
export async function sendChatStream(
  params: SendChatParams
): Promise<{
  stream: AsyncIterable<AgentEvent>;
  cancel: () => void;
}> {
  // 监听队列
  const queue: AgentEvent[] = [];
  let waiter: ((v: void) => void) | null = null;
  let done = false;
  let unlisten: UnlistenFn | null = null;

  const wake = () => {
    if (waiter) {
      const w = waiter;
      waiter = null;
      w();
    }
  };

  const push = (evt: AgentEvent) => {
    queue.push(evt);
    wake();
    if (evt.done) {
      done = true;
      wake();
    }
  };

  // 注册 listener
  unlisten = await listen<AgentEvent>("agent:event", (event) => {
    if (event.payload.sessionId !== params.sessionId) return;
    push(event.payload);
  });

  // 触发后端
  await invoke("agent_run", {
    req: {
      model: params.model,
      message: params.userMessage,
      sessionId: params.sessionId,
      messages: params.history || [],
      requireApproval: params.requireApproval ?? true,
      planMode: params.planMode ?? false, // v0.6
    },
  });

  async function* gen(): AsyncIterable<AgentEvent> {
    while (true) {
      // 等到有数据或 done
      while (queue.length === 0 && !done) {
        await new Promise<void>((resolve) => {
          waiter = resolve;
        });
      }
      if (queue.length === 0 && done) {
        break;
      }
      const evt = queue.shift()!;
      yield evt;
      if (evt.done) break;
    }
    if (unlisten) unlisten();
  }

  const cancel = () => {
    if (unlisten) unlisten();
    done = true;
    wake();
  };

  return { stream: gen(), cancel };
}