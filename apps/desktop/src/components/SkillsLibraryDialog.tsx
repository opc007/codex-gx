// v1.5：Skills 库 / 模板市场
// - 3 个 tab：已启用 / 全部 / 模板市场
// - 启用 / 禁用 / 导入 / 导出 / 删除 / 重置
// - 按 category 分组显示

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type SkillCategory =
  | "dev"
  | "write"
  | "research"
  | "productivity"
  | "fun"
  | "other";

type Skill = {
  name: string;
  description: string;
  category: SkillCategory;
  enabled: boolean;
  builtin: boolean;
  tags: string[];
  shell: string | null;
  prompt: string | null;
  chain: string[] | null;
  platform: string;
  author: string | null;
  version: string;
  created_at: number | null;
};

type SkillTemplate = {
  skill: Skill;
  downloads: number;
  rating: number;
  source: string;
};

type Props = {
  onClose: () => void;
};

export function SkillsLibraryDialog({ onClose }: Props) {
  const [grouped, setGrouped] = useState<Record<string, Skill[]>>({});
  const [templates, setTemplates] = useState<SkillTemplate[]>([]);
  const [tab, setTab] = useState<"enabled" | "all" | "market">("enabled");
  const [filter, setFilter] = useState("");
  const [importText, setImportText] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  const refresh = async () => {
    setBusy(true);
    try {
      const g = await invoke<Record<string, Skill[]>>("list_skills_grouped");
      setGrouped(g);
      const t = await invoke<SkillTemplate[]>("skill_market");
      setTemplates(t);
    } catch (e) {
      setStatus(`❌ ${e}`);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const handleToggle = async (s: Skill) => {
    await invoke("skill_toggle", { name: s.name, enabled: !s.enabled });
    await refresh();
  };

  const handleRemove = async (s: Skill) => {
    if (s.builtin) {
      setStatus("⚠️ 内置 skill 不可删除，只能禁用");
      return;
    }
    if (!confirm(`确定删除 skill \`${s.name}\`？`)) return;
    await invoke("skill_remove", { name: s.name });
    await refresh();
  };

  const handleExport = async (s: Skill) => {
    const json = await invoke<string>("skill_export", { name: s.name });
    await navigator.clipboard.writeText(json).catch(() => {});
    setStatus(`📋 已复制 \`${s.name}\` 的 JSON 到剪贴板`);
  };

  const handleImport = async () => {
    if (!importText.trim()) return;
    try {
      const name = await invoke<string>("skill_import", { json: importText });
      setStatus(`✅ 已导入 skill: ${name}`);
      setImportText("");
      setImportOpen(false);
      await refresh();
    } catch (e) {
      setStatus(`❌ 导入失败: ${e}`);
    }
  };

  const handleInstall = async (s: Skill) => {
    const json = JSON.stringify(s, null, 2);
    try {
      const name = await invoke<string>("skill_import", { json });
      setStatus(`✅ 已安装: ${name}`);
      await refresh();
    } catch (e) {
      setStatus(`❌ 安装失败: ${e}`);
    }
  };

  const handleResetBuiltin = async () => {
    if (!confirm("确定重置所有 user skills？（builtin 不会受影响）")) return;
    await invoke("skill_reset_builtin");
    await refresh();
    setStatus("✅ 已重置");
  };

  const allSkills: Skill[] = Object.values(grouped).flat();
  const visibleSkills = allSkills.filter((s) => {
    if (tab === "enabled" && !s.enabled) return false;
    if (filter) {
      const q = filter.toLowerCase();
      if (
        !s.name.toLowerCase().includes(q) &&
        !s.description.toLowerCase().includes(q) &&
        !s.tags.some((t) => t.toLowerCase().includes(q))
      ) {
        return false;
      }
    }
    return true;
  });

  const renderSkill = (s: Skill) => {
    const cat = s.category;
    const mode = s.shell ? "shell" : s.prompt ? "prompt" : s.chain ? "chain" : "?";
    return (
      <div key={s.name} className={`skill-row ${s.enabled ? "" : "skill-disabled"}`}>
        <div className="skill-row-head">
          <span className="skill-icon">{categoryIcon(cat)}</span>
          <span className="skill-name">/{s.name}</span>
          {s.builtin && <span className="skill-tag">官方</span>}
          <span className="skill-tag">{mode}</span>
          <span className="skill-tag">{s.platform}</span>
          {s.tags.map((t) => (
            <span key={t} className="skill-tag-soft">
              #{t}
            </span>
          ))}
          <span style={{ flex: 1 }} />
          <button
            className="btn small"
            onClick={() => handleToggle(s)}
            title={s.enabled ? "禁用" : "启用"}
          >
            {s.enabled ? "⏸" : "▶"}
          </button>
          <button
            className="btn small"
            onClick={() => handleExport(s)}
            title="导出 JSON"
          >
            📋
          </button>
          <button
            className="btn small"
            onClick={() => handleRemove(s)}
            title={s.builtin ? "内置不可删" : "删除"}
            disabled={s.builtin}
            style={s.builtin ? { opacity: 0.3 } : {}}
          >
            🗑
          </button>
        </div>
        <div className="skill-desc">{s.description}</div>
        {s.author && (
          <div className="skill-author">
            by {s.author} · v{s.version}
          </div>
        )}
        {s.shell && (
          <details className="skill-detail">
            <summary>shell</summary>
            <pre>{s.shell}</pre>
          </details>
        )}
        {s.prompt && (
          <details className="skill-detail">
            <summary>prompt</summary>
            <pre>{s.prompt}</pre>
          </details>
        )}
        {s.chain && (
          <details className="skill-detail">
            <summary>chain</summary>
            <pre>{s.chain.join(" → ")}</pre>
          </details>
        )}
      </div>
    );
  };

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 880, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>📚 Skills 库 / 模板市场（v1.5）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">×</button>
        </div>

        <div className="modal-body theme-body">
          <div className="skills-toolbar">
            <div className="skills-tabs">
              <button
                className={`btn small ${tab === "enabled" ? "primary" : ""}`}
                onClick={() => setTab("enabled")}
              >
                ✅ 已启用 ({allSkills.filter((s) => s.enabled).length})
              </button>
              <button
                className={`btn small ${tab === "all" ? "primary" : ""}`}
                onClick={() => setTab("all")}
              >
                📦 全部 ({allSkills.length})
              </button>
              <button
                className={`btn small ${tab === "market" ? "primary" : ""}`}
                onClick={() => setTab("market")}
              >
                🏪 模板市场 ({templates.length})
              </button>
            </div>
            <input
              className="vault-password-input skills-search"
              placeholder="搜索：name / description / tag"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            <button className="btn small" onClick={() => setImportOpen(!importOpen)}>
              {importOpen ? "× 取消" : "📥 导入"}
            </button>
            <button className="btn small" onClick={refresh} disabled={busy}>
              🔄
            </button>
            <button className="btn small" onClick={handleResetBuiltin}>
              🗑 重置
            </button>
          </div>

          {importOpen && (
            <div className="skills-import">
              <h4>📥 导入 Skill JSON</h4>
              <textarea
                className="vault-password-input"
                rows={8}
                placeholder='{"name": "...", "description": "...", "shell": "..."}'
                value={importText}
                onChange={(e) => setImportText(e.target.value)}
              />
              <div style={{ marginTop: 6 }}>
                <button className="btn primary" onClick={handleImport}>
                  导入
                </button>
                <button className="btn small" onClick={async () => {
                  try {
                    const t = await navigator.clipboard.readText();
                    setImportText(t);
                  } catch { /* ignore */ }
                }}>
                  📋 从剪贴板
                </button>
              </div>
            </div>
          )}

          {tab === "market" ? (
            <div className="skills-market">
              {templates.map((t) => (
                <div key={t.skill.name} className="market-item">
                  {renderSkill(t.skill)}
                  <div className="market-meta">
                    <span>📥 {t.downloads} 安装</span>
                    <span>⭐ {t.rating.toFixed(1)}</span>
                    <span>📦 {t.source}</span>
                    <span style={{ flex: 1 }} />
                    <button
                      className="btn small primary"
                      onClick={() => handleInstall(t.skill)}
                    >
                      ＋ 安装
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="skills-grouped">
              {(["dev", "write", "research", "productivity", "fun", "other"] as SkillCategory[])
                .filter((c) =>
                  visibleSkills.some((s) => s.category === c),
                )
                .map((c) => (
                  <div key={c} className="skills-group">
                    <h4>
                      {categoryIcon(c)} {categoryName(c)} (
                      {visibleSkills.filter((s) => s.category === c).length})
                    </h4>
                    {visibleSkills
                      .filter((s) => s.category === c)
                      .map(renderSkill)}
                  </div>
                ))}
            </div>
          )}

          {status && <p className="devices-status">{status}</p>}

          <div className="lint-help">
            <h4>💡 三种模式</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li><strong>shell</strong>：执行 shell 命令（$ARG 会被替换为参数）</li>
              <li><strong>prompt</strong>：注入 system prompt 片段，由 LLM 解析</li>
              <li><strong>chain</strong>：按顺序触发多个 skill（用 <code>/chain</code> 命令）</li>
              <li>官方 skill 标记为 <code>builtin</code>，可禁用但不能删</li>
              <li>用户自定义 skill 存在 <code>~/.agentshell/skills.json</code></li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}

function categoryIcon(c: SkillCategory): string {
  switch (c) {
    case "dev": return "💻";
    case "write": return "✍️";
    case "research": return "🔍";
    case "productivity": return "⚡";
    case "fun": return "🎮";
    case "other": return "📦";
  }
}

function categoryName(c: SkillCategory): string {
  switch (c) {
    case "dev": return "开发";
    case "write": return "写作";
    case "research": return "研究";
    case "productivity": return "效率";
    case "fun": return "趣味";
    case "other": return "其他";
  }
}
