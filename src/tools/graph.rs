//! Memory Knowledge Graph tools — graph_traverse, graph_path, graph_link_memory, graph_export (CE-5 / MCP-4)
//!
//! These tools require dakera core v0.9.x with CE-5 (Memory Knowledge Graph) merged.

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_graph_traverse".into(),
            description: "Traverse the memory knowledge graph from a starting memory using BFS. Returns all connected memories within the specified depth, along with the edge types that connect them (related_to, shares_entity, precedes, linked_by).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Memory ID to start traversal from"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "BFS traversal depth — how many hops to follow (1–5, default 3)",
                        "minimum": 1,
                        "maximum": 5
                    }
                },
                "required": ["memory_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_graph_path".into(),
            description: "Find the shortest path between two memories in the knowledge graph. Returns the ordered sequence of memory IDs and edge hops connecting them.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from_id": {
                        "type": "string",
                        "description": "Starting memory ID"
                    },
                    "to_id": {
                        "type": "string",
                        "description": "Target memory ID"
                    }
                },
                "required": ["from_id", "to_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_graph_link_memory".into(),
            description: "Create an explicit linked_by edge between two memories in the knowledge graph. Use this to manually connect memories that are related but would not be automatically linked by cosine similarity or entity overlap.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Source memory ID"
                    },
                    "target_id": {
                        "type": "string",
                        "description": "Target memory ID to link to"
                    }
                },
                "required": ["memory_id", "target_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_graph_export".into(),
            description: "Export the full memory knowledge graph for an agent as a list of edges. Useful for visualizing or analyzing the memory graph structure.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID whose graph to export"
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
    let encoded = urlencoding::encode(&memory_id);
    let path = format!("/v1/memories/{}/links", encoded);
    let body = json!({ "target_id": target_id });
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
    let encoded = urlencoding::encode(&agent_id);
    let path = format!("/v1/agents/{}/graph/export", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
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
    }

    #[tokio::test]
    async fn test_graph_traverse_missing_memory_id() {
        let result = tool_graph_traverse(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("memory_id"));
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
    async fn test_graph_export_missing_agent_id() {
        let result = tool_graph_export(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_execute_dispatches_graph_traverse() {
        let result = execute(&dummy_client(), "dakera_graph_traverse", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_dispatches_graph_path() {
        let result = execute(&dummy_client(), "dakera_graph_path", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_dispatches_graph_link_memory() {
        let result = execute(&dummy_client(), "dakera_graph_link_memory", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_dispatches_graph_export() {
        let result = execute(&dummy_client(), "dakera_graph_export", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown_xyz", &json!({})).await;
        assert!(result.is_none());
    }
}
