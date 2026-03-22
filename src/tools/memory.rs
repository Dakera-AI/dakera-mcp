//! Memory tools — store, recall, get, forget, batch_recall, batch_forget, update, importance, search, consolidate

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_store".into(),
            description: "Store a memory in Dakera. Memories are semantic units of information associated with an agent.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "content": { "type": "string", "description": "Memory content text" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"], "description": "Type of memory", "default": "episodic" },
                    "importance": { "type": "number", "description": "Importance score 0.0-1.0", "default": 0.5 },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for categorization", "default": [] },
                    "session_id": { "type": "string", "description": "Optional session ID to associate with" }
                },
                "required": ["agent_id", "content"]
            }),
        },
        ToolDefinition {
            name: "dakera_recall".into(),
            description: "Recall memories by semantic query. Returns the most relevant memories for the given query text.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "query": { "type": "string", "description": "Semantic query text" },
                    "top_k": { "type": "integer", "description": "Number of results to return", "default": 5 },
                    "min_importance": { "type": "number", "description": "Minimum importance threshold", "default": 0.0 }
                },
                "required": ["agent_id", "query"]
            }),
        },
        ToolDefinition {
            name: "dakera_forget".into(),
            description: "Delete memories matching a filter. Use with caution.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "memory_ids": { "type": "array", "items": { "type": "string" }, "description": "Specific memory IDs to delete" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Delete memories with these tags" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_batch_recall".into(),
            description: "Filter-based bulk recall without semantic search (CE-2). Returns memories matching all specified predicates. Requires at least one filter.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "All-match tag filter — only memories with ALL listed tags are returned" },
                    "min_importance": { "type": "number", "description": "Minimum importance threshold (inclusive)" },
                    "max_importance": { "type": "number", "description": "Maximum importance threshold (inclusive)" },
                    "created_after": { "type": "integer", "description": "Return memories created after this Unix timestamp" },
                    "created_before": { "type": "integer", "description": "Return memories created before this Unix timestamp" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"], "description": "Filter by memory type" },
                    "session_id": { "type": "string", "description": "Filter by session ID" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_batch_forget".into(),
            description: "Filter-based bulk delete (CE-2). Deletes all memories matching the predicates. At least one filter is required — the API enforces this to prevent accidental full-namespace wipe.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Delete memories that have ALL of these tags" },
                    "min_importance": { "type": "number", "description": "Delete memories at or above this importance" },
                    "max_importance": { "type": "number", "description": "Delete memories at or below this importance" },
                    "created_after": { "type": "integer", "description": "Delete memories created after this Unix timestamp" },
                    "created_before": { "type": "integer", "description": "Delete memories created before this Unix timestamp" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"], "description": "Delete memories of this type" },
                    "session_id": { "type": "string", "description": "Delete memories belonging to this session" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_search".into(),
            description: "Advanced memory search with filters. Supports tag filtering and importance thresholds.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "query": { "type": "string", "description": "Search query text" },
                    "top_k": { "type": "integer", "description": "Number of results", "default": 10 },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Filter by tags" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"], "description": "Filter by memory type" }
                },
                "required": ["agent_id", "query"]
            }),
        },
        ToolDefinition {
            name: "dakera_consolidate".into(),
            description: "Consolidate related memories into a summary. Reduces redundancy and creates a synthesized memory.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "memory_ids": { "type": "array", "items": { "type": "string" }, "description": "Memory IDs to consolidate" }
                },
                "required": ["agent_id", "memory_ids"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_get".into(),
            description: "Get a specific memory by ID. Returns the full memory object including content, metadata, and embedding info.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Memory ID to retrieve" },
                    "agent_id": { "type": "string", "description": "Agent identifier (owner of the memory)" }
                },
                "required": ["memory_id", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_update".into(),
            description: "Update an existing memory's content, importance, or tags. The memory is re-embedded if content changes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Memory ID to update" },
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "content": { "type": "string", "description": "New content (triggers re-embedding)" },
                    "importance": { "type": "number", "description": "New importance score 0.0-1.0" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Replace tags" }
                },
                "required": ["memory_id", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_importance".into(),
            description: "Batch-update importance scores for multiple memories. Useful for re-ranking after review.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
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
    let body = json!({
        "agent_id": agent_id,
        "content": content,
        "memory_type": args.get("memory_type").and_then(|v| v.as_str()).unwrap_or("episodic"),
        "importance": args.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.5),
        "tags": args.get("tags").cloned().unwrap_or(json!([])),
        "session_id": args.get("session_id"),
    });
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
    let body = json!({
        "agent_id": agent_id,
        "query": query,
        "top_k": args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5),
        "min_importance": args.get("min_importance").and_then(|v| v.as_f64()).unwrap_or(0.0),
    });
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
    let mut body = serde_json::Map::new();
    body.insert("agent_id".into(), json!(agent_id));
    if let Some(v) = args.get("tags") {
        body.insert("tags".into(), v.clone());
    }
    if let Some(v) = args.get("min_importance") {
        body.insert("min_importance".into(), v.clone());
    }
    if let Some(v) = args.get("max_importance") {
        body.insert("max_importance".into(), v.clone());
    }
    if let Some(v) = args.get("created_after") {
        body.insert("created_after".into(), v.clone());
    }
    if let Some(v) = args.get("created_before") {
        body.insert("created_before".into(), v.clone());
    }
    if let Some(v) = args.get("memory_type") {
        body.insert("memory_type".into(), v.clone());
    }
    if let Some(v) = args.get("session_id") {
        body.insert("session_id".into(), v.clone());
    }
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
    let mut body = serde_json::Map::new();
    body.insert("agent_id".into(), json!(agent_id));
    if let Some(v) = args.get("tags") {
        body.insert("tags".into(), v.clone());
    }
    if let Some(v) = args.get("min_importance") {
        body.insert("min_importance".into(), v.clone());
    }
    if let Some(v) = args.get("max_importance") {
        body.insert("max_importance".into(), v.clone());
    }
    if let Some(v) = args.get("created_after") {
        body.insert("created_after".into(), v.clone());
    }
    if let Some(v) = args.get("created_before") {
        body.insert("created_before".into(), v.clone());
    }
    if let Some(v) = args.get("memory_type") {
        body.insert("memory_type".into(), v.clone());
    }
    if let Some(v) = args.get("session_id") {
        body.insert("session_id".into(), v.clone());
    }
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
