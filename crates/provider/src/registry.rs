//! Provider 注册表

use std::collections::HashMap;

use crate::model::{Model, ModelInfo};

/// Provider 集合
pub struct ProviderRegistry {
    providers: HashMap<String, HashMap<String, Box<dyn Model>>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// 注册模型
    pub fn register(&mut self, provider_name: impl Into<String>, model: Box<dyn Model>) {
        let p = provider_name.into();
        let m = self.providers.entry(p).or_default();
        m.insert(model.info().id.clone(), model);
    }

    /// 获取特定模型
    pub fn get(&self, provider: &str, model: &str) -> Option<&dyn Model> {
        self.providers
            .get(provider)
            .and_then(|p| p.get(model))
            .map(|m| m.as_ref())
    }

    /// 列出某个 provider 的所有模型
    pub fn list_provider(&self, provider: &str) -> Vec<&dyn Model> {
        self.providers
            .get(provider)
            .map(|p| p.values().map(|m| m.as_ref()).collect())
            .unwrap_or_default()
    }

    /// 列出所有模型
    pub fn list_all(&self) -> Vec<ModelInfo> {
        self.providers
            .values()
            .flat_map(|p| p.values())
            .map(|m| m.info().clone())
            .collect()
    }

    pub fn provider_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.providers.keys().cloned().collect();
        v.sort();
        v
    }
}