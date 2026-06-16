//! 会话管理
//!
//! 设计参考：docs/开发文档.md §5.6 会话管理

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::message::Message;

/// 会话 ID 类型
pub type SessionId = Uuid;

/// 会话状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// 活跃
    Active,
    /// 暂停（可恢复）
    Paused,
    /// 已归档（只读）
    Archived,
    /// 已删除（标记，真删走 GC）
    Deleted,
}

/// 单个会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// 唯一 ID
    pub id: SessionId,
    /// 会话标题（用户可见）
    pub title: String,
    /// 状态
    pub status: SessionStatus,
    /// 关联的工作目录
    pub cwd: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后活跃时间
    pub updated_at: DateTime<Utc>,
    /// 消息历史（已持久化部分）
    pub messages: Vec<Message>,
    /// 自定义元数据
    pub metadata: HashMap<String, String>,
    /// 总 token 计数
    pub total_tokens: u64,
}

impl Session {
    /// 创建新会话
    pub fn new(title: impl Into<String>, cwd: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            status: SessionStatus::Active,
            cwd: cwd.into(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            metadata: HashMap::new(),
            total_tokens: 0,
        }
    }

    /// 添加消息
    pub fn push(&mut self, msg: Message) {
        self.updated_at = Utc::now();
        self.messages.push(msg);
    }

    /// 切换状态
    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// 消息数
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

/// 会话管理器（线程安全）
#[derive(Debug, Default, Clone)]
pub struct SessionManager {
    inner: Arc<RwLock<HashMap<SessionId, Session>>>,
}

impl SessionManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        Self::default()
    }

    /// 新建会话
    pub async fn create(&self, title: impl Into<String>, cwd: impl Into<String>) -> SessionId {
        let session = Session::new(title, cwd);
        let id = session.id;
        self.inner.write().await.insert(id, session);
        id
    }

    /// 获取会话（克隆）
    pub async fn get(&self, id: SessionId) -> Result<Session> {
        self.inner
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::SessionNotFound(id.to_string()))
    }

    /// 推送消息
    pub async fn push_message(&self, id: SessionId, msg: Message) -> Result<()> {
        let mut guard = self.inner.write().await;
        let session = guard
            .get_mut(&id)
            .ok_or_else(|| Error::SessionNotFound(id.to_string()))?;
        session.push(msg);
        Ok(())
    }

    /// 列出所有会话（按更新时间倒序）
    pub async fn list(&self) -> Vec<Session> {
        let guard = self.inner.read().await;
        let mut list: Vec<Session> = guard.values().cloned().collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    /// 删除会话
    pub async fn delete(&self, id: SessionId) -> Result<()> {
        self.inner
            .write()
            .await
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| Error::SessionNotFound(id.to_string()))
    }

    /// 数量
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// 是否空
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_lifecycle() {
        let mgr = SessionManager::new();
        let id = mgr.create("test", "/tmp").await;
        let s = mgr.get(id).await.unwrap();
        assert_eq!(s.title, "test");
        assert_eq!(s.cwd, "/tmp");
        assert_eq!(s.len(), 0);
        mgr.delete(id).await.unwrap();
        assert!(mgr.get(id).await.is_err());
    }

    #[tokio::test]
    async fn test_push_message() {
        let mgr = SessionManager::new();
        let id = mgr.create("test", "/tmp").await;
        mgr.push_message(id, Message::user(id, "hi")).await.unwrap();
        let s = mgr.get(id).await.unwrap();
        assert_eq!(s.len(), 1);
    }
}
