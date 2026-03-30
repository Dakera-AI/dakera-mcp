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
            description: "Submit a feedback signal on a memory. Upvotes increase the memory's \
                importance score; downvotes decrease it; flag marks it for review and may \
                accelerate decay. Each signal is recorded in the memory's feedback_history."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
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
                "required": ["memory_id", "signal"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_feedback_get".into(),
            description: "Retrieve the feedback history for a memory, including all upvote, \
                downvote, and flag signals with their timestamps."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "ID of the memory to get feedback history for"
                    }
                },
                "required": ["memory_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_agent_feedback_summary".into(),
            description: "Get a summary of feedback signals for all memories belonging to an \
                agent. Returns aggregate counts of upvotes, downvotes, and flags, along with \
                the most-upvoted and most-flagged memories."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID to get feedback summary for"
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
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let signal = match require_string(args, "signal") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!("/v1/memories/{}/feedback", urlencoding::encode(&memory_id));
    let body = json!({ "signal": signal });

    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_feedback_get(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!("/v1/memories/{}/feedback", urlencoding::encode(&memory_id));
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

    #[tokio::test]
    async fn test_feedback_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_memory_feedback",
            &json!({"memory_id": "mem_123", "signal": "upvote"}),
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
            &json!({"memory_id": "mem_123"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
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
            &json!({"memory_id": "mem_123"}),
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
