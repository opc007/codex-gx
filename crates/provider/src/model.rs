//! Model trait + 信息
//!
//! 设计参考：docs/开发文档.md §5.3 模型协议

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::request::ChatRequest;
use crate::response::ChatResponse;
use crate::stream::ChatStream;

/// 停止原因
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// 自然结束
    EndTurn,
    /// 触发了工具调用
    ToolUse,
    /// 达到 max_tokens
    MaxTokens,
    /// 内容过滤
    ContentFilter,
    /// 用户取消
    UserCancel,
}

/// Token 使用统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// 输入 token 数
    pub input_tokens: u32,
    /// 输出 token 数
    pub output_tokens: u32,
    /// 缓存命中（读取）token
    pub cache_read_tokens: u32,
    /// 缓存写入（创建）token
    pub cache_write_tokens: u32,
}

impl Usage {
    /// 总 token
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// 估算成本（USD）
    pub fn estimated_cost(&self, info: &ModelInfo) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * info.input_price_per_m;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * info.output_price_per_m;
        input_cost + output_cost
    }
}

/// 模型能力
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// 工具调用
    pub tools: bool,
    /// 视觉（图像理解）
    pub vision: bool,
    /// 视频理解
    pub video: bool,
    /// 推理（thinking）
    pub reasoning: bool,
    /// interleaved thinking
    pub interleaved_thinking: bool,
    /// Computer Use
    pub computer_use: bool,
}

/// 模型元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 模型 ID
    pub id: String,
    /// 显示名
    pub name: String,
    /// 提供方
    pub provider: String,
    /// 上下文窗口
    pub max_context: u32,
    /// 最大输出
    pub max_output: u32,
    /// 能力
    pub capabilities: ModelCapabilities,
    /// 输入价格（USD / 1M tokens）
    pub input_price_per_m: f64,
    /// 输出价格
    pub output_price_per_m: f64,
    /// 缓存读价格
    pub cache_read_price_per_m: f64,
    /// 推理 effort 选项
    pub reasoning_efforts: Vec<String>,
}

/// Model trait
#[async_trait]
pub trait Model: Send + Sync {
    /// 模型元信息
    fn info(&self) -> &ModelInfo;

    /// 同步聊天（一次性返回）
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;

    /// 流式聊天
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream>;

    /// 估算成本（如果支持）
    fn estimate_cost(&self, usage: &Usage) -> f64 {
        usage.estimated_cost(self.info())
    }
}

/// Provider 配置
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// 提供方 ID
    pub id: String,
    /// 显示名
    pub name: String,
    /// API base URL
    pub base_url: String,
    /// API key（env_key 解析后）
    pub api_key: String,
    /// 默认模型
    pub default_model: String,
    /// 协议类型
    pub wire_api: WireApi,
}

/// 协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireApi {
    /// OpenAI Chat Completions
    Chat,
    /// Anthropic Messages
    Messages,
    /// OpenAI Responses
    Responses,
}

/// Provider 共享注册表（多模型共存）
#[derive(Default)]
pub struct ModelRegistry {
    models: HashMap<String, Box<dyn Model>>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, model: Box<dyn Model>) {
        let id = model.info().id.clone();
        self.models.insert(id, model);
    }

    pub fn get(&self, id: &str) -> Option<&dyn Model> {
        self.models.get(id).map(|m| m.as_ref())
    }

    pub fn list(&self) -> Vec<&dyn Model> {
        self.models.values().map(|m| m.as_ref()).collect()
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_total() {
        let u = Usage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        };
        assert_eq!(u.total(), 150);
    }

    #[test]
    fn test_cost_estimate() {
        let info = ModelInfo {
            id: "test".into(),
            name: "Test".into(),
            provider: "test".into(),
            max_context: 1000,
            max_output: 1000,
            capabilities: ModelCapabilities::default(),
            input_price_per_m: 0.60,
            output_price_per_m: 2.40,
            cache_read_price_per_m: 0.12,
            reasoning_efforts: vec![],
        };
        let u = Usage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        // 1M input @ 0.60 + 1M output @ 2.40 = 3.00
        assert!((u.estimated_cost(&info) - 3.00).abs() < 0.001);
    }
}