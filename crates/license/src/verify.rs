//! License 验证

use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::code::{DeviceFingerprint, LicenseCode, LicensePayload};

type HmacSha256 = Hmac<Sha256>;

/// 验证结果
#[derive(Debug, Clone)]
pub enum VerifyResult {
    /// 有效
    Valid {
        /// 距离到期天数（None = 终身）
        remaining_days: Option<i64>,
        /// 档位
        tier: String,
    },
}

/// 验证错误
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("signature mismatch")]
    BadSignature,
    #[error("device mismatch: bound to `{bound}`, current is `{current}`")]
    DeviceMismatch { bound: String, current: String },
    #[error("expired on {0}")]
    Expired(String),
    #[error("invalid code: {0}")]
    Invalid(String),
}

/// 验证 License 码
///
/// 签名密钥：v0.1 用 demo key（生产从服务端下发的 public key + RSA 验签）
pub fn verify_code(
    code: &LicenseCode,
    current_device: &DeviceFingerprint,
    secret_key: &[u8],
) -> Result<VerifyResult, VerifyError> {
    // 1. 签名验证
    let payload_bytes =
        serde_json::to_vec(&code.payload).map_err(|e| VerifyError::Invalid(e.to_string()))?;
    let mut mac =
        HmacSha256::new_from_slice(secret_key).map_err(|e| VerifyError::Invalid(e.to_string()))?;
    mac.update(&payload_bytes);
    let expected = mac.finalize().into_bytes();
    let provided =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &code.signature)
            .map_err(|e| VerifyError::Invalid(e.to_string()))?;
    if expected.as_slice() != provided.as_slice() {
        return Err(VerifyError::BadSignature);
    }

    // 2. 设备绑定
    if code.payload.device.to_id() != current_device.to_id() {
        return Err(VerifyError::DeviceMismatch {
            bound: code.payload.device.to_id(),
            current: current_device.to_id(),
        });
    }

    // 3. 到期检查
    let now = Utc::now();
    if let Some(exp) = code.payload.expires_at {
        if now >= exp {
            return Err(VerifyError::Expired(exp.to_string()));
        }
    }

    Ok(VerifyResult::Valid {
        remaining_days: code.payload.remaining_days(now),
        tier: format!("{:?}", code.payload.tier),
    })
}

/// 生成 License（v0.1 demo 用，生产从服务端生成）
pub fn generate_license(
    tier: crate::code::LicenseTier,
    device: DeviceFingerprint,
    secret_key: &[u8],
    issue_id: impl Into<String>,
) -> LicenseCode {
    let now = Utc::now();
    let payload = LicensePayload {
        tier,
        activated_at: now,
        expires_at: tier
            .duration_days()
            .map(|d| now + chrono::Duration::days(d)),
        device,
        issue_id: issue_id.into(),
        note: None,
    };
    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    let mut mac = HmacSha256::new_from_slice(secret_key).unwrap();
    mac.update(&payload_bytes);
    let sig = mac.finalize().into_bytes();
    let signature = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sig);
    LicenseCode { payload, signature }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::LicenseTier;

    #[test]
    fn test_generate_and_verify() {
        let key = b"test-secret-key-v0.1";
        let device = DeviceFingerprint::current();
        let code = generate_license(LicenseTier::Yearly, device.clone(), key, "test-001");

        let result = verify_code(&code, &device, key);
        assert!(result.is_ok());
        let v = result.unwrap();
        match v {
            VerifyResult::Valid { remaining_days, .. } => {
                assert!(remaining_days.is_some());
                assert!(remaining_days.unwrap() > 300);
            }
        }
    }

    #[test]
    fn test_bad_signature() {
        let key = b"test-secret-key";
        let wrong_key = b"wrong-key";
        let device = DeviceFingerprint::current();
        let code = generate_license(LicenseTier::Monthly, device.clone(), key, "test");
        let r = verify_code(&code, &device, wrong_key);
        assert!(matches!(r, Err(VerifyError::BadSignature)));
    }

    #[test]
    fn test_device_mismatch() {
        let key = b"test-secret-key";
        let d1 = DeviceFingerprint::current();
        let d2 = DeviceFingerprint {
            os: "fake".into(),
            hostname: "fake".into(),
            mac_hash: "fake".into(),
            disk_serial: None,
        };
        let code = generate_license(LicenseTier::Monthly, d1, key, "test");
        let r = verify_code(&code, &d2, key);
        assert!(matches!(r, Err(VerifyError::DeviceMismatch { .. })));
    }

    #[test]
    fn test_roundtrip_code() {
        let key = b"test-key";
        let device = DeviceFingerprint::current();
        let code = generate_license(LicenseTier::Lifetime, device, key, "lt-001");
        let s = code.to_user_code().unwrap();
        let parsed = LicenseCode::from_user_code(&s).unwrap();
        assert_eq!(parsed.payload.issue_id, "lt-001");
    }
}
