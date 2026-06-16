//! License 本地存储

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::code::LicenseCode;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("deserialize error: {0}")]
    Deserialize(#[from] toml::de::Error),
}

/// 已存储的 License
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLicense {
    /// License 码
    pub code: LicenseCode,
    /// 激活时间戳（本地）
    pub installed_at: i64,
    /// 设备 ID（防止移动 license）
    pub device_id: String,
}

/// License 存储
pub struct LicenseStorage {
    path: std::path::PathBuf,
}

impl LicenseStorage {
    /// 创建存储（指向 ~/.agentshell/license.toml）
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// 读取（如果存在）
    pub fn load(&self) -> Result<Option<StoredLicense>, StorageError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&self.path)?;
        let stored: StoredLicense = toml::from_str(&text)?;
        Ok(Some(stored))
    }

    /// 保存
    pub fn save(&self, license: &StoredLicense) -> Result<(), StorageError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(license)?;
        std::fs::write(&self.path, text)?;
        Ok(())
    }

    /// 清除
    pub fn clear(&self) -> Result<(), StorageError> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    /// 路径
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::DeviceFingerprint;
    use crate::verify::generate_license;
    use crate::code::LicenseTier;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.toml");
        let storage = LicenseStorage::new(&path);

        let key = b"test-key";
        let device = DeviceFingerprint::current();
        let code = generate_license(LicenseTier::Monthly, device.clone(), key, "test");
        let stored = StoredLicense {
            code,
            installed_at: 1234567890,
            device_id: device.to_id(),
        };
        storage.save(&stored).unwrap();

        let loaded = storage.load().unwrap().unwrap();
        assert_eq!(loaded.code.payload.issue_id, "test");
        assert_eq!(loaded.installed_at, 1234567890);
    }

    #[test]
    fn test_clear() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.toml");
        let storage = LicenseStorage::new(&path);
        storage.clear().unwrap();
        assert!(storage.load().unwrap().is_none());
    }
}