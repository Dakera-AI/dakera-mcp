//! Knowledge tools — graph, summarize, deduplicate, cross-agent network

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_knowledge_graph".into(),
            description: "Build a knowledge graph from a seed memory. Finds related memories via embedding similarity, producing a root node with related edges.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "memory_id": { "type": "string", "description": "Seed memory ID to build graph from" },
                    "depth": { "type": "integer", "description": "Graph traversal depth (controls candidate count)", "default": 2 },
                    "min_similarity": { "type": "number", "description": "Minimum similarity threshold 0.0-1.0", "default": 0.7 }
                },
                "required": ["agent_id", "memory_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_knowledge_summarize".into(),
            description: "Summarize a set of memories into a single consolidated memory. Combines content, tags, and importance from source memories.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "memory_ids": { "type": "array", "items": { "type": "string" }, "description": "Memory IDs to summarize (minimum 2)" },
                    "target_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"], "description": "Type for the summarized memory (default: semantic)" }
                },
                "required": ["agent_id", "memory_ids"]
            }),
        },
        ToolDefinition {
            name: "dakera_knowledge_deduplicate".into(),
            description: "Find and optionally merge duplicate or near-duplicate memories for an agent.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent identifier" },
                    "threshold": { "type": "number", "description": "Similarity threshold 0.0-1.0 for duplicates", "default": 0.9 },
                    "dry_run": { "type": "boolean", "description": "If true, only report duplicates without merging", "default": true }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_knowledge_network_cross_agent".into(),
            description: "Build a cross-agent memory network spanning all agent namespaces. Returns nodes (memories) and edges (cross-agent similarity links). Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_ids": { "type": "array", "items": { "type": "string" }, "description": "Specific agent IDs to include (omit for all agents)" },
                    "min_similarity": { "type": "number", "description": "Minimum cosine similarity for a cross-agent edge (default: 0.3)", "default": 0.3 },
                    "max_nodes_per_agent": { "type": "integer", "description": "Max memories per agent, top N by importance (default: 50)", "default": 50 },
                    "min_importance": { "type": "number", "description": "Minimum importance score for included memories (default: 0.0)", "default": 0.0 },
                    "max_cross_edges": { "type": "integer", "description": "Maximum cross-agent edges to return (default: 200)", "default": 200 }
                },
                "required": []
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
        "dakera_knowledge_graph" => Some(tool_knowledge_graph(client, args).await),
        "dakera_knowledge_summarize" => Some(tool_knowledge_summarize(client, args).await),
        "dakera_knowledge_deduplicate" => Some(tool_knowledge_deduplicate(client, args).await),
        "dakera_knowledge_network_cross_agent" => {
            Some(tool_knowledge_network_cross_agent(client, args).await)
        }
        _ => None,
    }
}

async fn tool_knowledge_graph(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "agent_id": agent_id,
        "memory_id": memory_id,
        "depth": args.get("depth").and_then(|v| v.as_u64()).unwrap_or(2),
        "min_similarity": args.get("min_similarity").and_then(|v| v.as_f64()).unwrap_or(0.7),
    });
    match client.post_json("/v1/knowledge/graph", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_knowledge_summarize(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let memory_ids = match args.get("memory_ids").and_then(|v| v.as_array()) {
        Some(arr) if arr.len() >= 2 => arr,
        Some(_) => {
            return CallToolResult::error("memory_ids must contain at least 2 IDs".to_string())
        }
        None => return CallToolResult::error("memory_ids array is required".to_string()),
    };
    let mut body = json!({
        "agent_id": agent_id,
        "memory_ids": memory_ids,
    });
    if let Some(target_type) = args.get("target_type").and_then(|v| v.as_str()) {
        body["target_type"] = json!(target_type);
    }
    match client.post_json("/v1/knowledge/summarize", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_knowledge_deduplicate(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let body = json!({
        "agent_id": args.get("agent_id").and_then(|v| v.as_str()).unwrap_or(""),
        "threshold": args.get("threshold").and_then(|v| v.as_f64()).unwrap_or(0.9),
        "dry_run": args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(true),
    });
    match client.post_json("/v1/knowledge/deduplicate", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_knowledge_network_cross_agent(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let mut body = json!({
        "min_similarity": args.get("min_similarity").and_then(|v| v.as_f64()).unwrap_or(0.3),
        "max_nodes_per_agent": args.get("max_nodes_per_agent").and_then(|v| v.as_u64()).unwrap_or(50),
        "min_importance": args.get("min_importance").and_then(|v| v.as_f64()).unwrap_or(0.0),
        "max_cross_edges": args.get("max_cross_edges").and_then(|v| v.as_u64()).unwrap_or(200),
    });
    if let Some(ids) = args.get("agent_ids").and_then(|v| v.as_array()) {
        body["agent_ids"] = json!(ids);
    }
    match client
        .post_json("/v1/knowledge/network/cross-agent", &body)
        .await
    {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::DakeraApiClient;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        // Port 9 is discard — connections are refused immediately, so no actual
        // network traffic occurs.  Validation-path tests return before any HTTP call.
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    // --- tool_knowledge_graph input validation ---

    #[tokio::test]
    async fn test_knowledge_graph_missing_agent_id_returns_error() {
        let result =
            tool_knowledge_graph(&dummy_client(), &json!({"memory_id": "mem_abc"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_knowledge_graph_missing_memory_id_returns_error() {
        let result =
            tool_knowledge_graph(&dummy_client(), &json!({"agent_id": "qa"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("memory_id"));
    }

    // --- tool_knowledge_summarize input validation ---

    #[tokio::test]
    async fn test_knowledge_summarize_missing_agent_id_returns_error() {
        let result = tool_knowledge_summarize(
            &dummy_client(),
            &json!({"memory_ids": ["mem_1", "mem_2"]}),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_knowledge_summarize_missing_memory_ids_returns_error() {
        let result =
            tool_knowledge_summarize(&dummy_client(), &json!({"agent_id": "qa"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("memory_ids"));
    }

    #[tokio::test]
    async fn test_knowledge_summarize_too_few_ids_returns_error() {
        let result = tool_knowledge_summarize(
            &dummy_client(),
            &json!({"agent_id": "qa", "memory_ids": ["mem_1"]}),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("at least 2"));
    }

    // --- execute dispatch ---

    #[tokio::test]
    async fn test_execute_unknown_tool_returns_none() {
        let result = execute(&dummy_client(), "not_a_knowledge_tool", &json!({})).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_execute_knowledge_graph_dispatches() {
        // Missing agent_id — validates dispatch returns Some(is_error), not None.
        let result = execute(&dummy_client(), "dakera_knowledge_graph", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_knowledge_summarize_dispatches() {
        // Missing agent_id — validates dispatch returns Some(is_error), not None.
        let result = execute(&dummy_client(), "dakera_knowledge_summarize", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_execute_knowledge_deduplicate_dispatches() {
        // No required fields — will hit unreachable server and error, but dispatch is verified.
        let result = execute(&dummy_client(), "dakera_knowledge_deduplicate", &json!({})).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_execute_cross_agent_network_dispatches() {
        // No required fields — will hit unreachable server and error, but dispatch is verified.
        let result =
            execute(&dummy_client(), "dakera_knowledge_network_cross_agent", &json!({})).await;
        assert!(result.is_some());
    }
}
