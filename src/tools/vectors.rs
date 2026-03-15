//! Vector tools — upsert, query, delete, batch_query, bulk_update, bulk_delete,
//! count, export, aggregate

use serde_json::json;

use crate::protocol::{CallToolResult, ToolDefinition};
use super::{DakeraApiClient, ok_json, require_string};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_vector_upsert".into(),
            description: "Upsert vectors into a namespace. Each vector has an ID, float array, and optional metadata.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Target namespace" },
                    "vectors": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "values": { "type": "array", "items": { "type": "number" } },
                                "metadata": { "type": "object" }
                            },
                            "required": ["id", "values"]
                        },
                        "description": "Vectors to upsert"
                    }
                },
                "required": ["namespace", "vectors"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_query".into(),
            description: "Query vectors by similarity in a namespace. Returns the nearest neighbors to the given vector.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to query" },
                    "vector": { "type": "array", "items": { "type": "number" }, "description": "Query vector" },
                    "top_k": { "type": "integer", "description": "Number of results", "default": 10 },
                    "filter": { "type": "object", "description": "Optional metadata filter" }
                },
                "required": ["namespace", "vector"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_delete".into(),
            description: "Delete vectors by ID from a namespace.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace" },
                    "ids": { "type": "array", "items": { "type": "string" }, "description": "Vector IDs to delete" }
                },
                "required": ["namespace", "ids"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_batch_query".into(),
            description: "Batch query multiple vectors at once. Runs multiple similarity searches in parallel and returns results for each query.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to query" },
                    "queries": {
                        "type": "array",
                        "description": "Multiple query vectors to search",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Optional query identifier" },
                                "vector": { "type": "array", "items": { "type": "number" }, "description": "Query vector" },
                                "top_k": { "type": "integer", "description": "Number of results for this query", "default": 10 },
                                "filter": { "type": "object", "description": "Optional filter for this query" },
                                "include_metadata": { "type": "boolean", "description": "Include metadata in results", "default": true }
                            },
                            "required": ["vector"]
                        }
                    }
                },
                "required": ["namespace", "queries"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_bulk_update".into(),
            description: "Update metadata on vectors matching a filter. Applies the same update to all matching vectors in the namespace.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace containing vectors to update" },
                    "filter": { "type": "object", "description": "Filter to select vectors to update (same format as query filters)" },
                    "update": { "type": "object", "description": "Metadata update to apply to matching vectors (key-value pairs to set)" }
                },
                "required": ["namespace", "filter", "update"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_bulk_delete".into(),
            description: "Delete all vectors matching a filter from a namespace.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to delete vectors from" },
                    "filter": { "type": "object", "description": "Filter to select vectors to delete (same format as query filters)" }
                },
                "required": ["namespace", "filter"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_count".into(),
            description: "Count vectors in a namespace, optionally filtered. If no filter is provided, returns the total count.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to count vectors in" },
                    "filter": { "type": "object", "description": "Optional filter to count only matching vectors" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_export".into(),
            description: "Export vectors from a namespace with pagination. Returns vectors with optional values and metadata.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to export from" },
                    "top_k": { "type": "integer", "description": "Number of vectors per page (max 10000)", "default": 1000 },
                    "cursor": { "type": "string", "description": "Pagination cursor from previous response" },
                    "include_vectors": { "type": "boolean", "description": "Include vector values", "default": true },
                    "include_metadata": { "type": "boolean", "description": "Include metadata", "default": true }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_aggregate".into(),
            description: "Compute aggregations over vectors in a namespace. Supports Count, Sum, Avg, Min, Max on metadata fields with optional grouping.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to aggregate" },
                    "aggregate_by": {
                        "type": "object",
                        "description": "Named aggregations. Values are arrays like [\"Count\"], [\"Sum\", \"field\"], [\"Avg\", \"field\"], [\"Min\", \"field\"], [\"Max\", \"field\"]"
                    },
                    "group_by": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional fields to group results by"
                    },
                    "filter": { "type": "object", "description": "Optional filter before aggregation" }
                },
                "required": ["namespace", "aggregate_by"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_multi_search".into(),
            description: "Multi-vector search with positive and negative vectors. Supports weighted combination, MMR diversity, and score thresholds.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to search" },
                    "positive_vectors": {
                        "type": "array",
                        "items": { "type": "array", "items": { "type": "number" } },
                        "description": "Vectors to search towards (at least one required)"
                    },
                    "positive_weights": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Weights for positive vectors (optional, defaults to equal)"
                    },
                    "negative_vectors": {
                        "type": "array",
                        "items": { "type": "array", "items": { "type": "number" } },
                        "description": "Vectors to search away from (optional)"
                    },
                    "negative_weights": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Weights for negative vectors (optional)"
                    },
                    "top_k": { "type": "integer", "description": "Number of results", "default": 10 },
                    "score_threshold": { "type": "number", "description": "Minimum score threshold" },
                    "enable_mmr": { "type": "boolean", "description": "Enable MMR for diversity", "default": false },
                    "mmr_lambda": { "type": "number", "description": "MMR lambda (0=diversity, 1=relevance)", "default": 0.5 },
                    "filter": { "type": "object", "description": "Optional metadata filter" }
                },
                "required": ["namespace", "positive_vectors"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_upsert_columns".into(),
            description: "Upsert vectors in column format. All arrays must have equal length. More efficient for batch operations.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Target namespace" },
                    "ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of vector IDs"
                    },
                    "vectors": {
                        "type": "array",
                        "items": { "type": "array", "items": { "type": "number" } },
                        "description": "Array of vector values"
                    },
                    "attributes": {
                        "type": "object",
                        "description": "Column attributes as {name: [values...]}. Each array must match ids length."
                    }
                },
                "required": ["namespace", "ids", "vectors"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_explain".into(),
            description: "Explain a query execution plan. Shows index selection, execution stages, cost estimates, and optimization recommendations.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to explain query against" },
                    "query_type": {
                        "type": "string",
                        "enum": ["vector_search", "full_text_search", "hybrid_search", "multi_vector", "batch_query"],
                        "description": "Type of query to explain"
                    },
                    "vector": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Query vector (for vector/hybrid/multi_vector searches)"
                    },
                    "top_k": { "type": "integer", "description": "Number of results", "default": 10 },
                    "filter": { "type": "object", "description": "Optional filter expression" },
                    "text_query": { "type": "string", "description": "Text query (for full_text/hybrid searches)" },
                    "execute": { "type": "boolean", "description": "Actually execute the query for real stats", "default": false },
                    "verbose": { "type": "boolean", "description": "Include verbose output", "default": false }
                },
                "required": ["namespace", "query_type"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_warm".into(),
            description: "Warm the cache for a namespace. Pre-loads vectors into memory for faster subsequent queries.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to warm" },
                    "vector_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Specific vector IDs to warm (omit to warm all)"
                    }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_vector_unified_query".into(),
            description: "Unified query with flexible ranking. Supports vector search, full-text search, attribute ordering, and combined rankings (Sum, Max, Product).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to query" },
                    "rank_by": {
                        "description": "Ranking function as JSON array. Examples: [\"vector\", \"ANN\", [0.1, 0.2]], [\"text\", \"BM25\", \"query\"], [\"timestamp\", \"desc\"], [\"Sum\", [[\"title\", \"BM25\", \"q\"], [\"vector\", \"ANN\", [0.1]]]]"
                    },
                    "top_k": { "type": "integer", "description": "Number of results", "default": 10 },
                    "filter": { "type": "object", "description": "Optional metadata filter" },
                    "include_metadata": { "type": "boolean", "description": "Include metadata in results", "default": true },
                    "include_vectors": { "type": "boolean", "description": "Include vector values in results", "default": false }
                },
                "required": ["namespace", "rank_by"]
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
        "dakera_vector_upsert" => Some(tool_vector_upsert(client, args).await),
        "dakera_vector_query" => Some(tool_vector_query(client, args).await),
        "dakera_vector_delete" => Some(tool_vector_delete(client, args).await),
        "dakera_vector_batch_query" => Some(tool_vector_batch_query(client, args).await),
        "dakera_vector_bulk_update" => Some(tool_vector_bulk_update(client, args).await),
        "dakera_vector_bulk_delete" => Some(tool_vector_bulk_delete(client, args).await),
        "dakera_vector_count" => Some(tool_vector_count(client, args).await),
        "dakera_vector_export" => Some(tool_vector_export(client, args).await),
        "dakera_vector_aggregate" => Some(tool_vector_aggregate(client, args).await),
        "dakera_vector_multi_search" => Some(tool_vector_multi_search(client, args).await),
        "dakera_vector_upsert_columns" => Some(tool_vector_upsert_columns(client, args).await),
        "dakera_vector_explain" => Some(tool_vector_explain(client, args).await),
        "dakera_vector_warm" => Some(tool_vector_warm(client, args).await),
        "dakera_vector_unified_query" => Some(tool_vector_unified_query(client, args).await),
        _ => None,
    }
}

async fn tool_vector_upsert(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "vectors": args.get("vectors").cloned().unwrap_or(json!([])),
    });
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/vectors", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_query(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = serde_json::Map::new();
    body.insert("vector".into(), args.get("vector").cloned().unwrap_or(json!([])));
    body.insert("top_k".into(), json!(args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10)));
    if let Some(filter) = args.get("filter") {
        body.insert("filter".into(), filter.clone());
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/query", encoded);
    match client.post_json(&path, &serde_json::Value::Object(body)).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_delete(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "ids": args.get("ids").cloned().unwrap_or(json!([])),
    });
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/vectors/delete", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_batch_query(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let queries = match args.get("queries").and_then(|v| v.as_array()) {
        Some(q) if !q.is_empty() => q,
        _ => return CallToolResult::error("Missing or empty required parameter: queries".to_string()),
    };
    let body = json!({
        "queries": queries,
    });
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/batch-query", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_bulk_update(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let filter = match args.get("filter") {
        Some(f) if f.is_object() => f,
        _ => return CallToolResult::error("Missing or invalid required parameter: filter (must be a JSON object)".to_string()),
    };
    let update = match args.get("update") {
        Some(u) if u.is_object() => u,
        _ => return CallToolResult::error("Missing or invalid required parameter: update (must be a JSON object)".to_string()),
    };
    let body = json!({
        "filter": filter,
        "update": update,
    });
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/vectors/bulk-update", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_bulk_delete(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let filter = match args.get("filter") {
        Some(f) if f.is_object() => f,
        _ => return CallToolResult::error("Missing or invalid required parameter: filter (must be a JSON object)".to_string()),
    };
    let body = json!({
        "filter": filter,
    });
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/vectors/bulk-delete", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_count(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({});
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/vectors/count", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_export(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(1000);
    let include_vectors = args.get("include_vectors").and_then(|v| v.as_bool()).unwrap_or(true);
    let include_metadata = args.get("include_metadata").and_then(|v| v.as_bool()).unwrap_or(true);
    let mut body = json!({
        "top_k": top_k,
        "include_vectors": include_vectors,
        "include_metadata": include_metadata,
    });
    if let Some(cursor) = args.get("cursor").and_then(|v| v.as_str()) {
        body["cursor"] = json!(cursor);
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/export", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_aggregate(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let aggregate_by = match args.get("aggregate_by") {
        Some(a) if a.is_object() => a,
        _ => return CallToolResult::error("Missing or invalid required parameter: aggregate_by (must be a JSON object)".to_string()),
    };
    let mut body = json!({
        "aggregate_by": aggregate_by,
    });
    if let Some(group_by) = args.get("group_by").and_then(|v| v.as_array()) {
        body["group_by"] = json!(group_by);
    }
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/aggregate", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_multi_search(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let positive_vectors = match args.get("positive_vectors").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        _ => return CallToolResult::error("Missing or empty required parameter: positive_vectors".to_string()),
    };
    let mut body = json!({
        "positive_vectors": positive_vectors,
    });
    if let Some(pw) = args.get("positive_weights") {
        body["positive_weights"] = pw.clone();
    }
    if let Some(nv) = args.get("negative_vectors") {
        body["negative_vectors"] = nv.clone();
    }
    if let Some(nw) = args.get("negative_weights") {
        body["negative_weights"] = nw.clone();
    }
    body["top_k"] = json!(args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10));
    if let Some(st) = args.get("score_threshold") {
        body["score_threshold"] = st.clone();
    }
    body["enable_mmr"] = json!(args.get("enable_mmr").and_then(|v| v.as_bool()).unwrap_or(false));
    if let Some(ml) = args.get("mmr_lambda") {
        body["mmr_lambda"] = ml.clone();
    }
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/multi-vector", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_upsert_columns(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let ids = match args.get("ids").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        _ => return CallToolResult::error("Missing or empty required parameter: ids".to_string()),
    };
    let vectors = match args.get("vectors").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        _ => return CallToolResult::error("Missing or empty required parameter: vectors".to_string()),
    };
    let mut body = json!({
        "ids": ids,
        "vectors": vectors,
    });
    if let Some(attrs) = args.get("attributes") {
        body["attributes"] = attrs.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/upsert-columns", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_explain(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let query_type = match require_string(args, "query_type") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({
        "query_type": query_type,
    });
    if let Some(vector) = args.get("vector") {
        body["vector"] = vector.clone();
    }
    body["top_k"] = json!(args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10));
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    if let Some(text_query) = args.get("text_query") {
        body["text_query"] = text_query.clone();
    }
    body["execute"] = json!(args.get("execute").and_then(|v| v.as_bool()).unwrap_or(false));
    body["verbose"] = json!(args.get("verbose").and_then(|v| v.as_bool()).unwrap_or(false));
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/explain", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_warm(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({});
    if let Some(ids) = args.get("vector_ids") {
        body["vector_ids"] = ids.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/warm", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_vector_unified_query(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let rank_by = match args.get("rank_by") {
        Some(r) => r,
        None => return CallToolResult::error("Missing required parameter: rank_by".to_string()),
    };
    let mut body = json!({
        "rank_by": rank_by,
        "top_k": args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10),
        "include_metadata": args.get("include_metadata").and_then(|v| v.as_bool()).unwrap_or(true),
        "include_vectors": args.get("include_vectors").and_then(|v| v.as_bool()).unwrap_or(false),
    });
    if let Some(filter) = args.get("filter") {
        body["filter"] = filter.clone();
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/unified-query", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}
