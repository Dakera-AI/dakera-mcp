//! AutoPilot tools — PILOT-4
//!
//! Exposes `dakera_autopilot_status` and `dakera_autopilot_trigger` MCP tools
//! that wrap the PILOT-1 and PILOT-3 admin endpoints. Both require Admin scope.

use serde_json::json;

use super::{ok_json, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_autopilot_status".into(),
            description: "Return AutoPilot configuration and last-cycle stats (memories deduped, consolidated, timestamps). Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_autopilot_trigger".into(),
            description: "Trigger an AutoPilot cycle immediately. action: dedup|consolidate|all. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["dedup", "consolidate", "all"],
                        "description": "Which cycle to trigger: 'dedup' removes duplicate memories, 'consolidate' merges related memories, 'all' runs both in sequence."
                    }
                },
                "required": ["action"]
            }),
        },
    ]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_autopilot_status" => Some(tool_autopilot_status(client).await),
        "dakera_autopilot_trigger" => Some(tool_autopilot_trigger(client, args).await),
        _ => None,
    }
}

async fn tool_autopilot_status(client: &DakeraApiClient) -> CallToolResult {
    match client.get_json("/admin/autopilot/status").await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_autopilot_trigger(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let action = match args.get("action").and_then(|v| v.as_str()) {
        Some(a) if matches!(a, "dedup" | "consolidate" | "all") => a,
        Some(a) => {
            return CallToolResult::error(format!(
                "Invalid action '{}': must be 'dedup', 'consolidate', or 'all'",
                a
            ))
        }
        None => return CallToolResult::error("Missing required parameter: action".to_string()),
    };
    let body = json!({ "action": action });
    match client.post_json("/admin/autopilot/trigger", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autopilot_definitions() {
        let defs = definitions();
        assert_eq!(defs.len(), 2);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dakera_autopilot_status"));
        assert!(names.contains(&"dakera_autopilot_trigger"));
    }

    #[test]
    fn test_autopilot_definitions_have_descriptions() {
        for def in definitions() {
            assert!(
                !def.description.is_empty(),
                "'{}' has no description",
                def.name
            );
        }
    }

    #[tokio::test]
    async fn test_trigger_invalid_action() {
        let client = DakeraApiClient::new("http://localhost:9999".to_string(), None);
        let args = serde_json::json!({"action": "invalid"});
        let result = tool_autopilot_trigger(&client, &args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Invalid action"));
    }

    #[tokio::test]
    async fn test_trigger_missing_action() {
        let client = DakeraApiClient::new("http://localhost:9999".to_string(), None);
        let args = serde_json::json!({});
        let result = tool_autopilot_trigger(&client, &args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("action"));
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_none() {
        let client = DakeraApiClient::new("http://localhost:9999".to_string(), None);
        let result = execute(&client, "unknown_tool", &serde_json::json!({})).await;
        assert!(result.is_none());
    }
}
