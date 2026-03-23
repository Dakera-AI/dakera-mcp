//! Decay tools — DECAY-1 / DECAY-2
//!
//! Exposes `dakera_decay_config_get`, `dakera_decay_config_set`, and
//! `dakera_decay_stats` MCP tools that wrap the hot-reload decay config and
//! stats endpoints. All three require Admin scope.

use serde_json::json;

use super::{ok_json, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_decay_config_get".into(),
            description: "Get the current memory-decay configuration (strategy, half-life hours, minimum importance threshold). Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_decay_config_set".into(),
            description: "Update memory-decay settings at runtime (no restart required). Changes take effect on the next decay cycle. All fields are optional — omit fields you do not want to change. Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "strategy": {
                        "type": "string",
                        "enum": ["exponential", "linear", "step"],
                        "description": "Decay strategy: 'exponential' (default), 'linear', or 'step'."
                    },
                    "half_life_hours": {
                        "type": "number",
                        "description": "Half-life in hours — must be > 0. Lower values decay importance faster."
                    },
                    "min_importance": {
                        "type": "number",
                        "description": "Minimum importance threshold (0.0–1.0). Memories whose importance falls below this value are hard-deleted by the decay engine."
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_decay_stats".into(),
            description: "Get cumulative decay statistics and last-cycle details (memories decayed/deleted, cycle count, last run timestamp). Requires Admin scope.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
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
        "dakera_decay_config_get" => Some(tool_decay_config_get(client).await),
        "dakera_decay_config_set" => Some(tool_decay_config_set(client, args).await),
        "dakera_decay_stats" => Some(tool_decay_stats(client).await),
        _ => None,
    }
}

async fn tool_decay_config_get(client: &DakeraApiClient) -> CallToolResult {
    match client.get_json("/admin/decay/config").await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_decay_config_set(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    // Validate strategy if provided
    if let Some(strategy) = args.get("strategy").and_then(|v| v.as_str()) {
        if !matches!(strategy, "exponential" | "linear" | "step") {
            return CallToolResult::error(format!(
                "Invalid strategy '{}': must be 'exponential', 'linear', or 'step'",
                strategy
            ));
        }
    }
    // Validate half_life_hours if provided
    if let Some(half_life) = args.get("half_life_hours").and_then(|v| v.as_f64()) {
        if half_life <= 0.0 {
            return CallToolResult::error("half_life_hours must be > 0".to_string());
        }
    }
    // Validate min_importance if provided
    if let Some(min_imp) = args.get("min_importance").and_then(|v| v.as_f64()) {
        if !(0.0..=1.0).contains(&min_imp) {
            return CallToolResult::error(
                "min_importance must be between 0.0 and 1.0".to_string(),
            );
        }
    }

    let mut body = serde_json::Map::new();
    if let Some(v) = args.get("strategy") {
        body.insert("strategy".into(), v.clone());
    }
    if let Some(v) = args.get("half_life_hours") {
        body.insert("half_life_hours".into(), v.clone());
    }
    if let Some(v) = args.get("min_importance") {
        body.insert("min_importance".into(), v.clone());
    }

    match client
        .put_json("/admin/decay/config", &serde_json::Value::Object(body))
        .await
    {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_decay_stats(client: &DakeraApiClient) -> CallToolResult {
    match client.get_json("/admin/decay/stats").await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[test]
    fn test_decay_definitions() {
        let defs = definitions();
        assert_eq!(defs.len(), 3);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dakera_decay_config_get"));
        assert!(names.contains(&"dakera_decay_config_set"));
        assert!(names.contains(&"dakera_decay_stats"));
    }

    #[test]
    fn test_decay_definitions_have_descriptions() {
        for def in definitions() {
            assert!(
                !def.description.is_empty(),
                "'{}' has no description",
                def.name
            );
        }
    }

    #[tokio::test]
    async fn test_decay_config_set_invalid_strategy() {
        let args = json!({"strategy": "quadratic"});
        let result = tool_decay_config_set(&dummy_client(), &args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("strategy"));
    }

    #[tokio::test]
    async fn test_decay_config_set_invalid_half_life_zero() {
        let args = json!({"half_life_hours": 0.0});
        let result = tool_decay_config_set(&dummy_client(), &args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("half_life_hours"));
    }

    #[tokio::test]
    async fn test_decay_config_set_invalid_half_life_negative() {
        let args = json!({"half_life_hours": -10.0});
        let result = tool_decay_config_set(&dummy_client(), &args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("half_life_hours"));
    }

    #[tokio::test]
    async fn test_decay_config_set_invalid_min_importance_too_high() {
        let args = json!({"min_importance": 1.5});
        let result = tool_decay_config_set(&dummy_client(), &args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("min_importance"));
    }

    #[tokio::test]
    async fn test_decay_config_set_invalid_min_importance_negative() {
        let args = json!({"min_importance": -0.1});
        let result = tool_decay_config_set(&dummy_client(), &args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("min_importance"));
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_none() {
        let result = execute(&dummy_client(), "unknown_tool", &json!({})).await;
        assert!(result.is_none());
    }
}
