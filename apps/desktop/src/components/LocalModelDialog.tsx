// v1.4：本地 LLM 模型管理
// - 探测本机 Ollama / llama.cpp server
// - 列出所有可用本地模型
// - 一键 ping（健康检查 + 测速）
// - 复制模型 ID 到剪贴板

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Discovery = {
  ollama_url: string | null;
  ollama_models: Array<{
    name: string;
    size: number | null;
    details: { parameter_size?: string } | null;
  }>;
  ollama_error: string | null;
  llamacpp_url: string | null;
  llamacpp_models: Array<{ id: string }>;
  llamacpp_error: string | null;
};

type PingResult = {
  ok: boolean;
  latency_ms: number;
  error: string | null;
};

type Props = {
  onClose: () => void;
  onUseModel: (modelId: string) => void;
};

export function LocalModelDialog({ onClose, onUseModel }: Props) {
  const [ollamaUrl, setOllamaUrl] = useState("http://127.0.0.1:11434");
  const [llamacppUrl, setLlamacppUrl] = useState("http://127.0.0.1:8080");
  const [discovery, setDiscovery] = useState<Discovery | null>(null);
  const [pingResults, setPingResults] = useState<Record<string, PingResult>>({});
  const [busy, setBusy] = useState(false);
  const [autoRefresh, setAutoRefresh] = useState(true);

  const discover = async () => {
    setBusy(true);
    try {
      const d = await invoke<Discovery>("local_discover", {
        ollamaUrl: ollamaUrl || null,
        llamacppUrl: llamacppUrl || null,
      });
      setDiscovery(d);
    } catch (e) {
      alert(`❌ 探测失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (autoRefresh) void discover();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handlePing = async (backend: string, name: string) => {
    const id = `${backend}:${name}`;
    setPingResults((r) => ({ ...r, [id]: { ok: false, latency_ms: 0, error: "测试中…" } }));
    try {
      const r = await invoke<PingResult>("local_ping", {
        backend,
        model: name,
        baseUrl: backend === "ollama" ? ollamaUrl : llamacppUrl,
      });
      setPingResults((prev) => ({ ...prev, [id]: r }));
    } catch (e) {
      setPingResults((prev) => ({
        ...prev,
        [id]: { ok: false, latency_ms: 0, error: String(e) },
      }));
    }
  };

  const handleCopy = (id: string) => {
    navigator.clipboard.writeText(id).catch(() => {
      // ignore
    });
  };

  const renderList = (
    title: string,
    icon: string,
    backend: "ollama" | "llamacpp",
    items: Array<{ name: string; id?: string; size?: number | null; param_size?: string | null }>,
    error: string | null,
  ) => (
    <div className="local-section">
      <h4>
        {icon} {title}（{items.length}）
        {error && <span className="mp-error">⚠️ {error}</span>}
      </h4>
      {items.length === 0 ? (
        <p style={{ color: "var(--text-muted)", fontSize: 12, marginLeft: 8 }}>
          {error ? "（探测失败）" : "（未发现模型）"}
        </p>
      ) : (
        <div className="local-model-list">
          {items.map((m) => {
            const name = m.name;
            const fullId = `${backend}:${name}`;
            const pingKey = fullId;
            const ping = pingResults[pingKey];
            return (
              <div key={fullId} className="local-model-row">
                <span className="local-model-name">{name}</span>
                {m.param_size && (
                  <span className="local-model-tag">{m.param_size}</span>
                )}
                {m.size != null && (
                  <span className="local-model-tag">
                    {(m.size / (1024 ** 3)).toFixed(2)} GB
                  </span>
                )}
                <code className="local-model-id">{fullId}</code>
                {ping && (
                  <span
                    className={`local-model-ping ${ping.ok ? "ok" : "err"}`}
                    title={ping.error ?? "OK"}
                  >
                    {ping.ok
                      ? `✅ ${ping.latency_ms}ms`
                      : `❌ ${ping.error?.slice(0, 30) ?? "fail"}`}
                  </span>
                )}
                <button
                  className="btn small"
                  onClick={() => handlePing(backend, name)}
                  title="测速"
                >
                  📡
                </button>
                <button
                  className="btn small"
                  onClick={() => handleCopy(fullId)}
                  title="复制 ID"
                >
                  📋
                </button>
                <button
                  className="btn small primary"
                  onClick={() => onUseModel(fullId)}
                  title="在当前 session 用此模型"
                >
                  用
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 760, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🏠 本地 LLM（v1.4）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose} title="关闭">
            ×
          </button>
        </div>

        <div className="modal-body theme-body">
          <div className="local-urls">
            <div className="local-url-row">
              <label>Ollama URL</label>
              <input
                className="vault-password-input"
                value={ollamaUrl}
                onChange={(e) => setOllamaUrl(e.target.value)}
                placeholder="http://127.0.0.1:11434"
              />
            </div>
            <div className="local-url-row">
              <label>llama.cpp URL</label>
              <input
                className="vault-password-input"
                value={llamacppUrl}
                onChange={(e) => setLlamacppUrl(e.target.value)}
                placeholder="http://127.0.0.1:8080"
              />
            </div>
            <div style={{ marginTop: 8 }}>
              <button
                className="btn primary"
                onClick={discover}
                disabled={busy}
              >
                {busy ? "探测中…" : "🔍 探测"}
              </button>
              <label style={{ marginLeft: 12, fontSize: 12, color: "var(--text-muted)" }}>
                <input
                  type="checkbox"
                  checked={autoRefresh}
                  onChange={(e) => setAutoRefresh(e.target.checked)}
                />
                打开时自动探测
              </label>
            </div>
          </div>

          {discovery && (
            <>
              {renderList(
                "Ollama",
                "🦙",
                "ollama",
                discovery.ollama_models.map((m) => ({
                  name: m.name,
                  size: m.size,
                  param_size: m.details?.parameter_size,
                })),
                discovery.ollama_error,
              )}
              {renderList(
                "llama.cpp",
                "🐑",
                "llamacpp",
                discovery.llamacpp_models.map((m) => ({ name: m.id })),
                discovery.llamacpp_error,
              )}
            </>
          )}

          <div className="local-help">
            <h4>💡 提示</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li>
                <strong>Ollama</strong>：安装{" "}
                <a href="https://ollama.com" target="_blank" rel="noreferrer">
                  ollama
                </a>{" "}
                并 <code>ollama pull qwen2.5:7b</code>。
              </li>
              <li>
                <strong>llama.cpp</strong>：用{" "}
                <code>./server -m model.gguf --port 8080</code> 启动 server。
              </li>
              <li>
                点击 <strong>「用」</strong>会把模型 ID 设给当前 session
                （也可在 Top bar 模型下拉里手动选）。
              </li>
              <li>
                路由策略里加上 <code>ollama:qwen2.5:7b</code> 可实现「隐私任务自动走本地」。
              </li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}