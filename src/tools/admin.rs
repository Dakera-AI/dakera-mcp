//! Admin tools — cluster, quotas, backups, slow queries, namespaces, indexes, cache, config, TTL

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

// ── Tool definitions ──────────────────────────────────────────────────────────

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        // --- Cluster ---
        ToolDefinition {
            name: "dakera_cluster_status".into(),
            description: "Get cluster status including node count, leader, and health. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_cluster_nodes".into(),
            description: "List all cluster nodes with their roles and health state. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_cluster_replication".into(),
            description: "Get cluster replication lag and sync state per node. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_list_shards".into(),
            description: "List all shards and their distribution across cluster nodes. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_rebalance_shards".into(),
            description: "Trigger a shard rebalance across cluster nodes. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_maintenance_status".into(),
            description: "Check whether the cluster is currently in maintenance mode. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_enable_maintenance".into(),
            description: "Put the cluster into maintenance mode (rejects new writes). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_disable_maintenance".into(),
            description: "Take the cluster out of maintenance mode. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        // --- Admin Namespaces ---
        ToolDefinition {
            name: "dakera_list_namespaces_admin".into(),
            description: "List all namespaces with admin-level detail (vector count, size, policy). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_delete_namespace_admin".into(),
            description: "Force-delete a namespace and all its data via the admin endpoint. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to delete" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_optimize_namespace".into(),
            description: "Trigger HNSW index optimisation for a namespace (compacts segments). Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to optimise" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_migrate_dimensions".into(),
            description: "Migrate a namespace to a new vector dimension (re-embeds all vectors). Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Source namespace" },
                    "new_dimension": { "type": "integer", "description": "Target vector dimension" },
                    "distance": { "type": "string", "enum": ["cosine", "euclidean", "dot"], "description": "Distance metric for the new index" }
                },
                "required": ["namespace", "new_dimension"]
            }),
        },
        // --- Indexes ---
        ToolDefinition {
            name: "dakera_index_stats".into(),
            description: "Get HNSW index statistics (segment count, ef, M) per namespace. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_rebuild_indexes".into(),
            description: "Rebuild all HNSW indexes from scratch (expensive — use during maintenance). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        // --- Cache ---
        ToolDefinition {
            name: "dakera_cache_stats".into(),
            description: "Get in-memory Moka cache statistics (hit rate, evictions, size). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_cache_clear".into(),
            description: "Flush the entire in-memory vector cache. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        // --- Storage ---
        ToolDefinition {
            name: "dakera_storage_tiers".into(),
            description: "Get storage tier overview (hot/warm/cold distribution and sizes). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        // --- Config ---
        ToolDefinition {
            name: "dakera_get_server_config".into(),
            description: "Get the current server runtime configuration (inference, decay, quotas). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_update_server_config".into(),
            description: "Update server runtime configuration fields. Only provided fields are merged. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "config": {
                        "type": "object",
                        "description": "Partial config object with fields to update"
                    }
                },
                "required": ["config"]
            }),
        },
        // --- TTL ---
        ToolDefinition {
            name: "dakera_ttl_cleanup".into(),
            description: "Trigger an immediate TTL cleanup sweep (deletes expired memories). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_ttl_stats".into(),
            description: "Get TTL engine statistics (expired count, next sweep time). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        // --- Background ---
        ToolDefinition {
            name: "dakera_background_activity".into(),
            description: "Get background job activity metrics (decay sweeps, dedup, consolidation runs). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        // --- Quotas ---
        ToolDefinition {
            name: "dakera_list_quotas".into(),
            description: "List all namespace quota configurations and current usage. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_get_default_quota".into(),
            description: "Get the default quota configuration applied to namespaces without an explicit quota. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_set_default_quota".into(),
            description: "Set the default quota configuration. Requires Admin scope.\n\nQuota fields: max_vectors (integer), max_storage_bytes (integer), max_dimensions (integer), max_metadata_bytes (integer), enforcement (\"none\"|\"soft\"|\"hard\").".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "max_vectors": { "type": "integer", "description": "Maximum vector count (null = unlimited)" },
                    "max_storage_bytes": { "type": "integer", "description": "Maximum storage in bytes (null = unlimited)" },
                    "max_dimensions": { "type": "integer", "description": "Maximum vector dimensions (null = unlimited)" },
                    "max_metadata_bytes": { "type": "integer", "description": "Maximum metadata size per vector in bytes (null = unlimited)" },
                    "enforcement": { "type": "string", "enum": ["none", "soft", "hard"], "description": "Enforcement mode (default: hard)" }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_get_quota".into(),
            description: "Get the quota configuration and current usage for a specific namespace. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_set_quota".into(),
            description: "Set quota limits for a specific namespace. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" },
                    "max_vectors": { "type": "integer", "description": "Maximum vector count (null = unlimited)" },
                    "max_storage_bytes": { "type": "integer", "description": "Maximum storage in bytes (null = unlimited)" },
                    "max_dimensions": { "type": "integer", "description": "Maximum vector dimensions (null = unlimited)" },
                    "max_metadata_bytes": { "type": "integer", "description": "Maximum metadata size per vector in bytes (null = unlimited)" },
                    "enforcement": { "type": "string", "enum": ["none", "soft", "hard"], "description": "Enforcement mode (default: hard)" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_delete_quota".into(),
            description: "Remove the explicit quota for a namespace (falls back to default). Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_check_quota".into(),
            description: "Check whether a namespace is within its quota limits. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace name" }
                },
                "required": ["namespace"]
            }),
        },
        // --- Backups ---
        ToolDefinition {
            name: "dakera_list_backups".into(),
            description: "List all backups with metadata (id, type, status, size, created_at). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_create_backup".into(),
            description: "Create a new backup. Runs asynchronously — poll dakera_get_backup for status. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Human-readable backup name" },
                    "backup_type": { "type": "string", "enum": ["full", "incremental"], "description": "Backup type (default: full)" },
                    "namespaces": { "type": "array", "items": { "type": "string" }, "description": "Namespaces to include (empty = all)" },
                    "encrypt": { "type": "boolean", "description": "Encrypt the backup (default: false)" },
                    "compression": { "type": "string", "enum": ["none", "zstd", "lz4"], "description": "Compression algorithm" }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "dakera_get_backup".into(),
            description: "Get backup metadata and status by ID. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "backup_id": { "type": "string", "description": "Backup UUID" }
                },
                "required": ["backup_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_delete_backup".into(),
            description: "Delete a backup by ID. Irreversible. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "backup_id": { "type": "string", "description": "Backup UUID" }
                },
                "required": ["backup_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_download_backup".into(),
            description: "Download backup data by ID. Returns raw backup content. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "backup_id": { "type": "string", "description": "Backup UUID" }
                },
                "required": ["backup_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_upload_backup".into(),
            description: "Upload and register a backup from raw JSON content. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "Raw backup content (JSONL or JSON)" }
                },
                "required": ["content"]
            }),
        },
        ToolDefinition {
            name: "dakera_get_backup_schedule".into(),
            description: "Get the automated backup schedule configuration. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_set_backup_schedule".into(),
            description: "Set the automated backup schedule. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean", "description": "Enable scheduled backups" },
                    "cron": { "type": "string", "description": "Cron expression (e.g. \"0 2 * * *\" = daily at 2am UTC)" },
                    "backup_type": { "type": "string", "enum": ["full", "incremental"], "description": "Backup type for scheduled runs" },
                    "retention_days": { "type": "integer", "description": "Days to retain backups" },
                    "max_backups": { "type": "integer", "description": "Maximum number of backups to keep" },
                    "namespaces": { "type": "array", "items": { "type": "string" }, "description": "Namespaces to back up (empty = all)" },
                    "encrypt": { "type": "boolean", "description": "Encrypt scheduled backups" },
                    "compression": { "type": "string", "enum": ["none", "zstd", "lz4"], "description": "Compression for scheduled backups" }
                },
                "required": ["enabled"]
            }),
        },
        ToolDefinition {
            name: "dakera_restore_backup".into(),
            description: "Restore from a backup. Runs asynchronously — poll dakera_restore_status. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "backup_id": { "type": "string", "description": "Backup UUID to restore from" },
                    "target_namespaces": { "type": "array", "items": { "type": "string" }, "description": "Namespaces to restore (empty = all from backup)" },
                    "overwrite": { "type": "boolean", "description": "Overwrite existing namespaces (default: false)" },
                    "point_in_time": { "type": "integer", "description": "Unix timestamp for point-in-time restore (incremental only)" }
                },
                "required": ["backup_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_restore_status".into(),
            description: "Get status of a running or completed restore operation. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "restore_id": { "type": "string", "description": "Restore operation UUID (from dakera_restore_backup)" }
                },
                "required": ["restore_id"]
            }),
        },
        // --- Slow Queries ---
        ToolDefinition {
            name: "dakera_list_slow_queries".into(),
            description: "List captured slow queries above the configured threshold. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_slow_query_summary".into(),
            description: "Get aggregated slow query statistics (p50/p95/p99 latencies, top endpoints). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_clear_slow_queries".into(),
            description: "Clear all captured slow query records. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_slow_query_config".into(),
            description: "Update slow query logging configuration. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean", "description": "Enable slow query logging" },
                    "threshold_ms": { "type": "integer", "description": "Latency threshold in ms to log as slow (e.g. 100)" },
                    "max_entries": { "type": "integer", "description": "Maximum number of slow query records to keep in memory" }
                },
                "required": []
            }),
        },
    ]
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        // Cluster
        "dakera_cluster_status" => Some(
            client
                .get_json("/admin/cluster/status")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_cluster_nodes" => Some(
            client
                .get_json("/admin/cluster/nodes")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_cluster_replication" => Some(
            client
                .get_json("/admin/cluster/replication")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_list_shards" => Some(
            client
                .get_json("/admin/cluster/shards")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_rebalance_shards" => Some(
            client
                .post_json("/admin/cluster/shards/rebalance", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_maintenance_status" => Some(
            client
                .get_json("/admin/cluster/maintenance")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_enable_maintenance" => Some(
            client
                .post_json("/admin/cluster/maintenance/enable", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_disable_maintenance" => Some(
            client
                .post_json("/admin/cluster/maintenance/disable", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        // Admin namespaces
        "dakera_list_namespaces_admin" => Some(
            client
                .get_json("/admin/namespaces")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_delete_namespace_admin" => Some(tool_delete_namespace_admin(client, args).await),
        "dakera_optimize_namespace" => Some(tool_optimize_namespace(client, args).await),
        "dakera_migrate_dimensions" => Some(tool_migrate_dimensions(client, args).await),
        // Indexes / cache / storage
        "dakera_index_stats" => Some(
            client
                .get_json("/admin/indexes/stats")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_rebuild_indexes" => Some(
            client
                .post_json("/admin/indexes/rebuild", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_cache_stats" => Some(
            client
                .get_json("/admin/cache/stats")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_cache_clear" => Some(
            client
                .post_json("/admin/cache/clear", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_storage_tiers" => Some(
            client
                .get_json("/admin/storage/tiers")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        // Config
        "dakera_get_server_config" => Some(
            client
                .get_json("/admin/config")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_update_server_config" => Some(tool_update_server_config(client, args).await),
        // TTL / background
        "dakera_ttl_cleanup" => Some(
            client
                .post_json("/admin/ttl/cleanup", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_ttl_stats" => Some(
            client
                .get_json("/admin/ttl/stats")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_background_activity" => Some(
            client
                .get_json("/admin/background-activity")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        // Quotas
        "dakera_list_quotas" => Some(
            client
                .get_json("/admin/quotas")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_get_default_quota" => Some(
            client
                .get_json("/admin/quotas/default")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_set_default_quota" => Some(tool_set_default_quota(client, args).await),
        "dakera_get_quota" => Some(tool_get_quota(client, args).await),
        "dakera_set_quota" => Some(tool_set_quota(client, args).await),
        "dakera_delete_quota" => Some(tool_delete_quota(client, args).await),
        "dakera_check_quota" => Some(tool_check_quota(client, args).await),
        // Backups
        "dakera_list_backups" => Some(
            client
                .get_json("/admin/backups")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_create_backup" => Some(tool_create_backup(client, args).await),
        "dakera_get_backup" => Some(tool_get_backup(client, args).await),
        "dakera_delete_backup" => Some(tool_delete_backup(client, args).await),
        "dakera_download_backup" => Some(tool_download_backup(client, args).await),
        "dakera_upload_backup" => Some(tool_upload_backup(client, args).await),
        "dakera_get_backup_schedule" => Some(
            client
                .get_json("/admin/backups/schedule")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_set_backup_schedule" => Some(tool_set_backup_schedule(client, args).await),
        "dakera_restore_backup" => Some(tool_restore_backup(client, args).await),
        "dakera_restore_status" => Some(tool_restore_status(client, args).await),
        // Slow queries
        "dakera_list_slow_queries" => Some(
            client
                .get_json("/admin/slow-queries")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_slow_query_summary" => Some(
            client
                .get_json("/admin/slow-queries/summary")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_clear_slow_queries" => Some(
            client
                .delete_json("/admin/slow-queries")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_slow_query_config" => Some(tool_slow_query_config(client, args).await),
        _ => None,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn tool_delete_namespace_admin(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/admin/namespaces/{}", urlencoding::encode(&namespace));
    client
        .delete_json(&path)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_optimize_namespace(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!(
        "/admin/namespaces/{}/optimize",
        urlencoding::encode(&namespace)
    );
    client
        .post_json(&path, &json!({}))
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_migrate_dimensions(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let new_dimension = match args.get("new_dimension").and_then(|v| v.as_u64()) {
        Some(d) => d,
        None => {
            return CallToolResult::error(
                "Missing required parameter: new_dimension".to_string(),
            )
        }
    };
    let mut body = json!({ "namespace": namespace, "new_dimension": new_dimension });
    if let Some(d) = args.get("distance").and_then(|v| v.as_str()) {
        body["distance"] = json!(d);
    }
    client
        .post_json("/admin/namespaces/migrate-dimensions", &body)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_update_server_config(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let config = match args.get("config") {
        Some(c) => c.clone(),
        None => {
            return CallToolResult::error(
                "Missing required parameter: config".to_string(),
            )
        }
    };
    client
        .put_json("/admin/config", &config)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

fn build_quota_config(args: &serde_json::Value) -> serde_json::Value {
    let mut config = json!({});
    if let Some(v) = args.get("max_vectors") {
        config["max_vectors"] = v.clone();
    }
    if let Some(v) = args.get("max_storage_bytes") {
        config["max_storage_bytes"] = v.clone();
    }
    if let Some(v) = args.get("max_dimensions") {
        config["max_dimensions"] = v.clone();
    }
    if let Some(v) = args.get("max_metadata_bytes") {
        config["max_metadata_bytes"] = v.clone();
    }
    if let Some(v) = args.get("enforcement") {
        config["enforcement"] = v.clone();
    }
    config
}

async fn tool_set_default_quota(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let body = json!({ "config": build_quota_config(args) });
    client
        .put_json("/admin/quotas/default", &body)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_get_quota(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/admin/quotas/{}", urlencoding::encode(&namespace));
    client
        .get_json(&path)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_set_quota(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/admin/quotas/{}", urlencoding::encode(&namespace));
    let body = json!({ "config": build_quota_config(args) });
    client
        .put_json(&path, &body)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_delete_quota(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/admin/quotas/{}", urlencoding::encode(&namespace));
    client
        .delete_json(&path)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_check_quota(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/admin/quotas/{}/check", urlencoding::encode(&namespace));
    client
        .post_json(&path, &json!({}))
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_create_backup(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let name = match require_string(args, "name") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({ "name": name });
    if let Some(v) = args.get("backup_type") {
        body["backup_type"] = v.clone();
    }
    if let Some(v) = args.get("namespaces") {
        body["namespaces"] = v.clone();
    }
    if let Some(v) = args.get("encrypt") {
        body["encrypt"] = v.clone();
    }
    if let Some(v) = args.get("compression") {
        body["compression"] = v.clone();
    }
    client
        .post_json("/admin/backups", &body)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_get_backup(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let backup_id = match require_string(args, "backup_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/admin/backups/{}", urlencoding::encode(&backup_id));
    client
        .get_json(&path)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_delete_backup(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let backup_id = match require_string(args, "backup_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/admin/backups/{}", urlencoding::encode(&backup_id));
    client
        .delete_json(&path)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_download_backup(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let backup_id = match require_string(args, "backup_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!(
        "/admin/backups/{}/download",
        urlencoding::encode(&backup_id)
    );
    match client.get_text(&path).await {
        Ok(text) => CallToolResult::text(text),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_upload_backup(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let content = match require_string(args, "content") {
        Ok(v) => v,
        Err(e) => return e,
    };
    client
        .post_multipart_text("/admin/backups/upload", &content)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_set_backup_schedule(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let enabled = args
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut body = json!({ "enabled": enabled });
    let fields = [
        "cron",
        "backup_type",
        "retention_days",
        "max_backups",
        "namespaces",
        "encrypt",
        "compression",
    ];
    for f in &fields {
        if let Some(v) = args.get(*f) {
            body[*f] = v.clone();
        }
    }
    client
        .post_json("/admin/backups/schedule", &body)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_restore_backup(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let backup_id = match require_string(args, "backup_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({ "backup_id": backup_id });
    if let Some(v) = args.get("target_namespaces") {
        body["target_namespaces"] = v.clone();
    }
    if let Some(v) = args.get("overwrite") {
        body["overwrite"] = v.clone();
    }
    if let Some(v) = args.get("point_in_time") {
        body["point_in_time"] = v.clone();
    }
    client
        .post_json("/admin/backups/restore", &body)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_restore_status(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let restore_id = match require_string(args, "restore_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!(
        "/admin/backups/restore/{}",
        urlencoding::encode(&restore_id)
    );
    client
        .get_json(&path)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

async fn tool_slow_query_config(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let mut body = json!({});
    if let Some(v) = args.get("enabled") {
        body["enabled"] = v.clone();
    }
    if let Some(v) = args.get("threshold_ms") {
        body["threshold_ms"] = v.clone();
    }
    if let Some(v) = args.get("max_entries") {
        body["max_entries"] = v.clone();
    }
    client
        .patch_json("/admin/slow-queries/config", &body)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::DakeraApiClient;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_none() {
        assert!(
            execute(&dummy_client(), "not_an_admin_tool", &json!({}))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_delete_namespace_admin_missing_namespace() {
        let r = execute(
            &dummy_client(),
            "dakera_delete_namespace_admin",
            &json!({}),
        )
        .await
        .unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_optimize_namespace_missing_namespace() {
        let r = execute(&dummy_client(), "dakera_optimize_namespace", &json!({}))
            .await
            .unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_migrate_dimensions_missing_namespace() {
        let r = execute(
            &dummy_client(),
            "dakera_migrate_dimensions",
            &json!({"new_dimension": 768}),
        )
        .await
        .unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_migrate_dimensions_missing_dimension() {
        let r = execute(&dummy_client(), "dakera_migrate_dimensions", &json!({"namespace": "ns"})).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("new_dimension"));
    }

    #[tokio::test]
    async fn test_get_quota_missing_namespace() {
        let r = execute(&dummy_client(), "dakera_get_quota", &json!({})).await.unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_create_backup_missing_name() {
        let r = execute(&dummy_client(), "dakera_create_backup", &json!({})).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("name"));
    }

    #[tokio::test]
    async fn test_restore_backup_missing_backup_id() {
        let r = execute(&dummy_client(), "dakera_restore_backup", &json!({})).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("backup_id"));
    }

    #[tokio::test]
    async fn test_restore_status_missing_restore_id() {
        let r = execute(&dummy_client(), "dakera_restore_status", &json!({})).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("restore_id"));
    }

    #[tokio::test]
    async fn test_definitions_not_empty() {
        assert!(!definitions().is_empty());
    }

    #[tokio::test]
    async fn test_definitions_unique_names() {
        let defs = definitions();
        let mut seen = std::collections::HashSet::new();
        for d in &defs {
            assert!(seen.insert(d.name.as_str()), "duplicate: {}", d.name);
        }
    }
}
