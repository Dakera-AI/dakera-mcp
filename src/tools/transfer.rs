//! DX-1: Memory import / export tools
//!
//! Exposes Dakera's migration endpoints as MCP tools so AI agents can
//! export their memories to Mem0/Zep/JSONL/CSV and import from the same formats.
//!
//! Tools:
//!   - `dakera_memory_export`  — GET  /v1/export?agent_id=&format=
//!   - `dakera_memory_import`  — POST /v1/import (multipart, text field)

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_memory_export".into(),
            description: "Export all memories for an agent in a portable format (jsonl, csv, \
                mem0, zep). Returns the raw export text. Use to migrate memories between \
                Dakera instances or compatible memory systems."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "format": {
                        "type": "string",
                        "enum": ["jsonl", "csv", "mem0", "zep"],
                        "description": "Export format (jsonl|csv|mem0|zep)"
                    }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_import".into(),
            description: "Import memories from a string payload (JSONL, CSV, Mem0 JSON, or \
                Zep JSON). The format is auto-detected from the content unless explicitly \
                specified. Returns an import job status with counts of imported/skipped records."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "data": {
                        "type": "string",
                        "description": "Raw export content (JSONL lines, CSV text, Mem0 JSON, or Zep JSON)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["jsonl", "csv", "mem0", "zep"],
                        "description": "Explicit format hint — omit to auto-detect from content"
                    }
                },
                "required": ["agent_id", "data"]
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
        "dakera_memory_export" => Some(tool_memory_export(client, args).await),
        "dakera_memory_import" => Some(tool_memory_import(client, args).await),
        _ => None,
    }
}

async fn tool_memory_export(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("jsonl");

    let path = format!(
        "/v1/export?agent_id={}&format={}",
        urlencoding::encode(&agent_id),
        urlencoding::encode(format),
    );

    match client.get_text(&path).await {
        Ok(text) => CallToolResult::text(text),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_import(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let data = match require_string(args, "data") {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Build query string
    let mut qs = format!("agent_id={}", urlencoding::encode(&agent_id));
    if let Some(fmt) = args.get("format").and_then(|v| v.as_str()) {
        qs.push_str(&format!("&format={}", urlencoding::encode(fmt)));
    }
    let path = format!("/v1/import?{}", qs);

    match client.post_multipart_text(&path, &data).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://localhost:9999".to_string(), None)
    }

    #[test]
    fn test_definitions_count() {
        assert_eq!(definitions().len(), 2);
    }

    #[test]
    fn test_definition_names() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        assert!(names.iter().any(|n| n == "dakera_memory_export"));
        assert!(names.iter().any(|n| n == "dakera_memory_import"));
    }

    #[tokio::test]
    async fn test_export_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_export",
            &json!({"agent_id": "test-agent"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true)); // no server
    }

    #[tokio::test]
    async fn test_import_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_import",
            &json!({"agent_id": "test-agent", "data": "{\"content\":\"hello\"}"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true)); // no server
    }

    #[tokio::test]
    async fn test_export_missing_agent_id() {
        let result = execute(&dummy_client(), "dakera_memory_export", &json!({})).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_import_missing_data() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_import",
            &json!({"agent_id": "test-agent"}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("data"));
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }
}
