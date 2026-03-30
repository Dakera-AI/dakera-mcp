//! OBS-1: Audit log tool — dakera_audit_query
//!
//! Provides access to the business-event audit log persisted in graph.db.
//! Requires Scope::Admin API key.

use serde_json::json;

use super::{ok_json, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "dakera_audit_query".into(),
        description: "Query the business-event audit log. Returns memory lifecycle events \
            (stored, recalled, forgotten, session.started, etc.) with optional filters on \
            agent, event type, and time range. Requires Admin scope."
            .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Filter by agent ID (optional)"
                },
                "event_type": {
                    "type": "string",
                    "description": "Filter by event type, e.g. 'stored', 'recalled', 'forgotten', 'session_started' (optional)"
                },
                "from": {
                    "type": "integer",
                    "description": "Lower bound Unix milliseconds timestamp (inclusive, optional)"
                },
                "to": {
                    "type": "integer",
                    "description": "Upper bound Unix milliseconds timestamp (inclusive, optional)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of events to return (default 100, max 10000)",
                    "default": 100
                }
            },
            "required": []
        }),
    }]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_audit_query" => Some(tool_audit_query(client, args).await),
        _ => None,
    }
}

async fn tool_audit_query(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    // Build query params from optional args
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()) {
        params.push(("agent_id", agent_id.to_string()));
    }
    if let Some(event_type) = args.get("event_type").and_then(|v| v.as_str()) {
        params.push(("event_type", event_type.to_string()));
    }
    if let Some(from) = args.get("from").and_then(|v| v.as_u64()) {
        params.push(("from", from.to_string()));
    }
    if let Some(to) = args.get("to").and_then(|v| v.as_u64()) {
        params.push(("to", to.to_string()));
    }
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100)
        .min(10_000);
    params.push(("limit", limit.to_string()));

    // Build query string
    let qs = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let path = format!("/v1/audit?{}", qs);

    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://localhost:9999".to_string(), None)
    }

    #[test]
    fn test_definitions_count() {
        assert_eq!(definitions().len(), 1);
    }

    #[test]
    fn test_definition_name() {
        assert_eq!(definitions()[0].name, "dakera_audit_query");
    }

    #[tokio::test]
    async fn test_execute_dispatches() {
        let result = execute(&dummy_client(), "dakera_audit_query", &json!({})).await;
        assert!(result.is_some());
        // Server not running — will get a connection error (is_error = true)
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_with_all_params() {
        let result = execute(
            &dummy_client(),
            "dakera_audit_query",
            &json!({
                "agent_id": "core-engine",
                "event_type": "stored",
                "from": 1700000000000_u64,
                "to": 1800000000000_u64,
                "limit": 50
            }),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true)); // no server
    }
}
