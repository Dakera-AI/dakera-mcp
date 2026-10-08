//! Agent tools — create, stats, memories, sessions, wake-up

use serde_json::json;

use super::{ok_json, require_string, route_missing, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_agent_stats".into(),
            description: "Return an agent's memory statistics: total and per-type memory counts, average importance, oldest/newest timestamps, session counts. Use to monitor memory growth or compare agents.".into(),
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
            description: "Page through an agent's memories, newest first (limit default 50, max 1000). For semantic retrieval use dakera_recall. Content is cut to content_preview_chars (default 500; content_truncated marks a cut one, read it with dakera_memory_get).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "limit": { "type": "integer", "description": "Max memories to return" },
                    "offset": { "type": "integer", "description": "Pagination offset" },
                    "content_preview_chars": { "type": "integer", "description": "Characters of content per memory (1-10000, 0 = full)" },
                    "include_derived": { "type": "boolean", "description": "Also list derived sentence sub-memories (v0.12.2+ leaves them out)" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_agent_sessions".into(),
            description: "List an agent's sessions (open and ended) with timestamps and summaries, one page (default 50; use limit/offset). v0.12.2+ shows last_activity_at and ended_reason (client or idle).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "limit": { "type": "integer", "description": "Page size" },
                    "offset": { "type": "integer", "description": "Pagination offset" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_wake_up".into(),
            description: "Load an agent's startup context in one call: its top_n memories (default 20, max 100) ranked by importance x recency of use, with no query and no embedding. Use at the start of a run, before any dakera_recall.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "top_n": { "type": "integer", "description": "Memories to return (default 20, max 100)" },
                    "min_importance": { "type": "number", "description": "Skip memories below this importance (0.0-1.0)" },
                    "include_derived": { "type": "boolean", "description": "Also rank derived sentence sub-memories (v0.12.2+ leaves them out)" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_agent_create".into(),
            description: "Create an agent (its memory namespace) before its first memory (v0.12.2+). Idempotent: created=false for an existing agent. Use with a key limited to an agent prefix.".into(),
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
        "dakera_wake_up" => Some(tool_wake_up(client, args).await),
        "dakera_agent_create" => Some(tool_agent_create(client, args).await),
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
    let preview = match super::content_preview_chars(args) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&agent_id);
    let mut path = format!(
        "/v1/agents/{}/memories?limit={}&offset={}",
        encoded, limit, offset
    );
    if let Some(n) = preview {
        path.push_str(&format!("&content_preview_chars={n}"));
    }
    if include_derived(args) {
        path.push_str("&include_derived=true");
    }
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
    let path = format!(
        "/v1/agents/{}/sessions?{}",
        encoded,
        super::sessions::page_query(args)
    );
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_wake_up(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut path = format!("/v1/agents/{}/wake-up", urlencoding::encode(&agent_id));
    let mut sep = '?';
    if let Some(n) = args.get("top_n").and_then(|v| v.as_u64()) {
        path.push_str(&format!("{sep}top_n={n}"));
        sep = '&';
    }
    if let Some(min) = args.get("min_importance").and_then(|v| v.as_f64()) {
        path.push_str(&format!("{sep}min_importance={min}"));
        sep = '&';
    }
    if include_derived(args) {
        path.push_str(&format!("{sep}include_derived=true"));
    }
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

/// Whether the caller asked for derived records (sentence sub-memories) too.
/// Sent only when true: `false` is the v0.12.2 default, and an older server,
/// which does not read the parameter, lists them anyway.
fn include_derived(args: &serde_json::Value) -> bool {
    args.get("include_derived").and_then(|v| v.as_bool()) == Some(true)
}

/// The answer of `dakera_agent_create` on a server without `POST /v1/agents`.
pub const CREATE_UNSUPPORTED: &str = "This Dakera server predates POST /v1/agents (v0.12.2): \
     an agent is created by its first stored memory, so there is nothing to do before \
     dakera_store.";

async fn tool_agent_create(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({ "agent_id": agent_id });
    let (status, text) = match client
        .send_raw(reqwest::Method::POST, "/v1/agents", Some(&body))
        .await
    {
        Ok(answer) => answer,
        Err(e) => return CallToolResult::error(e),
    };
    if route_missing(status, &text) {
        return ok_json(&json!({
            "agent_id": agent_id,
            "created": null,
            "note": CREATE_UNSUPPORTED,
        }));
    }
    if !status.is_success() {
        return CallToolResult::error(format!("API error ({}): {}", status, text));
    }
    match serde_json::from_str(&text) {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(format!("JSON parse failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[tokio::test]
    async fn test_agent_create_requires_agent_id() {
        let result = execute(&dummy_client(), "dakera_agent_create", &json!({}))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[test]
    fn test_include_derived_only_when_true() {
        assert!(include_derived(&json!({"include_derived": true})));
        assert!(!include_derived(&json!({"include_derived": false})));
        assert!(!include_derived(&json!({})));
        assert!(!include_derived(&json!({"include_derived": "true"})));
    }

    #[test]
    fn test_definitions_include_agent_create() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        assert!(names.contains(&"dakera_agent_create".to_string()));
        assert_eq!(names.len(), 5);
    }
}
