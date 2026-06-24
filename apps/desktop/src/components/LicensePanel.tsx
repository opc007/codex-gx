/**
 * LicensePanel — 社区免费版
 * v2.0: 永久免费，任何人可使用完整功能
 */
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

type TierInfo = {
  tier: string;
  display_name: string;
  features: string[];
};

export function LicensePanel({ onClose }: { onClose: () => void }) {
  const [summary, setSummary] = useState<TierInfo | null>(null);

  useEffect(() => {
    invoke<TierInfo[]>("license_tiers").then((tiers) => {
      setSummary(tiers[0]);
    });
  }, []);

  return (
    <div className="dialog-overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <dialog open className="license-panel" style={{ maxWidth: 560 }}>
        <header className="panel-header">
          <h2>Codex GX — 社区免费版</h2>
          <button className="panel-close" onClick={onClose} aria-label="关闭">✕</button>
        </header>

        <div className="panel-body">
          <div className="license-badge">
            <span className="badge-free">✓ 完全免费</span>
            <span className="badge-open">✓ 开源 MIT</span>
            <span className="badge-community">✓ 社区共建</span>
          </div>

          <p className="license-desc">
            Codex GX v2.0 正式永久免费。任何人可使用完整功能，无需激活码、无需付费。
          </p>

          {summary && (
            <div className="license-features">
              <h3>完整功能清单</h3>
              <ul>
                {summary.features.map((f, i) => (
                  <li key={i}>{f}</li>
                ))}
              </ul>
            </div>
          )}

          <div className="license-join">
            <h3>欢迎参与社区共建</h3>
            <p>Codex GX 是开源项目，欢迎开发者参与贡献：</p>
            <ul>
              <li>提交 Issue 报告问题</li>
              <li>提交 PR 修复 Bug 或添加功能</li>
              <li>分享使用经验，帮助更多人</li>
              <li>提出功能建议，共同规划路线图</li>
            </ul>
            <div className="license-links">
              <a
                href="https://github.com/opc007/codex-gx"
                target="_blank"
                rel="noreferrer"
                className="btn-github"
              >
                🌐 GitHub 仓库
              </a>
              <a
                href="https://github.com/opc007/codex-gx/issues"
                target="_blank"
                rel="noreferrer"
                className="btn-issue"
              >
                🐛 提 Bug / 建议
              </a>
            </div>
          </div>
        </div>
      </dialog>
    </div>
  );
}
