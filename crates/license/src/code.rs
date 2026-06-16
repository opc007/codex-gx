//! License 码格式

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// License 档位
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LicenseTier {
    /// 月卡 ¥9.9
    Monthly,
    /// 季卡 ¥29.9
    Quarterly,
    /// 年卡 ¥99
    Yearly,
    /// 终身 ¥299
    Lifetime,
}

impl LicenseTier {
    pub fn duration_days(&self) -> Option<i64> {
        match self {
            Self::Monthly => Some(30),
            Self::Quarterly => Some(90),
            Self::Yearly => Some(365),
            Self::Lifetime => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Monthly => "月卡 ¥9.9",
            Self::Quarterly => "季卡 ¥29.9",
            Self::Yearly => "年卡 ¥99",
            Self::Lifetime => "终身 ¥299",
        }
    }
}

/// 设备指纹
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceFingerprint {
    /// OS（macos / windows / linux）
    pub os: String,
    /// 主机名
    pub hostname: String,
    /// MAC 地址 hash
    pub mac_hash: String,
    /// 磁盘序列号（可选）
    #[serde(default)]
    pub disk_serial: Option<String>,
}

impl DeviceFingerprint {
    /// 当前机器指纹
    pub fn current() -> Self {
        let os = if cfg!(target_os = "macos") {
            "macos".to_string()
        } else if cfg!(target_os = "windows") {
            "windows".to_string()
        } else {
            "linux".to_string()
        };
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown".into());
        // MAC hash 简化：实际会用 getifaddrs
        let mac_hash = format!("{:x}", md5_like_hash(hostname.as_bytes()));
        Self {
            os,
            hostname,
            mac_hash,
            disk_serial: None,
        }
    }

    /// 转字符串
    pub fn to_id(&self) -> String {
        format!("{}|{}|{}", self.os, self.hostname, self.mac_hash)
    }
}

fn md5_like_hash(data: &[u8]) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    let result = h.finalize();
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&result[..8]);
    u64::from_le_bytes(buf)
}

/// License payload（码里包含的数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    /// 档位
    pub tier: LicenseTier,
    /// 激活时间
    pub activated_at: DateTime<Utc>,
    /// 到期时间（None = 终身）
    pub expires_at: Option<DateTime<Utc>>,
    /// 绑定的设备指纹
    pub device: DeviceFingerprint,
    /// 签发 ID（防重放）
    pub issue_id: String,
    /// 备注
    #[serde(default)]
    pub note: Option<String>,
}

impl LicensePayload {
    /// 计算到期时间（从 activated_at 加上 duration）
    pub fn compute_expiry(&self) -> Option<DateTime<Utc>> {
        match self.tier.duration_days() {
            Some(days) => Some(self.activated_at + chrono::Duration::days(days)),
            None => None,
        }
    }

    /// 是否还在有效期内
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(exp) => now < exp,
            None => true, // 终身
        }
    }

    /// 剩余天数（None = 终身）
    pub fn remaining_days(&self, now: DateTime<Utc>) -> Option<i64> {
        self.expires_at.map(|exp| (exp - now).num_days())
    }
}

/// License 码（payload + 签名）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseCode {
    /// payload（明文 JSON）
    pub payload: LicensePayload,
    /// HMAC 签名
    pub signature: String,
}

impl LicenseCode {
    /// 用户输入的码字符串（Base64 编码的 JSON）
    pub fn from_user_code(user_code: &str) -> Result<Self, String> {
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            user_code.trim(),
        )
        .map_err(|e| format!("base64 decode: {}", e))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("json parse: {}", e))
    }

    /// 转用户输入的码字符串
    pub fn to_user_code(&self) -> Result<String, String> {
        let json = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            json,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_duration() {
        assert_eq!(LicenseTier::Monthly.duration_days(), Some(30));
        assert_eq!(LicenseTier::Quarterly.duration_days(), Some(90));
        assert_eq!(LicenseTier::Yearly.duration_days(), Some(365));
        assert_eq!(LicenseTier::Lifetime.duration_days(), None);
    }

    #[test]
    fn test_current_device() {
        let d = DeviceFingerprint::current();
        assert!(!d.os.is_empty());
        assert!(!d.hostname.is_empty());
    }

    #[test]
    fn test_payload_expiry() {
        let p = LicensePayload {
            tier: LicenseTier::Monthly,
            activated_at: Utc::now(),
            expires_at: None,
            device: DeviceFingerprint::current(),
            issue_id: "test".into(),
            note: None,
        };
        let exp = p.compute_expiry().unwrap();
        assert!(exp > p.activated_at);
    }
}