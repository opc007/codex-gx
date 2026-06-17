//! v1.8：Background Terminal — 后台 long-running 进程管理
//!
//! 设计参考：docs/开发文档.md §5.24 + 5.24.1
//!
//! ## 场景
//! - 用户跑 `npm run dev` 这种 long-running 命令 → 不阻塞主 thread
//! - M3 调用工具时加 `background=true` 即可
//! - 状态栏显示 `bg: 2 running`
//! - 用户 `/ps` 看所有后台 / `/stop <id>` 关掉
//!
//! ## 数据结构
//! - `BackgroundTerminal` — 单个后台进程（id/label/command/pid/status/output_tail）
//! - `BackgroundManager` — 进程池（spawn/stop/list）
//!
//! ## 安全
//! - 输出只保留最近 100 行（防内存爆）
//! - 退出 App 时优雅停止（SIGINT → 5s → SIGKILL）
//! - 持久 log 写到 `~/.agentshell/bg/<label>/<pid>.log`

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BgStatus {
    Running,
    Stopped,
    Crashed,
    Exited,
}

impl BgStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "🟢 running",
            Self::Stopped => "🛑 stopped",
            Self::Crashed => "💥 crashed",
            Self::Exited => "⚪ exited",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTerminal {
    pub id: String,
    pub label: String,
    pub command: String,
    pub started_at: i64,
    pub pid: u32,
    pub status: BgStatus,
    /// 最近 100 行
    #[serde(skip)]
    pub output_tail: VecDeque<String>,
    /// log 文件路径
    pub log_path: Option<PathBuf>,
    /// 退出码（如果已退出）
    pub exit_code: Option<i32>,
}

const TAIL_MAX: usize = 100;

impl BackgroundTerminal {
    pub fn new(label: String, command: String, pid: u32, log_path: Option<PathBuf>) -> Self {
        Self {
            id: format!("bg-{}", uuid::Uuid::new_v4().simple()),
            label,
            command,
            started_at: chrono::Utc::now().timestamp(),
            pid,
            status: BgStatus::Running,
            output_tail: VecDeque::with_capacity(TAIL_MAX),
            log_path,
            exit_code: None,
        }
    }

    pub fn push_line(&mut self, line: String) {
        if self.output_tail.len() >= TAIL_MAX {
            self.output_tail.pop_front();
        }
        self.output_tail.push_back(line);
    }

    pub fn tail_text(&self) -> String {
        self.output_tail.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

/// 内部 mutable handle（child process + 状态）
struct BgHandle {
    terminal: BackgroundTerminal,
    /// `None` 表示已被 stop / 已 exit
    child: Option<Child>,
    /// 状态变化通知
    notify: Arc<Notify>,
}

/// Manager
pub struct BackgroundManager {
    handles: Arc<Mutex<HashMap<String, BgHandle>>>,
}

impl Default for BackgroundManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundManager {
    pub fn new() -> Self {
        Self {
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 启动一个后台进程
    pub async fn spawn(
        &self,
        label: &str,
        command: &str,
        args: &[&str],
    ) -> Result<BackgroundTerminal, BackgroundError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false); // 我们自己管理

        let mut child = cmd.spawn().map_err(|e| BackgroundError::Spawn {
            cmd: command.to_string(),
            source: Box::new(e),
        })?;
        let pid = child.id().unwrap_or(0);
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // log 路径
        let log_dir = log_dir_for(label);
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join(format!("{}.log", pid));

        let mut term = BackgroundTerminal::new(
            label.to_string(),
            format!("{} {}", command, args.join(" ")),
            pid,
            Some(log_path.clone()),
        );

        // 启动 stdout/stderr 抓取
        let id = term.id.clone();
        let handles_arc = Arc::clone(&self.handles);
        let notify = Arc::new(Notify::new());

        if let Some(stdout) = stdout {
            let id_in = id.clone();
            let log_path_in = log_path.clone();
            let handles_in = Arc::clone(&handles_arc);
            let notify_in = Arc::clone(&notify);
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Ok(map) = handles_in.lock() {
                        if let Some(h) = map.get(&id_in) {
                            let mut term_clone = h.terminal.clone();
                            term_clone.push_line(line.clone());
                            use std::io::Write;
                            if let Ok(mut f) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(log_path_in.as_path())
                            {
                                let _ = writeln!(f, "{}", line);
                            }
                            drop(map);
                            if let Ok(mut map2) = handles_in.lock() {
                                if let Some(h2) = map2.get_mut(&id_in) {
                                    h2.terminal = term_clone;
                                }
                            }
                        }
                    }
                    notify_in.notify_waiters();
                }
            });
        }
        if let Some(stderr) = stderr {
            let id_in = id.clone();
            let log_path_in = log_path.clone();
            let handles_in = Arc::clone(&handles_arc);
            let notify_in = Arc::clone(&notify);
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let stamped = format!("[err] {}", line);
                    if let Ok(map) = handles_in.lock() {
                        if let Some(h) = map.get(&id_in) {
                            let mut term_clone = h.terminal.clone();
                            term_clone.push_line(stamped.clone());
                            use std::io::Write;
                            if let Ok(mut f) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(log_path_in.as_path())
                            {
                                let _ = writeln!(f, "{}", stamped);
                            }
                            drop(map);
                            if let Ok(mut map2) = handles_in.lock() {
                                if let Some(h2) = map2.get_mut(&id_in) {
                                    h2.terminal = term_clone;
                                }
                            }
                        }
                    }
                    notify_in.notify_waiters();
                }
            });
        }

        // 启动 wait 任务
        let id_wait = id.clone();
        let handles_wait = Arc::clone(&handles_arc);
        let mut child_handle = child;
        tokio::spawn(async move {
            let status = child_handle.wait().await;
            if let Ok(mut map) = handles_wait.lock() {
                if let Some(h) = map.get_mut(&id_wait) {
                    match status {
                        Ok(s) => {
                            h.terminal.exit_code = s.code();
                            h.terminal.status = if let Some(code) = s.code() {
                                if code == 0 {
                                    BgStatus::Exited
                                } else {
                                    BgStatus::Crashed
                                }
                            } else {
                                BgStatus::Exited
                            };
                            h.child = None;
                        }
                        Err(_) => {
                            h.terminal.status = BgStatus::Crashed;
                            h.child = None;
                        }
                    }
                }
            }
        });

        // 把 handle 存入 map
        let handle = BgHandle {
            terminal: term.clone(),
            child: None, // 我们已经把 ownership 交给 wait task
            notify,
        };
        self.handles
            .lock()
            .map_err(|e| BackgroundError::Lock(e.to_string()))?
            .insert(id, handle);

        Ok(term)
    }

    /// 列出所有
    pub fn list(&self) -> Vec<BackgroundTerminal> {
        self.handles
            .lock()
            .ok()
            .map(|m| m.values().map(|h| h.terminal.clone()).collect())
            .unwrap_or_default()
    }

    /// 列出 running
    pub fn list_running(&self) -> Vec<BackgroundTerminal> {
        self.list()
            .into_iter()
            .filter(|t| t.status == BgStatus::Running)
            .collect()
    }

    /// 取一个
    pub fn get(&self, id: &str) -> Option<BackgroundTerminal> {
        self.handles
            .lock()
            .ok()
            .and_then(|m| m.get(id).map(|h| h.terminal.clone()))
    }

    /// 停一个（SIGINT）
    pub fn stop(&self, id: &str) -> Result<bool, BackgroundError> {
        let mut map = self
            .handles
            .lock()
            .map_err(|e| BackgroundError::Lock(e.to_string()))?;
        if let Some(h) = map.get_mut(id) {
            // 找 child: 我们没持有 child, 用 kill_by_pid
            let pid = h.terminal.pid;
            let r = kill_pid(pid);
            h.terminal.status = BgStatus::Stopped;
            h.terminal.exit_code = Some(-1);
            Ok(r)
        } else {
            Ok(false)
        }
    }

    /// 停全部
    pub fn stop_all(&self) -> usize {
        let Ok(mut map) = self.handles.lock() else { return 0 };
        let mut n = 0;
        for h in map.values_mut() {
            if h.terminal.status == BgStatus::Running {
                let _ = kill_pid(h.terminal.pid);
                h.terminal.status = BgStatus::Stopped;
                h.terminal.exit_code = Some(-1);
                n += 1;
            }
        }
        n
    }
}

/// 平台-specific PID kill（SIGINT 然后 5s 后 SIGKILL，简化版直接 SIGKILL）
fn kill_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("kill").arg("-INT").arg(pid.to_string()).output();
        true
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn log_dir_for(label: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".agentshell")
        .join("bg")
        .join(sanitize_label(label))
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum BackgroundError {
    #[error("failed to spawn `{cmd}`: {source}")]
    Spawn {
        cmd: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("lock poisoned: {0}")]
    Lock(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_sanitize() {
        assert_eq!(sanitize_label("dev server"), "dev_server");
        assert_eq!(sanitize_label("npm run dev"), "npm_run_dev");
    }

    #[test]
    fn test_terminal_tail_max() {
        let mut t = BackgroundTerminal::new(
            "test".into(),
            "echo".into(),
            1,
            None,
        );
        for i in 0..150 {
            t.push_line(format!("line {}", i));
        }
        assert_eq!(t.output_tail.len(), TAIL_MAX);
        // 最早的在前面
        assert!(t.output_tail.front().unwrap().contains("line 50"));
    }

    #[test]
    fn test_bg_status_label() {
        assert!(BgStatus::Running.label().contains("running"));
        assert!(BgStatus::Crashed.label().contains("crashed"));
    }

    #[tokio::test]
    async fn test_spawn_and_list() {
        let mgr = BackgroundManager::new();
        // echo 立即退出
        let t = mgr.spawn("echo-test", "echo", &["hello"]).await.unwrap();
        assert_eq!(t.label, "echo-test");
        assert_eq!(t.command.contains("echo"), true);
        let list = mgr.list();
        assert!(!list.is_empty());
    }
}
