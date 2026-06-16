//! Patch 应用

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::parser::{Patch, PatchHunk, PatchLine, PatchLineKind, PatchOperation};

#[derive(Debug, Error)]
pub enum PatchApplyError {
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("context mismatch in {file} at line {line}: expected `{expected}`, got `{actual}`")]
    ContextMismatch {
        file: String,
        line: usize,
        expected: String,
        actual: String,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid hunk at {file}: {msg}")]
    InvalidHunk { file: String, msg: String },
}

#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: String,
    pub action: String,
    pub hunks_applied: usize,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PatchResult {
    pub files: Vec<FileResult>,
}

impl PatchResult {
    pub fn summary(&self) -> String {
        let mut s = String::new();
        for f in &self.files {
            s.push_str(&format!("{}: {}\n", f.path, f.action));
        }
        s
    }
}

/// 应用单个 patch 操作
pub fn apply_op(
    root: &Path,
    op: &PatchOperation,
) -> Result<FileResult, PatchApplyError> {
    match op {
        PatchOperation::Add { path, content } => {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let joined = content.join("\n");
            std::fs::write(&full, joined.as_bytes())?;
            Ok(FileResult {
                path: path.clone(),
                action: "created".into(),
                hunks_applied: 0,
                bytes_written: joined.len(),
            })
        }
        PatchOperation::Update {
            path,
            hunks,
            move_to,
        } => {
            let full = root.join(path);
            let original = std::fs::read_to_string(&full)
                .map_err(|_| PatchApplyError::FileNotFound(path.clone()))?;
            let had_trailing_newline = original.ends_with('\n');
            let mut lines: Vec<String> = original.split('\n').map(|s| s.to_string()).collect();
            // split('\n') 会留下一个尾部空行（如果原文件以 \n 结尾），去掉它避免重复
            if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
                lines.pop();
            }
            let mut hunks_applied = 0;
            for hunk in hunks {
                apply_hunk(&mut lines, hunk, path)?;
                hunks_applied += 1;
            }
            let new_content = lines.join("\n");
            let final_str: String = if had_trailing_newline && !new_content.is_empty() && !new_content.ends_with('\n') {
                format!("{}\n", new_content)
            } else {
                new_content
            };
            let bytes_written = final_str.len();
            std::fs::write(&full, final_str.as_bytes())?;
            let mut action = format!("updated ({} hunks)", hunks_applied);
            if let Some(to) = move_to {
                let target = root.join(to);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(&full, &target)?;
                action = format!("renamed {} -> {}", path, to);
            }
            Ok(FileResult {
                path: path.clone(),
                action,
                hunks_applied,
                bytes_written,
            })
        }
        PatchOperation::Delete { path } => {
            let full = root.join(path);
            std::fs::remove_file(&full)
                .map_err(|_| PatchApplyError::FileNotFound(path.clone()))?;
            Ok(FileResult {
                path: path.clone(),
                action: "deleted".into(),
                hunks_applied: 0,
                bytes_written: 0,
            })
        }
    }
}

/// 应用单个 hunk
fn apply_hunk(
    lines: &mut Vec<String>,
    hunk: &PatchHunk,
    file: &str,
) -> Result<(), PatchApplyError> {
    if hunk.lines.is_empty() {
        return Err(PatchApplyError::InvalidHunk {
            file: file.into(),
            msg: "empty hunk".into(),
        });
    }

    // 找 hunk 在 lines 里的 anchor（hunk 起点在 lines 里的位置）
    let anchor = find_hunk_anchor(lines, hunk, file)?;

    let mut out: Vec<String> = Vec::new();
    out.extend_from_slice(&lines[..anchor]);

    let mut src_idx = anchor;
    for pl in &hunk.lines {
        match pl.kind {
            PatchLineKind::Context => {
                if src_idx >= lines.len() || lines[src_idx] != pl.content {
                    let actual = lines.get(src_idx).cloned().unwrap_or_default();
                    return Err(PatchApplyError::ContextMismatch {
                        file: file.into(),
                        line: src_idx + 1,
                        expected: pl.content.clone(),
                        actual,
                    });
                }
                out.push(pl.content.clone());
                src_idx += 1;
            }
            PatchLineKind::Add => {
                out.push(pl.content.clone());
            }
            PatchLineKind::Remove => {
                if src_idx >= lines.len() || lines[src_idx] != pl.content {
                    let actual = lines.get(src_idx).cloned().unwrap_or_default();
                    return Err(PatchApplyError::ContextMismatch {
                        file: file.into(),
                        line: src_idx + 1,
                        expected: pl.content.clone(),
                        actual,
                    });
                }
                src_idx += 1;
            }
        }
    }

    out.extend_from_slice(&lines[src_idx..]);
    *lines = out;
    Ok(())
}

/// 找 hunk anchor：返回 hunk 起点在 lines 里的位置
///
/// 算法：尝试把 hunk 放在 lines 的每个位置 pos（0..=lines.len()），
/// 模拟逐行匹配 context/remove 行，如果全部能匹配上就是 anchor
fn find_hunk_anchor(
    lines: &[String],
    hunk: &PatchHunk,
    file: &str,
) -> Result<usize, PatchApplyError> {
    if hunk.lines.is_empty() {
        return Err(PatchApplyError::InvalidHunk {
            file: file.into(),
            msg: "empty hunk".into(),
        });
    }

    let max_pos = lines.len();
    for pos in 0..=max_pos {
        if verify_hunk_at_pos(lines, pos, &hunk.lines) {
            return Ok(pos);
        }
    }
    Err(PatchApplyError::InvalidHunk {
        file: file.into(),
        msg: "no matching context line found".into(),
    })
}

/// 验证 hunk 放在 lines[pos] 处是否能匹配
/// 模拟逐行消费 src，遇到 add 跳过 src，遇到 context/remove 严格匹配 src
fn verify_hunk_at_pos(lines: &[String], pos: usize, sub: &[PatchLine]) -> bool {
    let mut idx = pos;
    for pl in sub {
        match pl.kind {
            PatchLineKind::Context | PatchLineKind::Remove => {
                if idx >= lines.len() || lines[idx] != pl.content {
                    return false;
                }
                idx += 1;
            }
            _ => {}
        }
    }
    true
}

/// 验证从 lines[start] 开始，sub 里所有 context/remove 行能严格匹配
fn verify_hunk_at(lines: &[String], start: usize, sub: &[PatchLine]) -> bool {
    let mut idx = start;
    for pl in sub {
        match pl.kind {
            PatchLineKind::Context | PatchLineKind::Remove => {
                if idx >= lines.len() || lines[idx] != pl.content {
                    return false;
                }
                idx += 1;
            }
            _ => {}
        }
    }
    true
}

/// 应用整个 patch 到目录
pub fn apply_to_dir(root: &Path, patch: &Patch) -> Result<PatchResult, PatchApplyError> {
    let mut result = PatchResult::default();
    for op in &patch.operations {
        let r = apply_op(root, op)?;
        result.files.push(r);
    }
    Ok(result)
}

/// 应用 patch 字符串到目录
pub fn apply_patch(root: &Path, patch_text: &str) -> Result<PatchResult, PatchApplyError> {
    let patch = crate::parser::parse_patch(patch_text)
        .map_err(|e| PatchApplyError::InvalidHunk {
            file: "?".into(),
            msg: format!("parse error: {}", e),
        })?;
    apply_to_dir(root, &patch)
}

pub fn expand_path(root: &Path, file: &str) -> PathBuf {
    root.join(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_apply_add() {
        let dir = tempdir().unwrap();
        let patch_text = "*** Begin Patch\n*** Add File: hello.txt\n+hello world\n+second line\n*** End Patch\n";
        let r = apply_patch(dir.path(), patch_text).unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].action, "created");
        let content = std::fs::read_to_string(dir.path().join("hello.txt")).unwrap();
        assert_eq!(content, "hello world\nsecond line");
    }

    #[test]
    fn test_apply_update() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("foo.txt"), "line1\nline2\nline3\n").unwrap();

        let patch_text = "*** Begin Patch\n*** Update File: foo.txt\n@@\n line1\n-line2\n+new line2\n line3\n*** End Patch\n";
        let _r = apply_patch(dir.path(), patch_text).unwrap();
        let content = std::fs::read_to_string(dir.path().join("foo.txt")).unwrap();
        assert_eq!(content, "line1\nnew line2\nline3\n");
    }

    #[test]
    fn test_apply_delete() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("old.txt"), "x").unwrap();
        let patch_text = "*** Begin Patch\n*** Delete File: old.txt\n*** End Patch\n";
        let r = apply_patch(dir.path(), patch_text).unwrap();
        assert!(!dir.path().join("old.txt").exists());
    }

    #[test]
    fn test_context_mismatch() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("foo.txt"), "actual content\n").unwrap();
        let patch_text = "*** Begin Patch\n*** Update File: foo.txt\n@@\n-wrong content\n+new\n*** End Patch\n";
        let r = apply_patch(dir.path(), patch_text);
        assert!(r.is_err());
    }

    #[test]
    fn test_hunk_anchor_skip() {
        let dir = tempdir().unwrap();
        // "target" 只出现一次，hunk 应该锚到正确位置
        std::fs::write(
            dir.path().join("foo.txt"),
            "skip me\nskip me\ntarget\nskip me\n",
        )
        .unwrap();
        let patch_text = "*** Begin Patch\n*** Update File: foo.txt\n@@\n-target\n+replaced\n*** End Patch\n";
        let r = apply_patch(dir.path(), patch_text).unwrap();
        let content = std::fs::read_to_string(dir.path().join("foo.txt")).unwrap();
        assert_eq!(content, "skip me\nskip me\nreplaced\nskip me\n");
    }

    #[test]
    fn test_multi_hunks() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("f.txt"),
            "a\nb\nc\nd\ne\nf\n",
        )
        .unwrap();
        let patch_text = "*** Begin Patch\n*** Update File: f.txt\n@@\n-b\n+B\n@@\n-e\n+E\n*** End Patch\n";
        let r = apply_patch(dir.path(), patch_text).unwrap();
        assert_eq!(r.files[0].hunks_applied, 2);
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "a\nB\nc\nd\nE\nf\n");
    }

    #[test]
    fn test_pure_add_hunk() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "a\nb\nc\n").unwrap();
        let patch_text = "*** Begin Patch\n*** Update File: f.txt\n@@\n b\n+INSERTED\n*** End Patch\n";
        let r = apply_patch(dir.path(), patch_text).unwrap();
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "a\nb\nINSERTED\nc\n");
    }
}