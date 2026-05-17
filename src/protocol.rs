//! MCP JSON-RPC protocol types
//!
//! Implements the Model Context Protocol message format over JSON-RPC 2.0.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(rename = "jsonrpc")]
    _jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<serde_json::Value>, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }

    pub fn method_not_found(id: Option<serde_json::Value>, method: &str) -> Self {
        Self::error(id, -32601, format!("Method not found: {}", method))
    }
}

/// Tier classification for tool exposure control (MCP-8 hybrid exposure).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTier {
    /// 12 high-frequency tools — always included in default tools/list
    Core,
    /// Power-user tools — included with profile=power or profile=all
    Power,
    /// Administrative tools (namespace/encryption/bulk) — only with profile=all
    Admin,
    /// Discovery meta-tools — always included alongside Core
    Meta,
}

impl ToolTier {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolTier::Core => "core",
            ToolTier::Power => "power",
            ToolTier::Admin => "admin",
            ToolTier::Meta => "meta",
        }
    }
}

/// MCP Tool definition
#[derive(Debug, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// MCP Tool call result content
#[derive(Debug, Serialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// MCP CallTool result
#[derive(Debug, Serialize)]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

impl CallToolResult {
    pub fn text(text: String) -> Self {
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text,
            }],
            is_error: None,
        }
    }

    pub fn error(text: String) -> Self {
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text,
            }],
            is_error: Some(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_success_response_fields() {
        let resp = JsonRpcResponse::success(Some(json!(1)), json!({"ok": true}));
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        assert_eq!(resp.id, Some(json!(1)));
    }

    #[test]
    fn test_success_serializes_without_error_field() {
        let resp = JsonRpcResponse::success(Some(json!(42)), json!({}));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));
    }

    #[test]
    fn test_error_response_fields() {
        let resp = JsonRpcResponse::error(Some(json!(2)), -32600, "Invalid Request".to_string());
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid Request");
        assert!(err.data.is_none());
    }

    #[test]
    fn test_error_serializes_without_result_field() {
        let resp = JsonRpcResponse::error(None, -32600, "bad".to_string());
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
    }

    #[test]
    fn test_method_not_found_code_and_message() {
        let resp = JsonRpcResponse::method_not_found(Some(json!("abc")), "foo/bar");
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("foo/bar"));
    }

    #[test]
    fn test_null_id_response() {
        let resp = JsonRpcResponse::success(None, json!(null));
        assert!(resp.id.is_none());
    }

    #[test]
    fn test_call_tool_result_text() {
        let r = CallToolResult::text("hello world".to_string());
        assert!(r.is_error.is_none());
        assert_eq!(r.content.len(), 1);
        assert_eq!(r.content[0].content_type, "text");
        assert_eq!(r.content[0].text, "hello world");
    }

    #[test]
    fn test_call_tool_result_error() {
        let r = CallToolResult::error("something failed".to_string());
        assert_eq!(r.is_error, Some(true));
        assert_eq!(r.content.len(), 1);
        assert_eq!(r.content[0].text, "something failed");
    }

    #[test]
    fn test_call_tool_result_error_serializes_is_error() {
        let r = CallToolResult::error("oops".to_string());
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"isError\""));
    }

    #[test]
    fn test_call_tool_result_text_omits_is_error() {
        let r = CallToolResult::text("ok".to_string());
        let s = serde_json::to_string(&r).unwrap();
        assert!(!s.contains("isError"));
    }

    #[test]
    fn test_request_deserialization_with_params() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"foo"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.method, "tools/call");
        assert_eq!(req.id, Some(json!(1)));
        assert_eq!(req.params["name"], "foo");
    }

    #[test]
    fn test_request_deserialization_without_params() {
        let raw = r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.method, "ping");
        // params defaults to null when absent
        assert!(req.params.is_null());
    }
}
