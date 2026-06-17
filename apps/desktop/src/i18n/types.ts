// i18n 类型 + 字典
export type Locale = "zh" | "en";

export const SUPPORTED_LOCALES: Locale[] = ["zh", "en"];
export const DEFAULT_LOCALE: Locale = "zh";

export interface Dict {
  // 通用
  appName: string;
  send: string;
  stop: string;
  cancel: string;
  confirm: string;
  deny: string;
  approve: string;
  reject: string;
  edit: string;
  save: string;
  loading: string;
  error: string;
  retry: string;
  newSession: string;
  settings: string;

  // 模型
  model: string;
  auto: string;
  autoRoute: string;

  // 工具栏
  approvalOn: string;
  approvalOff: string;
  planOn: string;
  planOff: string;

  // 输入
  placeholder: string;
  /** 无当前会话时输入框提示（勿用 cancel） */
  noSessionPlaceholder: string;
  inputHint: string;

  // 空态
  emptyHint: string;
  emptySubHint: string;

  // 错误
  sendError: string;

  // 命令
  cmdHelp: string;
  cmdStatus: string;
  cmdApproval: string;
  cmdPlan: string;
  cmdRoute: string;
  cmdRemember: string;
  cmdMemories: string;
  cmdRecall: string;
  cmdForget: string;
  cmdSkills: string;
  cmdUsage: string;
  cmdIde: string;
  cmdDiff: string;
  cmdReview: string;

  // 状态
  thinking: string;
  planning: string;
  acting: string;
  verifying: string;
  done: string;

  // 记忆
  memoryAdded: string;
  memoryEmpty: string;
  memoryListed: string;
  memoryRecalled: string;
  memoryForgotten: string;
  memoryNoMatch: string;

  // Skill
  skillsEmpty: string;
  skillsLoaded: string;
  skillNotFound: string;
  skillExample: string;
  skillCallHint: string;

  // 工具调用
  toolRunning: string;
  toolDone: string;
  toolFailed: string;
  toolReplay: string;

  // 审批
  approvalTitle: string;
  approvalMessage: (tool: string) => string;
  approvalCountdown: (sec: number) => string;
  approvalRiskLow: string;
  approvalRiskMid: string;
  approvalRiskHigh: string;

  // 计划
  planTitle: string;
  planEditHint: string;
  planApprove: string;
  planDeny: string;

  // Subagent
  subagentStarted: string;
  subagentRunning: string;
  subagentDone: string;
  subagentError: string;
}

export const LOCALE_LABELS: Record<Locale, string> = {
  zh: "中文",
  en: "English",
};