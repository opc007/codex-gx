import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ToolCallRecord = {
  id: string;
  name: string;
  arguments: unknown;
  result?: string;
  success?: boolean;
  error?: string;
};

type Props = {
  record: ToolCallRecord | null;
  onClose: () => void;
};

export default function ReplayDialog({ record, onClose }: Props) {
  const [argsText, setArgsText] = useState("");
  const [running, setRunning] = useState(false);
  const [output, setOutput] = useState<{
    success: boolean;
    output: string;
    error: string | null;
  } | null>(null);

  useEffect(() => {
    if (record) {
      setArgsText(JSON.stringify(record.arguments ?? {}, null, 2));
      setOutput(null);
      setRunning(false);
    }
  }, [record?.id]);

  if (!record) return null;

  const onRun = async () => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(argsText);
    } catch (e) {
      setOutput({
        success: false,
        output: "",
        error: `参数 JSON 解析失败：${(e as Error).message}`,
      });
      return;
    }
    setRunning(true);
    try {
      const r = await invoke<{
        success: boolean;
        output: string;
        error: string | null;
        truncated: boolean;
      }>("execute_tool", {
        name: record.name,
        arguments: parsed,
      });
      setOutput({
        success: r.success,
        output: r.output,
        error: r.error,
      });
    } catch (e) {
      setOutput({
        success: false,
        output: "",
        error: String(e),
      });
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal replay-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="replay-header">
          <h2>🔁 重新执行 tool call</h2>
          <span className="replay-tool-name">{record.name}</span>
        </div>

        {record.result !== undefined && (
          <div className="replay-section">
            <h4>原始结果</h4>
            <pre className="replay-original">
              {record.success ? record.result : `❌ ${record.error}`}
            </pre>
          </div>
        )}

        <div className="replay-section">
          <h4>参数（可编辑后重新执行）</h4>
          <textarea
            className="replay-textarea"
            value={argsText}
            onChange={(e) => setArgsText(e.target.value)}
            rows={10}
            spellCheck={false}
          />
        </div>

        {output && (
          <div className="replay-section">
            <h4>{output.success ? "✅ 执行成功" : "❌ 执行失败"}</h4>
            <pre className={`replay-output ${output.success ? "ok" : "err"}`}>
              {output.success
                ? output.output
                : output.error || output.output}
            </pre>
          </div>
        )}

        <div className="replay-footer">
          <button className="btn-secondary" onClick={onClose} disabled={running}>
            关闭
          </button>
          <button
            className="btn-primary"
            onClick={onRun}
            disabled={running || !argsText.trim()}
          >
            {running ? "执行中..." : "▶ 重新执行"}
          </button>
        </div>
      </div>
    </div>
  );
}
