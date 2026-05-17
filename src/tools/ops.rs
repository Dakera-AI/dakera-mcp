//! Ops tools — diagnostics, jobs, compact, shutdown

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_diagnostics".into(),
            description: "Run a diagnostics report (storage integrity, index health, memory pressure). Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_list_jobs".into(),
            description: "List all background jobs (backup, restore, migration, reindex) and their status. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_get_job".into(),
            description: "Get the status and progress of a specific background job by ID. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string", "description": "Job UUID" }
                },
                "required": ["job_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_compact".into(),
            description: "Trigger a RocksDB compaction to reclaim storage space. Runs asynchronously. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_shutdown".into(),
            description: "Initiate a graceful server shutdown. WARNING: this will stop the Dakera process. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
    ]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_diagnostics" => Some(
            client
                .get_json("/ops/diagnostics")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_list_jobs" => Some(
            client
                .get_json("/ops/jobs")
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_get_job" => Some(tool_get_job(client, args).await),
        "dakera_compact" => Some(
            client
                .post_json("/ops/compact", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        "dakera_shutdown" => Some(
            client
                .post_json("/ops/shutdown", &json!({}))
                .await
                .map(|v| ok_json(&v))
                .unwrap_or_else(CallToolResult::error),
        ),
        _ => None,
    }
}

async fn tool_get_job(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let job_id = match require_string(args, "job_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/ops/jobs/{}", urlencoding::encode(&job_id));
    client
        .get_json(&path)
        .await
        .map(|v| ok_json(&v))
        .unwrap_or_else(CallToolResult::error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::DakeraApiClient;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        assert!(execute(&dummy_client(), "not_ops", &json!({}))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_get_job_missing_id() {
        let r = execute(&dummy_client(), "dakera_get_job", &json!({}))
            .await
            .unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("job_id"));
    }

    #[tokio::test]
    async fn test_definitions_unique() {
        let defs = definitions();
        let mut seen = std::collections::HashSet::new();
        for d in &defs {
            assert!(seen.insert(d.name.as_str()), "duplicate: {}", d.name);
        }
        assert_eq!(defs.len(), 5);
    }
}
