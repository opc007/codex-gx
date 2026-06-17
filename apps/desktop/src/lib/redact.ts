// v1.1：敏感数据脱敏
//
// 检测：
// - API key 风格 (sk-xxx, ghp_xxx, AKIA..., xoxb-...)
// - JWT (eyJ...)
// - Email
// - IPv4
// - Bearer / Token 头
// - 私钥 PEM
// - 16 进制 64 字符（疑似 hash/secret）

const PATTERNS: Array<{ name: string; re: RegExp; replace: (m: RegExpMatchArray) => string }> = [
  {
    name: "openai-key",
    re: /\bsk-[A-Za-z0-9_-]{20,}\b/g,
    replace: () => "[REDACTED:openai-key]",
  },
  {
    name: "anthropic-key",
    re: /\bsk-ant-[A-Za-z0-9_-]{20,}\b/g,
    replace: () => "[REDACTED:anthropic-key]",
  },
  {
    name: "github-pat",
    re: /\bghp_[A-Za-z0-9]{36,}\b/g,
    replace: () => "[REDACTED:github-pat]",
  },
  {
    name: "github-fine",
    re: /\bgithub_pat_[A-Za-z0-9_]{60,}\b/g,
    replace: () => "[REDACTED:github-fine]",
  },
  {
    name: "aws-key",
    re: /\bAKIA[0-9A-Z]{16}\b/g,
    replace: () => "[REDACTED:aws-key]",
  },
  {
    name: "slack-token",
    re: /\bxox[baprs]-[A-Za-z0-9-]{10,}\b/g,
    replace: () => "[REDACTED:slack-token]",
  },
  {
    name: "jwt",
    re: /\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g,
    replace: () => "[REDACTED:jwt]",
  },
  {
    name: "bearer",
    re: /\b(Bearer|Token)\s+[A-Za-z0-9_.-]{16,}/gi,
    replace: (m) => `${m[1]} [REDACTED]`,
  },
  {
    name: "email",
    re: /\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/g,
    replace: (m) => {
      const email = m[0];
      const at = email.indexOf("@");
      const domain = email.slice(at + 1);
      const first = email[0] ?? "x";
      return `${first}***@${domain}`;
    },
  },
  {
    name: "ipv4",
    re: /\b(?:\d{1,3}\.){3}\d{1,3}\b/g,
    replace: (m) => {
      const parts = m[0].split(".");
      if (parts.length !== 4) return m[0];
      // 内网地址保留
      if (parts[0] === "10" || (parts[0] === "192" && parts[1] === "168")) return m[0];
      if (parts[0] === "172" && +parts[1] >= 16 && +parts[1] <= 31) return m[0];
      return `${parts[0]}.${parts[1]}.x.x`;
    },
  },
  {
    name: "hex-secret",
    re: /\b[0-9a-fA-F]{64}\b/g,
    replace: () => "[REDACTED:hex-64]",
  },
  {
    name: "private-key",
    re: /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g,
    replace: () => "[REDACTED:private-key]",
  },
];

export interface RedactionResult {
  text: string;
  redactions: Array<{ type: string; count: number }>;
}

export function redact(input: string): RedactionResult {
  let text = input;
  const redactions: Array<{ type: string; count: number }> = [];
  for (const p of PATTERNS) {
    const re = new RegExp(p.re.source, p.re.flags);
    const before = text;
    text = text.replace(p.re, (...args) => {
      const m = args[0] as string;
      return p.replace([m, m] as unknown as RegExpMatchArray);
    });
    // 统计命中数（用同一个 g 标志 regex）
    const matches = before.match(re);
    if (matches && matches.length > 0) {
      redactions.push({ type: p.name, count: matches.length });
    }
  }
  return { text, redactions };
}

/** 直接做替换（不返回统计） */
export function redactSimple(input: string): string {
  return redact(input).text;
}

/** 检测文本中是否含敏感数据 */
export function hasSecrets(input: string): boolean {
  return PATTERNS.some((p) => p.re.test(input));
}

/** 列出匹配的敏感类型（不含位置） */
export function detectTypes(input: string): string[] {
  const out: string[] = [];
  for (const p of PATTERNS) {
    if (p.re.test(input)) out.push(p.name);
  }
  return out;
}