//! Tool trait + Registry
//!
//! 设计参考：docs/开发文档.md §5.1 Tool 定义

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Result;

/// Tool 输入（JSON Value）
pub type ToolInput = serde_json::Value;

/// Tool 输出（字符串 / 文本 / 错误）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// 成功？
    pub success: bool,
    /// 文本输出
    pub output: String,
    /// 错误信息（失败时）
    #[serde(default)]
    pub error: Option<String>,
    /// 截断标记（输出过长）
    #[serde(default)]
    pub truncated: bool,
}

impl ToolOutput {
    /// 成功
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
            truncated: false,
        }
    }

    /// 失败
    pub fn err(err: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(err.into()),
            truncated: false,
        }
    }

    /// 标记截断
    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = truncated;
        self
    }
}

/// Tool schema（用于发给 LLM）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// 工具名
    pub name: String,
    /// 描述
    pub description: String,
    /// 参数 JSON Schema
    pub parameters: serde_json::Value,
}

/// Tool trait
#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug {
    /// 工具名
    fn name(&self) -> &str;

    /// 工具描述
    fn description(&self) -> &str;

    /// 参数 schema（OpenAI function calling 格式）
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    /// 完整 schema
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }

    /// 执行
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput>;
}

/// 工具注册表
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// 新建空注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    /// 注册 Arc
    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// 获取
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// 列出所有 schema
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema()).collect()
    }

    /// 工具数
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// 列出所有名
    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.tools.keys().cloned().collect();
        v.sort();
        v
    }
}

// 注：Arc<T> 已经实现了 Clone（只要 T: ?Sized），所以不需要自定义 impl

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug)]
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "回显输入"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"]
            })
        }
        async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
            Ok(ToolOutput::ok(input["text"].as_str().unwrap_or("")))
        }
    }

    #[tokio::test]
    async fn test_register_and_execute() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool);
        let t = reg.get("echo").unwrap();
        let out = t.execute(json!({"text": "hello"})).await.unwrap();
        assert!(out.success);
        assert_eq!(out.output, "hello");
    }
}
