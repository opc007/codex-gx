// v1.3：主题工作室
// - 浏览内置主题（Default / Solarized / Nord / Dracula / Gruvbox / Monokai）
// - 应用主题（立即生效）
// - 编辑当前主题的颜色（live preview）
// - 导出为 JSON / 从 JSON 导入
// - 保存为用户自定义主题
// - 删除自定义主题

import { useState } from "react";
import {
  BUILTIN_THEMES,
  type CustomTheme,
  type ThemeColors,
  useThemeStore,
  reapplyTheme,
} from "../stores/theme";

type Props = {
  onClose: () => void;
};

const COLOR_FIELDS: Array<{ key: keyof ThemeColors; label: string; hint: string }> = [
  { key: "bg", label: "背景 (bg)", hint: "主背景" },
  { key: "bgSecondary", label: "次背景", hint: "卡片 / 顶栏" },
  { key: "bgHover", label: "悬浮背景", hint: "hover 时" },
  { key: "text", label: "主文字", hint: "" },
  { key: "textMuted", label: "次文字", hint: "描述 / 标签" },
  { key: "border", label: "边框", hint: "" },
  { key: "primary", label: "主色", hint: "按钮 / 链接" },
  { key: "primaryText", label: "主色文字", hint: "在主色上的字" },
  { key: "danger", label: "危险", hint: "删除 / 错误" },
  { key: "warning", label: "警告", hint: "" },
  { key: "success", label: "成功", hint: "" },
  { key: "codeBg", label: "代码块背景", hint: "" },
];

export function ThemeStudioDialog({ onClose }: Props) {
  const allThemes = useThemeStore((s) => s.themes);
  const activeId = useThemeStore((s) => s.activeThemeId);
  const setActive = useThemeStore((s) => s.setActiveThemeId);
  const save = useThemeStore((s) => s.saveCustomTheme);
  const del = useThemeStore((s) => s.deleteCustomTheme);
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);

  const [editing, setEditing] = useState<CustomTheme | null>(null);
  const [importText, setImportText] = useState("");
  const [importErr, setImportErr] = useState<string | null>(null);
  const [tab, setTab] = useState<"browse" | "edit" | "import">("browse");

  const active = allThemes.find((t) => t.id === activeId) ?? BUILTIN_THEMES[0];

  const handleApply = (t: CustomTheme) => {
    setActive(t.id);
  };

  const handleStartEdit = (t: CustomTheme) => {
    setEditing({ ...t, colors: { ...t.colors } });
  };

  const handleSave = () => {
    if (!editing) return;
    save(editing);
    setActive(editing.id);
    reapplyTheme();
  };

  const handleColorChange = (key: keyof ThemeColors, val: string) => {
    if (!editing) return;
    const next: CustomTheme = {
      ...editing,
      colors: { ...editing.colors, [key]: val },
    };
    setEditing(next);
    // live preview
    save(next);
    setActive(next.id);
  };

  const handleDelete = (id: string) => {
    if (BUILTIN_THEMES.find((t) => t.id === id)) {
      alert("内置主题不能删除");
      return;
    }
    if (!confirm("确认删除此自定义主题？")) return;
    del(id);
  };

  const handleExport = () => {
    const blob = new Blob([JSON.stringify(active, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${active.id}.theme.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleImport = () => {
    setImportErr(null);
    try {
      const obj = JSON.parse(importText);
      if (!obj.id || !obj.name || !obj.colors || typeof obj.colors !== "object") {
        throw new Error("主题 JSON 必须包含 id / name / colors 字段");
      }
      // 给一个安全默认值
      const t: CustomTheme = {
        id: String(obj.id),
        name: String(obj.name),
        base: obj.base === "light" ? "light" : "dark",
        colors: { ...BUILTIN_THEMES[0].colors, ...obj.colors },
      };
      save(t);
      setActive(t.id);
      setImportText("");
      setTab("browse");
    } catch (e) {
      setImportErr(String(e));
    }
  };

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 720, width: "92vw" }}
      >
        <div className="modal-head">
          <h3>🎨 主题工作室</h3>
          <div className="theme-tabs">
            <button
              className={`btn tab ${tab === "browse" ? "active" : ""}`}
              onClick={() => setTab("browse")}
            >
              浏览
            </button>
            <button
              className={`btn tab ${tab === "edit" ? "active" : ""}`}
              onClick={() => setTab("edit")}
            >
              编辑
            </button>
            <button
              className={`btn tab ${tab === "import" ? "active" : ""}`}
              onClick={() => setTab("import")}
            >
              导入
            </button>
          </div>
          <button className="topbar-btn" onClick={onClose} title="关闭">
            ×
          </button>
        </div>

        <div className="modal-body theme-body">
          {tab === "browse" && (
            <div className="theme-grid">
              {allThemes.map((t) => {
                const isActive = t.id === activeId;
                const isBuiltin = !!BUILTIN_THEMES.find((b) => b.id === t.id);
                return (
                  <div
                    key={t.id}
                    className={`theme-card ${isActive ? "active" : ""}`}
                    onClick={() => handleApply(t)}
                  >
                    <div className="theme-preview">
                      <div
                        className="theme-preview-bar"
                        style={{ background: t.colors.bgSecondary, color: t.colors.text }}
                      >
                        {t.name}
                      </div>
                      <div
                        className="theme-preview-body"
                        style={{ background: t.colors.bg, color: t.colors.text }}
                      >
                        <div
                          className="theme-preview-btn"
                          style={{
                            background: t.colors.primary,
                            color: t.colors.primaryText,
                          }}
                        >
                          主按钮
                        </div>
                        <div
                          className="theme-preview-text"
                          style={{ color: t.colors.textMuted }}
                        >
                          次要文字
                        </div>
                        <div
                          className="theme-preview-code"
                          style={{ background: t.colors.codeBg, color: t.colors.text }}
                        >
                          {`# code { }`}
                        </div>
                      </div>
                    </div>
                    <div className="theme-card-foot">
                      <span className="theme-card-name">
                        {t.name}
                        {isBuiltin && <span className="mp-tag-small">内置</span>}
                      </span>
                      <div className="theme-card-actions">
                        <button
                          className="btn small"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleStartEdit(t);
                            setTab("edit");
                          }}
                          title="编辑颜色"
                        >
                          ✎
                        </button>
                        {!isBuiltin && (
                          <button
                            className="btn small danger"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDelete(t.id);
                            }}
                            title="删除"
                          >
                            ×
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          {tab === "edit" && (
            <div className="theme-edit">
              <div className="theme-edit-head">
                <input
                  className="vault-password-input"
                  value={editing?.name ?? active.name}
                  placeholder="主题名称"
                  onChange={(e) => {
                    const t = editing ?? active;
                    setEditing({ ...t, name: e.target.value });
                  }}
                />
                <select
                  className="topbar-select"
                  value={editing?.base ?? active.base}
                  onChange={(e) => {
                    const t = editing ?? active;
                    setEditing({ ...t, base: e.target.value as "light" | "dark" });
                  }}
                >
                  <option value="dark">深色</option>
                  <option value="light">浅色</option>
                </select>
                <button
                  className="btn primary"
                  onClick={() => {
                    const t = editing ?? active;
                    save({ ...t, id: `custom_${Date.now()}` });
                    setActive(t.id === `custom_${Date.now()}` ? t.id : t.id);
                    reapplyTheme();
                  }}
                >
                  另存为
                </button>
                <button
                  className="btn primary"
                  onClick={handleSave}
                  disabled={!editing}
                >
                  保存
                </button>
                <button className="btn" onClick={handleExport}>
                  导出 JSON
                </button>
              </div>
              <div className="theme-colors-grid">
                {COLOR_FIELDS.map((f) => {
                  const cur = editing?.colors[f.key] ?? active.colors[f.key];
                  return (
                    <div className="theme-color-row" key={f.key}>
                      <label title={f.hint}>
                        <span>{f.label}</span>
                        <code>{f.hint}</code>
                      </label>
                      <input
                        type="color"
                        value={cur}
                        onChange={(e) => handleColorChange(f.key, e.target.value)}
                        className="color-input"
                      />
                      <input
                        className="vault-password-input"
                        value={cur}
                        onChange={(e) => handleColorChange(f.key, e.target.value)}
                        spellCheck={false}
                        style={{ flex: 1 }}
                      />
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {tab === "import" && (
            <div className="theme-import">
              <p style={{ color: "var(--text-muted)", fontSize: 13 }}>
                粘贴主题 JSON（从「编辑 → 导出 JSON」得到）：
              </p>
              <textarea
                className="vault-password-input"
                style={{
                  width: "100%",
                  minHeight: 280,
                  fontFamily: "ui-monospace, monospace",
                  fontSize: 12,
                }}
                value={importText}
                onChange={(e) => setImportText(e.target.value)}
                spellCheck={false}
                placeholder={`{
  "id": "my-theme",
  "name": "My Theme",
  "base": "dark",
  "colors": {
    "bg": "#1e1e1e",
    "primary": "#ff6b6b",
    ...
  }
}`}
              />
              {importErr && <div className="mp-error">❌ {importErr}</div>}
              <div className="modal-actions">
                <button className="btn" onClick={() => setImportText("")}>
                  清空
                </button>
                <button className="btn primary" onClick={handleImport}>
                  导入
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="modal-foot theme-foot">
          <span style={{ color: "var(--text-muted)", fontSize: 12 }}>
            当前: <strong>{active.name}</strong> · 模式: {mode}
          </span>
          <div className="theme-mode-toggle">
            <button
              className={`btn small ${mode === "light" ? "primary" : ""}`}
              onClick={() => setMode("light")}
            >
              ☀️
            </button>
            <button
              className={`btn small ${mode === "dark" ? "primary" : ""}`}
              onClick={() => setMode("dark")}
            >
              🌙
            </button>
            <button
              className={`btn small ${mode === "system" ? "primary" : ""}`}
              onClick={() => setMode("system")}
            >
              🖥️
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}