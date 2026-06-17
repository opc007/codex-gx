//! AgentShell License 激活码
//!
//! 设计参考：docs/开发文档.md §13.6 License 商业化
//!
//! ## 规则
//! - 月卡 ¥9.9 / 季卡 ¥29.9 / 年卡 ¥99 / 终身 ¥299
//! - 一机一码（绑定 device_id）
//! - 一次性购买，到期失效，重新输入
//! - 终身卡：免 v1.x 升级费
//!
//! ## 验证
//! - HMAC-SHA256 签名（密钥从服务器分发；本机只做离线校验格式）
//! - 时间用本地时间（不做强制校验，可被改时间绕过；这是离线模式的妥协）
//!
//! ## v1.6 新增
//! - `LicenseProvider` trait + 4 种实现（ActivationCode / Trial / Community / Enterprise）
//! - `LicenseManager` 在线/离线双通道 + 状态机 + 7 天滚动窗口
//! - `LicenseStatus` 6 种状态（Unactivated / Valid / Expiring / Expired / OfflineGrace / Invalid）

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod code;
pub mod verify;
pub mod storage;
pub mod provider;
pub mod manager;

pub use code::{LicenseCode, LicenseTier, LicensePayload, DeviceFingerprint};
pub use verify::{verify_code, VerifyResult, VerifyError, generate_license};
pub use storage::{LicenseStorage, StoredLicense, StorageError};
pub use provider::{
    ActivationCodeProvider, CommunityProvider, EnterpriseProvider, LicenseError, LicenseProvider,
    LicenseStatus, TrialProvider,
};
pub use manager::{LicenseManager, LicenseSummary};
