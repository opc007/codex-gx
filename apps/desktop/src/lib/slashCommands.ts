// v1.9.6+：Slash 命令注册表（Codex App 风格：动态聚合 built-in + skills + plugins）
//
// 设计参考：https://developers.openai.com/codex/app/commands
//
// 单一真理源：所有内置 slash 命令在这里声明。
// 动态命令（用户 skills / plugins）由 Composer 在需要时拉取后追加到 BUILTINS 后面。

export type SlashCommand = {
  /** 命令名（不含 /，如 "help"） */
  name: string;
  /** 显示在菜单里的提示文本 */
  description: string;
  /** 菜单分组（用于分组显示） */
  group: "通用" | "会话" | "Git" | "模型" | "学习" | "多模态" | "背景" | "插件";
  /** 完整命令字符串（不含 /），用于点击菜单时填到输入框 */
  template: string;
  /** 可选别名 */
  aliases?: string[];
  /** 是否在菜单隐藏（仍可手输） */
  hidden?: boolean;
};

/**
 * 内置 slash 命令注册表。
 * 任何 if (trimmed === "/foo") 都应该在这里有对应条目；menu 渲染也读这张表。
 */
export const BUILTIN_SLASH_COMMANDS: SlashCommand[] = [
  // —— 通用 ——
  { name: "help", description: "显示命令帮助", group: "通用", template: "help" },
  { name: "status", description: "查看当前会话状态", group: "通用", template: "status" },
  { name: "clear", description: "清空当前会话", group: "通用", template: "clear" },
  { name: "usage", description: "本会话 token 用量 + 费用估算", group: "通用", template: "usage" },
  { name: "theme", description: "切换主题：/theme dark|light|system", group: "通用", template: "theme " },
  { name: "lang", description: "切换界面语言：/lang zh|en", group: "通用", template: "lang " },
  { name: "apikey", description: "打开 API Key 设置", group: "通用", template: "apikey", aliases: ["key"] },

  // —— 会话 ——
  { name: "compress", description: "压缩长会话（保留最近 N 条）", group: "会话", template: "compress " },
  { name: "fork", description: "Fork 当前 session（保留历史副本）", group: "会话", template: "fork" },
  { name: "side", description: "旁问（24h 临时 session）", group: "会话", template: "side " },
  { name: "approval", description: "切换手动批准模式", group: "会话", template: "approval" },
  { name: "plan", description: "切换 plan mode（先看计划再执行）", group: "会话", template: "plan" },
  { name: "redact", description: "测试脱敏：/redact <文本>", group: "会话", template: "redact " },
  { name: "init", description: "为当前项目生成 AGENTS.md 骨架", group: "会话", template: "init" },
  { name: "goal", description: "为当前项目设置持久目标", group: "会话", template: "goal " },

  // —— Git & IDE ——
  { name: "ide", description: "获取 IDE context（Cursor/VSCode）", group: "Git", template: "ide" },
  { name: "diff", description: "Git diff vs HEAD", group: "Git", template: "diff" },
  { name: "review", description: "AI 评审当前 diff", group: "Git", template: "review" },
  { name: "lint", description: "运行代码 review（clippy / tsc / TODO）", group: "Git", template: "lint" },

  // —— 模型路由 ——
  { name: "route", description: "看某条消息会被路由到哪个模型", group: "模型", template: "route " },
  { name: "model", description: "列出可用模型（Top bar 下拉直接选）", group: "模型", template: "model", hidden: true },

  // —— 学习 / 长期记忆 ——
  { name: "remember", description: "记一条长期记忆：/remember <内容> [#tag]", group: "学习", template: "remember " },
  { name: "memories", description: "列出所有长期记忆", group: "学习", template: "memories" },
  { name: "recall", description: "检索相关记忆：/recall <查询>", group: "学习", template: "recall " },
  { name: "forget", description: "遗忘一条记忆：/forget <id 前 8 位>", group: "学习", template: "forget " },
  { name: "learn", description: "学习统计 / 反馈", group: "学习", template: "learn" },
  { name: "skills", description: "列出已加载的自定义 skill", group: "学习", template: "skills" },

  // —— 多模态 ——
  { name: "image", description: "MiniMax 文/图生图：/image <提示词>", group: "多模态", template: "image " },
  { name: "video", description: "MiniMax 文生视频：/video <提示词>", group: "多模态", template: "video " },
  { name: "vision", description: "本地图像 / OCR / 标注", group: "多模态", template: "vision " },
  { name: "screenshot", description: "截当前主屏幕（同步返回 base64）", group: "多模态", template: "screenshot", aliases: ["ss"] },
  { name: "coord", description: "屏幕相对坐标换算", group: "多模态", template: "coord " },
  { name: "voice", description: "Whisper 状态 / 下载模型 / 流式 TTS", group: "多模态", template: "voice " },
  { name: "speak", description: "TTS 朗读：/speak <文本>", group: "多模态", template: "speak ", aliases: ["say"] },
  { name: "tts", description: "打开 TTS 面板", group: "多模态", template: "tts" },

  // —— 后台 / 任务 ——
  { name: "ps", description: "列出后台进程", group: "背景", template: "ps" },
  { name: "stop", description: "停止后台进程", group: "背景", template: "stop " },
  { name: "bg", description: "启动后台进程：/bg <label> <cmd>", group: "背景", template: "bg " },
  { name: "queue", description: "任务队列（不阻塞 chat）", group: "背景", template: "queue" },
  { name: "local", description: "探测本地 Ollama / llama.cpp", group: "背景", template: "local" },
  { name: "flow", description: "打开 Agent 流程图", group: "背景", template: "flow" },
  { name: "sync", description: "同步当前 session 到本地缓存", group: "背景", template: "sync" },
  { name: "pocket", description: "Pocket webhook 配对 / 状态", group: "背景", template: "pocket" },
  { name: "mobile", description: "Mobile Remote 管理", group: "背景", template: "mobile" },
  { name: "plugin", description: "打开插件热加载面板", group: "背景", template: "plugin" },
  { name: "perm", description: "Desktop 权限列表 / 协议", group: "背景", template: "perm" },
];

/** 索引：name + aliases → command */
const BUILTIN_INDEX = new Map<string, SlashCommand>();
for (const c of BUILTIN_SLASH_COMMANDS) {
  BUILTIN_INDEX.set(c.name, c);
  for (const a of c.aliases ?? []) BUILTIN_INDEX.set(a, c);
}

/** 已知命令名集合（用于"是否走动态 skill"判断） */
export const BUILTIN_NAME_SET: Set<string> = new Set(BUILTIN_INDEX.keys());

/** 模糊匹配 — 支持名称和模板前缀 */
export function searchSlashCommands(
  query: string,
  dynamicSkills: Array<{ name: string; description: string }> = [],
  dynamicPlugins: Array<{ name: string; description: string }> = [],
): SlashCommand[] {
  const q = query.replace(/^\//, "").toLowerCase().trim();
  const list: SlashCommand[] = [];
  for (const c of BUILTIN_SLASH_COMMANDS) {
    if (c.hidden && q !== c.name) continue;
    if (!q) {
      list.push(c);
    } else if (
      c.name.toLowerCase().startsWith(q) ||
      c.template.toLowerCase().startsWith(q) ||
      (c.aliases ?? []).some((a) => a.toLowerCase().startsWith(q))
    ) {
      list.push(c);
    }
  }
  for (const s of dynamicSkills) {
    if (!q || s.name.toLowerCase().includes(q)) {
      list.push({
        name: s.name,
        description: s.description || "(user skill)",
        group: "插件",
        template: s.name + " ",
      });
    }
  }
  for (const p of dynamicPlugins) {
    if (!q || p.name.toLowerCase().includes(q)) {
      list.push({
        name: p.name,
        description: p.description || "(plugin)",
        group: "插件",
        template: p.name + " ",
      });
    }
  }
  return list;
}
