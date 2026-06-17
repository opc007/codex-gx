// v1.5：Session 同步面板
// - 后端 sync_publish / sync_fetch / sync_list / sync_remove
// - 把当前 session（或选中的）发布为 bundle 缓存到 ~/.agentshell/sync/
// - 显示缓存列表
// - 一键复制 bundle JSON
// - 导入 JSON bundle
// - 配合 P2P DevicesPanel：synced 后 P2P 内自动可用

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionsStore, getSessionsState } from "../stores/sessions";

type SessionBundle = {
  schema_version: number;
  session_id: string;
  title: string;
  created_at: number;
  updated_at: number;
  owner_id: string | null;
  workspace_id: string | null;
  messages: unknown;
  source_device: string;
  synced_at: number;
};

type SyncStatus = {
  cached: number;
  total_size: number;
  sessions: Array<{
    session_id: string;
    title: string;
    updated_at: number;
    size: number;
    source: string;
  }>;
};

type Props = {
  onClose: () => void;
};

export function SyncPanel({ onClose }: Props) {
  const sessions = useSessionsStore((s) => s.sessions);
  const messages = useSessionsStore((s) => s.messages);
  const currentId = useSessionsStore((s) => s.currentId);
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [version, setVersion] = useState<number>(0);
  const [busy, setBusy] = useState(false);
  const [importText, setImportText] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [hint, setHint] = useState("");
  const [autoSync, setAutoSync] = useState(true);

  const refresh = async () => {
    setBusy(true);
    try {
      const s = await invoke<SyncStatus>("sync_list");
      setStatus(s);
      const v = await invoke<number>("sync_schema_version");
      setVersion(v);
    } catch (e) {
      setHint(`❌ ${e}`);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  // v1.5：自动同步当前 session
  useEffect(() => {
    if (!autoSync || !currentId) return;
    const cid: string = currentId;
    const state = getSessionsState();
    const session = state.sessions.find((s) => s.id === cid);
    const msgs = state.messages[cid] || [];
    if (!session || msgs.length === 0) return;
    // debounce
    const t = setTimeout(() => {
      void publishBundle(session.id, session.title, session.createdAt, session.updatedAt, msgs);
    }, 1500);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentId, messages, autoSync]);

  const publishBundle = async (
    sessionId: string,
    title: string,
    createdAt: number,
    updatedAt: number,
    msgs: unknown,
  ) => {
    try {
      const b: SessionBundle = {
        schema_version: version || 1,
        session_id: sessionId,
        title,
        created_at: createdAt,
        updated_at: updatedAt,
        owner_id: null,
        workspace_id: null,
        messages: msgs,
        source_device: getDeviceName(),
        synced_at: Date.now(),
      };
      await invoke("sync_publish", { bundle: b });
      setHint(`✅ 已同步：${title}`);
      await refresh();
    } catch (e) {
      setHint(`❌ ${e}`);
    }
  };

  const handlePublish = async () => {
    if (!currentId) {
      setHint("⚠️ 没有当前 session");
      return;
    }
    const session = sessions.find((s) => s.id === currentId);
    if (!session) return;
    const msgs = messages[currentId] || [];
    await publishBundle(session.id, session.title, session.createdAt, session.updatedAt, msgs);
  };

  const handleFetch = async (sessionId: string) => {
    try {
      const b = await invoke<SessionBundle | null>("sync_fetch", { sessionId });
      if (!b) {
        setHint("⚠️ 未找到");
        return;
      }
      const json = JSON.stringify(b, null, 2);
      await navigator.clipboard.writeText(json).catch(() => {});
      setHint(`📋 已复制 bundle JSON（${(json.length / 1024).toFixed(1)} KB）到剪贴板`);
    } catch (e) {
      setHint(`❌ ${e}`);
    }
  };

  const handleImport = async () => {
    if (!importText.trim()) return;
    try {
      const b: SessionBundle = JSON.parse(importText);
      if (!b.session_id) {
        setHint("❌ 缺少 session_id");
        return;
      }
      await invoke("sync_publish", { bundle: b });
      setHint(`✅ 已导入：${b.title || b.session_id}`);
      setImportText("");
      setImportOpen(false);
      await refresh();
    } catch (e) {
      setHint(`❌ 解析失败: ${e}`);
    }
  };

  const handleRemove = async (sessionId: string) => {
    if (!confirm(`确定从同步缓存中删除 ${sessionId}？`)) return;
    await invoke("sync_remove", { sessionId });
    await refresh();
  };

  const handleClearAll = async () => {
    if (!confirm("确定清空整个同步缓存？")) return;
    const n = await invoke<number>("sync_clear_all");
    setHint(`🗑 已清空 ${n} 个 session`);
    await refresh();
  };

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 760, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>☁️ Session 同步（v1.5）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose}>×</button>
        </div>

        <div className="modal-body theme-body">
          <div className="sync-summary">
            <div>
              <strong>缓存</strong>：{status?.cached ?? 0} 个 session · {((status?.total_size ?? 0) / 1024).toFixed(1)} KB
            </div>
            <div>
              <strong>schema</strong>：v{version} · <strong>device</strong>：{getDeviceName()}
            </div>
          </div>

          <div className="sync-toolbar">
            <button
              className="btn primary"
              onClick={handlePublish}
              disabled={!currentId}
            >
              📤 同步当前 Session
            </button>
            <button className="btn small" onClick={refresh} disabled={busy}>
              🔄
            </button>
            <button
              className="btn small"
              onClick={() => setImportOpen(!importOpen)}
            >
              {importOpen ? "× 取消" : "📥 导入"}
            </button>
            <button className="btn small" onClick={handleClearAll}>
              🗑 清空
            </button>
            <label className="sync-auto">
              <input
                type="checkbox"
                checked={autoSync}
                onChange={(e) => setAutoSync(e.target.checked)}
              />
              <span>自动同步</span>
            </label>
          </div>

          {importOpen && (
            <div className="skills-import">
              <h4>📥 导入 Bundle JSON</h4>
              <textarea
                className="vault-password-input"
                rows={8}
                placeholder='{"session_id": "...", "title": "...", "messages": [...]}'
                value={importText}
                onChange={(e) => setImportText(e.target.value)}
              />
              <div style={{ marginTop: 6 }}>
                <button className="btn primary" onClick={handleImport}>
                  导入
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

          <div className="sync-list">
            <h4>📦 已同步 Session</h4>
            {status?.sessions.length === 0 && (
              <p style={{ color: "var(--text-muted)", fontSize: 12 }}>暂无</p>
            )}
            {status?.sessions.map((e) => (
              <div key={e.session_id} className="sync-row">
                <div className="sync-row-head">
                  <span className="skill-name">{e.title}</span>
                  <span className="skill-tag">{e.source}</span>
                  <span className="skill-tag-soft">{(e.size / 1024).toFixed(1)}KB</span>
                  <span style={{ flex: 1 }} />
                  <button
                    className="btn small"
                    onClick={() => handleFetch(e.session_id)}
                    title="复制 bundle"
                  >
                    📋
                  </button>
                  <button
                    className="btn small"
                    onClick={() => handleRemove(e.session_id)}
                    title="删除"
                  >
                    🗑
                  </button>
                </div>
                <div className="sync-row-meta">
                  <code>{e.session_id}</code>
                  <span> · {new Date(e.updated_at).toLocaleString()}</span>
                </div>
              </div>
            ))}
          </div>

          <div className="lint-help">
            <h4>💡 用法</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li>开启 <strong>自动同步</strong> 后，当前 session 每次更新会自动 publish 到本地 cache</li>
              <li>点击 📋 复制 bundle JSON，可贴到 Notion / Slack / 邮件</li>
              <li>📥 导入：从剪贴板 / 别人分享的 JSON 恢复</li>
              <li>配合 <strong>P2P DevicesPanel</strong>：synced 后同网段设备能通过 P2P 拉取</li>
              <li>缓存位置：<code>~/.agentshell/sync/</code></li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}

function getDeviceName(): string {
  if (typeof navigator === "undefined") return "device";
  // @ts-expect-error: userAgentData 可选
  const uad = navigator.userAgentData;
  if (uad?.platform) return uad.platform;
  return navigator.platform || "device";
}
