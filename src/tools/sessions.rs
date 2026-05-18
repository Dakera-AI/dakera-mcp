//! Session tools — start, end, list, get, memories

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_session_start".into(),
            description:
                "Start a new session for an agent. Sessions group related memories together.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "metadata": { "type": "object", "description": "Optional session metadata" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_end".into(),
            description: "End an active session, optionally with a summary.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session ID to end" },
                    "summary": { "type": "string", "description": "Optional session summary" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_list".into(),
            description: "List sessions for an agent. Optionally filter to active sessions only."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "active_only": { "type": "boolean", "description": "Only return active sessions" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_get".into(),
            description:
                "Get details for a specific session by ID, including metadata and summary.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session ID to retrieve" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_memories".into(),
            description: "List all memories associated with a session.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session ID" }
                },
                "required": ["session_id"]
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
        "dakera_session_start" => Some(tool_session_start(client, args).await),
        "dakera_session_end" => Some(tool_session_end(client, args).await),
        "dakera_session_list" => Some(tool_session_list(client, args).await),
        "dakera_session_get" => Some(tool_session_get(client, args).await),
        "dakera_session_memories" => Some(tool_session_memories(client, args).await),
        _ => None,
    }
}

async fn tool_session_start(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "agent_id": agent_id,
        "metadata": args.get("metadata").cloned().unwrap_or(json!(null)),
    });
    match client.post_json("/v1/sessions/start", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_session_end(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let session_id = match require_string(args, "session_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "summary": args.get("summary").and_then(|v| v.as_str()),
    });
    let encoded = urlencoding::encode(&session_id);
    let path = format!("/v1/sessions/{}/end", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_session_list(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let active_only = args
        .get("active_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let encoded = urlencoding::encode(&agent_id);
    let path = format!(
        "/v1/sessions?agent_id={}&active_only={}",
        encoded, active_only
    );
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_session_get(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let session_id = match require_string(args, "session_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&session_id);
    let path = format!("/v1/sessions/{}", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_session_memories(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let session_id = match require_string(args, "session_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&session_id);
    let path = format!("/v1/sessions/{}/memories", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}
