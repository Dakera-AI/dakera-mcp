//! Inference tools — text_query, upsert_text, batch_query_text

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_text_query".into(),
            description: "Query a namespace using natural language text. The text is embedded server-side and used for similarity search.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to query" },
                    "text": { "type": "string", "description": "Natural language query text" },
                    "top_k": { "type": "integer", "description": "Number of results", "default": 10 },
                    "filter": { "type": "object", "description": "Optional metadata filter" }
                },
                "required": ["namespace", "text"]
            }),
        },
        ToolDefinition {
            name: "dakera_upsert_text".into(),
            description: "Upsert text documents with automatic embedding generation. The text is embedded server-side using the specified model and stored as vectors.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to upsert into" },
                    "documents": {
                        "type": "array",
                        "description": "Text documents to embed and store",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Document ID" },
                                "text": { "type": "string", "description": "Text content to embed" },
                                "metadata": { "type": "object", "description": "Optional metadata" },
                                "ttl_seconds": { "type": "integer", "description": "Optional TTL in seconds" }
                            },
                            "required": ["id", "text"]
                        }
                    },
                    "model": { "type": "string", "description": "Embedding model (default: minilm)" }
                },
                "required": ["namespace", "documents"]
            }),
        },
        ToolDefinition {
            name: "dakera_batch_query_text".into(),
            description: "Batch query using multiple text queries with automatic embedding. Runs multiple similarity searches in one request.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to query" },
                    "queries": {
                        "type": "array",
                        "description": "Multiple query texts",
                        "items": { "type": "string" }
                    },
                    "top_k": { "type": "integer", "description": "Number of results per query", "default": 10 },
                    "filter": { "type": "object", "description": "Optional filter for all queries" },
                    "include_vectors": { "type": "boolean", "description": "Include vectors in response", "default": false },
                    "model": { "type": "string", "description": "Embedding model (default: minilm)" }
                },
                "required": ["namespace", "queries"]
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
        "dakera_text_query" => Some(tool_text_query(client, args).await),
        "dakera_upsert_text" => Some(tool_upsert_text(client, args).await),
        "dakera_batch_query_text" => Some(tool_batch_query_text(client, args).await),
        _ => None,
    }
}

async fn tool_text_query(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let text = match require_string(args, "text") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10);
    let mut body = json!({
        "text": text,
        "top_k": top_k,
    });
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/query-text", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_upsert_text(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let documents = match args.get("documents").and_then(|v| v.as_array()) {
        Some(d) if !d.is_empty() => d,
        _ => {
            return CallToolResult::error(
                "Missing or empty required parameter: documents".to_string(),
            )
        }
    };
    let mut body = json!({
        "documents": documents,
    });
    if let Some(model) = args.get("model").and_then(|v| v.as_str()) {
        body["model"] = json!(model);
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/upsert-text", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_batch_query_text(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let queries = match args.get("queries").and_then(|v| v.as_array()) {
        Some(q) if !q.is_empty() => q,
        _ => {
            return CallToolResult::error(
                "Missing or empty required parameter: queries".to_string(),
            )
        }
    };
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10);
    let include_vectors = args
        .get("include_vectors")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut body = json!({
        "queries": queries,
        "top_k": top_k,
        "include_vectors": include_vectors,
    });
    if let Some(model) = args.get("model").and_then(|v| v.as_str()) {
        body["model"] = json!(model);
    }
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/batch-query-text", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}
