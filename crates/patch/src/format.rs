//! Patch 格式化 / 摘要

use crate::parser::{Patch, PatchOperation};

/// 序列化为字符串
pub fn format_patch(patch: &Patch) -> String {
    let mut s = String::from("*** Begin Patch\n");
    for op in &patch.operations {
        match op {
            PatchOperation::Add { path, content } => {
                s.push_str(&format!("*** Add File: {}\n", path));
                for line in content {
                    s.push_str(&format!("+{}\n", line));
                }
            }
            PatchOperation::Update { path, hunks, move_to } => {
                s.push_str(&format!("*** Update File: {}\n", path));
                if let Some(to) = move_to {
                    s.push_str(&format!("*** Move to: {}\n", to));
                }
                for hunk in hunks {
                    if !hunk.lines.is_empty() {
                        s.push_str("@@\n");
                    }
                    for l in &hunk.lines {
                        let prefix = match l.kind {
                            crate::parser::PatchLineKind::Add => "+",
                            crate::parser::PatchLineKind::Remove => "-",
                            crate::parser::PatchLineKind::Context => " ",
                        };
                        s.push_str(&format!("{}{}\n", prefix, l.content));
                    }
                }
            }
            PatchOperation::Delete { path } => {
                s.push_str(&format!("*** Delete File: {}\n", path));
            }
        }
    }
    s.push_str("*** End Patch\n");
    s
}

/// 简短摘要
pub fn summarize(patch: &Patch) -> String {
    let adds = patch
        .operations
        .iter()
        .filter(|o| matches!(o, PatchOperation::Add { .. }))
        .count();
    let updates = patch
        .operations
        .iter()
        .filter(|o| matches!(o, PatchOperation::Update { .. }))
        .count();
    let deletes = patch
        .operations
        .iter()
        .filter(|o| matches!(o, PatchOperation::Delete { .. }))
        .count();
    format!(
        "{} ops: {} add, {} update, {} delete",
        patch.operations.len(),
        adds,
        updates,
        deletes
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Patch, PatchOperation};

    #[test]
    fn test_summarize() {
        let p = Patch {
            operations: vec![
                PatchOperation::Add {
                    path: "a".into(),
                    content: vec!["x".into()],
                },
                PatchOperation::Delete { path: "b".into() },
            ],
        };
        assert_eq!(summarize(&p), "2 ops: 1 add, 0 update, 1 delete");
    }

    #[test]
    fn test_format_roundtrip() {
        let p = Patch {
            operations: vec![PatchOperation::Add {
                path: "x.txt".into(),
                content: vec!["hello".into()],
            }],
        };
        let text = format_patch(&p);
        assert!(text.contains("*** Begin Patch"));
        assert!(text.contains("*** Add File: x.txt"));
        assert!(text.contains("+hello"));
        assert!(text.contains("*** End Patch"));

        let parsed = crate::parser::parse_patch(&text).unwrap();
        assert_eq!(parsed.operations.len(), 1);
    }
}