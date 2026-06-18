/** 相对时间（Codex 风格：20 小时、1 周） */
export function formatRelativeTime(ts: number, now = Date.now()): string {
  const diff = Math.max(0, now - ts);
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return "刚刚";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} 分钟`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} 小时`;
  const day = Math.floor(hr / 24);
  if (day < 7) return `${day} 天`;
  const week = Math.floor(day / 7);
  if (week < 5) return `${week} 周`;
  const month = Math.floor(day / 30);
  if (month < 12) return `${month} 月`;
  return `${Math.floor(day / 365)} 年`;
}
