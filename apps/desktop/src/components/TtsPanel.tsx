// v1.5：TTS 语音输出设置
// - 探测可用 TTS 后端（macOS say / Windows PS / Linux espeak 等）
// - 启用 / 禁用 / 选声音 / 调速度 / 调音量 / auto-play
// - 测试按钮：朗读一段示例

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type TtsBackend =
  | "auto"
  | "say"
  | "espeak"
  | "spd-say"
  | "festival"
  | "powershell";

type TtsConfig = {
  enabled: boolean;
  voice: string;
  rate: number;
  volume: number;
  backend: TtsBackend;
  auto_play: boolean;
};

type TtsStatus = {
  available: boolean;
  backend: TtsBackend;
  version: string | null;
  error: string | null;
  voices: string[];
};

type Props = {
  onClose: () => void;
};

export function TtsPanel({ onClose }: Props) {
  const [cfg, setCfg] = useState<TtsConfig | null>(null);
  const [status, setStatus] = useState<TtsStatus | null>(null);
  const [testText, setTestText] = useState(
    "你好，我是 Codex gx，桌面 AI 代理。文本转语音测试中。",
  );
  const [busy, setBusy] = useState(false);
  const [hint, setHint] = useState("");

  useEffect(() => {
    (async () => {
      try {
        const c = await invoke<TtsConfig>("tts_get_config");
        setCfg(c);
      } catch {
        setCfg({
          enabled: false,
          voice: "auto",
          rate: 200,
          volume: 1.0,
          backend: "auto",
          auto_play: false,
        });
      }
      try {
        const s = await invoke<TtsStatus>("tts_detect");
        setStatus(s);
      } catch (e) {
        setHint(`探测失败: ${e}`);
      }
    })();
  }, []);

  const save = async (next: TtsConfig) => {
    setCfg(next);
    setBusy(true);
    try {
      await invoke("tts_save_config", { config: next });
    } catch (e) {
      setHint(`保存失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleSpeak = async () => {
    if (!cfg) return;
    setBusy(true);
    try {
      await invoke("tts_speak_with", { text: testText, config: cfg });
      setHint("▶ 播放中…");
    } catch (e) {
      setHint(`❌ ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    // 简单实现：发送新内容会打断；这里只是更新提示
    setHint("⏹ 停止需要关闭 TTS 进程（mac: killall say, win: 关闭 TTS 进程）");
  };

  if (!cfg) {
    return (
      <div className="modal-mask" onClick={onClose}>
        <div className="modal-dialog theme-studio" onClick={(e) => e.stopPropagation()}>
          <div className="modal-body theme-body">
            <p>加载中…</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal-dialog theme-studio"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 600, width: "95vw" }}
      >
        <div className="modal-head">
          <h3>🔊 语音输出 TTS（v1.5）</h3>
          <div style={{ flex: 1 }} />
          <button className="topbar-btn" onClick={onClose}>×</button>
        </div>

        <div className="modal-body theme-body">
          {status && (
            <div className="tts-status">
              {status.available ? (
                <p>
                  ✅ 可用：<strong>{status.backend}</strong>{" "}
                  {status.voices.length > 0 && `· ${status.voices.length} 声音`}
                </p>
              ) : (
                <p>❌ 不可用：{status.error}</p>
              )}
            </div>
          )}

          <div className="tts-form">
            <label className="tts-row">
              <span>启用 TTS</span>
              <input
                type="checkbox"
                checked={cfg.enabled}
                onChange={(e) => save({ ...cfg, enabled: e.target.checked })}
                disabled={!status?.available}
              />
            </label>

            <label className="tts-row">
              <span>后端</span>
              <select
                value={cfg.backend}
                onChange={(e) =>
                  save({ ...cfg, backend: e.target.value as TtsBackend })
                }
                disabled={!status?.available}
              >
                <option value="auto">自动</option>
                <option value="say">macOS say</option>
                <option value="espeak">Linux espeak</option>
                <option value="spd-say">Linux spd-say</option>
                <option value="festival">Linux festival</option>
                <option value="powershell">Windows PowerShell</option>
              </select>
            </label>

            <label className="tts-row">
              <span>声音</span>
              <select
                value={cfg.voice}
                onChange={(e) => save({ ...cfg, voice: e.target.value })}
                disabled={!status?.available}
              >
                <option value="auto">自动 / 默认</option>
                {status?.voices.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </label>

            <label className="tts-row">
              <span>语速（{cfg.rate} wpm）</span>
              <input
                type="range"
                min={80}
                max={400}
                value={cfg.rate}
                onChange={(e) =>
                  save({ ...cfg, rate: Number(e.target.value) })
                }
                disabled={!status?.available}
              />
            </label>

            <label className="tts-row">
              <span>音量（{Math.round(cfg.volume * 100)}%）</span>
              <input
                type="range"
                min={0}
                max={100}
                value={Math.round(cfg.volume * 100)}
                onChange={(e) =>
                  save({ ...cfg, volume: Number(e.target.value) / 100 })
                }
                disabled={!status?.available}
              />
            </label>

            <label className="tts-row">
              <span>自动播放助手回复</span>
              <input
                type="checkbox"
                checked={cfg.auto_play}
                onChange={(e) =>
                  save({ ...cfg, auto_play: e.target.checked })
                }
                disabled={!status?.available || !cfg.enabled}
              />
            </label>
          </div>

          <div className="tts-test">
            <h4>🧪 测试朗读</h4>
            <textarea
              className="vault-password-input"
              rows={3}
              value={testText}
              onChange={(e) => setTestText(e.target.value)}
            />
            <div style={{ marginTop: 6, display: "flex", gap: 6 }}>
              <button
                className="btn primary"
                onClick={handleSpeak}
                disabled={busy || !cfg.enabled}
              >
                ▶ 朗读
              </button>
              <button className="btn small" onClick={handleStop}>
                ⏹ 停止提示
              </button>
            </div>
            {hint && <p className="tts-hint">{hint}</p>}
          </div>

          <div className="lint-help">
            <h4>💡 平台支持</h4>
            <ul style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 20 }}>
              <li><strong>macOS</strong>：内置 <code>say</code> 命令，无需安装</li>
              <li><strong>Windows</strong>：使用 PowerShell + System.Speech</li>
              <li><strong>Linux</strong>：需要 <code>espeak</code> / <code>spd-say</code> / <code>festival</code> 之一</li>
              <li>markdown 标记、emoji、HTML 标签会在朗读前自动清理</li>
              <li>文本 &gt; 2000 字符会被截断（保护后端）</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
