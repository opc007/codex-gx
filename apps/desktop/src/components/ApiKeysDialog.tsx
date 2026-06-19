import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type CustomProviderInfo = {
  name: string;
  base_url: string;
  api_key: string | null;
  default_model: string;
  extra_models: string[];
};

type ApiKeysStatus = {
  minimax_configured: boolean;
  minimax_masked: string | null;
  anthropic_configured: boolean;
  anthropic_masked: string | null;
  deepseek_configured: boolean;
  deepseek_masked: string | null;
  openai_configured: boolean;
  openai_masked: string | null;
  zhipu_configured: boolean;
  zhipu_masked: string | null;
  mimo_configured: boolean;
  mimo_masked: string | null;
  moonshot_configured: boolean;
  moonshot_masked: string | null;
  custom_provider: CustomProviderInfo | null;
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
    hint: "deepseek-chat / deepseek-chat",
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
  const [customBusy, setCustomBusy] = useState(false);

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
        {/* v1.9.16：自定义 OpenAI 协议 provider — 填 base_url + key + 模型即可用 */}
        <CustomProviderSection
          value={status?.custom_provider ?? null}
          busy={customBusy}
          onSave={async (form) => {
            setCustomBusy(true);
            setErr(null);
            setMsg(null);
            try {
              const s = await invoke<ApiKeysStatus>("api_keys_set_custom", {
                args: {
                  name: form.name,
                  base_url: form.baseUrl,
                  api_key: form.apiKey,
                  default_model: form.defaultModel,
                  extra_models: form.extraModels,
                },
              });
              setStatus(s);
              setMsg(
                s.custom_provider
                  ? `已保存自定义 provider「${s.custom_provider.name}」，模型菜单将自动出现。`
                  : "已清空自定义 provider。",
              );
              notifyChanged();
            } catch (e) {
              setErr(String(e));
            } finally {
              setCustomBusy(false);
            }
          }}
          onTest={async () => {
            setCustomBusy(true);
            setErr(null);
            setMsg(null);
            try {
              const r = await invoke<string>("api_keys_test", {
                args: { provider: "custom" },
              });
              setMsg(r);
              await refresh();
            } catch (e) {
              setErr(String(e));
            } finally {
              setCustomBusy(false);
            }
          }}
        />

        <div className="update-dialog-footer">
          <button className="update-cancel" onClick={onClose}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}


/** v1.9.16：自定义 OpenAI 协议 provider 配置表单 */
type CustomForm = {
  name: string;
  baseUrl: string;
  apiKey: string;
  defaultModel: string;
  extraModels: string[];
};

function CustomProviderSection({
  value,
  busy,
  onSave,
  onTest,
}: {
  value: CustomProviderInfo | null;
  busy: boolean;
  onSave: (form: CustomForm) => Promise<void>;
  onTest: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(value?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(value?.base_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [defaultModel, setDefaultModel] = useState(value?.default_model ?? "");
  const [extras, setExtras] = useState(
    (value?.extra_models ?? []).join(","),
  );
  // 当 value 变化时同步表单（保存/清空后）
  useEffect(() => {
    setName(value?.name ?? "");
    setBaseUrl(value?.base_url ?? "");
    setDefaultModel(value?.default_model ?? "");
    setExtras((value?.extra_models ?? []).join(","));
    setApiKey("");
  }, [value?.name, value?.base_url, value?.default_model]);

  const hasValue = !!value;
  return (
    <details
      className="api-key-section"
      style={{ borderTop: "1px solid var(--border, #333)", paddingTop: 16, marginTop: 16 }}
      open={open}
      onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}
    >
      <summary
        style={{
          cursor: "pointer",
          fontWeight: 600,
          listStyle: "none",
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}
      >
        <span style={{ color: hasValue ? "#22c55e" : "var(--text-muted)" }}>
          {hasValue ? "✅" : "➕"}
        </span>
        自定义 OpenAI 协议 provider
        {hasValue && (
          <span style={{ color: "var(--text-muted)", fontWeight: 400, fontSize: 12 }}>
            （已配置：{value.name} · {value.base_url} · {value.default_model}）
          </span>
        )}
      </summary>
      <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 8 }}>
        <p className="api-key-hint" style={{ margin: 0 }}>
          适合任何 OpenAI 协议兼容的服务（自建代理 / 其他国产模型 / 第三方平台）。
          保存后模型菜单立即出现，无需启用。
        </p>
        <input
          className="api-key-input"
          placeholder="名称（如 我的代理 / SiliconFlow）"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <input
          className="api-key-input"
          placeholder="Base URL（如 https://api.siliconflow.cn/v1）"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
        />
        <input
          className="api-key-input"
          type="password"
          placeholder={
            value?.api_key
              ? "API Key（已配置；留空保持原值）"
              : "API Key（可空，依赖服务端是否要求鉴权）"
          }
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          autoComplete="off"
        />
        <input
          className="api-key-input"
          placeholder="默认模型（如 Qwen/Qwen2.5-72B-Instruct）"
          value={defaultModel}
          onChange={(e) => setDefaultModel(e.target.value)}
        />
        <input
          className="api-key-input"
          placeholder="额外模型（逗号分隔，可选）"
          value={extras}
          onChange={(e) => setExtras(e.target.value)}
        />
        <div className="api-key-actions">
          <button
            className="update-cancel"
            disabled={busy || (!name.trim() && !hasValue)}
            onClick={() =>
              void onSave({
                name: name.trim(),
                baseUrl: baseUrl.trim(),
                apiKey: apiKey.trim(),
                defaultModel: defaultModel.trim(),
                extraModels: extras
                  .split(",")
                  .map((s) => s.trim())
                  .filter((s) => s.length > 0),
              })
            }
          >
            {busy ? "保存中…" : hasValue ? "更新" : "保存"}
          </button>
          {hasValue && (
            <>
              <button
                className="update-cancel"
                disabled={busy}
                onClick={() => void onTest()}
              >
                {busy ? "测试中…" : "测试"}
              </button>
              <button
                className="update-cancel"
                disabled={busy}
                onClick={() =>
                  void onSave({ name: "", baseUrl: "", apiKey: "", defaultModel: "", extraModels: [] })
                }
                title="删除自定义 provider"
              >
                删除
              </button>
            </>
          )}
        </div>
        {hasValue && (
          <p className="api-key-hint" style={{ margin: 0 }}>
            在 Composer 模型菜单选择 <code>custom:&lt;模型名&gt;</code> 即可使用。
          </p>
        )}
      </div>
    </details>
  );
}
