//! ODE-2: Entity extraction via the dakera-ode sidecar
//!
//! This module routes calls to the dakera-ode sidecar service rather than the
//! main dakera core API. The sidecar URL is read from `DAKERA_ODE_URL`
//! (default: `http://localhost:8080`).
//!
//! Tools:
//!   - `dakera_extract_entities` — POST /ode/extract

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "dakera_extract_entities".into(),
        description: "Extract named entities from text via the dakera-ode GLiNER sidecar, returning label, offsets, and confidence. \
            Defaults: PERSON, ORG, LOCATION, DATE, TECHNOLOGY, PRODUCT, EVENT, CONCEPT. \
            Requires DAKERA_ODE_URL. Use dakera_auto_tag if the ODE sidecar is not deployed."
            .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Text to extract entities from"
                },
                "agent_id": { "type": "string" },
                "memory_id": {
                    "type": "string",
                    "description": "Optional memory ID for context"
                },
                "entity_types": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Override default entity types (e.g. [\"PERSON\", \"ORG\"]). \
                        If omitted, uses the service defaults."
                }
            },
            "required": ["content", "agent_id"]
        }),
    }]
}

pub async fn execute(
    _client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_extract_entities" => Some(tool_extract_entities(args).await),
        _ => None,
    }
}

async fn tool_extract_entities(args: &serde_json::Value) -> CallToolResult {
    let content = match require_string(args, "content") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let ode_url =
        std::env::var("DAKERA_ODE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let ode_url = ode_url.trim_end_matches('/');
    let api_key = std::env::var("DAKERA_API_KEY").ok();

    let mut body = json!({
        "content": content,
        "agent_id": agent_id,
    });

    if let Some(mid) = args.get("memory_id").and_then(|v| v.as_str()) {
        body["memory_id"] = json!(mid);
    }
    if let Some(types) = args.get("entity_types") {
        if types.is_array() {
            body["entity_types"] = types.clone();
        }
    }

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut req = http.post(format!("{}/ode/extract", ode_url)).json(&body);

    if let Some(key) = api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                match serde_json::from_str(&text) {
                    Ok(v) => ok_json(&v),
                    Err(e) => CallToolResult::error(format!("JSON parse failed: {}", e)),
                }
            } else {
                CallToolResult::error(format!(
                    "ODE API error ({}): {}",
                    status,
                    &text[..text.len().min(200)]
                ))
            }
        }
        Err(e) => CallToolResult::error(format!(
            "ODE request failed (is DAKERA_ODE_URL set?): {}",
            e
        )),
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
        assert_eq!(definitions().len(), 1);
    }

    #[test]
    fn test_definition_name() {
        assert_eq!(definitions()[0].name, "dakera_extract_entities");
    }

    #[test]
    fn test_definition_has_description() {
        assert!(!definitions()[0].description.is_empty());
    }

    #[tokio::test]
    async fn test_extract_entities_missing_content() {
        let result = execute(
            &dummy_client(),
            "dakera_extract_entities",
            &json!({"agent_id": "agent-1"}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("content"));
    }

    #[tokio::test]
    async fn test_extract_entities_missing_agent_id() {
        let result = execute(
            &dummy_client(),
            "dakera_extract_entities",
            &json!({"content": "Alice visited Berlin."}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn test_extract_entities_dispatches_to_unreachable_ode() {
        // No real ODE running — expect a connection error (not None)
        std::env::set_var("DAKERA_ODE_URL", "http://localhost:19999");
        let result = execute(
            &dummy_client(),
            "dakera_extract_entities",
            &json!({"content": "Alice visited Berlin.", "agent_id": "agent-1"}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }
}
