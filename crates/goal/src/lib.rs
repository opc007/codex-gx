//! v1.7：Goal 任务目标
//!
//! 设计参考：docs/开发文档.md §5.22
//!
//! ## 数据结构
//! - Goal: 大任务（title + description + todos[]）
//! - Todo: 子任务（content + status + depends_on + evidence）
//! - 状态机：Active / Paused / Completed / Abandoned
//!
//! ## 持久化
//! `~/.agentshell/goals/<goal_id>.json`

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GoalStatus {
    Active,
    Paused,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub thread_id: String,
    pub title: String,
    pub description: String,
    pub status: GoalStatus,
    pub todos: Vec<Todo>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub checkpoint_id: Option<String>,
}

impl Goal {
    pub fn new(thread_id: &str, title: &str, description: &str) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: format!("goal-{}", uuid::Uuid::new_v4()),
            thread_id: thread_id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            status: GoalStatus::Active,
            todos: Vec::new(),
            created_at: now,
            updated_at: now,
            checkpoint_id: None,
        }
    }

    /// 进度（0.0 - 1.0）
    pub fn progress(&self) -> f32 {
        if self.todos.is_empty() {
            return 0.0;
        }
        let done = self
            .todos
            .iter()
            .filter(|t| t.status == TodoStatus::Done)
            .count();
        done as f32 / self.todos.len() as f32
    }

    /// done/total 字符串
    pub fn progress_str(&self) -> String {
        let done = self
            .todos
            .iter()
            .filter(|t| t.status == TodoStatus::Done)
            .count();
        format!("{}/{}", done, self.todos.len())
    }

    /// 添加 todo
    pub fn add_todo(&mut self, content: &str) -> String {
        let id = format!("todo-{}", uuid::Uuid::new_v4());
        let todo = Todo {
            id: id.clone(),
            content: content.to_string(),
            status: TodoStatus::Pending,
            depends_on: Vec::new(),
            completed_at: None,
            evidence: None,
        };
        self.todos.push(todo);
        self.touch();
        id
    }

    /// 标记 done（带 evidence）
    pub fn mark_done(&mut self, id: &str, evidence: Option<String>) -> Result<(), GoalError> {
        let todo = self
            .todos
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        if todo.status == TodoStatus::Blocked {
            return Err(GoalError::TodoBlocked(id.to_string()));
        }
        todo.status = TodoStatus::Done;
        todo.completed_at = Some(chrono::Utc::now().timestamp());
        if let Some(ev) = evidence {
            todo.evidence = Some(ev);
        }
        self.touch();
        // 全 done → goal 完成
        if self.todos.iter().all(|t| t.status == TodoStatus::Done) {
            self.status = GoalStatus::Completed;
        }
        Ok(())
    }

    /// 标记 in progress
    pub fn mark_in_progress(&mut self, id: &str) -> Result<(), GoalError> {
        let todo = self
            .todos
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        todo.status = TodoStatus::InProgress;
        self.touch();
        Ok(())
    }

    /// 标记 blocked
    pub fn mark_blocked(&mut self, id: &str) -> Result<(), GoalError> {
        let todo = self
            .todos
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        todo.status = TodoStatus::Blocked;
        self.touch();
        Ok(())
    }

    /// 暂停
    pub fn pause(&mut self) {
        self.status = GoalStatus::Paused;
        self.touch();
    }

    /// 继续
    pub fn resume(&mut self) {
        self.status = GoalStatus::Active;
        self.touch();
    }

    /// 放弃
    pub fn abandon(&mut self) {
        self.status = GoalStatus::Abandoned;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GoalError {
    #[error("todo not found: {0}")]
    TodoNotFound(String),
    #[error("todo is blocked: {0}")]
    TodoBlocked(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Goal registry（按 thread_id 索引，持久化到 `~/.agentshell/goals/`）
pub struct GoalRegistry {
    goals: HashMap<String, Goal>,
    storage_dir: PathBuf,
}

impl GoalRegistry {
    pub fn load() -> Self {
        let storage_dir = goal_dir();
        let mut goals = HashMap::new();
        if storage_dir.exists() {
            if let Ok(rd) = std::fs::read_dir(&storage_dir) {
                for entry in rd.flatten() {
                    if let Ok(text) = std::fs::read_to_string(entry.path()) {
                        if let Ok(g) = serde_json::from_str::<Goal>(&text) {
                            goals.insert(g.id.clone(), g);
                        }
                    }
                }
            }
        }
        Self {
            goals,
            storage_dir,
        }
    }

    /// 创建一个新 goal
    pub fn create(&mut self, thread_id: &str, title: &str, description: &str) -> Goal {
        let g = Goal::new(thread_id, title, description);
        self.save(&g);
        self.goals.insert(g.id.clone(), g.clone());
        g
    }

    /// 取一个
    pub fn get(&self, id: &str) -> Option<&Goal> {
        self.goals.get(id)
    }

    /// 取可变的
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Goal> {
        if self.goals.contains_key(id) {
            self.goals.get_mut(id)
        } else {
            None
        }
    }

    /// 按 thread 找 active goal
    pub fn find_active_for_thread(&self, thread_id: &str) -> Option<&Goal> {
        self.goals
            .values()
            .find(|g| g.thread_id == thread_id && g.status == GoalStatus::Active)
    }

    /// 列出所有
    pub fn list(&self) -> Vec<&Goal> {
        let mut v: Vec<&Goal> = self.goals.values().collect();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        v
    }

    /// 删除
    pub fn delete(&mut self, id: &str) -> bool {
        let removed = self.goals.remove(id).is_some();
        if removed {
            let path = self.storage_dir.join(format!("{}.json", id));
            let _ = std::fs::remove_file(path);
        }
        removed
    }

    /// 持久化
    pub fn save(&self, g: &Goal) {
        let _ = std::fs::create_dir_all(&self.storage_dir);
        let path = self.storage_dir.join(format!("{}.json", g.id));
        if let Ok(text) = serde_json::to_string_pretty(g) {
            let _ = std::fs::write(path, text);
        }
    }

    /// 标记 done（mutate + save）
    pub fn mark_done(
        &mut self,
        id: &str,
        todo_id: &str,
        evidence: Option<String>,
    ) -> Result<(), GoalError> {
        let g = self
            .goals
            .get_mut(id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        g.mark_done(todo_id, evidence)?;
        let g_clone = g.clone();
        self.save(&g_clone);
        Ok(())
    }

    pub fn mark_in_progress(&mut self, id: &str, todo_id: &str) -> Result<(), GoalError> {
        let g = self
            .goals
            .get_mut(id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        g.mark_in_progress(todo_id)?;
        let g_clone = g.clone();
        self.save(&g_clone);
        Ok(())
    }

    pub fn mark_blocked(&mut self, id: &str, todo_id: &str) -> Result<(), GoalError> {
        let g = self
            .goals
            .get_mut(id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        g.mark_blocked(todo_id)?;
        let g_clone = g.clone();
        self.save(&g_clone);
        Ok(())
    }

    pub fn add_todo(&mut self, id: &str, content: &str) -> Result<String, GoalError> {
        let g = self
            .goals
            .get_mut(id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        let todo_id = g.add_todo(content);
        let g_clone = g.clone();
        self.save(&g_clone);
        Ok(todo_id)
    }

    pub fn pause(&mut self, id: &str) -> Result<(), GoalError> {
        let g = self
            .goals
            .get_mut(id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        g.pause();
        let g_clone = g.clone();
        self.save(&g_clone);
        Ok(())
    }

    pub fn resume(&mut self, id: &str) -> Result<(), GoalError> {
        let g = self
            .goals
            .get_mut(id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        g.resume();
        let g_clone = g.clone();
        self.save(&g_clone);
        Ok(())
    }

    pub fn abandon(&mut self, id: &str) -> Result<(), GoalError> {
        let g = self
            .goals
            .get_mut(id)
            .ok_or_else(|| GoalError::TodoNotFound(id.to_string()))?;
        g.abandon();
        let g_clone = g.clone();
        self.save(&g_clone);
        Ok(())
    }
}

pub fn goal_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".agentshell").join("goals")
}

/// 把 active goal 渲染成 system prompt addon
pub fn goal_prompt_addon(g: &Goal) -> String {
    let mut s = format!(
        "[Active Goal: {}]\n{}\n\nTodos ({}, {:.0}% done):\n",
        g.title,
        g.description,
        g.progress_str(),
        g.progress() * 100.0
    );
    for t in &g.todos {
        let icon = match t.status {
            TodoStatus::Pending => "⏳",
            TodoStatus::InProgress => "🔄",
            TodoStatus::Done => "✅",
            TodoStatus::Blocked => "⛔",
        };
        s.push_str(&format!("  {} {} {}\n", icon, t.id, t.content));
    }
    s.push_str(
        "\nWhen you complete a todo, mark it done with evidence (commit hash, file path, etc).\n",
    );
    s.push_str("When blocked, mark Blocked and ask user for clarification.\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goal_lifecycle() {
        let mut r = GoalRegistry::load();
        let g = r.create("thread-1", "Migrate to App Router", "Big task");
        let id = g.id.clone();

        let t1 = r.add_todo(&id, "Update next.config.js").unwrap();
        let t2 = r.add_todo(&id, "Create app/ dir").unwrap();
        let t3 = r.add_todo(&id, "Migrate pages/index.tsx").unwrap();

        r.mark_in_progress(&id, &t1).unwrap();
        r.mark_done(&id, &t1, Some("commit abc123".into())).unwrap();
        r.mark_done(&id, &t2, None).unwrap();
        r.mark_done(&id, &t3, Some("file app/page.tsx".into())).unwrap();

        let g = r.get(&id).unwrap();
        assert_eq!(g.status, GoalStatus::Completed);
        assert_eq!(g.progress(), 1.0);
    }

    #[test]
    fn test_pause_resume() {
        let mut r = GoalRegistry::load();
        let g = r.create("thread-2", "Refactor auth", "Refactor auth flow");
        let id = g.id.clone();
        r.pause(&id).unwrap();
        assert_eq!(r.get(&id).unwrap().status, GoalStatus::Paused);
        r.resume(&id).unwrap();
        assert_eq!(r.get(&id).unwrap().status, GoalStatus::Active);
    }

    #[test]
    fn test_blocked_done_fails() {
        let mut r = GoalRegistry::load();
        let g = r.create("thread-3", "Test", "");
        let id = g.id.clone();
        let t1 = r.add_todo(&id, "Test todo").unwrap();
        r.mark_blocked(&id, &t1).unwrap();
        let r2 = r.mark_done(&id, &t1, None);
        assert!(r2.is_err());
    }

    #[test]
    fn test_progress_str() {
        let mut g = Goal::new("t", "Test", "");
        assert_eq!(g.progress_str(), "0/0");
        g.add_todo("a");
        g.add_todo("b");
        assert_eq!(g.progress_str(), "0/2");
    }
}
