//! License Manager — 在线/离线双通道 + 状态机
//!
//! 设计参考：docs/开发文档.md §13.6.3
//!
//! ## 行为
//! - 启动时读本地 license
//! - 优先在线校验（演示版默认走离线，server 留 TODO）
//! - 联网失败 → 7 天本地缓存滚动窗口
//! - 7 天未联网 → 降级到 OfflineGrace（只读）
//! - 30 天未联网 → 强制 Expired
//! - 关键操作前可实时校验
//!
//! ## 单例
//! Manager 内部用 `Arc<Mutex<...>>`，可被多线程 / 多 tauri command 共享

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::code::DeviceFingerprint;
use crate::provider::{ActivationCodeProvider, LicenseError, LicenseProvider, LicenseStatus};

/// Manager 状态（缓存 + 上次校验时间）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CacheState {
    /// 上次成功校验时间戳（秒）
    last_validated_at: i64,
    /// 上次的状态
    last_status: Option<LicenseStatus>,
}

impl CacheState {
    fn cache_path(home: &Path) -> PathBuf {
        home.join(".agentshell").join("license-cache.json")
    }
}

/// License Manager（核心 facade）
pub struct LicenseManager {
    provider: Arc<dyn LicenseProvider>,
    device: DeviceFingerprint,
    cache: Arc<RwLock<CacheState>>,
    cache_file: PathBuf,
}

/// 公开的 summary（前端 / tauri command 用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseSummary {
    pub status: LicenseStatus,
    /// 上次校验时间（秒）
    pub last_validated_at: i64,
    /// 是否离线模式
    pub offline: bool,
}

impl LicenseManager {
    /// 创建（默认 demo provider + 当前设备指纹）
    pub fn new_default() -> Self {
        let device = DeviceFingerprint::current();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let home = PathBuf::from(home);
        let cache_file = CacheState::cache_path(&home);

        let provider: Arc<dyn LicenseProvider> = Arc::new(ActivationCodeProvider::default_demo());

        // 尝试读旧 cache
        let cache = std::fs::read_to_string(&cache_file)
            .ok()
            .and_then(|t| serde_json::from_str::<CacheState>(&t).ok())
            .unwrap_or_default();

        Self {
            provider,
            device,
            cache: Arc::new(RwLock::new(cache)),
            cache_file,
        }
    }

    /// 创建（自定义 provider + 设备指纹）—— 测试用
    pub fn with_provider(
        provider: Arc<dyn LicenseProvider>,
        device: DeviceFingerprint,
        cache_file: impl AsRef<Path>,
    ) -> Self {
        Self {
            provider,
            device,
            cache: Arc::new(RwLock::new(CacheState::default())),
            cache_file: cache_file.as_ref().to_path_buf(),
        }
    }

    /// 持久化 cache
    async fn persist_cache(&self) {
        let cache = self.cache.read().await.clone();
        if let Some(parent) = self.cache_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(&cache) {
            let _ = std::fs::write(&self.cache_file, text);
        }
    }

    /// 启动时校验（在线优先 → 离线降级）
    pub async fn check(&self) -> LicenseSummary {
        // 尝试在线（演示版默认失败 → 走离线）
        // 真实版本用 reqwest 调 `https://api.agentshell.io/v1/license/validate`
        let online_status = self.check_online().await;

        let was_online = online_status.is_some();
        let status = match online_status {
            Some(s) => {
                // 在线成功 → 更新 cache
                let mut cache = self.cache.write().await;
                cache.last_validated_at = chrono::Utc::now().timestamp();
                cache.last_status = Some(s.clone());
                drop(cache);
                self.persist_cache().await;
                s
            }
            None => {
                // 离线模式
                self.check_offline().await
            }
        };

        let cache = self.cache.read().await;
        LicenseSummary {
            status,
            last_validated_at: cache.last_validated_at,
            offline: !was_online,
        }
    }

    /// 在线校验（演示版本：直接调本地 validate，TODO 接服务端）
    async fn check_online(&self) -> Option<LicenseStatus> {
        // 演示版：用本地存储的 license 调 provider.validate
        // 真实版：调 API + 比对 server 时间
        let storage_path = format!(
            "{}/.agentshell/license.toml",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        );
        let storage = crate::storage::LicenseStorage::new(&storage_path);
        let stored = match storage.load() {
            Ok(Some(s)) => s,
            _ => return Some(LicenseStatus::Unactivated),
        };
        match self.provider.validate(&stored).await {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }

    /// 离线校验
    async fn check_offline(&self) -> LicenseStatus {
        let cache = self.cache.read().await;
        if cache.last_validated_at == 0 {
            return LicenseStatus::Unactivated;
        }
        // 复用 provider 的 validate_offline（粗略）
        let storage_path = format!(
            "{}/.agentshell/license.toml",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        );
        let storage = crate::storage::LicenseStorage::new(&storage_path);
        let stored = match storage.load() {
            Ok(Some(s)) => s,
            _ => return LicenseStatus::Unactivated,
        };
        match self
            .provider
            .validate_offline(&stored, cache.last_validated_at)
            .await
        {
            Ok(s) => s,
            Err(_) => LicenseStatus::Unactivated,
        }
    }

    /// 激活（用户输入码）
    pub async fn activate(&self, code: &str) -> Result<LicenseSummary, LicenseError> {
        let stored = self.provider.activate(code, &self.device).await?;
        // 激活后立即在线校验
        let summary = self.check().await;
        let _ = stored;
        Ok(summary)
    }

    /// 移除 license
    pub async fn deactivate(&self) -> Result<(), LicenseError> {
        self.provider.deactivate().await?;
        let mut cache = self.cache.write().await;
        cache.last_validated_at = 0;
        cache.last_status = None;
        drop(cache);
        self.persist_cache().await;
        Ok(())
    }

    /// 获取当前 summary（不触发校验，纯 cache 读）
    pub async fn summary(&self) -> LicenseSummary {
        let cache = self.cache.read().await;
        LicenseSummary {
            status: cache
                .last_status
                .clone()
                .unwrap_or(LicenseStatus::Unactivated),
            last_validated_at: cache.last_validated_at,
            offline: false,
        }
    }

    /// 强制 refresh（用户主动触发 / 启动时）
    pub async fn refresh(&self) -> LicenseSummary {
        self.check().await
    }

    /// 生成 demo license code（仅 demo / 测试用）
    ///
    /// 返回的字符串是用户可粘贴的激活码（Base64）
    pub fn generate_demo_code(
        &self,
        tier: crate::code::LicenseTier,
    ) -> Result<String, LicenseError> {
        // 需要拿到 ActivationCodeProvider 调 generate_demo_code
        // 演示：临时建一个 demo provider（不写存储）
        let demo_provider = ActivationCodeProvider::default_demo();
        let code = demo_provider.generate_demo_code(tier, &self.device);
        code.to_user_code()
            .map_err(|e| LicenseError::InvalidCode(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_default() {
        let m = LicenseManager::new_default();
        let s = m.check().await;
        // 默认无 license → Unactivated
        assert!(matches!(s.status, LicenseStatus::Unactivated));
    }

    #[tokio::test]
    async fn test_manager_activate_deactivate() {
        let m = LicenseManager::new_default();
        let device = DeviceFingerprint::current();

        // 生成测试码
        let demo = ActivationCodeProvider::default_demo();
        let code = demo.generate_demo_code(crate::code::LicenseTier::Yearly, &device);
        let user_code = code.to_user_code().unwrap();

        // 激活
        let s = m.activate(&user_code).await.unwrap();
        assert!(matches!(s.status, LicenseStatus::Valid { .. }));

        // 移除
        m.deactivate().await.unwrap();
        let s2 = m.check().await;
        assert!(matches!(s2.status, LicenseStatus::Unactivated));
    }
}
