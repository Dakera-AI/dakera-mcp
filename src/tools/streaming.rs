//! Streaming tools — SSE event streams for namespace events, global events, audit
//!
//! These endpoints emit Server-Sent Events. The MCP tool connects and returns whatever
//! data arrives before the 30-second client timeout. For continuous consumption use the
//! raw HTTP SSE endpoints directly.

use serde_json::json;

use super::{require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_namespace_events".into(),
            description: "Subscribe to the SSE memory lifecycle event stream for a specific namespace (store, update, forget, decay events). Returns initial events received within the request timeout. Requires Read scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to subscribe to" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_global_events".into(),
            description: "Subscribe to the SSE global memory lifecycle event stream across all namespaces. Returns events received within the request timeout. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_audit_stream".into(),
            description: "Subscribe to the SSE audit event stream (API key usage, admin actions, auth events). Returns events received within the request timeout. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
    ]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_namespace_events" => Some(tool_namespace_events(client, args).await),
        "dakera_global_events" => Some(match client.get_text("/ops/events").await {
            Ok(text) => CallToolResult::text(text),
            Err(e) => CallToolResult::error(e),
        }),
        "dakera_audit_stream" => Some(match client.get_text("/v1/audit/stream").await {
            Ok(text) => CallToolResult::text(text),
            Err(e) => CallToolResult::error(e),
        }),
        _ => None,
    }
}

async fn tool_namespace_events(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") { Ok(v) => v, Err(e) => return e };
    let path = format!("/v1/namespaces/{}/events", urlencoding::encode(&namespace));
    match client.get_text(&path).await {
        Ok(text) => CallToolResult::text(text),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::DakeraApiClient;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        assert!(execute(&dummy_client(), "not_stream", &json!({})).await.is_none());
    }

    #[tokio::test]
    async fn test_namespace_events_missing_namespace() {
        let r = execute(&dummy_client(), "dakera_namespace_events", &json!({})).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_definitions_unique() {
        let defs = definitions();
        let mut seen = std::collections::HashSet::new();
        for d in &defs {
            assert!(seen.insert(d.name.as_str()), "duplicate: {}", d.name);
        }
        assert_eq!(defs.len(), 3);
    }
}
