//! Memory Knowledge Graph tools — consolidated CE-5 + KG-2
//!
//! `graph_traverse` supports memory-anchored BFS (CE-5) and agent-scoped traversal (KG-2).
//! `graph_export` supports JSON and GraphML formats.
//! `kg_traverse`, `kg_export`, and `kg_query` are removed; use `graph_traverse`/`graph_export`
//! and REST API for advanced graph queries.

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_graph_traverse".into(),
            description: "Traverse the memory knowledge graph via BFS to discover connected memories. \
                Memory-anchored mode: provide memory_id to explore outbound links from a known memory. \
                Agent-scoped mode: provide agent_id + root_id with optional edge-type or min-weight filters. \
                Returns connected memories and edge metadata up to the specified depth (1–5)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Memory ID to start traversal from (memory-anchored mode)"
                    },
                    "agent_id": {
                        "type": "string"
                    },
                    "root_id": {
                        "type": "string",
                        "description": "Root memory ID for agent-scoped traversal"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "BFS depth limit (1–5)",
                        "minimum": 1,
                        "maximum": 5
                    },
                    "edge_type": {
                        "type": "string",
                        "description": "Comma-separated edge type filter (e.g. \"related_to,precedes\"; agent-scoped only)"
                    },
                    "min_weight": {
                        "type": "number",
                        "description": "Min edge weight 0.0–1.0 (agent-scoped only)",
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max edges to return (agent-scoped only, max 1000)",
                        "minimum": 1,
                        "maximum": 1000
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_graph_path".into(),
            description: "Find the shortest path between two memory nodes, returning the ordered sequence of IDs and edge hops. \
                Use to explain why two memories are related or trace an inference chain."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from_id": { "type": "string", "description": "Starting memory ID" },
                    "to_id": { "type": "string", "description": "Target memory ID" }
                },
                "required": ["from_id", "to_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_graph_link_memory".into(),
            description: "Create an explicit linked_by edge between two memories. \
                Use when semantic similarity did not auto-link them; affects graph traversal and dakera_recall_associated."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Source memory ID" },
                    "target_id": { "type": "string", "description": "Target memory ID to link to" },
                    "agent_id": { "type": "string", "description": "Agent that owns both memories (authorizes the write)" }
                },
                "required": ["memory_id", "target_id", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_graph_export".into(),
            description: "Export an agent's knowledge graph as JSON or GraphML (compatible with Gephi, yEd, Cytoscape). \
                Use for offline visualization or compliance audit."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "format": {
                        "type": "string",
                        "description": "Export format: \"json\" (default) or \"graphml\"",
                        "enum": ["json", "graphml"]
                    }
                },
                "required": ["agent_id"]
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
        "dakera_graph_traverse" => Some(tool_graph_traverse(client, args).await),
        "dakera_graph_path" => Some(tool_graph_path(client, args).await),
        "dakera_graph_link_memory" => Some(tool_graph_link_memory(client, args).await),
        "dakera_graph_export" => Some(tool_graph_export(client, args).await),
        _ => None,
    }
}

async fn tool_graph_traverse(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    // KG-2 mode: both agent_id and root_id must be present
    if let (Some(agent_id), Some(root_id)) = (
        args.get("agent_id").and_then(|v| v.as_str()),
        args.get("root_id").and_then(|v| v.as_str()),
    ) {
        let encoded_agent = urlencoding::encode(agent_id);
        let encoded_root = urlencoding::encode(root_id);
        let mut qs = format!(
            "/v1/knowledge/query?agent_id={}&root_id={}",
            encoded_agent, encoded_root
        );
        if let Some(d) = args.get("depth").and_then(|v| v.as_u64()) {
            qs.push_str(&format!("&max_depth={}", d.min(5)));
        }
        if let Some(et) = args.get("edge_type").and_then(|v| v.as_str()) {
            qs.push_str(&format!("&edge_type={}", urlencoding::encode(et)));
        }
        if let Some(mw) = args.get("min_weight").and_then(|v| v.as_f64()) {
            qs.push_str(&format!("&min_weight={:.4}", mw));
        }
        if let Some(lim) = args.get("limit").and_then(|v| v.as_u64()) {
            qs.push_str(&format!("&limit={}", lim.min(1000)));
        }
        return match client.get_json(&qs).await {
            Ok(result) => ok_json(&result),
            Err(e) => CallToolResult::error(e),
        };
    }

    // CE-5 mode: memory_id required
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&memory_id);
    let path = match args.get("depth").and_then(|v| v.as_u64()) {
        Some(d) => format!("/v1/memories/{}/graph?depth={}", encoded, d.min(5)),
        None => format!("/v1/memories/{}/graph", encoded),
    };
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_graph_path(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let from_id = match require_string(args, "from_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let to_id = match require_string(args, "to_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded_from = urlencoding::encode(&from_id);
    let encoded_to = urlencoding::encode(&to_id);
    let path = format!("/v1/memories/{}/path?to={}", encoded_from, encoded_to);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_graph_link_memory(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let target_id = match require_string(args, "target_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&memory_id);
    let path = format!("/v1/memories/{}/links", encoded);
    // Server contract (MemoryLinkRequest): agent_id is required for
    // authorization — omitting it is a 422. Same drift class as the
    // dakera_memory_feedback fix (#136).
    let body = json!({ "target_id": target_id, "agent_id": agent_id });
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_graph_export(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");
    let encoded = urlencoding::encode(&agent_id);
    // KG-2 export endpoint supports both json and graphml.
    let path = format!(
        "/v1/knowledge/export?agent_id={}&format={}",
        encoded, format
    );
    if format == "graphml" {
        match client.get_text(&path).await {
            Ok(xml) => CallToolResult::text(xml),
            Err(e) => CallToolResult::error(e),
        }
    } else {
        match client.get_json(&path).await {
            Ok(result) => ok_json(&result),
            Err(e) => CallToolResult::error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://localhost:9999".to_string(), None)
    }

    #[test]
    fn test_definitions_count() {
        assert_eq!(definitions().len(), 4);
    }

    #[test]
    fn test_definitions_names() {
        let defs = definitions();
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dakera_graph_traverse"));
        assert!(names.contains(&"dakera_graph_path"));
        assert!(names.contains(&"dakera_graph_link_memory"));
        assert!(names.contains(&"dakera_graph_export"));
        assert!(!names.contains(&"dakera_kg_traverse"));
        assert!(!names.contains(&"dakera_kg_export"));
        assert!(!names.contains(&"dakera_kg_query"));
    }

    #[tokio::test]
    async fn test_graph_traverse_ce5_missing_memory_id() {
        let result = tool_graph_traverse(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("memory_id"));
    }

    #[tokio::test]
    async fn test_graph_traverse_kg2_mode_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_graph_traverse",
            &json!({"agent_id": "ag-1", "root_id": "mem-x"}),
        )
        .await;
        // Will error connecting to dummy server, but dispatch must occur
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_graph_path_missing_from_id() {
        let result = tool_graph_path(&dummy_client(), &json!({"to_id": "mem_b"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("from_id"));
    }

    #[tokio::test]
    async fn test_graph_path_missing_to_id() {
        let result = tool_graph_path(&dummy_client(), &json!({"from_id": "mem_a"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("to_id"));
    }

    #[tokio::test]
    async fn test_graph_link_memory_missing_memory_id() {
        let result = tool_graph_link_memory(&dummy_client(), &json!({"target_id": "mem_b"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("memory_id"));
    }

    #[tokio::test]
    async fn test_graph_link_memory_missing_target_id() {
        let result = tool_graph_link_memory(&dummy_client(), &json!({"memory_id": "mem_a"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("target_id"));
    }

    #[tokio::test]
    async fn test_graph_link_memory_missing_agent_id() {
        // Server contract: MemoryLinkRequest.agent_id is required (422 without it).
        let result = tool_graph_link_memory(
            &dummy_client(),
            &json!({"memory_id": "mem_a", "target_id": "mem_b"}),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_graph_export_missing_agent_id() {
        let result = tool_graph_export(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_execute_dispatches_all_tools() {
        for tool in &[
            "dakera_graph_traverse",
            "dakera_graph_path",
            "dakera_graph_link_memory",
            "dakera_graph_export",
        ] {
            let result = execute(&dummy_client(), tool, &json!({})).await;
            assert!(result.is_some(), "{} should dispatch", tool);
        }
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_none() {
        assert!(execute(&dummy_client(), "dakera_unknown_xyz", &json!({}))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_removed_tools_return_none() {
        for tool in &["dakera_kg_traverse", "dakera_kg_export", "dakera_kg_query"] {
            let result = execute(&dummy_client(), tool, &json!({})).await;
            assert!(result.is_none(), "{} should be removed", tool);
        }
    }
}
