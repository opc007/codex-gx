// v1.2：Vault Tauri commands
//
// - vault_is_encrypted     检查 session 是否已加密
// - vault_list_encrypted   列出所有加密 session
// - vault_encrypt_session  加密 session（输入 JSON 字符串 + 密码）
// - vault_decrypt_session  解密 session（返回 JSON 字符串）
// - vault_remove_session   移除加密文件

use crate::VaultState;
use serde::{Deserialize, Serialize};

#[tauri::command]
pub async fn vault_is_encrypted(
    state: tauri::State<'_, VaultState>,
    session_id: String,
) -> Result<bool, String> {
    let v = state.inner().lock().await;
    Ok(v.is_encrypted(&session_id))
}

#[derive(Debug, Serialize)]
pub struct EncryptedSessionInfo {
    pub session_id: String,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[tauri::command]
pub async fn vault_list_encrypted(
    state: tauri::State<'_, VaultState>,
) -> Result<Vec<EncryptedSessionInfo>, String> {
    let v = state.inner().lock().await;
    let ids = v.list_encrypted().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for id in ids {
        if let Ok(vf) = v.read_file(&id) {
            out.push(EncryptedSessionInfo {
                session_id: id,
                created_at: vf.created_at,
                updated_at: vf.updated_at,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct VaultEncryptArgs {
    pub session_id: String,
    pub plaintext: String, // JSON 字符串（PersistedMessage[]）
    pub password: String,
}

#[tauri::command]
pub async fn vault_encrypt_session(
    state: tauri::State<'_, VaultState>,
    args: VaultEncryptArgs,
) -> Result<String, String> {
    let v = state.inner().lock().await;
    let vf = v
        .encrypt(&args.session_id, args.plaintext.as_bytes(), &args.password)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&vf).unwrap_or_default())
}

#[derive(Debug, Deserialize)]
pub struct VaultDecryptArgs {
    pub session_id: String,
    pub password: String,
}

#[tauri::command]
pub async fn vault_decrypt_session(
    state: tauri::State<'_, VaultState>,
    args: VaultDecryptArgs,
) -> Result<String, String> {
    let v = state.inner().lock().await;
    let bytes = v
        .decrypt(&args.session_id, &args.password)
        .map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| format!("非 UTF-8: {}", e))
}

#[derive(Debug, Deserialize)]
pub struct VaultRemoveArgs {
    pub session_id: String,
}

#[tauri::command]
pub async fn vault_remove_session(
    state: tauri::State<'_, VaultState>,
    args: VaultRemoveArgs,
) -> Result<(), String> {
    let v = state.inner().lock().await;
    v.remove(&args.session_id).map_err(|e| e.to_string())
}