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
            description: "Extract structured information (entities, topics, key phrases, summary) \
                from arbitrary text using the configured provider hierarchy: per-request override \
                → namespace default → server default → GLiNER local. Supported providers: \
                `gliner` (zero-config local ONNX), `openai`, `anthropic`, `openrouter`, `ollama`, \
                `none`."
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
                        "description": "Namespace whose default extractor config is used. \
                            If omitted, the server-level default is used."
                    },
                    "entity_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "GLiNER entity type labels (e.g. [\"person\", \"org\", \"location\"]). \
                            Only used when provider is `gliner`."
                    },
                    "extractor_override": {
                        "type": "object",
                        "description": "Per-request provider override — highest priority in the \
                            resolution hierarchy. Fields: provider, model, base_url, api_key.",
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
            description: "Read the default extraction provider configuration for a namespace. \
                Returns the stored ExtractorConfig (provider, model, base_url). \
                If no namespace-level config has been set, returns provider=none."
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
            description: "Set or update the default extraction provider for a namespace. \
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
    if let Some(et) = args.get("entity_types") {
        if et.is_array() {
            body["entity_types"] = et.clone();
        }
    }
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
