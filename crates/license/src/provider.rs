//! LicenseProvider trait + 4 种实现
//!
//! 设计参考：docs/开发文档.md §13.6.2
//!
//! ## 设计原则
//! - 同一 trait，未来切换不重写核心代码
//! - v0.1.0 极简：只做 `ActivationCodeProvider`，其它 3 个占位
//! - 离线优先：本地验证 + 7 天滚动窗口，30 天强制退出

use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::code::{DeviceFingerprint, LicenseCode, LicenseTier};
use crate::storage::{LicenseStorage, StoredLicense};
use crate::verify::{generate_license, verify_code, VerifyError, VerifyResult};

/// License 状态（暴露给前端 / 调用方）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LicenseStatus {
    /// 未激活（首次启动 / 清除后）
    Unactivated,
    /// v1.9.x：3 天免费试用（首次启动后自动开始）
    Trial {
        /// 试用剩余天数（None = 已超过 3 天）
        remaining_days: Option<u32>,
        /// 试用开始时间
        started_at: i64,
    },
    /// 有效
    Valid {
        tier: LicenseTier,
        /// 距到期天数（None = 终身）
        remaining_days: Option<i64>,
        /// 激活时间
        activated_at: i64,
        /// 到期时间（None = 终身）
        expires_at: Option<i64>,
    },
    /// 临期（< 7 天）
    Expiring { tier: LicenseTier, days_left: u32 },
    /// 过期
    Expired { tier: LicenseTier, expired_at: i64 },
    /// 离线宽限期（> 7 天未联网）
    OfflineGrace { days_offline: u32 },
    /// 不可用
    Invalid { reason: String },
}

impl LicenseStatus {
    /// 是否能写文件 / 调生成（受限模式判定）
    pub fn can_write(&self) -> bool {
        matches!(self, Self::Valid { .. } | Self::Expiring { .. } | Self::Trial { .. })
    }

    /// 是否完全正常
    pub fn is_fully_active(&self) -> bool {
        matches!(self, Self::Valid { .. } | Self::Trial { .. })
    }

    /// 降级标题（UI 显示用）
    pub fn banner(&self) -> Option<&'static str> {
        match self {
            Self::Unactivated => Some("未激活 — 部分功能受限"),
            Self::Trial { .. } => Some("免费试用中"),
            Self::Expired { .. } => Some("已过期 — 处于只读模式"),
            Self::OfflineGrace { .. } => Some("离线时间过长 — 处于只读模式"),
            Self::Expiring { .. } => Some("即将到期 — 建议续费"),
            Self::Invalid { .. } => Some("License 异常 — 请重新激活"),
            Self::Valid { .. } => None,
        }
    }
}

/// Provider 错误
#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("code is invalid: {0}")]
    InvalidCode(String),
    #[error("signature verification failed: {0}")]
    BadSignature(#[from] VerifyError),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// LicenseProvider trait（4 种实现共享接口）
#[async_trait::async_trait]
pub trait LicenseProvider: Send + Sync {
    /// provider 名称
    fn name(&self) -> &'static str;

    /// 激活：用户输入码 → 返回 StoredLicense
    async fn activate(
        &self,
        code_str: &str,
        device: &DeviceFingerprint,
    ) -> Result<StoredLicense, LicenseError>;

    /// 校验当前已存储的 license
    async fn validate(&self, stored: &StoredLicense) -> Result<LicenseStatus, LicenseError>;

    /// 离线校验（无网络，本地缓存）
    async fn validate_offline(
        &self,
        stored: &StoredLicense,
        last_validated_at: i64,
    ) -> Result<LicenseStatus, LicenseError>;

    /// 移除 license
    async fn deactivate(&self) -> Result<(), LicenseError> {
        Ok(())
    }
}

/// ActivationCodeProvider（默认，v0.1.0 唯一生产实现）
///
/// - 验证 HMAC 签名（用预共享 demo key；生产从服务端下发的 public key + RSA 验签）
/// - 设备绑定
/// - 写本地存储 `~/.agentshell/license.toml`
pub struct ActivationCodeProvider {
    storage: LicenseStorage,
    secret_key: Vec<u8>,
}

impl ActivationCodeProvider {
    /// 创建 provider
    /// - `storage_path` 通常是 `~/.agentshell/license.toml`
    /// - `secret_key` 默认 demo key；生产从服务端取
    pub fn new(storage_path: impl AsRef<Path>, secret_key: impl Into<Vec<u8>>) -> Self {
        Self {
            storage: LicenseStorage::new(storage_path),
            secret_key: secret_key.into(),
        }
    }

    /// 默认 demo provider（写到 `~/.agentshell/license.toml`，key 为 demo key）
    pub fn default_demo() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let path = PathBuf::from(home).join(".agentshell").join("license.toml");
        Self::new(path, b"agentshell-demo-key-v0.1-DO-NOT-USE-IN-PRODUCTION")
    }

    /// 生成测试 license（开发 / 自用）
    pub fn generate_demo_code(&self, tier: LicenseTier, device: &DeviceFingerprint) -> LicenseCode {
        generate_license(
            tier,
            device.clone(),
            &self.secret_key,
            format!("demo-{}", chrono::Utc::now().timestamp()),
        )
    }
}

#[async_trait::async_trait]
impl LicenseProvider for ActivationCodeProvider {
    fn name(&self) -> &'static str {
        "activation"
    }

    async fn activate(
        &self,
        code_str: &str,
        device: &DeviceFingerprint,
    ) -> Result<StoredLicense, LicenseError> {
        // 1. parse
        let code =
            LicenseCode::from_user_code(code_str).map_err(|e| LicenseError::InvalidCode(e))?;

        // 2. verify
        let result: VerifyResult = verify_code(&code, device, &self.secret_key)?;

        // 3. ensure expires_at set（compute from tier if None）
        let mut stored_code = code;
        if stored_code.payload.expires_at.is_none() {
            stored_code.payload.expires_at = stored_code.payload.compute_expiry();
        }

        // 4. save
        let stored = StoredLicense {
            code: stored_code,
            installed_at: chrono::Utc::now().timestamp(),
            device_id: device.to_id(),
        };
        self.storage
            .save(&stored)
            .map_err(|e| LicenseError::Storage(e.to_string()))?;

        // touch result
        let _ = result;
        Ok(stored)
    }

    async fn validate(&self, stored: &StoredLicense) -> Result<LicenseStatus, LicenseError> {
        let now = chrono::Utc::now();
        let code = &stored.code;
        let tier = code.payload.tier;
        let activated_at = code.payload.activated_at.timestamp();
        let expires_at = code.payload.expires_at.map(|e| e.timestamp());

        let status = match code.payload.is_active(now) {
            false => LicenseStatus::Expired {
                tier,
                expired_at: expires_at.unwrap_or(0),
            },
            true => {
                let remaining = code.payload.remaining_days(now);
                if let Some(days) = remaining {
                    if days < 7 {
                        LicenseStatus::Expiring {
                            tier,
                            days_left: days.max(0) as u32,
                        }
                    } else {
                        LicenseStatus::Valid {
                            tier,
                            remaining_days: Some(days),
                            activated_at,
                            expires_at,
                        }
                    }
                } else {
                    // 终身
                    LicenseStatus::Valid {
                        tier,
                        remaining_days: None,
                        activated_at,
                        expires_at: None,
                    }
                }
            }
        };
        Ok(status)
    }

    async fn validate_offline(
        &self,
        stored: &StoredLicense,
        last_validated_at: i64,
    ) -> Result<LicenseStatus, LicenseError> {
        let now = chrono::Utc::now().timestamp();
        let days_offline = ((now - last_validated_at) / 86400).max(0) as u32;

        if days_offline > 30 {
            return Ok(LicenseStatus::Expired {
                tier: stored.code.payload.tier,
                expired_at: 0,
            });
        }
        if days_offline > 7 {
            return Ok(LicenseStatus::OfflineGrace { days_offline });
        }

        // 7 天内 = 正常本地校验
        self.validate(stored).await
    }

    async fn deactivate(&self) -> Result<(), LicenseError> {
        self.storage
            .clear()
            .map_err(|e| LicenseError::Storage(e.to_string()))?;
        Ok(())
    }
}

/// TrialProvider（v0.3.0+ 占位 — 7 天试用）
pub struct TrialProvider {
    storage: LicenseStorage,
}

impl TrialProvider {
    pub fn new(storage_path: impl AsRef<Path>) -> Self {
        Self {
            storage: LicenseStorage::new(storage_path),
        }
    }
}

#[async_trait::async_trait]
impl LicenseProvider for TrialProvider {
    fn name(&self) -> &'static str {
        "trial"
    }

    async fn activate(
        &self,
        _code: &str,
        _device: &DeviceFingerprint,
    ) -> Result<StoredLicense, LicenseError> {
        // 占位：v0.3.0 实现
        Err(LicenseError::InvalidCode(
            "Trial 暂未启用（v0.3.0+ 上线）".into(),
        ))
    }

    async fn validate(&self, _stored: &StoredLicense) -> Result<LicenseStatus, LicenseError> {
        Ok(LicenseStatus::Unactivated)
    }

    async fn validate_offline(
        &self,
        _stored: &StoredLicense,
        _last_validated_at: i64,
    ) -> Result<LicenseStatus, LicenseError> {
        Ok(LicenseStatus::Unactivated)
    }
}

/// CommunityProvider（v0.4+ 占位 — 开源贡献者永久免费）
pub struct CommunityProvider;

#[async_trait::async_trait]
impl LicenseProvider for CommunityProvider {
    fn name(&self) -> &'static str {
        "community"
    }

    async fn activate(
        &self,
        _code: &str,
        _device: &DeviceFingerprint,
    ) -> Result<StoredLicense, LicenseError> {
        Err(LicenseError::InvalidCode(
            "Community 暂未启用（v0.4+ 上线）".into(),
        ))
    }

    async fn validate(&self, _stored: &StoredLicense) -> Result<LicenseStatus, LicenseError> {
        Ok(LicenseStatus::Valid {
            tier: LicenseTier::Lifetime,
            remaining_days: None,
            activated_at: 0,
            expires_at: None,
        })
    }

    async fn validate_offline(
        &self,
        _stored: &StoredLicense,
        _last_validated_at: i64,
    ) -> Result<LicenseStatus, LicenseError> {
        Ok(LicenseStatus::Valid {
            tier: LicenseTier::Lifetime,
            remaining_days: None,
            activated_at: 0,
            expires_at: None,
        })
    }
}

/// EnterpriseProvider（v1.0+ 占位 — 企业 SAML/OIDC）
pub struct EnterpriseProvider;

#[async_trait::async_trait]
impl LicenseProvider for EnterpriseProvider {
    fn name(&self) -> &'static str {
        "enterprise"
    }

    async fn activate(
        &self,
        _code: &str,
        _device: &DeviceFingerprint,
    ) -> Result<StoredLicense, LicenseError> {
        Err(LicenseError::InvalidCode(
            "Enterprise 暂未启用（v1.0+ 上线）".into(),
        ))
    }

    async fn validate(&self, _stored: &StoredLicense) -> Result<LicenseStatus, LicenseError> {
        Ok(LicenseStatus::Unactivated)
    }

    async fn validate_offline(
        &self,
        _stored: &StoredLicense,
        _last_validated_at: i64,
    ) -> Result<LicenseStatus, LicenseError> {
        Ok(LicenseStatus::Unactivated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_can_write() {
        assert!(!LicenseStatus::Unactivated.can_write());
        assert!(!LicenseStatus::Expired {
            tier: LicenseTier::Monthly,
            expired_at: 0
        }
        .can_write());
        assert!(LicenseStatus::Valid {
            tier: LicenseTier::Yearly,
            remaining_days: Some(100),
            activated_at: 0,
            expires_at: None,
        }
        .can_write());
    }

    #[test]
    fn test_status_banner() {
        assert_eq!(
            LicenseStatus::Unactivated.banner(),
            Some("未激活 — 部分功能受限")
        );
        assert_eq!(
            LicenseStatus::Valid {
                tier: LicenseTier::Yearly,
                remaining_days: Some(100),
                activated_at: 0,
                expires_at: None,
            }
            .banner(),
            None
        );
    }
}
