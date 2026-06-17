//! v1.5：Agent 流程图（GraphSpec）
//!
//! 统一图规范：
//! - `Node` 描述一个步骤（plan step / tool call / sub-agent / message / decision）
//! - `Edge` 描述节点之间的依赖 / 时序
//! - `Graph` 是节点 + 边的集合，可被前端用 SVG / Mermaid 渲染
//!
//! 4 个内置来源：
//! 1. `from_plan(text)` — 从 plan markdown 文本解析步骤
//! 2. `from_queue_tasks(tasks)` — 任务队列实时状态
//! 3. `from_subagents(sessions)` — sub-agent 父子关系
//! 4. `from_messages(msgs)` — 消息流（user / assistant / tool）

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Start,
    End,
    Plan,
    Message,
    Tool,
    SubAgent,
    Decision,
    Queue,
    Skill,
}

impl NodeKind {
    pub fn icon(&self) -> &'static str {
        match self {
            NodeKind::Start => "🟢",
            NodeKind::End => "🔴",
            NodeKind::Plan => "📋",
            NodeKind::Message => "💬",
            NodeKind::Tool => "🔧",
            NodeKind::SubAgent => "🤖",
            NodeKind::Decision => "🔀",
            NodeKind::Queue => "📋",
            NodeKind::Skill => "🪄",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub detail: Option<String>,
    pub status: Option<String>, // "pending" | "running" | "success" | "error" | "skipped"
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub title: Option<String>,
}

impl Graph {
    pub fn new(title: Option<String>) -> Self {
        Graph {
            title,
            ..Default::default()
        }
    }
    pub fn add_node(&mut self, n: Node) {
        self.nodes.push(n);
    }
    pub fn add_edge(&mut self, e: Edge) {
        self.edges.push(e);
    }

    /// 输出 Mermaid flowchart 字符串（用于导出 / 在 Markdown 中嵌入）
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("```mermaid\nflowchart TD\n");
        if let Some(t) = &self.title {
            out.push_str(&format!("  %% {t}\n"));
        }
        for n in &self.nodes {
            let safe_label = n.label.replace('"', "'").replace('\n', " ");
            let shape = match n.kind {
                NodeKind::Start | NodeKind::End => format!("(({}))", n.id), // circle
                NodeKind::Decision => format!("{{\"{}\"}}", safe_label),
                _ => format!("[\"{}\"]", safe_label),
            };
            // id 是 mermaid 关键字
            let id = n.id.replace('-', "_");
            out.push_str(&format!("  {}{}:::{} \n", id, shape, kind_class(n.kind)));
            // 把 label 写到节点元信息中显示
            let _ = n;
        }
        for e in &self.edges {
            let from = e.from.replace('-', "_");
            let to = e.to.replace('-', "_");
            match &e.label {
                Some(l) => out.push_str(&format!("  {} -->|{}| {}\n", from, l, to)),
                None => out.push_str(&format!("  {} --> {}\n", from, to)),
            }
        }
        out.push_str("```\n");
        out
    }
}

fn kind_class(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Start | NodeKind::End => "terminal",
        NodeKind::Plan => "plan",
        NodeKind::Message => "message",
        NodeKind::Tool => "tool",
        NodeKind::SubAgent => "subagent",
        NodeKind::Decision => "decision",
        NodeKind::Queue => "queue",
        NodeKind::Skill => "skill",
    }
}

/// 从 plan 文本解析步骤
/// 简单解析：识别有序列表 `- step` / `1. step`
pub fn from_plan(plan: &str) -> Graph {
    let mut g = Graph::new(Some("Plan Steps".to_string()));
    g.add_node(Node {
        id: "start".to_string(),
        kind: NodeKind::Start,
        label: "开始".to_string(),
        detail: None,
        status: Some("pending".to_string()),
        meta: None,
    });
    let mut last = "start".to_string();
    let mut idx = 0;
    for line in plan.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // 匹配 "1. xxx" / "- xxx" / "* xxx"
        let step = if let Some(rest) = strip_prefix(trimmed, &[". ", ") "]) {
            Some(rest)
        } else if let Some(rest) = trimmed.strip_prefix("- ") {
            Some(rest)
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            Some(rest)
        } else {
            None
        };
        if let Some(text) = step {
            idx += 1;
            let id = format!("step-{idx}");
            g.add_node(Node {
                id: id.clone(),
                kind: NodeKind::Plan,
                label: truncate(text, 60),
                detail: Some(text.to_string()),
                status: Some("pending".to_string()),
                meta: None,
            });
            g.add_edge(Edge {
                from: last.clone(),
                to: id.clone(),
                label: None,
            });
            last = id;
        }
    }
    if idx == 0 {
        // 没解析出步骤，把整段当成 1 个节点
        g.add_node(Node {
            id: "plan-1".to_string(),
            kind: NodeKind::Plan,
            label: truncate(plan, 60),
            detail: Some(plan.to_string()),
            status: Some("pending".to_string()),
            meta: None,
        });
        g.add_edge(Edge {
            from: "start".to_string(),
            to: "plan-1".to_string(),
            label: None,
        });
        last = "plan-1".to_string();
    }
    g.add_node(Node {
        id: "end".to_string(),
        kind: NodeKind::End,
        label: "完成".to_string(),
        detail: None,
        status: Some("pending".to_string()),
        meta: None,
    });
    g.add_edge(Edge {
        from: last,
        to: "end".to_string(),
        label: None,
    });
    g
}

fn strip_prefix<'a>(s: &'a str, suffixes: &[&str]) -> Option<&'a str> {
    // s 形如 "1. xxx" / "12) xxx"
    let bytes = s.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == 0 {
        return None;
    }
    let after = &s[idx..];
    for sfx in suffixes {
        if let Some(rest) = after.strip_prefix(sfx) {
            return Some(rest);
        }
    }
    None
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_from_plan_simple() {
        let p = "1. 读取文件\n2. 分析内容\n3. 写报告";
        let g = from_plan(p);
        assert_eq!(g.nodes.len(), 5); // start + 3 + end
        assert_eq!(g.edges.len(), 4);
    }

    #[test]
    fn graph_from_plan_dash() {
        let p = "- 第一步\n- 第二步\n- 第三步";
        let g = from_plan(p);
        assert_eq!(g.nodes.len(), 5);
    }

    #[test]
    fn graph_from_plan_empty() {
        let p = "随便一段文字，没有列表";
        let g = from_plan(p);
        assert!(g.nodes.len() >= 3); // start + 1 + end
    }

    #[test]
    fn graph_to_mermaid() {
        let mut g = Graph::new(Some("测试".to_string()));
        g.add_node(Node {
            id: "a".into(),
            kind: NodeKind::Start,
            label: "A".into(),
            detail: None,
            status: None,
            meta: None,
        });
        g.add_node(Node {
            id: "b".into(),
            kind: NodeKind::Plan,
            label: "B".into(),
            detail: None,
            status: None,
            meta: None,
        });
        g.add_edge(Edge {
            from: "a".into(),
            to: "b".into(),
            label: Some("next".into()),
        });
        let m = g.to_mermaid();
        assert!(m.contains("flowchart TD"));
        assert!(m.contains("a -->|next| b"));
    }

    #[test]
    fn node_kind_icon() {
        assert_eq!(NodeKind::Start.icon(), "🟢");
        assert_eq!(NodeKind::End.icon(), "🔴");
    }

    #[test]
    fn graph_new() {
        let g = Graph::new(Some("X".into()));
        assert!(g.title.is_some());
        assert_eq!(g.nodes.len(), 0);
    }

    #[test]
    fn strip_prefix_dot() {
        let s = "1. hello";
        assert_eq!(strip_prefix(s, &[". ", ") "]), Some("hello"));
    }

    #[test]
    fn strip_prefix_paren() {
        let s = "12) hello world";
        assert_eq!(strip_prefix(s, &[". ", ") "]), Some("hello world"));
    }

    #[test]
    fn strip_prefix_invalid() {
        assert!(strip_prefix("abc", &[". ", ") "]).is_none());
    }

    #[test]
    fn truncate_long() {
        let s = "a".repeat(100);
        let t = truncate(&s, 10);
        assert!(t.chars().count() <= 11); // 10 + …
    }

    #[test]
    fn truncate_short() {
        assert_eq!(truncate("abc", 10), "abc");
    }
}
