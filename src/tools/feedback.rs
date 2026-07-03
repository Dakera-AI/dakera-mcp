//! INT-1: Memory feedback tools
//!
//! Allows agents to upvote, downvote, or flag memories, adjusting their
//! importance scores via the feedback signal system.
//!
//! Tools:
//!   - `dakera_memory_feedback`         — POST /v1/memories/:id/feedback
//!   - `dakera_memory_feedback_get`     — GET  /v1/memories/:id/feedback
//!   - `dakera_agent_feedback_summary`  — GET  /v1/agents/:id/feedback/summary

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_memory_feedback".into(),
            description: "Submit a feedback signal (upvote/downvote/flag) on a memory. \
                Upvote raises importance; downvote lowers it; flag marks for review and may accelerate decay."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "memory_id": {
                        "type": "string",
                        "description": "ID of the memory to give feedback on"
                    },
                    "signal": {
                        "type": "string",
                        "enum": ["upvote", "downvote", "flag"],
                        "description": "Feedback signal: upvote (increase importance), downvote (decrease), flag (mark for review)"
                    }
                },
                "required": ["agent_id", "memory_id", "signal"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_feedback_get".into(),
            description: "Retrieve the feedback history for a memory: all upvote, downvote, and flag signals with timestamps."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "memory_id": {
                        "type": "string",
                        "description": "ID of the memory to get feedback history for"
                    }
                },
                "required": ["agent_id", "memory_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_agent_feedback_summary".into(),
            description: "Get aggregate feedback stats for an agent's memories: total upvotes, downvotes, flags, and the top-rated/most-flagged memories."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" }
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
        "dakera_memory_feedback" => Some(tool_memory_feedback(client, args).await),
        "dakera_memory_feedback_get" => Some(tool_memory_feedback_get(client, args).await),
        "dakera_agent_feedback_summary" => Some(tool_agent_feedback_summary(client, args).await),
        _ => None,
    }
}

async fn tool_memory_feedback(
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
    let signal = match require_string(args, "signal") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!("/v1/memories/{}/feedback", urlencoding::encode(&memory_id));
    // Server contract (MemoryFeedbackRequest): agent_id is required to derive
    // the memory namespace and authorize write scope — omitting it is a 422.
    let body = json!({ "agent_id": agent_id, "signal": signal });

    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_feedback_get(
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

    let path = format!(
        "/v1/memories/{}/feedback?agent_id={}",
        urlencoding::encode(&memory_id),
        urlencoding::encode(&agent_id)
    );
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_agent_feedback_summary(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!(
        "/v1/agents/{}/feedback/summary",
        urlencoding::encode(&agent_id)
    );
    match client.get_json(&path).await {
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
        assert_eq!(definitions().len(), 3);
    }

    #[test]
    fn test_definition_names() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        assert!(names.iter().any(|n| n == "dakera_memory_feedback"));
        assert!(names.iter().any(|n| n == "dakera_memory_feedback_get"));
        assert!(names.iter().any(|n| n == "dakera_agent_feedback_summary"));
    }

    /// Regression guard for the 422 schema/API drift: the MCP tool params must
    /// match the server's deserialization contract — `feedback_by_id` requires
    /// `agent_id` + `signal` in the body, `get_feedback_history` requires an
    /// `agent_id` query parameter.
    #[test]
    fn test_schema_requires_agent_id() {
        let defs = definitions();
        let feedback = defs
            .iter()
            .find(|d| d.name == "dakera_memory_feedback")
            .unwrap();
        let required: Vec<&str> = feedback.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["agent_id", "memory_id", "signal"]);

        let feedback_get = defs
            .iter()
            .find(|d| d.name == "dakera_memory_feedback_get")
            .unwrap();
        let required: Vec<&str> = feedback_get.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["agent_id", "memory_id"]);
    }

    #[tokio::test]
    async fn test_feedback_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_feedback",
            &json!({"agent_id": "core-engine", "memory_id": "mem_123", "signal": "upvote"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_feedback_get_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_feedback_get",
            &json!({"agent_id": "core-engine", "memory_id": "mem_123"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_feedback_missing_agent_id() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_feedback",
            &json!({"memory_id": "mem_123", "signal": "upvote"}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_feedback_get_missing_agent_id() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_feedback_get",
            &json!({"memory_id": "mem_123"}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_agent_summary_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_agent_feedback_summary",
            &json!({"agent_id": "core-engine"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_feedback_missing_signal() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_feedback",
            &json!({"agent_id": "core-engine", "memory_id": "mem_123"}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("signal"));
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }
}
