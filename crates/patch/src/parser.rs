//! Patch 解析

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Patch 行类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatchLineKind {
    /// 上下文（保留）
    Context,
    /// 添加（行首 +）
    Add,
    /// 删除（行首 -）
    Remove,
}

/// Patch 行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchLine {
    pub kind: PatchLineKind,
    pub content: String,
}

impl PatchLine {
    pub fn context(s: impl Into<String>) -> Self {
        Self { kind: PatchLineKind::Context, content: s.into() }
    }
    pub fn add(s: impl Into<String>) -> Self {
        Self { kind: PatchLineKind::Add, content: s.into() }
    }
    pub fn remove(s: impl Into<String>) -> Self {
        Self { kind: PatchLineKind::Remove, content: s.into() }
    }
}

/// Hunk（一段连续修改）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunk {
    /// hunk 内所有行
    pub lines: Vec<PatchLine>,
}

/// 单个文件操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatchOperation {
    /// 创建或覆盖
    Add {
        /// 文件路径
        path: String,
        /// 内容（每行）
        content: Vec<String>,
    },
    /// 修改已有文件
    Update {
        /// 文件路径
        path: String,
        /// hunk 列表（顺序应用）
        hunks: Vec<PatchHunk>,
        /// 移动到新路径（rename）
        #[serde(default)]
        move_to: Option<String>,
    },
    /// 删除
    Delete {
        /// 文件路径
        path: String,
    },
}

impl PatchOperation {
    pub fn path(&self) -> &str {
        match self {
            Self::Add { path, .. } | Self::Update { path, .. } | Self::Delete { path } => path,
        }
    }
}

/// 整个 patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub operations: Vec<PatchOperation>,
}

/// 解析错误
#[derive(Debug, Error)]
pub enum PatchParseError {
    #[error("missing `*** Begin Patch` header")]
    MissingHeader,
    #[error("missing `*** End Patch` footer")]
    MissingFooter,
    #[error("invalid file directive at line {0}: `{1}`")]
    InvalidDirective(usize, String),
    #[error("unknown operation `{0}` at line {1}")]
    UnknownOperation(String, usize),
    #[error("file path missing at line {0}")]
    MissingPath(usize),
    #[error("invalid line at {0}: `{1}`")]
    InvalidLine(usize, String),
}

impl fmt::Display for PatchOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Add { path, .. } => write!(f, "Add({})", path),
            Self::Update { path, move_to, .. } => {
                if let Some(to) = move_to {
                    write!(f, "Rename({} -> {})", path, to)
                } else {
                    write!(f, "Update({})", path)
                }
            }
            Self::Delete { path } => write!(f, "Delete({})", path),
        }
    }
}

/// 解析 patch 字符串
pub fn parse_patch(input: &str) -> Result<Patch, PatchParseError> {
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return Err(PatchParseError::MissingHeader);
    }
    let header = lines[0].trim();
    if header != "*** Begin Patch" {
        return Err(PatchParseError::MissingHeader);
    }
    let footer = lines.last().unwrap().trim();
    if footer != "*** End Patch" {
        return Err(PatchParseError::MissingFooter);
    }

    let mut operations: Vec<PatchOperation> = Vec::new();
    let mut i = 1;

    while i < lines.len() - 1 {
        let line = lines[i];
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("*** Update File:") {
            let path = rest.trim().to_string();
            if path.is_empty() {
                return Err(PatchParseError::MissingPath(i + 1));
            }
            i += 1;
            let mut hunks: Vec<PatchHunk> = Vec::new();
            let mut current_hunk = PatchHunk { lines: Vec::new() };
            let mut move_to: Option<String> = None;

            while i < lines.len() - 1 {
                let l = lines[i];
                let t = l.trim();
                if t.starts_with("*** Update File:")
                    || t.starts_with("*** Add File:")
                    || t.starts_with("*** Delete File:")
                    || t == "*** End Patch"
                {
                    break;
                }
                if let Some(rest) = t.strip_prefix("*** Move to:") {
                    move_to = Some(rest.trim().to_string());
                    i += 1;
                    continue;
                }
                if let Some(rest) = t.strip_prefix("@@") {
                    // 新 hunk（可选）
                    if !current_hunk.lines.is_empty() {
                        hunks.push(std::mem::replace(&mut current_hunk, PatchHunk { lines: Vec::new() }));
                    }
                    // @@ 后是可选的 context label，跳过
                    let _ctx = rest.trim();
                    i += 1;
                    continue;
                }
                let (kind, content) = parse_line(l);
                current_hunk.lines.push(PatchLine {
                    kind,
                    content: content.to_string(),
                });
                i += 1;
            }
            if !current_hunk.lines.is_empty() {
                hunks.push(current_hunk);
            }
            if hunks.is_empty() {
                return Err(PatchParseError::InvalidLine(i + 1, format!("empty hunks for Update {}", path)));
            }
            operations.push(PatchOperation::Update {
                path,
                hunks,
                move_to,
            });
        } else if let Some(rest) = trimmed.strip_prefix("*** Add File:") {
            let path = rest.trim().to_string();
            if path.is_empty() {
                return Err(PatchParseError::MissingPath(i + 1));
            }
            i += 1;
            let mut content = Vec::new();
            while i < lines.len() - 1 {
                let l = lines[i];
                let t = l.trim();
                if t.starts_with("*** Update File:")
                    || t.starts_with("*** Add File:")
                    || t.starts_with("*** Delete File:")
                    || t == "*** End Patch"
                {
                    break;
                }
                if let Some(rest) = l.strip_prefix('+') {
                    content.push(rest.to_string());
                } else {
                    return Err(PatchParseError::InvalidLine(i + 1, l.to_string()));
                }
                i += 1;
            }
            operations.push(PatchOperation::Add { path, content });
        } else if let Some(rest) = trimmed.strip_prefix("*** Delete File:") {
            let path = rest.trim().to_string();
            if path.is_empty() {
                return Err(PatchParseError::MissingPath(i + 1));
            }
            operations.push(PatchOperation::Delete { path });
            i += 1;
        } else if trimmed.is_empty() {
            i += 1;
        } else {
            return Err(PatchParseError::UnknownOperation(trimmed.to_string(), i + 1));
        }
    }

    Ok(Patch { operations })
}

fn parse_line(line: &str) -> (PatchLineKind, &str) {
    if let Some(rest) = line.strip_prefix('+') {
        (PatchLineKind::Add, rest)
    } else if let Some(rest) = line.strip_prefix('-') {
        (PatchLineKind::Remove, rest)
    } else if let Some(rest) = line.strip_prefix(' ') {
        (PatchLineKind::Context, rest)
    } else {
        // 默认为 context（无前缀或裸行）
        (PatchLineKind::Context, line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_update() {
        let patch = "*** Begin Patch\n*** Update File: foo.txt\n@@\n-old\n+new\n context\n*** End Patch\n";
        let p = parse_patch(patch).unwrap();
        assert_eq!(p.operations.len(), 1);
        match &p.operations[0] {
            PatchOperation::Update { path, hunks, .. } => {
                assert_eq!(path, "foo.txt");
                assert_eq!(hunks.len(), 1);
                assert_eq!(hunks[0].lines.len(), 3);
            }
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn test_parse_add() {
        let patch = "*** Begin Patch\n*** Add File: new.txt\n+hello\n+world\n*** End Patch\n";
        let p = parse_patch(patch).unwrap();
        match &p.operations[0] {
            PatchOperation::Add { path, content } => {
                assert_eq!(path, "new.txt");
                assert_eq!(content, &vec!["hello".to_string(), "world".to_string()]);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_delete() {
        let patch = "*** Begin Patch\n*** Delete File: old.txt\n*** End Patch\n";
        let p = parse_patch(patch).unwrap();
        match &p.operations[0] {
            PatchOperation::Delete { path } => assert_eq!(path, "old.txt"),
            _ => panic!("expected Delete"),
        }
    }

    #[test]
    fn test_parse_multi_ops() {
        let patch = "*** Begin Patch\n*** Add File: a.txt\n+x\n*** Delete File: b.txt\n*** Update File: c.txt\n@@\n-y\n+z\n*** End Patch\n";
        let p = parse_patch(patch).unwrap();
        assert_eq!(p.operations.len(), 3);
    }

    #[test]
    fn test_missing_header() {
        assert!(parse_patch("*** Update File: a\n").is_err());
    }
}