//! IDE context（VSCode / Cursor）
//!
//! 设计参考：docs/开发文档.md §5.42 IDE context
//!
//! 通过 stdin pipe 与 VSCode / Cursor 通信，获取当前打开的文件 / 光标位置

use serde::{Deserialize, Serialize};

/// IDE 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeContext {
    /// IDE 名（vscode / cursor）
    pub ide: String,
    /// 当前打开的文件路径
    pub current_file: Option<String>,
    /// 选中的文本
    pub selection: Option<String>,
    /// 光标所在行号（1-based）
    pub cursor_line: Option<u32>,
    /// 光标所在列号
    pub cursor_column: Option<u32>,
}

impl IdeContext {
    /// 是否可用
    pub fn is_available(&self) -> bool {
        self.current_file.is_some()
    }

    /// 渲染为文本
    pub fn render(&self) -> String {
        let mut s = format!("# IDE: {}\n", self.ide);
        if let Some(f) = &self.current_file {
            s.push_str(&format!("Current file: {}\n", f));
        }
        if let Some(line) = self.cursor_line {
            s.push_str(&format!(
                "Cursor: line {}, col {}\n",
                line,
                self.cursor_column.unwrap_or(0)
            ));
        }
        if let Some(sel) = &self.selection {
            s.push_str("\nSelection:\n```\n");
            s.push_str(sel);
            s.push_str("\n```\n");
        }
        s
    }
}

impl Default for IdeContext {
    fn default() -> Self {
        Self {
            ide: "none".into(),
            current_file: None,
            selection: None,
            cursor_line: None,
            cursor_column: None,
        }
    }
}

/// 检测当前是否在 IDE 内运行（通过环境变量）
pub fn detect_from_env() -> Option<IdeContext> {
    // VSCode / Cursor 通过环境变量暴露
    let ide = if let Ok(_) = std::env::var("CURSOR_TRACE_ID") {
        "cursor"
    } else if let Ok(_) = std::env::var("VSCODE_IPC_HOOK_CLI") {
        "vscode"
    } else if let Ok(_) = std::env::var("TERM_PROGRAM") {
        let p = std::env::var("TERM_PROGRAM").unwrap_or_default();
        if p.contains("vscode") {
            "vscode"
        } else if p.contains("cursor") {
            "cursor"
        } else {
            return None;
        }
    } else {
        return None;
    };

    Some(IdeContext {
        ide: ide.into(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_basic() {
        let ctx = IdeContext {
            ide: "vscode".into(),
            current_file: Some("/tmp/foo.rs".into()),
            selection: None,
            cursor_line: Some(10),
            cursor_column: Some(5),
        };
        let s = ctx.render();
        assert!(s.contains("vscode"));
        assert!(s.contains("/tmp/foo.rs"));
        assert!(s.contains("line 10"));
    }
}
