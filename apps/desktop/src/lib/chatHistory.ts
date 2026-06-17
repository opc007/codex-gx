import { getSessionsState } from "../stores/sessions";

/** 构建 agent_run 所需的历史消息（不含当前轮 user / streaming 占位） */
export function buildChatHistory(
  sessionId: string,
  excludeIds: string[] = [],
): Array<{ role: string; content: string; tool_call_id?: string }> {
  const exclude = new Set(excludeIds);
  const msgs = getSessionsState().messages[sessionId] ?? [];
  return msgs
    .filter((m) => !m.streaming && !exclude.has(m.id))
    .map((m) => ({
      role: m.role,
      content: m.text,
      ...(m.role === "tool" && m.toolCalls?.[0]?.id
        ? { tool_call_id: m.toolCalls[0].id }
        : {}),
    }));
}
