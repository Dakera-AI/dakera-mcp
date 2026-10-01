//! Namespace tools — list, get, create, delete, configure (upsert)

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_namespace_list".into(),
            description: "List the names of the namespaces this API key can see (use dakera_namespace_get for a namespace's dimension, metric and count). Agent memories live in _dakera_agent_<agent_id>.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_get".into(),
            description: "Fetch a namespace's dimension, distance metric, vector count, index type and estimated storage. Use to confirm dimension compatibility before upserting.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_create".into(),
            description: "Create a vector namespace with the specified dimension and distance metric. dimension must match your model (e.g. 384 for MiniLM, 1536 for ada-002). Use dakera_namespace_configure for idempotent create-or-update.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Namespace name" },
                    "dimension": { "type": "integer", "description": "Vector dimensions (e.g. 384, 768, 1536)" },
                    "distance": { "type": "string", "enum": ["cosine", "euclidean", "dot"], "description": "Distance metric" }
                },
                "required": ["name", "dimension"]
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_delete".into(),
            description: "Permanently delete a namespace and all of its vectors, full-text index, and metadata. This action is irreversible and takes effect immediately — verify the namespace name before calling.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name to delete" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_configure".into(),
            description: "Idempotent create-or-update for a namespace. Creates if absent; updates distance metric if present. Prefer over dakera_namespace_create in deployment scripts. dimension must match on updates.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" },
                    "dimension": { "type": "integer", "description": "Vector dimensions (e.g. 384, 768, 1536). Required on creation; must match existing value on update." },
                    "distance": { "type": "string", "enum": ["cosine", "euclidean", "dot"], "description": "Distance metric" }
                },
                "required": ["namespace", "dimension"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_policy_get".into(),
            description: "Read the memory lifecycle policy for a namespace: TTL per type, decay curve, spaced-repetition, consolidation, and rate limits. Check before modifying with dakera_memory_policy_set.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_policy_set".into(),
            description: "Update the memory lifecycle policy for a namespace. Only provided fields are overwritten. Decay curves: exponential|linear|step_function|power_law|logarithmic|flat. Use to tune memory TTLs, store-time dedup or auto-consolidation. Needs write on the namespace.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" },
                    "working_ttl_seconds": { "type": "integer", "description": "TTL for working memories (seconds)" },
                    "episodic_ttl_seconds": { "type": "integer", "description": "TTL for episodic memories (seconds)" },
                    "semantic_ttl_seconds": { "type": "integer", "description": "TTL for semantic memories (seconds)" },
                    "procedural_ttl_seconds": { "type": "integer", "description": "TTL for procedural memories (seconds)" },
                    "working_decay": { "type": "string", "enum": ["exponential", "linear", "step_function", "power_law", "logarithmic", "flat"], "description": "Working decay curve" },
                    "episodic_decay": { "type": "string", "enum": ["exponential", "linear", "step_function", "power_law", "logarithmic", "flat"], "description": "Episodic decay curve" },
                    "semantic_decay": { "type": "string", "enum": ["exponential", "linear", "step_function", "power_law", "logarithmic", "flat"], "description": "Semantic decay curve" },
                    "procedural_decay": { "type": "string", "enum": ["exponential", "linear", "step_function", "power_law", "logarithmic", "flat"], "description": "Procedural decay curve" },
                    "spaced_repetition_factor": { "type": "number", "description": "TTL extension multiplier per recall (0.0 disables)" },
                    "spaced_repetition_base_interval_seconds": { "type": "integer", "description": "Base interval for spaced repetition (seconds)" },
                    "consolidation_enabled": { "type": "boolean", "description": "Enable background deduplication" },
                    "consolidation_threshold": { "type": "number", "description": "Cosine similarity threshold for merging (0.85–0.99)" },
                    "consolidation_interval_hours": { "type": "integer", "description": "Deduplication job interval (hours)" },
                    "rate_limit_enabled": { "type": "boolean", "description": "Enable rate limiting" },
                    "rate_limit_stores_per_minute": { "type": ["integer", "null"], "description": "Max store ops/min (null = unlimited)" },
                    "rate_limit_recalls_per_minute": { "type": ["integer", "null"], "description": "Max recall ops/min (null = unlimited)" },
                    "dedup_on_store": { "type": "boolean", "description": "Reject a store whose nearest memory is at least dedup_threshold similar (returns the existing id)" },
                    "dedup_threshold": { "type": "number", "description": "Similarity for dedup_on_store (server default 0.95)" }
                },
                "required": ["namespace"]
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
        "dakera_namespace_list" => Some(tool_namespace_list(client).await),
        "dakera_namespace_get" => Some(tool_namespace_get(client, args).await),
        "dakera_namespace_create" => Some(tool_namespace_create(client, args).await),
        "dakera_namespace_delete" => Some(tool_namespace_delete(client, args).await),
        "dakera_namespace_configure" => Some(tool_namespace_configure(client, args).await),
        "dakera_memory_policy_get" => Some(tool_memory_policy_get(client, args).await),
        "dakera_memory_policy_set" => Some(tool_memory_policy_set(client, args).await),
        _ => None,
    }
}

async fn tool_namespace_list(client: &DakeraApiClient) -> CallToolResult {
    match client.get_json("/v1/namespaces").await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_namespace_get(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_namespace_create(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let name = match require_string(args, "name") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let dimension = match args.get("dimension").and_then(|v| v.as_u64()) {
        Some(d) => d,
        None => return CallToolResult::error("Missing required parameter: dimension".to_string()),
    };
    let body = json!({
        "name": name,
        "dimension": dimension,
        "distance": args.get("distance").and_then(|v| v.as_str()).unwrap_or("cosine"),
    });
    match client.post_json("/v1/namespaces", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_namespace_delete(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}", encoded);
    match client.delete_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_namespace_configure(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let dimension = match args.get("dimension").and_then(|v| v.as_u64()) {
        Some(d) => d,
        None => return CallToolResult::error("Missing required parameter: dimension".to_string()),
    };
    let mut body = json!({ "dimension": dimension });
    if let Some(distance) = args.get("distance").and_then(|v| v.as_str()) {
        body["distance"] = json!(distance);
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}", encoded);
    match client.put_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_policy_get(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/memory_policy", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_policy_set(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&namespace);
    let policy_path = format!("/v1/namespaces/{}/memory_policy", encoded);

    // GET current policy so we only overwrite provided fields (safe partial update).
    let mut policy = match client.get_json(&policy_path).await {
        Ok(p) => p,
        Err(e) => return CallToolResult::error(e),
    };

    // Merge provided fields over the fetched policy.
    let fields = [
        "working_ttl_seconds",
        "episodic_ttl_seconds",
        "semantic_ttl_seconds",
        "procedural_ttl_seconds",
        "working_decay",
        "episodic_decay",
        "semantic_decay",
        "procedural_decay",
        "spaced_repetition_factor",
        "spaced_repetition_base_interval_seconds",
        "consolidation_enabled",
        "consolidation_threshold",
        "consolidation_interval_hours",
        "rate_limit_enabled",
        "rate_limit_stores_per_minute",
        "rate_limit_recalls_per_minute",
        "dedup_on_store",
        "dedup_threshold",
    ];
    for field in &fields {
        if let Some(v) = args.get(*field) {
            policy[*field] = v.clone();
        }
    }

    strip_read_only_policy_fields(&mut policy);
    match client.put_json(&policy_path, &policy).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

/// Drop what the policy GET reports but a PUT must not set: `consolidated_count`
/// is the server's own counter (v0.12 refuses a PUT that changes it, and the
/// background consolidation can move it between the GET and the PUT).
pub fn strip_read_only_policy_fields(policy: &mut serde_json::Value) {
    if let Some(obj) = policy.as_object_mut() {
        obj.remove("consolidated_count");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::DakeraApiClient;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        // Port 9 is discard — connections are refused immediately, so no actual
        // network traffic occurs.  These tests return before reaching the HTTP
        // call, so the URL is irrelevant.
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    // --- tool_namespace_configure input validation (v0.2.2) ---

    #[tokio::test]
    async fn test_configure_missing_namespace_returns_error() {
        let result = tool_namespace_configure(&dummy_client(), &json!({"dimension": 4})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_configure_missing_dimension_returns_error() {
        let result =
            tool_namespace_configure(&dummy_client(), &json!({"namespace": "test-ns"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("dimension"));
    }

    #[tokio::test]
    async fn test_configure_empty_args_returns_error() {
        let result = tool_namespace_configure(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
    }

    // --- execute dispatch ---

    #[tokio::test]
    async fn test_execute_unknown_tool_returns_none() {
        let result = execute(&dummy_client(), "not_a_namespace_tool", &json!({})).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_execute_configure_dispatches() {
        // Passes validation but hits unreachable server — verifies dispatch not validation.
        let result = execute(&dummy_client(), "dakera_namespace_configure", &json!({})).await;
        // Returns Some (dispatched) and is_error because namespace param is missing.
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    // --- MCP-5: dakera_memory_policy_get ---

    #[tokio::test]
    async fn test_memory_policy_get_missing_namespace_returns_error() {
        let result = tool_memory_policy_get(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_execute_memory_policy_get_dispatches() {
        let result = execute(&dummy_client(), "dakera_memory_policy_get", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    // --- MCP-5: dakera_memory_policy_set ---

    #[tokio::test]
    async fn test_memory_policy_set_missing_namespace_returns_error() {
        let result = tool_memory_policy_set(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_execute_memory_policy_set_dispatches() {
        let result = execute(&dummy_client(), "dakera_memory_policy_set", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }
}
