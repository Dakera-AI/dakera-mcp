//! Namespace tools — list, get, create, delete, configure (upsert)

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_namespace_list".into(),
            description: "List all namespaces in the Dakera instance. Namespaces are isolated vector collections.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_get".into(),
            description: "Get details for a specific namespace including vector count, dimensions, and index stats.".into(),
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
            description: "Create a new namespace with specified dimensions and distance metric.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Namespace name" },
                    "dimension": { "type": "integer", "description": "Vector dimensions (e.g. 384, 768, 1536)" },
                    "distance": { "type": "string", "enum": ["cosine", "euclidean", "dot"], "description": "Distance metric", "default": "cosine" }
                },
                "required": ["name", "dimension"]
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_delete".into(),
            description: "Delete a namespace and all its vectors. This action is irreversible.".into(),
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
            description: "Create-or-update (upsert) a namespace configuration. Creates the namespace if it does not exist; updates the distance metric if it does. Dimension must match on updates. Requires Write scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" },
                    "dimension": { "type": "integer", "description": "Vector dimensions (e.g. 384, 768, 1536). Required on creation; must match existing value on update." },
                    "distance": { "type": "string", "enum": ["cosine", "euclidean", "dot"], "description": "Distance metric (default: cosine)" }
                },
                "required": ["namespace", "dimension"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_policy_get".into(),
            description: "Get the memory lifecycle policy for a namespace (COG-1). Returns TTLs, decay curves, spaced repetition config, consolidation settings (COG-3), and rate limit config (SEC-5). Requires Read scope.".into(),
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
            description: "Update the memory lifecycle policy for a namespace (COG-1). All fields are optional — only provided fields overwrite the current policy; all others are preserved via GET+merge+PUT. Requires Write scope.\n\nSettable fields: working_ttl_seconds, episodic_ttl_seconds, semantic_ttl_seconds, procedural_ttl_seconds, working_decay, episodic_decay, semantic_decay, procedural_decay (values: exponential|linear|step|power_law|logarithmic|flat), spaced_repetition_factor, spaced_repetition_base_interval_seconds, consolidation_enabled, consolidation_threshold (0.85–0.99), consolidation_interval_hours, rate_limit_enabled, rate_limit_stores_per_minute, rate_limit_recalls_per_minute.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" },
                    "working_ttl_seconds": { "type": "integer", "description": "TTL for working memories in seconds (default: 14400 = 4h)" },
                    "episodic_ttl_seconds": { "type": "integer", "description": "TTL for episodic memories in seconds (default: 2592000 = 30d)" },
                    "semantic_ttl_seconds": { "type": "integer", "description": "TTL for semantic memories in seconds (default: 31536000 = 365d)" },
                    "procedural_ttl_seconds": { "type": "integer", "description": "TTL for procedural memories in seconds (default: 63072000 = 730d)" },
                    "working_decay": { "type": "string", "enum": ["exponential", "linear", "step", "power_law", "logarithmic", "flat"], "description": "Decay strategy for working memories" },
                    "episodic_decay": { "type": "string", "enum": ["exponential", "linear", "step", "power_law", "logarithmic", "flat"], "description": "Decay strategy for episodic memories" },
                    "semantic_decay": { "type": "string", "enum": ["exponential", "linear", "step", "power_law", "logarithmic", "flat"], "description": "Decay strategy for semantic memories" },
                    "procedural_decay": { "type": "string", "enum": ["exponential", "linear", "step", "power_law", "logarithmic", "flat"], "description": "Decay strategy for procedural memories" },
                    "spaced_repetition_factor": { "type": "number", "description": "Multiplier for TTL extension per recall (0.0 disables, default: 1.0)" },
                    "spaced_repetition_base_interval_seconds": { "type": "integer", "description": "Base interval in seconds for spaced repetition TTL extension (default: 86400 = 1d)" },
                    "consolidation_enabled": { "type": "boolean", "description": "COG-3: enable background DBSCAN deduplication (default: false)" },
                    "consolidation_threshold": { "type": "number", "description": "COG-3: cosine similarity threshold for merging (0.85–0.99, default: 0.92)" },
                    "consolidation_interval_hours": { "type": "integer", "description": "COG-3: how often the consolidation job runs in hours (default: 24)" },
                    "rate_limit_enabled": { "type": "boolean", "description": "SEC-5: master rate-limit switch (default: false)" },
                    "rate_limit_stores_per_minute": { "type": "integer", "description": "SEC-5: max store ops/min for this namespace (null = unlimited)" },
                    "rate_limit_recalls_per_minute": { "type": "integer", "description": "SEC-5: max recall ops/min for this namespace (null = unlimited)" }
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
    ];
    for field in &fields {
        if let Some(v) = args.get(*field) {
            policy[*field] = v.clone();
        }
    }

    match client.put_json(&policy_path, &policy).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
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
