//! Agent tools — stats, memories, sessions

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_agent_stats".into(),
            description: "Return an agent's memory footprint: count, session count, approximate storage, and top tags. Use to monitor memory growth or compare agents.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_agent_memories".into(),
            description: "Paginate through all memories belonging to an agent, ordered by recency. Use for inspection or bulk operations — for semantic retrieval use dakera_recall; for filter-based listing use dakera_batch_recall. limit defaults to 50 (max 1000).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "limit": { "type": "integer", "description": "Max memories to return" },
                    "offset": { "type": "integer", "description": "Pagination offset" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_agent_sessions".into(),
            description: "List all sessions (active and closed) for an agent with timestamps and summaries. Use to find a session_id for dakera_session_memories.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" }
                },
                "required": ["agent_id"]
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
        "dakera_agent_stats" => Some(tool_agent_stats(client, args).await),
        "dakera_agent_memories" => Some(tool_agent_memories(client, args).await),
        "dakera_agent_sessions" => Some(tool_agent_sessions(client, args).await),
        _ => None,
    }
}

async fn tool_agent_stats(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&agent_id);
    let path = format!("/v1/agents/{}/stats", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_agent_memories(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let limit = std::cmp::min(
        args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50),
        1000,
    );
    let offset = std::cmp::min(
        args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0),
        100_000,
    );
    let encoded = urlencoding::encode(&agent_id);
    let path = format!(
        "/v1/agents/{}/memories?limit={}&offset={}",
        encoded, limit, offset
    );
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_agent_sessions(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&agent_id);
    let path = format!("/v1/agents/{}/sessions", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}
