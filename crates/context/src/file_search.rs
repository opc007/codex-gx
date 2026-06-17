//! Fuzzy 文件搜索（@ mention）
//!
//! 设计参考：docs/开发文档.md §5.37 @ 模糊文件搜索 / §5.37A Unified Mentions
//!
//! 输入：query（部分文件名）
//! 输出：按相关度排序的文件列表

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMatch {
    /// 相对路径
    pub path: String,
    /// 绝对路径
    pub absolute: String,
    /// 匹配分数（越小越相关）
    pub score: u32,
    /// 是否是目录
    pub is_dir: bool,
}

/// 在 cwd 下递归找匹配文件
pub fn search(cwd: &Path, query: &str, max: usize) -> Vec<FileMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    let mut matches: Vec<FileMatch> = Vec::new();
    walk(cwd, cwd, &query_lower, &mut matches);
    matches.sort_by(|a, b| a.score.cmp(&b.score).then_with(|| a.path.cmp(&b.path)));
    matches.truncate(max);
    matches
}

fn walk(root: &Path, current: &Path, query: &str, out: &mut Vec<FileMatch>) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // 跳过隐藏目录（.git / node_modules / target）
        if name_str.starts_with('.')
            || name_str == "node_modules"
            || name_str == "target"
            || name_str == "dist"
        {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy();
        let name_lower = name_str.to_lowercase();
        let score = score_match(&name_lower, &rel_str.to_lowercase(), query);
        if let Some(s) = score {
            out.push(FileMatch {
                path: rel_str.to_string(),
                absolute: path.to_string_lossy().to_string(),
                score: s,
                is_dir,
            });
        }
        if is_dir {
            walk(root, &path, query, out);
        }
    }
}

/// 计算匹配分数；不匹配返回 None
fn score_match(name: &str, rel: &str, query: &str) -> Option<u32> {
    // 完全匹配文件名 → 0
    if name == query {
        return Some(0);
    }
    // 文件名开头匹配 → 10
    if name.starts_with(query) {
        return Some(10);
    }
    // 文件名包含 query，且 query 长度 > 2 → 100；否则 50（避免 fr 匹配 frame 等短查询）
    if name.contains(query) {
        return Some(if query.len() > 2 { 100 } else { 50 });
    }
    // 完整路径包含 → 200
    if rel.contains(query) {
        return Some(200);
    }
    // 模糊匹配（每个 query char 按顺序在 name 中出现）→ 500
    if fuzzy_match(name, query) {
        return Some(500);
    }
    None
}

fn fuzzy_match(name: &str, query: &str) -> bool {
    let mut chars = query.chars().peekable();
    for c in name.chars() {
        if chars.peek() == Some(&c) {
            chars.next();
        }
    }
    chars.peek().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_score_exact() {
        assert_eq!(score_match("foo.rs", "src/foo.rs", "foo.rs"), Some(0));
    }

    #[test]
    fn test_score_prefix() {
        assert_eq!(score_match("foobar.rs", "src/foobar.rs", "foo"), Some(10));
    }

    #[test]
    fn test_score_contains() {
        assert_eq!(score_match("my_foo.rs", "src/my_foo.rs", "foo"), Some(100));
    }

    #[test]
    fn test_score_no_match() {
        assert_eq!(score_match("hello.rs", "src/hello.rs", "world"), None);
    }

    #[test]
    fn test_fuzzy() {
        assert!(fuzzy_match("foobar", "fb"));
        assert!(!fuzzy_match("foobar", "bf"));
    }

    #[test]
    fn test_search_in_tree() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("foo.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("foobar.md"), "").unwrap();

        let r = search(dir.path(), "foo", 10);
        assert!(r.len() >= 2);
        // foo.txt 字典序在 foobar.md 前面，且 score 都是 10
        assert_eq!(r[0].path, "foo.txt");
        assert_eq!(r[1].path, "sub/foobar.md");
    }
}
