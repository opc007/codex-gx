// v1.2：Plugin marketplace 弹窗
//
// 显示：
// - 索引 URL
// - 远程插件列表 + 描述/版本/类型
// - 已安装列表
// - 安装/卸载按钮
// - 切换注册表 URL

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type PluginManifest = {
  name: string;
  version: string;
  type: "skill" | "tool" | "mcp_server";
  description: string;
  author?: string | null;
  homepage?: string | null;
  tags?: string[];
};

type InstalledSummary = {
  name: string;
  version: string;
  type: string;
  installed_at: string;
  local_path: string;
};

type Props = {
  onClose: () => void;
};

export function MarketplaceDialog({ onClose }: Props) {
  const [indexUrl, setIndexUrl] = useState<string>("");
  const [editingUrl, setEditingUrl] = useState(false);
  const [urlInput, setUrlInput] = useState("");
  const [plugins, setPlugins] = useState<PluginManifest[]>([]);
  const [installed, setInstalled] = useState<InstalledSummary[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const reload = async () => {
    setErr(null);
    try {
      const [url, list, pl] = await Promise.all([
        invoke<string>("marketplace_get_index_url"),
        invoke<InstalledSummary[]>("marketplace_list_installed"),
        invoke<PluginManifest[]>("marketplace_fetch_index"),
      ]);
      setIndexUrl(url);
      setUrlInput(url);
      setInstalled(list);
      setPlugins(pl);
    } catch (e: any) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  const installedSet = new Set(installed.map((p) => p.name));

  const doInstall = async (name: string) => {
    setBusy(name);
    setErr(null);
    try {
      await invoke("marketplace_install", { args: { name, version: null } });
      await reload();
    } catch (e: any) {
      setErr(`安装 ${name} 失败: ${e}`);
    } finally {
      setBusy(null);
    }
  };

  const doUninstall = async (name: string) => {
    setBusy(name);
    setErr(null);
    try {
      await invoke("marketplace_uninstall", { args: { name } });
      await reload();
    } catch (e: any) {
      setErr(`卸载 ${name} 失败: ${e}`);
    } finally {
      setBusy(null);
    }
  };

  const saveUrl = async () => {
    if (!urlInput.trim()) return;
    try {
      await invoke("marketplace_set_index_url", { args: { url: urlInput.trim() } });
      setEditingUrl(false);
      await reload();
    } catch (e: any) {
      setErr(String(e));
    }
  };

  return (
    <div className="update-dialog-overlay" onClick={onClose}>
      <div className="update-dialog marketplace-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="update-dialog-header">
          <h2>🧩 Plugin Marketplace</h2>
          <button className="update-cancel" onClick={onClose}>×</button>
        </div>

        <div className="update-dialog-body">
          {/* 索引 URL */}
          <div className="mp-section">
            <h3>注册表 (Registry)</h3>
            {editingUrl ? (
              <div className="mp-url-edit">
                <input
                  type="text"
                  className="mp-url-input"
                  value={urlInput}
                  onChange={(e) => setUrlInput(e.target.value)}
                  placeholder="https://raw.githubusercontent.com/.../index.json"
                />
                <button onClick={saveUrl}>保存</button>
                <button onClick={() => { setEditingUrl(false); setUrlInput(indexUrl); }}>取消</button>
              </div>
            ) : (
              <div className="mp-url-row">
                <code className="mp-url">{indexUrl}</code>
                <button onClick={() => setEditingUrl(true)}>修改</button>
                <button onClick={() => void reload()}>刷新</button>
              </div>
            )}
          </div>

          {err && <div className="mp-error">❌ {err}</div>}

          {/* 已安装 */}
          <div className="mp-section">
            <h3>已安装 ({installed.length})</h3>
            {installed.length === 0 ? (
              <div className="mp-empty">还没有安装任何插件</div>
            ) : (
              <ul className="mp-installed-list">
                {installed.map((p) => (
                  <li key={p.name} className="mp-installed-item">
                    <div>
                      <strong>{p.name}</strong> <span className="mp-version">v{p.version}</span>
                      <span className="mp-tag">{p.type}</span>
                    </div>
                    <button
                      onClick={() => void doUninstall(p.name)}
                      disabled={busy === p.name}
                    >
                      {busy === p.name ? "..." : "卸载"}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {/* 可用插件 */}
          <div className="mp-section">
            <h3>可用插件 ({plugins.length})</h3>
            {plugins.length === 0 ? (
              <div className="mp-empty">
                没有可用插件（检查网络 / 注册表 URL）
              </div>
            ) : (
              <ul className="mp-plugin-list">
                {plugins.map((p) => {
                  const isInstalled = installedSet.has(p.name);
                  return (
                    <li key={p.name} className="mp-plugin-item">
                      <div className="mp-plugin-head">
                        <strong>{p.name}</strong>{" "}
                        <span className="mp-version">v{p.version}</span>{" "}
                        <span className="mp-tag">{p.type}</span>
                        {p.author && <span className="mp-author">by {p.author}</span>}
                      </div>
                      <div className="mp-plugin-desc">{p.description}</div>
                      {p.tags && p.tags.length > 0 && (
                        <div className="mp-plugin-tags">
                          {p.tags.map((t) => (
                            <span key={t} className="mp-tag-small">#{t}</span>
                          ))}
                        </div>
                      )}
                      <div className="mp-plugin-actions">
                        {isInstalled ? (
                          <button
                            onClick={() => void doUninstall(p.name)}
                            disabled={busy === p.name}
                          >
                            {busy === p.name ? "..." : "卸载"}
                          </button>
                        ) : (
                          <button
                            className="mp-install-btn"
                            onClick={() => void doInstall(p.name)}
                            disabled={busy === p.name}
                          >
                            {busy === p.name ? "安装中..." : "安装"}
                          </button>
                        )}
                        {p.homepage && (
                          <a
                            href="#"
                            onClick={(e) => {
                              e.preventDefault();
                              void import("@tauri-apps/plugin-shell").then((m) =>
                                m.open(p.homepage!),
                              );
                            }}
                          >
                            主页
                          </a>
                        )}
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}