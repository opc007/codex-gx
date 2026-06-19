// Provider 列表（动态加载）
import { invoke } from "@tauri-apps/api/core";

export type ProviderInfo = {
  id: string;
  name: string;
  models: string[];
  defaultModel: string;
  envKey: string;
};

export async function loadProviders(): Promise<ProviderInfo[]> {
  try {
    return await invoke<ProviderInfo[]>("list_providers");
  } catch {
    // fallback 给浏览器开发用
    return [
      {
        id: "minimax",
        name: "MiniMax (国产)",
        models: ["MiniMax-M3"],
        defaultModel: "MiniMax-M3",
        envKey: "MINIMAX_API_KEY",
      },
      {
        id: "deepseek",
        name: "DeepSeek",
        models: ["deepseek-chat", "deepseek-chat", "deepseek-reasoner"],
        defaultModel: "deepseek-chat",
        envKey: "DEEPSEEK_API_KEY",
      },
    ];
  }
}