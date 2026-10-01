//! EXT-1: Pluggable extraction provider tools
//!
//! Covers the unified extraction endpoint and per-namespace extractor
//! configuration. Supports all five backends: `gliner`, `openai`,
//! `anthropic`, `openrouter`, `ollama`, and `none`.
//!
//! Tools:
//!   - `dakera_extract`        — POST /v1/extract
//!   - `dakera_extractor_get`  — GET  /v1/namespaces/:namespace/extractor
//!   - `dakera_extractor_set`  — PATCH /v1/namespaces/:namespace/extractor

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_extract".into(),
            description: "Extract entities, topics, key phrases and a summary from text with the \
                provider chain: extractor_override, then the namespace default, then the server \
                default (GLiNER local). For ad-hoc GLiNER types use dakera_auto_tag. Needs a write \
                key for the namespace (for all namespaces when neither namespace nor agent_id is given)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to extract information from"
                    },
                    "namespace": {
                        "type": "string",
                        "description": "Namespace whose extractor config applies (default: agent_id's)"
                    },
                    "agent_id": { "type": "string" },
                    "lang": {
                        "type": "string",
                        "description": "en, de, fr, es, it, pt or nl (v0.12+)"
                    },
                    "extractor_override": {
                        "type": "object",
                        "description": "Provider for this request only",
                        "properties": {
                            "provider": {
                                "type": "string",
                                "enum": ["none", "gliner", "openai", "anthropic", "openrouter", "ollama"]
                            },
                            "model": { "type": "string" },
                            "base_url": { "type": "string" },
                            "api_key": { "type": "string", "description": "Never persisted — used for this request only." }
                        },
                        "required": ["provider"]
                    }
                },
                "required": ["text"]
            }),
        },
        ToolDefinition {
            name: "dakera_extractor_get".into(),
            description: "Read the default extraction provider config for a namespace. \
                Returns provider, model, and base_url; defaults to provider=none if not set."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to read the extractor config for"
                    }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_extractor_set".into(),
            description: "Set the default extraction provider for a namespace (replaces the whole config: \
                omitted fields such as model are cleared). \
                The config is stored server-side and used by all subsequent calls to \
                dakera_extract (unless a per-request override is provided). \
                Set provider=none to clear the namespace default. \
                Note: api_key is accepted here but is NEVER persisted — pass it \
                via extractor_override in dakera_extract for per-call auth."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to configure"
                    },
                    "provider": {
                        "type": "string",
                        "enum": ["none", "gliner", "openai", "anthropic", "openrouter", "ollama"],
                        "description": "Extraction backend to use as the namespace default"
                    },
                    "model": {
                        "type": "string",
                        "description": "Model name (provider-specific). Omit to use the recommended default."
                    },
                    "base_url": {
                        "type": "string",
                        "description": "Base URL override — used for openrouter and ollama."
                    }
                },
                "required": ["namespace", "provider"]
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
        "dakera_extract" => Some(tool_extract(client, args).await),
        "dakera_extractor_get" => Some(tool_extractor_get(client, args).await),
        "dakera_extractor_set" => Some(tool_extractor_set(client, args).await),
        _ => None,
    }
}

async fn tool_extract(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let text = match require_string(args, "text") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let mut body = json!({ "text": text });

    if let Some(ns) = args.get("namespace").and_then(|v| v.as_str()) {
        body["namespace"] = json!(ns);
    }
    // POST /v1/extract has no `entity_types` (ExtractRequest): GLiNER takes the
    // namespace's entity config, so the tool no longer offers a field the
    // server drops. `agent_id` selects the agent's memory namespace.
    super::memory::forward_optional_strings(&mut body, args, &["lang", "agent_id"]);
    if let Some(ov) = args.get("extractor_override") {
        if ov.is_object() {
            body["extractor_override"] = ov.clone();
        }
    }

    match client.post_json("/v1/extract", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_extractor_get(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!(
        "/v1/namespaces/{}/extractor",
        urlencoding::encode(&namespace)
    );
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_extractor_set(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let provider = match require_string(args, "provider") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let mut body = json!({ "provider": provider });
    if let Some(m) = args.get("model").and_then(|v| v.as_str()) {
        body["model"] = json!(m);
    }
    if let Some(u) = args.get("base_url").and_then(|v| v.as_str()) {
        body["base_url"] = json!(u);
    }

    let path = format!(
        "/v1/namespaces/{}/extractor",
        urlencoding::encode(&namespace)
    );
    match client.patch_json(&path, &body).await {
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
        assert!(names.iter().any(|n| n == "dakera_extract"));
        assert!(names.iter().any(|n| n == "dakera_extractor_get"));
        assert!(names.iter().any(|n| n == "dakera_extractor_set"));
    }

    #[tokio::test]
    async fn test_extract_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_extract",
            &json!({"text": "Alice met Bob at Anthropic HQ."}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_extract_missing_text() {
        let result = execute(&dummy_client(), "dakera_extract", &json!({})).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("text"));
    }

    #[tokio::test]
    async fn test_extractor_get_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_extractor_get",
            &json!({"namespace": "agents"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_extractor_set_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_extractor_set",
            &json!({"namespace": "agents", "provider": "gliner"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_extractor_set_missing_provider() {
        let result = execute(
            &dummy_client(),
            "dakera_extractor_set",
            &json!({"namespace": "agents"}),
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("provider"));
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }
}
