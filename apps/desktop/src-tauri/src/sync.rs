//! v1.5：远程 / 云端 session 同步
//!
//! 简化路径：
//! - Frontend 把 session 打包成 JSON bundle（schema 见 `SessionBundle`）
//! - 通过 `sync_publish(session_id, bundle_json)` 写入本地 cache
//! - 通过 `sync_fetch(session_id)` 读回
//! - 通过 P2P：`P2pHost` / `P2pClient` 已有 `Message::SessionList/Pull/Data`
//!   直接复用，bundle 进 `SessionData::messages`
//!
//! 这个模块只做 4 件事：
//! 1. 在 ~/.agentshell/sync/ 下缓存所有 publish 过的 session
//! 2. 列出 cache（同步状态）
//! 3. 导出 / 导入 bundle（供前端打包用）
//! 4. 提供版本字段（`schema_version`），向前兼容

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const SCHEMA_VERSION: u32 = 1;
const CACHE_DIR: &str = "sync";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBundle {
    pub schema_version: u32,
    pub session_id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub owner_id: Option<String>,
    pub workspace_id: Option<String>,
    pub messages: serde_json::Value,
    pub source_device: String,
    pub synced_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub cached: usize,
    pub total_size: u64,
    pub sessions: Vec<SyncEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEntry {
    pub session_id: String,
    pub title: String,
    pub updated_at: u64,
    pub size: u64,
    pub source: String, // "local" | "p2p" | "import"
}

pub fn sync_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".agentshell").join(CACHE_DIR)
}

fn entry_path(session_id: &str) -> PathBuf {
    let safe = sanitize_id(session_id);
    sync_dir().join(format!("{safe}.json"))
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub fn publish(bundle: SessionBundle) -> Result<(), String> {
    if bundle.session_id.is_empty() {
        return Err("session_id is empty".to_string());
    }
    if bundle.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "schema_version {} 不支持（需要 {}）",
            bundle.schema_version, SCHEMA_VERSION
        ));
    }
    let path = entry_path(&bundle.session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

pub fn fetch(session_id: &str) -> Result<Option<SessionBundle>, String> {
    let path = entry_path(session_id);
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let bundle: SessionBundle = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(Some(bundle))
}

pub fn list() -> Result<SyncStatus, String> {
    let dir = sync_dir();
    if !dir.exists() {
        return Ok(SyncStatus {
            cached: 0,
            total_size: 0,
            sessions: vec![],
        });
    }
    let mut entries = Vec::new();
    let mut total = 0u64;
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let bundle: SessionBundle = match serde_json::from_str(&data) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let size = data.len() as u64;
        total += size;
        entries.push(SyncEntry {
            session_id: bundle.session_id.clone(),
            title: bundle.title.clone(),
            updated_at: bundle.updated_at,
            size,
            source: bundle.source_device.clone(),
        });
    }
    entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(SyncStatus {
        cached: entries.len(),
        total_size: total,
        sessions: entries,
    })
}

pub fn remove(session_id: &str) -> Result<(), String> {
    let path = entry_path(session_id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn clear_all() -> Result<usize, String> {
    let dir = sync_dir();
    if !dir.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_file() {
            std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

/// 把 bundle 转成 Message::SessionData（用于 P2P 转发）
pub fn bundle_to_messages(b: &SessionBundle) -> Vec<serde_json::Value> {
    if let Some(arr) = b.messages.as_array() {
        arr.clone()
    } else {
        vec![b.messages.clone()]
    }
}

pub fn new_bundle(
    session_id: String,
    title: String,
    created_at: u64,
    updated_at: u64,
    owner_id: Option<String>,
    workspace_id: Option<String>,
    messages: serde_json::Value,
    source_device: String,
) -> SessionBundle {
    SessionBundle {
        schema_version: SCHEMA_VERSION,
        session_id,
        title,
        created_at,
        updated_at,
        owner_id,
        workspace_id,
        messages,
        source_device,
        synced_at: now_ms(),
    }
}

pub fn schema_version() -> u32 {
    SCHEMA_VERSION
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bundle(id: &str) -> SessionBundle {
        SessionBundle {
            schema_version: 1,
            session_id: id.to_string(),
            title: "测试".to_string(),
            created_at: 1000,
            updated_at: 2000,
            owner_id: Some("u1".to_string()),
            workspace_id: Some("default".to_string()),
            messages: serde_json::json!([{"role": "user", "text": "hi"}]),
            source_device: "test".to_string(),
            synced_at: now_ms(),
        }
    }

    #[test]
    fn schema_version_constant() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn sanitize_id_basic() {
        assert_eq!(sanitize_id("abc-123_XYZ"), "abc-123_XYZ");
        assert_eq!(sanitize_id("a/b c"), "a_b_c");
        // .. → __  + / → _  → ___etc_passwd
        let s = sanitize_id("../etc/passwd");
        assert!(s.contains("etc"));
        assert!(s.contains("passwd"));
        assert!(!s.contains('/'));
        assert!(!s.contains('.'));
    }

    #[test]
    fn bundle_to_messages_array() {
        let b = make_bundle("s1");
        let m = bundle_to_messages(&b);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn bundle_to_messages_object() {
        let mut b = make_bundle("s1");
        b.messages = serde_json::json!({"role": "user", "text": "hi"});
        let m = bundle_to_messages(&b);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn new_bundle_sets_synced_at() {
        let b = new_bundle(
            "s1".into(),
            "title".into(),
            1,
            2,
            None,
            None,
            serde_json::json!([]),
            "dev".into(),
        );
        assert_eq!(b.schema_version, 1);
        assert!(b.synced_at > 0);
    }

    #[test]
    fn serialize_roundtrip() {
        let b = make_bundle("test-1");
        let s = serde_json::to_string(&b).unwrap();
        let d: SessionBundle = serde_json::from_str(&s).unwrap();
        assert_eq!(d.session_id, b.session_id);
        assert_eq!(d.title, b.title);
    }
}
