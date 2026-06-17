// v1.5：Plugin 热加载面板
// - 列出 ~/.agentshell/plugins/*.json 中所有插件
// - 安装 / 卸载 / 重新加载（热加载）
// - 一键安装 5 个默认插件
// - 试运行 step 链（debug 工具）

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type HookKind = "pre_send" | "post_recv" | "slash";

type PluginStep = {
  op: string;
  arg?: string;
};

type Hook = {
  kind: HookKind;
  command: string | null;
  description: string | null;
  script: string | null;
  steps: PluginStep[];
};

type PluginManifest = {
  name: string;
  version: string;
  description: string;
  author: string | null;
  tags: string[];
  hooks: Hook[];
};

type Props = {
  onClose: () => void;
};

const KIND_ICON: Record<HookKind, string> = {
  pre_send: "✏️",
  post_recv: "📤",
  slash: "⚡",
};

const STEP_OP_LIST = [
  "trim",
  "lower",
  "upper",
  "append",
  "prepend",
  "replace",
  "truncate",
  "wrap",
  "template",
  "to_checklist",
  "to_bullets",
  "repeat",
  "reverse",
  "meta",
];

export function PluginPanel({ onClose }: Props) {
  const [plugins, setPlugins] = useState<PluginManifest[]>([]);
  const [busy, setBusy] = useState(false);
  const [hint, setHint] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [importText, setImportText] = useState("");
  const [stepInput, setStepInput] = useState("Hello, World");
  const [stepResult, setStepResult] = useState("");
  const [selectedPlugin, setSelectedPlugin] = useState<string | null>(null);

  const refresh = async () => {
    setBusy(true);
    try {
      const p = await invoke<PluginManifest[]>("plugin_list");
      setPlugins(p);
    } catch (e) {
      setHint(`❌ ${e}`);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const handleReload = async () => {
    setBusy(true);
    try {
      const n = await invoke<number>("plugin_reload");
      await refresh();
      setHint(`🔄 已重载 ${n} 个插件`);
    } catch (e) {
      setHint(`❌ ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleInstallDefaults = async () => {
    setBusy(true);
    try {
      const installed = await invoke<string[]>("plugin_install_defaults");
      await refresh();
      setHint(`✅ 已安装默认：${installed.join(", ")}`);
    } catch (e) {
      setHint(`❌ ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async (name: string) => {
    if (!confirm(`确定删除插件 ${name}？`)) return;
    await invoke("plugin_remove", { name });
    await refresh();
  };

  const handleImport = async () => {
    if (!importText.trim()) return;
    try {
      const name = await invoke<string>("plugin_install", { json: importText });
      setHint(`✅ 已安装插件：${name}`);
      setImportText("");
      setImportOpen(false);
      await refresh();
    } catch (e) {
      setHint(`❌ 解析失败: ${e}`);
    }
  };

  const handleExport = async (p: PluginManifest) => {
    const json = JSON.stringify(p, null, 2);
    await navigator.clipboard.writeText(json).catch(() => {});
    setHint(`📋 已复制 ${p.name} 的 JSON 到剪贴板`);
  };

  const handleTryStep = async (op: string, arg?: string) => {
    try {
      const r = await invoke<string>("plugin_run_steps", {
        steps: [{ op, arg }],
        input: stepInput,
      });
      setStepResult(r);
    } catch (e) {
      setStepResult(`❌ ${e}`);
    }
  };

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 840, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🧩 插件热加载（v1.5）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose}>×</button>
        </div>

        <div className="modal-body theme-body">
          <div className="sync-toolbar">
            <button className="btn primary" onClick={handleReload} disabled={busy}>
              🔄 重新加载
            </button>
            <button className="btn small" onClick={handleInstallDefaults}>
              📦 安装默认
            </button>
            <button
              className="btn small"
              onClick={() => setImportOpen(!importOpen)}
            >
              {importOpen ? "× 取消" : "📥 导入"}
            </button>
            <span style={{ flex: 1 }} />
            <span style={{ color: "var(--text-muted)", fontSize: 11 }}>
              {plugins.length} 个插件 · ~/.agentshell/plugins/
            </span>
          </div>

          {importOpen && (
            <div className="skills-import">
              <h4>📥 导入插件 JSON</h4>
              <textarea
                className="vault-password-input"
                rows={8}
                placeholder='{"name": "...", "version": "1.0", "hooks": [...]}'
                value={importText}
                onChange={(e) => setImportText(e.target.value)}
              />
              <div style={{ marginTop: 6 }}>
                <button className="btn primary" onClick={handleImport}>
                  安装
                </button>
                <button
                  className="btn small"
                  onClick={async () => {
                    try {
                      const t = await navigator.clipboard.readText();
                      setImportText(t);
                    } catch { /* ignore */ }
                  }}
                >
                  📋 从剪贴板
                </button>
              </div>
            </div>
          )}

          {hint && <p className="devices-status">{hint}</p>}

          <div className="plugin-list">
            {plugins.length === 0 && (
              <p style={{ color: "var(--text-muted)", fontSize: 12 }}>
                暂无插件，点「📦 安装默认」试试
              </p>
            )}
            {plugins.map((p) => (
              <div
                key={p.name}
                className={`plugin-row ${selectedPlugin === p.name ? "plugin-sel" : ""}`}
                onClick={() => setSelectedPlugin(p.name === selectedPlugin ? null : p.name)}
              >
                <div className="plugin-row-head">
                  <span className="plugin-name">📦 {p.name}</span>
                  <span className="skill-tag">v{p.version}</span>
                  {p.author === "Codex gx" ? <span className="skill-tag">默认</span> : null}
                  {p.author && <span className="skill-tag-soft">by {p.author}</span>}
                  {p.tags.map((t) => (
                    <span key={t} className="skill-tag-soft">#{t}</span>
                  ))}
                  <span style={{ flex: 1 }} />
                  <button
                    className="btn small"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleExport(p);
                    }}
                    title="导出 JSON"
                  >
                    📋
                  </button>
                  <button
                    className="btn small"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleRemove(p.name);
                    }}
                  >
                    🗑
                  </button>
                </div>
                <div className="plugin-desc">{p.description}</div>
                {selectedPlugin === p.name && (
                  <div className="plugin-hooks">
                    {p.hooks.map((h, i) => (
                      <div key={i} className="plugin-hook">
                        <span className="skill-name">
                          {KIND_ICON[h.kind]} {h.kind}
                          {h.command ? ` /${h.command}` : ""}
                        </span>
                        {h.description && (
                          <span style={{ color: "var(--text-muted)", fontSize: 11, marginLeft: 6 }}>
                            {h.description}
                          </span>
                        )}
                        {h.script && (
                          <pre className="plugin-script">script: {h.script}</pre>
                        )}
                        {h.steps.length > 0 && (
                          <pre className="plugin-script">
                            steps: {JSON.stringify(h.steps)}
                          </pre>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>

          <details className="plugin-debug">
            <summary>🧪 试运行 step 链</summary>
            <div style={{ marginTop: 6 }}>
              <input
                className="vault-password-input"
                value={stepInput}
                onChange={(e) => setStepInput(e.target.value)}
                placeholder="输入文本"
              />
              <div style={{ marginTop: 6, display: "flex", flexWrap: "wrap", gap: 4 }}>
                {STEP_OP_LIST.map((op) => (
                  <button
                    key={op}
                    className="btn small"
                    onClick={() => handleTryStep(op)}
                  >
                    {op}
                  </button>
                ))}
              </div>
              {stepResult && (
                <pre className="plugin-script" style={{ marginTop: 6 }}>
                  {stepResult}
                </pre>
              )}
            </div>
          </details>

          <div className="lint-help">
            <h4>💡 用法</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li>插件 = JSON 清单 + 步骤链 / script</li>
              <li><strong>pre_send</strong>：改写 user 消息（发送前）</li>
              <li><strong>post_recv</strong>：改写 assistant 回复（接收后）</li>
              <li><strong>slash</strong>：注册新的 /命令（plugin_run_steps 不直接调）</li>
              <li>热加载：编辑 JSON 后点「🔄 重新加载」</li>
              <li>未来可扩展：WASM 脚本、libloading 原生 dylib、远程注册中心</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
