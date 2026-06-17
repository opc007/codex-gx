import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type ApiKeysStatus = {
  minimax_configured: boolean;
  minimax_masked: string | null;
  anthropic_configured: boolean;
  anthropic_masked: string | null;
  deepseek_configured: boolean;
  deepseek_masked: string | null;
  openai_configured: boolean;
  openai_masked: string | null;
};

type ProviderDef = {
  id: string;
  label: string;
  hint: string;
  placeholder: string;
  configured: (s: ApiKeysStatus) => boolean;
  masked: (s: ApiKeysStatus) => string | null;
};

const PROVIDERS: ProviderDef[] = [
  {
    id: "minimax",
    label: "MiniMax M3",
    hint: "默认推荐，国产大模型",
    placeholder: "sk-...",
    configured: (s) => s.minimax_configured,
    masked: (s) => s.minimax_masked,
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    hint: "deepseek-v4-pro / deepseek-chat",
    placeholder: "sk-...",
    configured: (s) => s.deepseek_configured,
    masked: (s) => s.deepseek_masked,
  },
  {
    id: "anthropic",
    label: "Anthropic Claude",
    hint: "claude-sonnet / claude-opus",
    placeholder: "sk-ant-...",
    configured: (s) => s.anthropic_configured,
    masked: (s) => s.anthropic_masked,
  },
  {
    id: "openai",
    label: "OpenAI GPT",
    hint: "gpt-5-mini 等",
    placeholder: "sk-...",
    configured: (s) => s.openai_configured,
    masked: (s) => s.openai_masked,
  },
];

type Props = {
  onClose: () => void;
};

export function ApiKeysDialog({ onClose }: Props) {
  const [status, setStatus] = useState<ApiKeysStatus | null>(null);
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const refresh = async () => {
    const s = await invoke<ApiKeysStatus>("api_keys_status");
    setStatus(s);
  };

  useEffect(() => {
    void refresh();
  }, []);

  const notifyChanged = () => {
    window.dispatchEvent(new CustomEvent("api-keys:changed"));
  };

  const onSave = async (providerId: string) => {
    setBusy(providerId);
    setErr(null);
    setMsg(null);
    try {
      const s = await invoke<ApiKeysStatus>("api_keys_set", {
        args: { provider: providerId, key: keys[providerId] ?? "" },
      });
      setStatus(s);
      setKeys((prev) => ({ ...prev, [providerId]: "" }));
      setMsg(`${PROVIDERS.find((p) => p.id === providerId)?.label ?? providerId} 已保存到本机。`);
      notifyChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const onTest = async (providerId: string) => {
    setBusy(providerId);
    setErr(null);
    setMsg(null);
    try {
      const draft = keys[providerId]?.trim();
      if (draft) {
        await invoke("api_keys_set", {
          args: { provider: providerId, key: draft },
        });
      }
      const r = await invoke<string>("api_keys_test", {
        args: { provider: providerId },
      });
      setMsg(r);
      await refresh();
      notifyChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="update-dialog-overlay" onClick={onClose}>
      <div
        className="update-dialog api-keys-dialog"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="update-dialog-header">
          <h2>🔑 API Key 设置</h2>
          <button className="update-cancel" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="update-dialog-body">
          <p style={{ color: "var(--text-muted)", fontSize: 13, marginTop: 0 }}>
            可同时配置多个大模型 Key，保存在本机 ~/.agentshell/secrets.json，不会上传云端。
            配置后在输入框下方切换模型即可使用。
          </p>

          {PROVIDERS.map((p) => {
            const configured = status ? p.configured(status) : false;
            const masked = status ? p.masked(status) : null;
            const draft = keys[p.id] ?? "";
            const isBusy = busy === p.id;
            return (
              <div key={p.id} className="api-key-section">
                <label className="api-key-label">{p.label}</label>
                <p className="api-key-hint">
                  {configured && masked ? `已配置：${masked}` : p.hint}
                </p>
                <input
                  className="api-key-input"
                  type="password"
                  placeholder={p.placeholder}
                  value={draft}
                  onChange={(e) =>
                    setKeys((prev) => ({ ...prev, [p.id]: e.target.value }))
                  }
                  autoComplete="off"
                />
                <div className="api-key-actions">
                  <button
                    className="update-cancel"
                    disabled={!!busy}
                    onClick={() => void onTest(p.id)}
                  >
                    {isBusy ? "测试中…" : "测试"}
                  </button>
                  <button
                    className="update-go"
                    disabled={!!busy || (!draft.trim() && !configured)}
                    onClick={() => void onSave(p.id)}
                  >
                    保存
                  </button>
                </div>
              </div>
            );
          })}

          {msg && <div className="api-key-ok">{msg}</div>}
          {err && <div className="error-banner">{err}</div>}
        </div>
        <div className="update-dialog-footer">
          <button className="update-cancel" onClick={onClose}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
