//! Memory Knowledge Graph tools — CE-5 (graph_traverse, graph_path, graph_link_memory, graph_export)
//! and KG-2 (kg_traverse, kg_query, kg_export)
//!
//! CE-5 tools require dakera core v0.9.x with CE-5 merged.
//! KG-2 tools require dakera core v0.9.6+ with KG-2 merged.

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    let mut tools = ce5_definitions();
    tools.extend(kg2_definitions());
    tools
}

fn ce5_definitions() -> Vec<ToolDefinition> {
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

fn kg2_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_kg_traverse".into(),
            description: "Traverse the memory knowledge graph from a root memory with optional edge-type and weight filters. Returns matching edges and connected node IDs. Requires dakera core v0.9.6+ (KG-2).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID whose memory graph to traverse"
                    },
                    "root_id": {
                        "type": "string",
                        "description": "Memory ID to start BFS traversal from"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "BFS depth limit (1–5, default 3)",
                        "minimum": 1,
                        "maximum": 5
                    },
                    "edge_type": {
                        "type": "string",
                        "description": "Comma-separated edge type filter (e.g. \"related_to,shares_entity\")"
                    },
                    "min_weight": {
                        "type": "number",
                        "description": "Minimum edge weight threshold (0.0–1.0)",
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of edges to return (default 100, max 1000)",
                        "minimum": 1,
                        "maximum": 1000
                    }
                },
                "required": ["agent_id", "root_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_kg_query".into(),
            description: "Query the memory knowledge graph using a filter DSL. Filters edges by type, minimum weight, and optional depth. Returns matching edges without requiring a root node. Requires dakera core v0.9.6+ (KG-2).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID whose memory graph to query"
                    },
                    "edge_type": {
                        "type": "string",
                        "description": "Comma-separated edge type filter (e.g. \"related_to,precedes\")"
                    },
                    "min_weight": {
                        "type": "number",
                        "description": "Minimum edge weight (0.0–1.0)",
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum edges to return (default 100, max 1000)",
                        "minimum": 1,
                        "maximum": 1000
                    }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_kg_export".into(),
            description: "Export the full memory knowledge graph for an agent as JSON or GraphML. JSON returns structured edge data; GraphML returns an XML document suitable for graph visualization tools (Gephi, yEd, Cytoscape). Requires dakera core v0.9.6+ (KG-2).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID whose graph to export"
                    },
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
        // KG-2 tools
        "dakera_kg_traverse" => Some(tool_kg_traverse(client, args).await),
        "dakera_kg_query" => Some(tool_kg_query(client, args).await),
        "dakera_kg_export" => Some(tool_kg_export(client, args).await),
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

// ---------------------------------------------------------------------------
// KG-2 tool implementations
// ---------------------------------------------------------------------------

async fn tool_kg_traverse(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let root_id = match require_string(args, "root_id") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let encoded_agent = urlencoding::encode(&agent_id);
    let encoded_root = urlencoding::encode(&root_id);

    let mut qs = format!(
        "/v1/knowledge/query?agent_id={}&root_id={}",
        encoded_agent, encoded_root
    );
    if let Some(d) = args.get("max_depth").and_then(|v| v.as_u64()) {
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
    match client.get_json(&qs).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_kg_query(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let encoded_agent = urlencoding::encode(&agent_id);
    let mut qs = format!("/v1/knowledge/query?agent_id={}", encoded_agent);
    if let Some(et) = args.get("edge_type").and_then(|v| v.as_str()) {
        qs.push_str(&format!("&edge_type={}", urlencoding::encode(et)));
    }
    if let Some(mw) = args.get("min_weight").and_then(|v| v.as_f64()) {
        qs.push_str(&format!("&min_weight={:.4}", mw));
    }
    if let Some(lim) = args.get("limit").and_then(|v| v.as_u64()) {
        qs.push_str(&format!("&limit={}", lim.min(1000)));
    }
    match client.get_json(&qs).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_kg_export(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let encoded_agent = urlencoding::encode(&agent_id);
    let path = format!(
        "/v1/knowledge/export?agent_id={}&format={}",
        encoded_agent, format
    );

    if format == "graphml" {
        // GraphML returns XML, not JSON — return as raw text
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
        // CE-5: 4 + KG-2: 3 = 7
        assert_eq!(definitions().len(), 7);
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

    // --- KG-2 tool tests ---

    #[test]
    fn test_definitions_includes_kg2_tools() {
        let defs = definitions();
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dakera_kg_traverse"));
        assert!(names.contains(&"dakera_kg_query"));
        assert!(names.contains(&"dakera_kg_export"));
    }

    #[test]
    fn test_definitions_total_count_with_kg2() {
        // CE-5: 4 tools + KG-2: 3 tools = 7
        assert_eq!(definitions().len(), 7);
    }

    #[tokio::test]
    async fn test_kg_traverse_missing_agent_id() {
        let result = tool_kg_traverse(&dummy_client(), &json!({"root_id": "mem-x"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_kg_traverse_missing_root_id() {
        let result = tool_kg_traverse(&dummy_client(), &json!({"agent_id": "ag-1"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("root_id"));
    }

    #[tokio::test]
    async fn test_kg_query_missing_agent_id() {
        let result = tool_kg_query(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_kg_export_missing_agent_id() {
        let result = tool_kg_export(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_execute_dispatches_kg_traverse() {
        let result = execute(&dummy_client(), "dakera_kg_traverse", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_dispatches_kg_query() {
        let result = execute(&dummy_client(), "dakera_kg_query", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_dispatches_kg_export() {
        let result = execute(&dummy_client(), "dakera_kg_export", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }
}
