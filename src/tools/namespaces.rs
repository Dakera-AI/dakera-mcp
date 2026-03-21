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
