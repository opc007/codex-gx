//! 对话历史压缩
//!
//! 设计参考：docs/开发文档.md §8.5 Context Compaction

use agent_core::message::{ContentBlock, Message, MessageRole};

/// 压缩策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionStrategy {
    /// 保留前 N 条 + 后 M 条，中间丢弃
    TruncateMiddle { keep_head: usize, keep_tail: usize },
    /// 简单截断到最近 N 条
    KeepLast(usize),
    /// 保留所有 system + 截断 user/assistant 到最近 N 条
    SystemPlusTail(usize),
}

/// 估算消息 token 数（粗略：每 4 字符 ≈ 1 token）
pub fn estimate_tokens(messages: &[Message]) -> u32 {
    let mut total = 0;
    for m in messages {
        for c in &m.content {
            match c {
                ContentBlock::Text { text } => total += (text.len() / 4) as u32,
                ContentBlock::Image { data, .. } => total += (data.len() / 100) as u32,
                _ => total += 10,
            }
        }
        total += 5; // role overhead
    }
    total
}

/// 压缩消息列表
pub fn compact_messages(
    messages: &[Message],
    strategy: CompactionStrategy,
) -> Vec<Message> {
    match strategy {
        CompactionStrategy::KeepLast(n) => {
            let start = messages.len().saturating_sub(n);
            messages[start..].to_vec()
        }
        CompactionStrategy::TruncateMiddle {
            keep_head,
            keep_tail,
        } => {
            if messages.len() <= keep_head + keep_tail {
                return messages.to_vec();
            }
            let mut out = Vec::new();
            out.extend_from_slice(&messages[..keep_head]);
            // 中间用一个 marker 表示截断（system message）
            let marker_id = uuid::Uuid::new_v4();
            let mut marker_msg = Message::system(
                messages.first().map(|m| m.session_id).unwrap_or(marker_id),
                "[... context truncated for length ...]",
            );
            marker_msg.id = marker_id;
            out.push(marker_msg);
            out.extend_from_slice(&messages[messages.len() - keep_tail..]);
            out
        }
        CompactionStrategy::SystemPlusTail(n) => {
            let mut out = Vec::new();
            for m in messages.iter().filter(|m| m.role == MessageRole::System) {
                out.push(m.clone());
            }
            let others: Vec<&Message> = messages
                .iter()
                .filter(|m| m.role != MessageRole::System)
                .collect();
            let start = others.len().saturating_sub(n);
            out.extend(others[start..].iter().map(|m| (*m).clone()));
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::message::Message;

    #[test]
    fn test_estimate_tokens() {
        let sid = uuid::Uuid::new_v4();
        let msgs = vec![
            Message::user(sid, "hello world"), // 11 chars / 4 ≈ 2 tokens + 5 = 7
        ];
        let t = estimate_tokens(&msgs);
        assert!(t > 0);
    }

    #[test]
    fn test_keep_last() {
        let sid = uuid::Uuid::new_v4();
        let msgs: Vec<Message> = (0..10).map(|i| Message::user(sid, format!("msg {}", i))).collect();
        let out = compact_messages(&msgs, CompactionStrategy::KeepLast(3));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn test_truncate_middle() {
        let sid = uuid::Uuid::new_v4();
        let msgs: Vec<Message> = (0..20).map(|i| Message::user(sid, format!("msg {}", i))).collect();
        let out = compact_messages(&msgs, CompactionStrategy::TruncateMiddle {
            keep_head: 3,
            keep_tail: 5,
        });
        assert_eq!(out.len(), 3 + 1 + 5); // head + marker + tail
    }

    #[test]
    fn test_system_plus_tail() {
        let sid = uuid::Uuid::new_v4();
        let mut msgs = vec![Message::system(sid, "be helpful")];
        msgs.extend((0..10).map(|i| Message::user(sid, format!("msg {}", i))));
        let out = compact_messages(&msgs, CompactionStrategy::SystemPlusTail(3));
        // 1 system + 3 user = 4
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].role, MessageRole::System);
    }
}