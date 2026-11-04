//! Entity extraction tools — auto_tag, entity_types_set, memory_entities (CE-4 / MCP-4)
//!
//! These tools require dakera core v0.9.x with CE-4 (GLiNER NER) merged.

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

/// The most labels a namespace's `entity_types` may hold (Dakera v0.12.4).
pub const MAX_ENTITY_LABELS: usize = 200;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_auto_tag".into(),
            description: "Extract typed entity spans from text without storing: a rule-based pass (dates, URLs, emails, IPs) plus the server's GLiNER model (v0.12.4: chosen by the embedding model's languages, gliner-bi-small English or gliner-x-small multilingual). extractor says which ran; warnings a failed model pass.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Text content to extract entities from"
                    },
                    "entity_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Labels (default: person, organization, location); presets @conversational, @technical, @personal, @pii; at most 200, no ':'"
                    },
                    "lang": { "type": "string", "description": "Language of the text: en, de, fr, es, it, pt, nl; Dakera v0.12+" }
                },
                "required": ["content"]
            }),
        },
        ToolDefinition {
            name: "dakera_entity_types_set".into(),
            description: "Turn automatic entity tagging of a namespace's stored memories on or off and set its labels (only the fields given change). Since v0.12.4 it is on by default (person, organization, location; source server_default) and tags are added in the background just after each store.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to configure"
                    },
                    "extract_entities": {
                        "type": "boolean",
                        "description": "Tag memories when they are stored"
                    },
                    "entity_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Labels, any words (e.g. person, product); presets @conversational, @technical, @personal, @pii; at most 200 of 64 chars, no ':'; [] = rules only"
                    }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_entity_types_get".into(),
            description: "Get the entity tagging a namespace's stores run: extract_entities, entity_types, and source (v0.12.4+: namespace = set for it, server_default = the server's default).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to retrieve entity configuration for"
                    }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_entities".into(),
            description: "Retrieve entity tags extracted when a memory was written: type, value, and confidence score per entity.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Memory ID to retrieve entities for"
                    }
                },
                "required": ["memory_id"]
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
        "dakera_auto_tag" => Some(tool_auto_tag(client, args).await),
        "dakera_entity_types_set" => Some(tool_entity_types_set(client, args).await),
        "dakera_entity_types_get" => Some(tool_entity_types_get(client, args).await),
        "dakera_memory_entities" => Some(tool_memory_entities(client, args).await),
        _ => None,
    }
}

async fn tool_auto_tag(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let content = match require_string(args, "content") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({ "content": content });
    if let Some(types) = args.get("entity_types").and_then(|v| v.as_array()) {
        body["entity_types"] = json!(types);
    }
    super::memory::forward_optional_strings(&mut body, args, &["lang"]);
    match client.post_json("/v1/memories/extract", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_entity_types_set(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    // PATCH: only the fields given change.
    let mut body = json!({});
    if let Some(v) = args.get("extract_entities").filter(|v| !v.is_null()) {
        match v.as_bool() {
            Some(b) => body["extract_entities"] = json!(b),
            None => {
                return CallToolResult::error(format!(
                    "extract_entities must be true or false; got {v}"
                ))
            }
        }
    }
    if let Some(v) = args.get("entity_types").filter(|v| !v.is_null()) {
        let Some(types) = v.as_array() else {
            return CallToolResult::error(format!("entity_types must be an array; got {v}"));
        };
        if types.len() > MAX_ENTITY_LABELS {
            return CallToolResult::error(format!(
                "entity_types holds at most {MAX_ENTITY_LABELS} labels; got {}",
                types.len()
            ));
        }
        body["entity_types"] = json!(types);
    }
    if body.as_object().is_some_and(|b| b.is_empty()) {
        return CallToolResult::error(
            "Give extract_entities, entity_types or both; nothing was changed.".to_string(),
        );
    }
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/config", encoded);
    match client.patch_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_entity_types_get(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&namespace);
    let path = format!("/v1/namespaces/{}/config", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_memory_entities(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&memory_id);
    let path = format!("/v1/memory/entities/{}", encoded);
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
        assert!(names.contains(&"dakera_auto_tag"));
        assert!(names.contains(&"dakera_entity_types_set"));
        assert!(names.contains(&"dakera_entity_types_get"));
        assert!(names.contains(&"dakera_memory_entities"));
    }

    #[tokio::test]
    async fn test_auto_tag_missing_content_returns_error() {
        let result = tool_auto_tag(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("content"));
    }

    #[tokio::test]
    async fn test_entity_types_set_missing_namespace() {
        let result = tool_entity_types_set(
            &dummy_client(),
            &json!({"extract_entities": true, "entity_types": []}),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_entity_types_set_needs_a_field_to_change() {
        let result = tool_entity_types_set(&dummy_client(), &json!({"namespace": "ns"})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("extract_entities"));
    }

    #[tokio::test]
    async fn test_entity_types_set_refuses_more_labels_than_the_server() {
        let labels: Vec<String> = (0..201).map(|i| format!("l{i}")).collect();
        let result = tool_entity_types_set(
            &dummy_client(),
            &json!({"namespace": "ns", "entity_types": labels}),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("at most 200"));
    }

    #[tokio::test]
    async fn test_entity_types_get_missing_namespace() {
        let result = tool_entity_types_get(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("namespace"));
    }

    #[tokio::test]
    async fn test_entity_types_get_dispatches() {
        let result = execute(&dummy_client(), "dakera_entity_types_get", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true)); // missing namespace param
    }

    #[tokio::test]
    async fn test_memory_entities_missing_id() {
        let result = tool_memory_entities(&dummy_client(), &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("memory_id"));
    }

    #[tokio::test]
    async fn test_execute_dispatches_auto_tag() {
        let result = execute(&dummy_client(), "dakera_auto_tag", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true)); // missing content param
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown_xyz", &json!({})).await;
        assert!(result.is_none());
    }
}
