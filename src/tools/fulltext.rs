//! Full-text search tools — index, search, delete, stats, hybrid

use serde_json::json;

use crate::protocol::{CallToolResult, ToolDefinition};
use super::{DakeraApiClient, ok_json, require_string};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_fulltext_index".into(),
            description: "Index documents for full-text search in a namespace. Each document has an id, text content, and optional metadata.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to index documents in" },
                    "documents": {
                        "type": "array",
                        "description": "Documents to index",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Document ID" },
                                "text": { "type": "string", "description": "Document text content" },
                                "metadata": { "type": "object", "description": "Optional metadata" }
                            },
                            "required": ["id", "text"]
                        }
                    }
                },
                "required": ["namespace", "documents"]
            }),
        },
        ToolDefinition {
            name: "dakera_fulltext_search".into(),
            description: "Search documents using full-text search with BM25 scoring.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to search in" },
                    "query": { "type": "string", "description": "Search query text" },
                    "top_k": { "type": "integer", "description": "Number of results to return", "default": 10 },
                    "filter": { "type": "object", "description": "Optional metadata filter" }
                },
                "required": ["namespace", "query"]
            }),
        },
        ToolDefinition {
            name: "dakera_fulltext_delete".into(),
            description: "Delete documents from the full-text index by their IDs.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to delete from" },
                    "ids": {
                        "type": "array",
                        "description": "Document IDs to delete",
                        "items": { "type": "string" }
                    }
                },
                "required": ["namespace", "ids"]
            }),
        },
        ToolDefinition {
            name: "dakera_fulltext_stats".into(),
            description: "Get statistics about the full-text index for a namespace, including document count, unique terms, and average document length.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to get stats for" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_hybrid_search".into(),
            description: "Hybrid search combining vector similarity and full-text BM25 search. Returns results scored by weighted combination of both signals.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to search in" },
                    "vector": {
                        "type": "array",
                        "description": "Query vector for similarity search",
                        "items": { "type": "number" }
                    },
                    "text": { "type": "string", "description": "Text query for full-text search" },
                    "top_k": { "type": "integer", "description": "Number of results to return", "default": 10 },
                    "vector_weight": { "type": "number", "description": "Weight for vector score (0.0-1.0). Text weight is 1-vector_weight.", "default": 0.5 },
                    "include_metadata": { "type": "boolean", "description": "Include metadata in results", "default": true },
                    "include_vectors": { "type": "boolean", "description": "Include vectors in results", "default": false },
                    "filter": { "type": "object", "description": "Optional metadata filter" }
                },
                "required": ["namespace", "vector", "text"]
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
        "dakera_fulltext_index" => Some(tool_fulltext_index(client, args).await),
        "dakera_fulltext_search" => Some(tool_fulltext_search(client, args).await),
        "dakera_fulltext_delete" => Some(tool_fulltext_delete(client, args).await),
        "dakera_fulltext_stats" => Some(tool_fulltext_stats(client, args).await),
        "dakera_hybrid_search" => Some(tool_hybrid_search(client, args).await),
        _ => None,
    }
}

async fn tool_fulltext_index(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
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
    let body = json!({
        "documents": documents,
    });
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/fulltext/index", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_fulltext_search(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let query = match require_string(args, "query") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10);
    let mut body = json!({
        "query": query,
        "top_k": top_k,
    });
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/fulltext/search", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_fulltext_delete(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let ids = match args.get("ids").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return CallToolResult::error("Missing or empty required parameter: ids".to_string()),
    };
    let body = json!({
        "ids": ids,
    });
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/fulltext/delete", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_fulltext_stats(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/fulltext/stats", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_hybrid_search(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let vector = match args.get("vector").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        _ => {
            return CallToolResult::error(
                "Missing or empty required parameter: vector".to_string(),
            )
        }
    };
    let text = match require_string(args, "text") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10);
    let vector_weight = args.get("vector_weight").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let include_metadata = args.get("include_metadata").and_then(|v| v.as_bool()).unwrap_or(true);
    let include_vectors = args.get("include_vectors").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut body = json!({
        "vector": vector,
        "text": text,
        "top_k": top_k,
        "vector_weight": vector_weight,
        "include_metadata": include_metadata,
        "include_vectors": include_vectors,
    });
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/hybrid", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}
