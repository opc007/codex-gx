// v1.9.14：项目组（workspace）右键菜单 — 仿 Codex Desktop
// 菜单项（按 Codex 真实截图顺序）：
//   1. 🔖 置顶项目（toggle）
//   2. 📂 在 Finder 中显示（要求有 folderPath）
//   3. 🌱 创建永久工作树（要求有 folderPath 且是 git 仓库）
//   4. ✏️ 重命名项目（弹编辑 dialog）
//   5. 📦 归档对话（标记 sessions.archived = true）
//   6. ×  移除（仅非 default 项目；弹确认）

import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  togglePinWorkspace,
  deleteWorkspace,
  type WorkspaceMeta,
} from "../stores/workspace";
import { useSessionsStore } from "../stores/sessions";

export type ProjectContextMenuProps = {
  workspace: WorkspaceMeta;
  x: number;
  y: number;
  onClose: () => void;
  onRename: (ws: WorkspaceMeta) => void;
};

export function ProjectContextMenu({
  workspace,
  x,
  y,
  onClose,
  onRename,
}: ProjectContextMenuProps) {
  const archiveWorkspace = useSessionsStore((s) => s.archiveWorkspace);
  const restoreWorkspace = useSessionsStore((s) => s.restoreWorkspace);
  const ref = useRef<HTMLDivElement | null>(null);

  // 点击菜单外关闭
  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onEsc);
    };
  }, [onClose]);

  // 防止菜单超出屏幕右边/底部
  useEffect(() => {
    if (!ref.current) return;
    const r = ref.current.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let nx = x;
    let ny = y;
    if (x + r.width > vw - 8) nx = vw - r.width - 8;
    if (y + r.height > vh - 8) ny = vh - r.height - 8;
    if (nx !== x || ny !== y) {
      ref.current.style.left = `${nx}px`;
      ref.current.style.top = `${ny}px`;
    }
  }, [x, y]);

  const hasFolder = !!workspace.folderPath;
  const isDefault = workspace.id === "default";

  const doReveal = async () => {
    if (!workspace.folderPath) return;
    onClose();
    try {
      await invoke("reveal_in_finder", { path: workspace.folderPath });
    } catch (e) {
      alert(`在 Finder 中显示失败：${e}`);
    }
  };

  const doWorktree = async () => {
    if (!workspace.folderPath) return;
    onClose();
    const label = window.prompt(
      `为「${workspace.name}」创建永久工作树，输入新分支名：`,
      `${workspace.name}-wt`,
    );
    if (!label?.trim()) return;
    try {
      const newPath = await invoke<string>("create_permanent_worktree", {
        sourcePath: workspace.folderPath,
        workspaceId: workspace.id,
        branchLabel: label.trim(),
      });
      // 自动注册为新项目
      const { createWorkspace } = await import("../stores/workspace");
      createWorkspace(`${workspace.name} · ${label.trim()}`, {
        folderPath: newPath,
        color: workspace.color,
      });
      alert(`已创建永久工作树：${newPath}`);
    } catch (e) {
      alert(`创建工作树失败：${e}`);
    }
  };

  const doPin = () => {
    togglePinWorkspace(workspace.id);
    onClose();
  };

  const doArchive = () => {
    archiveWorkspace(workspace.id);
    onClose();
  };

  const doRestore = () => {
    restoreWorkspace(workspace.id);
    onClose();
  };

  const doDelete = () => {
    if (isDefault) return;
    if (!confirm(`移除项目「${workspace.name}」？\n会话文件保留在归档，可重新恢复。`)) return;
    deleteWorkspace(workspace.id);
    onClose();
  };

  return (
    <div
      ref={ref}
      className="project-context-menu"
      role="menu"
      style={{ left: x, top: y }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <button type="button" className="pcm-item" role="menuitem" onClick={doPin}>
        <span className="pcm-icon">{workspace.pinned ? "📌" : "🔖"}</span>
        <span className="pcm-label">{workspace.pinned ? "取消置顶" : "置顶项目"}</span>
      </button>
      <button
        type="button"
        className="pcm-item"
        role="menuitem"
        onClick={doReveal}
        disabled={!hasFolder}
        title={hasFolder ? "在 Finder 中打开此项目目录" : "项目未绑定目录"}
      >
        <span className="pcm-icon">📂</span>
        <span className="pcm-label">在 Finder 中显示</span>
      </button>
      <button
        type="button"
        className="pcm-item"
        role="menuitem"
        onClick={doWorktree}
        disabled={!hasFolder}
        title={hasFolder ? "基于该项目创建 git worktree 副本" : "项目未绑定目录"}
      >
        <span className="pcm-icon">🌱</span>
        <span className="pcm-label">创建永久工作树</span>
      </button>
      <button type="button" className="pcm-item" role="menuitem" onClick={() => { onRename(workspace); onClose(); }}>
        <span className="pcm-icon">✏️</span>
        <span className="pcm-label">重命名项目</span>
      </button>
      <div className="pcm-sep" />
      <button
        type="button"
        className="pcm-item"
        role="menuitem"
        onClick={doArchive}
        title="把该项目下所有对话移到归档"
      >
        <span className="pcm-icon">📦</span>
        <span className="pcm-label">归档对话</span>
      </button>
      <button
        type="button"
        className="pcm-item"
        role="menuitem"
        onClick={doRestore}
        title="恢复该项目下所有归档对话"
      >
        <span className="pcm-icon">📤</span>
        <span className="pcm-label">恢复对话</span>
      </button>
      <div className="pcm-sep" />
      <button
        type="button"
        className="pcm-item pcm-danger"
        role="menuitem"
        onClick={doDelete}
        disabled={isDefault}
        title={isDefault ? "默认项目不可移除" : "从 sidebar 移除该项目（会话归档保留）"}
      >
        <span className="pcm-icon">×</span>
        <span className="pcm-label">移除</span>
      </button>
    </div>
  );
}
