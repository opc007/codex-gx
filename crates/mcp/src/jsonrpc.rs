//! JSON-RPC 2.0 消息定义

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// 协议版本
    pub jsonrpc: String,
    /// 方法名
    pub method: String,
    /// 参数
    #[serde(default)]
    pub params: Value,
    /// 请求 ID
    pub id: u64,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>, params: Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
            id,
        }
    }
}

/// JSON-RPC 2.0 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    /// 成功时存在
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 失败时存在
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }
}

/// JSON-RPC 2.0 通知（无 ID，不需要响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialize() {
        let r = JsonRpcRequest::new("tools/list", serde_json::json!({}), 1);
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"jsonrpc\":\"2.0\""));
        assert!(s.contains("\"method\":\"tools/list\""));
    }

    #[test]
    fn test_response_parse() {
        let r = JsonRpcResponse::success(1, serde_json::json!({"ok": true}));
        let s = serde_json::to_string(&r).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&s).unwrap();
        assert!(parsed.result.is_some());
        assert!(parsed.error.is_none());
    }

    #[test]
    fn test_error_response() {
        let r = JsonRpcResponse::error(2, -32601, "method not found");
        let s = serde_json::to_string(&r).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&s).unwrap();
        assert!(parsed.error.is_some());
        assert_eq!(parsed.error.unwrap().code, -32601);
    }
}