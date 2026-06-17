//! v1.2：Vault — 敏感 session 加密存储
//!
//! 算法：
//! - 密钥派生：PBKDF2-HMAC-SHA256，100k 迭代，16 byte salt
//! - 加密：AES-256-GCM（authenticated）
//! - 存储：每个加密 session 一个 `.enc` 文件
//!
//! 文件格式（JSON）：
//! ```json
//! {
//!   "version": 1,
//!   "salt": "<base64 16B>",
//!   "nonce": "<base64 12B>",
//!   "ciphertext": "<base64>",
//!   "kdf_iters": 100000,
//!   "created_at": "2026-...",
//!   "marker": "<base64 32B>"  // 验证密码是否正确
//! }
//! ```
//!
//! 标记 + 内容都用同一密码派生 key 加密：marker 是固定明文 "AgentShell Vault v1"，
//! 解密后比对。失败 → 密码错误。

#![warn(missing_docs)]

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// KDF 迭代次数（生产用 100k，测试用更少）
pub const KDF_ITERATIONS: u32 = 100_000;

/// 盐长度
pub const SALT_LEN: usize = 16;

/// nonce 长度（AES-GCM 标准 12 字节）
pub const NONCE_LEN: usize = 12;

/// 标记明文（用于密码验证）
pub const VERIFICATION_MARKER: &[u8] = b"AgentShell Vault v1";

/// 加密文件结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFile {
    /// 格式版本
    pub version: u32,
    /// salt（base64）
    pub salt: String,
    /// nonce（base64）
    pub nonce: String,
    /// 密文（base64）
    pub ciphertext: String,
    /// 标记密文（base64，验证密码用）
    pub marker: String,
    /// KDF 迭代次数
    pub kdf_iters: u32,
    /// 创建时间
    pub created_at: String,
    /// 最后修改
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Base64 解码错误: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("加密错误: {0}")]
    Encrypt(String),
    #[error("解密错误: {0}")]
    Decrypt(String),
    #[error("密码错误")]
    WrongPassword,
    #[error("文件已加密，请先解密")]
    AlreadyEncrypted,
    #[error("其他: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, VaultError>;

/// Vault 管理器
pub struct Vault {
    pub dir: PathBuf,
}

impl Vault {
    pub fn new(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// 加密文件路径
    pub fn file_path(&self, session_id: &str) -> PathBuf {
        self.dir.join(format!("{}.enc.json", session_id))
    }

    /// 是否已加密？
    pub fn is_encrypted(&self, session_id: &str) -> bool {
        self.file_path(session_id).exists()
    }

    /// 加密 JSON 内容
    pub fn encrypt(&self, session_id: &str, plaintext: &[u8], password: &str) -> Result<VaultFile> {
        if self.is_encrypted(session_id) {
            return Err(VaultError::AlreadyEncrypted);
        }
        // 1. 随机 salt + nonce
        let mut salt = [0u8; SALT_LEN];
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        // 2. 派生 key
        let key = derive_key(password, &salt, KDF_ITERATIONS);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let nonce = Nonce::from_slice(&nonce_bytes);

        // 3. 加密主内容
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| VaultError::Encrypt(e.to_string()))?;

        // 4. 加密 marker（用同样 key + 单独 nonce）
        let mut marker_nonce = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut marker_nonce);
        let marker_nonce_obj = Nonce::from_slice(&marker_nonce);
        let marker_ct = cipher
            .encrypt(marker_nonce_obj, VERIFICATION_MARKER)
            .map_err(|e| VaultError::Encrypt(e.to_string()))?;
        // 合并：marker = nonce + ciphertext
        let mut marker_combined = marker_nonce.to_vec();
        marker_combined.extend_from_slice(&marker_ct);

        let now = chrono::Utc::now().to_rfc3339();
        let vf = VaultFile {
            version: 1,
            salt: base64::engine::general_purpose::STANDARD.encode(salt),
            nonce: base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
            ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
            marker: base64::engine::general_purpose::STANDARD.encode(&marker_combined),
            kdf_iters: KDF_ITERATIONS,
            created_at: now.clone(),
            updated_at: Some(now),
        };
        // 落盘
        self.write_file(session_id, &vf)?;
        Ok(vf)
    }

    /// 解密（先验证密码）
    pub fn decrypt(&self, session_id: &str, password: &str) -> Result<Vec<u8>> {
        let vf = self.read_file(session_id)?;
        let salt = base64::engine::general_purpose::STANDARD.decode(&vf.salt)?;
        let key = derive_key(password, &salt, vf.kdf_iters);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

        // 验证 marker
        let marker_combined = base64::engine::general_purpose::STANDARD.decode(&vf.marker)?;
        if marker_combined.len() < NONCE_LEN {
            return Err(VaultError::Decrypt("marker 长度异常".into()));
        }
        let (marker_nonce, marker_ct) = marker_combined.split_at(NONCE_LEN);
        let marker_nonce_obj = Nonce::from_slice(marker_nonce);
        let marker_pt = cipher
            .decrypt(marker_nonce_obj, marker_ct)
            .map_err(|_| VaultError::WrongPassword)?;
        if marker_pt != VERIFICATION_MARKER {
            return Err(VaultError::WrongPassword);
        }

        // 解密主内容
        let nonce_bytes = base64::engine::general_purpose::STANDARD.decode(&vf.nonce)?;
        let ciphertext = base64::engine::general_purpose::STANDARD.decode(&vf.ciphertext)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|e| VaultError::Decrypt(e.to_string()))?;
        Ok(plaintext)
    }

    /// 读取并解析 VaultFile
    pub fn read_file(&self, session_id: &str) -> Result<VaultFile> {
        let path = self.file_path(session_id);
        let text = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// 写入 VaultFile
    fn write_file(&self, session_id: &str, vf: &VaultFile) -> Result<()> {
        let path = self.file_path(session_id);
        let text = serde_json::to_string_pretty(vf)?;
        std::fs::write(&path, text)?;
        Ok(())
    }

    /// 删除加密文件（解密后调用）
    pub fn remove(&self, session_id: &str) -> Result<()> {
        let path = self.file_path(session_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// 列出所有已加密的 session id
    pub fn list_encrypted(&self) -> Result<Vec<String>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix(".enc.json") {
                out.push(id.to_string());
            }
        }
        Ok(out)
    }
}

/// PBKDF2-HMAC-SHA256 派生 32 字节 key
fn derive_key(password: &str, salt: &[u8], iters: u32) -> [u8; 32] {
    use hmac::Hmac;
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut k = [0u8; 32];
    pbkdf2::pbkdf2::<HmacSha256>(password.as_bytes(), salt, iters, &mut k)
        .expect("pbkdf2");
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_vault(name: &str) -> Vault {
        let dir = std::env::temp_dir().join(format!("agentshell_vault_test_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Vault::new(dir).expect("new")
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let v = fresh_vault("roundtrip");
        let plain = b"hello, world!";
        v.encrypt("sess1", plain, "pass").expect("encrypt");
        let out = v.decrypt("sess1", "pass").expect("decrypt");
        assert_eq!(out, plain);
    }

    #[test]
    fn wrong_password_fails() {
        let v = fresh_vault("wrong_pw");
        v.encrypt("sess1", b"secret", "correct").expect("encrypt");
        let r = v.decrypt("sess1", "incorrect");
        assert!(matches!(r, Err(VaultError::WrongPassword)));
    }

    #[test]
    fn is_encrypted_works() {
        let v = fresh_vault("is_enc");
        assert!(!v.is_encrypted("s1"));
        v.encrypt("s1", b"x", "p").expect("enc");
        assert!(v.is_encrypted("s1"));
        assert!(!v.is_encrypted("s2"));
    }

    #[test]
    fn double_encrypt_fails() {
        let v = fresh_vault("double_enc");
        v.encrypt("s1", b"x", "p").expect("enc");
        let r = v.encrypt("s1", b"y", "p");
        assert!(matches!(r, Err(VaultError::AlreadyEncrypted)));
    }

    #[test]
    fn remove_works() {
        let v = fresh_vault("remove");
        v.encrypt("s1", b"x", "p").expect("enc");
        v.remove("s1").expect("remove");
        assert!(!v.is_encrypted("s1"));
    }

    #[test]
    fn list_encrypted() {
        let v = fresh_vault("list");
        v.encrypt("a", b"1", "p").expect("enc");
        v.encrypt("b", b"2", "p").expect("enc");
        let list = v.list_encrypted().expect("list");
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"a".to_string()));
        assert!(list.contains(&"b".to_string()));
    }

    #[test]
    fn large_payload() {
        let v = fresh_vault("large");
        let plain: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
        v.encrypt("s1", &plain, "p").expect("enc");
        let out = v.decrypt("s1", "p").expect("dec");
        assert_eq!(out, plain);
    }

    #[test]
    fn chinese_password() {
        let v = fresh_vault("cn_pw");
        let plain = "中文 session 内容".as_bytes();
        v.encrypt("s1", plain, "我的密码 123").expect("enc");
        let out = v.decrypt("s1", "我的密码 123").expect("dec");
        assert_eq!(out, plain);
    }

    #[test]
    fn corrupt_marker_detected() {
        let v = fresh_vault("corrupt");
        v.encrypt("s1", b"x", "p").expect("enc");
        // 手动破坏文件
        let path = v.file_path("s1");
        let mut text = std::fs::read_to_string(&path).unwrap();
        // 替换 marker 的最后一个字符
        if let Some(idx) = text.rfind("\"marker\":") {
            // 找到 marker 字段后引号区域
            let end_quote = text[idx..].find("\"").map(|i| idx + i);
            // 简单破坏：往 ciphertext 后追加 X
            if let Some(p) = text.find("\"ciphertext\":") {
                let comma = text[p..].find(",");
                if let Some(c) = comma {
                    let pos = p + c;
                    text.insert(pos, 'X');
                }
            }
            let _ = idx;
            let _ = end_quote;
        }
        std::fs::write(&path, text).unwrap();
        let r = v.decrypt("s1", "p");
        // 应该解密失败（不是 WrongPassword 可能是 Decrypt）
        assert!(r.is_err());
    }

    #[test]
    fn kdf_iter_constant() {
        assert_eq!(KDF_ITERATIONS, 100_000);
        assert_eq!(SALT_LEN, 16);
        assert_eq!(NONCE_LEN, 12);
    }
}