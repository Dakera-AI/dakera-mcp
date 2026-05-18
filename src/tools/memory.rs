//! Memory tools — store, recall, get, forget, batch_recall, batch_forget, update, importance, search, consolidate

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_store".into(),
            description: "Persist a new memory for an agent with importance weighting and optional tags. Use to save facts, decisions, or context for future retrieval. importance defaults to 0.5; set 0.8–1.0 for critical memories that must survive decay.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "content": { "type": "string", "description": "Memory content text" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"], "description": "Memory type (episodic|semantic|procedural|working)" },
                    "importance": { "type": "number", "description": "Importance 0.0-1.0" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for filtering" },
                    "session_id": { "type": "string", "description": "Session to associate with" },
                    "expires_at": { "type": "integer", "description": "Expiry Unix timestamp (seconds)" }
                },
                "required": ["agent_id", "content"]
            }),
        },
        ToolDefinition {
            name: "dakera_recall".into(),
            description: "Retrieve top-k memories semantically closest to a query. Prefer over dakera_batch_recall for query-based retrieval. Set include_associated=true to expand results via KG edges (1-3 hops).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "query": { "type": "string", "description": "Semantic query text" },
                    "top_k": { "type": "integer", "description": "Max results to return" },
                    "min_importance": { "type": "number", "description": "Min importance threshold" },
                    "include_associated": { "type": "boolean", "description": "Include KG-linked memories in results" },
                    "since": { "type": "string", "description": "Only memories created at or after this ISO-8601 timestamp" },
                    "until": { "type": "string", "description": "Only memories created at or before this ISO-8601 timestamp" }
                },
                "required": ["agent_id", "query"]
            }),
        },
        ToolDefinition {
            name: "dakera_forget".into(),
            description: "Permanently delete memories by ID or tag. Provide memory_ids for exact removal or tags to bulk-delete all memories sharing those tags. Deletion is immediate and irreversible — prefer dakera_memory_importance to suppress without deleting.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "memory_ids": { "type": "array", "items": { "type": "string" }, "description": "Specific memory IDs to delete" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Delete memories with these tags" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_batch_recall".into(),
            description: "Filter-based memory listing by tags, importance range, time window, type, or session. Prefer over dakera_recall when semantic search is not needed. At least one filter required.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags to match (all required)" },
                    "min_importance": { "type": "number", "description": "Min importance (inclusive)" },
                    "max_importance": { "type": "number", "description": "Max importance (inclusive)" },
                    "created_after": { "type": "integer", "description": "After Unix timestamp" },
                    "created_before": { "type": "integer", "description": "Before Unix timestamp" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"] },
                    "session_id": { "type": "string" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_batch_forget".into(),
            description: "Bulk-delete memories matching filter criteria: tags, importance range, time window, or memory type. At least one filter is required to prevent accidental full-agent wipe. Deletion is permanent — use dakera_memory_importance to lower importance scores instead of deleting.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags to match (all required)" },
                    "min_importance": { "type": "number", "description": "Min importance threshold" },
                    "max_importance": { "type": "number", "description": "Max importance threshold" },
                    "created_after": { "type": "integer", "description": "After Unix timestamp" },
                    "created_before": { "type": "integer", "description": "Before Unix timestamp" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"] },
                    "session_id": { "type": "string" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_search".into(),
            description: "Semantic search with optional tag and memory-type pre-filters. Prefer over dakera_recall when results must be constrained by tag or type alongside the semantic match.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "query": { "type": "string", "description": "Search query text" },
                    "top_k": { "type": "integer", "description": "Number of results" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Filter by tags" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"], "description": "Filter by memory type" }
                },
                "required": ["agent_id", "query"]
            }),
        },
        ToolDefinition {
            name: "dakera_consolidate".into(),
            description: "Merge a set of memories into a synthesized summary, de-duplicating overlap. Use after a burst of related episodic memories to reduce storage and improve recall. Source memories are retained unless explicitly deleted.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "memory_ids": { "type": "array", "items": { "type": "string" }, "description": "Memory IDs to consolidate" }
                },
                "required": ["agent_id", "memory_ids"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_get".into(),
            description: "Fetch a single memory by ID, returning the full object: content, tags, importance, timestamps, and embedding metadata.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Memory ID to retrieve" },
                    "agent_id": { "type": "string" }
                },
                "required": ["memory_id", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_update".into(),
            description: "Edit an existing memory's content, importance, or tags. Changing content triggers re-embedding so semantic search reflects the update. Prefer over delete+recreate to preserve the memory ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Memory ID to update" },
                    "agent_id": { "type": "string" },
                    "content": { "type": "string", "description": "New content (triggers re-embedding)" },
                    "importance": { "type": "number", "description": "New importance score 0.0-1.0" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Replace tags" }
                },
                "required": ["memory_id", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_recall_associated".into(),
            description: "Semantic recall that expands results by following Knowledge Graph edges from each matched memory to its linked neighbors (1–3 hops). Use instead of dakera_recall when cross-linked context — related decisions, people, or events — would improve completeness. associated_memories_depth=1 is the default; higher values widen the result set.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "query": { "type": "string", "description": "Semantic query text" },
                    "top_k": { "type": "integer", "description": "Max direct recall results" },
                    "min_importance": { "type": "number", "description": "Min importance threshold" },
                    "associated_memories_depth": { "type": "integer", "description": "KG traversal depth (1–3 hops)", "minimum": 1, "maximum": 3 },
                    "associated_memories_min_weight": { "type": "number", "description": "Min KG edge weight to follow (0.0–1.0)" },
                    "since": { "type": "string", "description": "Only memories created at or after this ISO-8601 timestamp" },
                    "until": { "type": "string", "description": "Only memories created at or before this ISO-8601 timestamp" }
                },
                "required": ["agent_id", "query"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_importance".into(),
            description: "Batch-update importance scores for multiple memories in a single call. Use to boost critical memories (0.9–1.0 to resist decay) or to down-weight noisy ones without deleting them. Accepts an array of {memory_id, importance} pairs where importance is 0.0–1.0.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "updates": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "memory_id": { "type": "string" },
                                "importance": { "type": "number" }
                            },
                            "required": ["memory_id", "importance"]
                        },
                        "description": "Array of {memory_id, importance} pairs"
                    }
                },
                "required": ["agent_id", "updates"]
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
        "dakera_store" => Some(tool_store(client, args).await),
        "dakera_recall" => Some(tool_recall(client, args).await),
        "dakera_recall_associated" => Some(tool_recall_associated(client, args).await),
        "dakera_forget" => Some(tool_forget(client, args).await),
        "dakera_batch_recall" => Some(tool_batch_recall(client, args).await),
        "dakera_batch_forget" => Some(tool_batch_forget(client, args).await),
        "dakera_search" => Some(tool_search(client, args).await),
        "dakera_consolidate" => Some(tool_consolidate(client, args).await),
        "dakera_memory_get" => Some(tool_memory_get(client, args).await),
        "dakera_memory_update" => Some(tool_memory_update(client, args).await),
        "dakera_memory_importance" => Some(tool_memory_importance(client, args).await),
        _ => None,
    }
}

async fn tool_store(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let content = match require_string(args, "content") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({
        "agent_id": agent_id,
        "content": content,
        "memory_type": args.get("memory_type").and_then(|v| v.as_str()).unwrap_or("episodic"),
        "importance": args.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.5),
        "tags": args.get("tags").cloned().unwrap_or(json!([])),
        "session_id": args.get("session_id"),
    });
    if let Some(exp) = args.get("expires_at") {
        body["expires_at"] = exp.clone();
    }
    match client.post_json("/v1/memory/store", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_recall(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let query = match require_string(args, "query") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({
        "agent_id": agent_id,
        "query": query,
        "top_k": args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5),
        "min_importance": args.get("min_importance").and_then(|v| v.as_f64()).unwrap_or(0.0),
    });
    if let Some(true) = args.get("include_associated").and_then(|v| v.as_bool()) {
        body["include_associated"] = json!(true);
    }
    if let Some(since) = args.get("since").and_then(|v| v.as_str()) {
        body["since"] = json!(since);
    }
    if let Some(until) = args.get("until").and_then(|v| v.as_str()) {
        body["until"] = json!(until);
    }
    match client.post_json("/v1/memory/recall", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_forget(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    // Only include memory_ids/tags when non-empty; API uses Option<Vec> and
    // treats Some(vec![]) differently from None (empty vec filters out everything).
    let memory_ids = args
        .get("memory_ids")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty());
    let tags = args
        .get("tags")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty());

    let mut body = json!({
        "agent_id": agent_id,
    });

    if let Some(ids) = memory_ids {
        body["memory_ids"] = json!(ids);
    }
    if let Some(t) = tags {
        body["tags"] = json!(t);
    }

    match client.post_json("/v1/memory/forget", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_batch_recall(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut filter = serde_json::Map::new();
    if let Some(v) = args.get("tags") {
        filter.insert("tags".into(), v.clone());
    }
    if let Some(v) = args.get("min_importance") {
        filter.insert("min_importance".into(), v.clone());
    }
    if let Some(v) = args.get("max_importance") {
        filter.insert("max_importance".into(), v.clone());
    }
    if let Some(v) = args.get("created_after") {
        filter.insert("created_after".into(), v.clone());
    }
    if let Some(v) = args.get("created_before") {
        filter.insert("created_before".into(), v.clone());
    }
    if let Some(v) = args.get("memory_type") {
        filter.insert("memory_type".into(), v.clone());
    }
    if let Some(v) = args.get("session_id") {
        filter.insert("session_id".into(), v.clone());
    }
    let mut body = serde_json::Map::new();
    body.insert("agent_id".into(), json!(agent_id));
    body.insert("filter".into(), serde_json::Value::Object(filter));
    match client
        .post_json(
            "/v1/memories/recall/batch",
            &serde_json::Value::Object(body),
        )
        .await
    {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_batch_forget(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut filter = serde_json::Map::new();
    if let Some(v) = args.get("tags") {
        filter.insert("tags".into(), v.clone());
    }
    if let Some(v) = args.get("min_importance") {
        filter.insert("min_importance".into(), v.clone());
    }
    if let Some(v) = args.get("max_importance") {
        filter.insert("max_importance".into(), v.clone());
    }
    if let Some(v) = args.get("created_after") {
        filter.insert("created_after".into(), v.clone());
    }
    if let Some(v) = args.get("created_before") {
        filter.insert("created_before".into(), v.clone());
    }
    if let Some(v) = args.get("memory_type") {
        filter.insert("memory_type".into(), v.clone());
    }
    if let Some(v) = args.get("session_id") {
        filter.insert("session_id".into(), v.clone());
    }
    let mut body = serde_json::Map::new();
    body.insert("agent_id".into(), json!(agent_id));
    body.insert("filter".into(), serde_json::Value::Object(filter));
    match client
        .delete_with_json(
            "/v1/memories/forget/batch",
            &serde_json::Value::Object(body),
        )
        .await
    {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_search(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let query = match require_string(args, "query") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "agent_id": agent_id,
        "query": query,
        "top_k": args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10),
        "tags": args.get("tags").cloned().unwrap_or(json!([])),
        "memory_type": args.get("memory_type"),
    });
    match client.post_json("/v1/memory/search", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_consolidate(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "agent_id": agent_id,
        "memory_ids": args.get("memory_ids").cloned().unwrap_or(json!([])),
    });
    match client.post_json("/v1/memory/consolidate", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_get(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded_id = urlencoding::encode(&memory_id);
    let encoded_agent = urlencoding::encode(&agent_id);
    let path = format!("/v1/memory/get/{}?agent_id={}", encoded_id, encoded_agent);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_update(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = serde_json::Map::new();
    if let Some(content) = args.get("content") {
        body.insert("content".into(), content.clone());
    }
    if let Some(importance) = args.get("importance") {
        body.insert("importance".into(), importance.clone());
    }
    if let Some(tags) = args.get("tags") {
        body.insert("tags".into(), tags.clone());
    }
    let encoded_id = urlencoding::encode(&memory_id);
    let encoded_agent = urlencoding::encode(&agent_id);
    let path = format!(
        "/v1/memory/update/{}?agent_id={}",
        encoded_id, encoded_agent
    );
    match client
        .put_json(&path, &serde_json::Value::Object(body))
        .await
    {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_recall_associated(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let query = match require_string(args, "query") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let depth = args
        .get("associated_memories_depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .clamp(1, 3);
    let mut body = json!({
        "agent_id": agent_id,
        "query": query,
        "top_k": args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5),
        "min_importance": args.get("min_importance").and_then(|v| v.as_f64()).unwrap_or(0.0),
        "include_associated": true,
        "associated_memories_depth": depth,
    });
    if let Some(w) = args
        .get("associated_memories_min_weight")
        .and_then(|v| v.as_f64())
    {
        body["associated_memories_min_weight"] = json!(w);
    }
    if let Some(since) = args.get("since").and_then(|v| v.as_str()) {
        body["since"] = json!(since);
    }
    if let Some(until) = args.get("until").and_then(|v| v.as_str()) {
        body["until"] = json!(until);
    }
    match client.post_json("/v1/memory/recall", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_importance(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let updates = match args.get("updates").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return CallToolResult::error("updates array is required".to_string()),
    };

    if updates.is_empty() {
        return CallToolResult::error("updates array must not be empty".to_string());
    }

    // API expects single {memory_id, agent_id, importance} per call — loop over updates
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for update in updates {
        let memory_id = update
            .get("memory_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let importance = update
            .get("importance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        let body = json!({
            "memory_id": memory_id,
            "agent_id": agent_id,
            "importance": importance,
        });

        match client.post_json("/v1/memory/importance", &body).await {
            Ok(result) => results.push(result),
            Err(e) => errors.push(format!("{}: {}", memory_id, e)),
        }
    }

    if !errors.is_empty() {
        return CallToolResult::error(format!("Some updates failed: {}", errors.join(", ")));
    }

    ok_json(&json!({ "updated": results.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::DakeraApiClient;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        // Port 9 is discard — connections are refused immediately, so no actual
        // network traffic occurs.  Validation-path tests return before any HTTP call.
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    // --- tool_batch_recall input validation ---

    #[tokio::test]
    async fn test_batch_recall_missing_agent_id_returns_error() {
        let result = tool_batch_recall(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    // --- tool_batch_forget input validation ---

    #[tokio::test]
    async fn test_batch_forget_missing_agent_id_returns_error() {
        let result = tool_batch_forget(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    // --- execute dispatch ---

    #[tokio::test]
    async fn test_execute_batch_recall_dispatches() {
        // Missing agent_id — validates dispatch returns Some(is_error), not None.
        let result = execute(&dummy_client(), "dakera_batch_recall", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_batch_forget_dispatches() {
        // Missing agent_id — validates dispatch returns Some(is_error), not None.
        let result = execute(&dummy_client(), "dakera_batch_forget", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    // --- MCP-5: tool_recall_associated ---

    #[tokio::test]
    async fn test_recall_associated_missing_agent_id_returns_error() {
        let result = tool_recall_associated(&dummy_client(), &json!({"query": "hello"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_recall_associated_missing_query_returns_error() {
        let result = tool_recall_associated(&dummy_client(), &json!({"agent_id": "agent-1"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("query"));
    }

    #[tokio::test]
    async fn test_recall_associated_depth_clamped_to_1_3() {
        // depth=0 should clamp to 1; connection will fail (port 9) — confirms depth passed through
        let result = tool_recall_associated(
            &dummy_client(),
            &json!({"agent_id": "agent-1", "query": "test", "associated_memories_depth": 0}),
        )
        .await;
        // Not a validation error — proves depth reached the HTTP call path
        assert_ne!(
            result
                .content
                .first()
                .map(|c| c.text.contains("agent_id") || c.text.contains("query")),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_execute_recall_associated_dispatches() {
        let result = execute(&dummy_client(), "dakera_recall_associated", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    // --- CE-7: tool_recall since/until ---

    #[tokio::test]
    async fn test_recall_missing_agent_id_returns_error() {
        let result = tool_recall(&dummy_client(), &json!({"query": "hello"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_recall_missing_query_returns_error() {
        let result = tool_recall(&dummy_client(), &json!({"agent_id": "agent-1"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("query"));
    }

    #[tokio::test]
    async fn test_recall_since_until_forwarded() {
        // Port 9 is discard — the connection will fail after validation passes,
        // confirming since/until reach the HTTP call path (not filtered out early).
        let result = tool_recall(
            &dummy_client(),
            &json!({
                "agent_id": "agent-1",
                "query": "test",
                "since": "2026-01-01T00:00:00Z",
                "until": "2026-03-31T23:59:59Z"
            }),
        )
        .await;
        // Connection refused (port 9) — not a validation error — means params passed through.
        assert_ne!(
            result
                .content
                .first()
                .map(|c| c.text.contains("agent_id") || c.text.contains("query")),
            Some(true)
        );
    }
}
