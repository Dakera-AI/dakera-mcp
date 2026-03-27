//! Entity extraction tools — auto_tag, entity_types_set, memory_entities (CE-4 / MCP-4)
//!
//! These tools require dakera core v0.9.x with CE-4 (GLiNER NER) merged.

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_auto_tag".into(),
            description: "Extract structured entities from text using the GLiNER zero-shot NER engine and optional rule-based pre-pass (dates, URLs, UUIDs, emails, IPs). Returns typed entity spans without storing anything.".into(),
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
                        "description": "Entity types to extract (e.g. [\"person\", \"organization\", \"location\"]). Empty list uses rule-based extraction only."
                    }
                },
                "required": ["content"]
            }),
        },
        ToolDefinition {
            name: "dakera_entity_types_set".into(),
            description: "Configure entity extraction for a namespace. Enable or disable automatic GLiNER entity tagging at memory write time, and set which entity types to extract.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to configure"
                    },
                    "extract_entities": {
                        "type": "boolean",
                        "description": "Whether to automatically extract entities when memories are stored"
                    },
                    "entity_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Entity types to extract (e.g. [\"person\", \"organization\", \"date\"])"
                    }
                },
                "required": ["namespace", "extract_entities", "entity_types"]
            }),
        },
        ToolDefinition {
            name: "dakera_entity_types_get".into(),
            description: "Get the current entity extraction configuration for a namespace — whether automatic tagging is enabled and which entity types are configured.".into(),
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
            description: "Retrieve the structured entity tags that were extracted and stored when a memory was written. Returns entity type, value, and confidence score for each entity.".into(),
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
    } else {
        body["entity_types"] = json!([]);
    }
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
    let extract_entities = match args.get("extract_entities").and_then(|v| v.as_bool()) {
        Some(v) => v,
        None => {
            return CallToolResult::error(
                "Missing required parameter: extract_entities (boolean)".to_string(),
            )
        }
    };
    let entity_types = match args.get("entity_types").and_then(|v| v.as_array()) {
        Some(v) => v.clone(),
        None => {
            return CallToolResult::error(
                "Missing required parameter: entity_types (array)".to_string(),
            )
        }
    };
    let body = json!({
        "extract_entities": extract_entities,
        "entity_types": entity_types,
    });
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
    async fn test_entity_types_set_missing_extract_entities() {
        let result = tool_entity_types_set(
            &dummy_client(),
            &json!({"namespace": "ns", "entity_types": []}),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("extract_entities"));
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
