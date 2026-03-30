//! SEC-1: Namespace-scoped API key tools
//!
//! Allows namespace admins to create and manage API keys that are automatically
//! scoped to their namespace without needing SuperAdmin privileges.
//!
//! Tools:
//!   - `dakera_namespace_key_create` — POST   /v1/namespaces/:ns/keys
//!   - `dakera_namespace_key_list`   — GET    /v1/namespaces/:ns/keys
//!   - `dakera_namespace_key_delete` — DELETE /v1/namespaces/:ns/keys/:key_id
//!   - `dakera_namespace_key_usage`  — GET    /v1/namespaces/:ns/keys/:key_id/usage

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_namespace_key_create".into(),
            description: "Create a namespace-scoped API key. The key is automatically restricted \
                to the given namespace. Requires Admin scope on that namespace. \
                Cannot create super_admin keys through this endpoint."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to scope the key to"
                    },
                    "name": {
                        "type": "string",
                        "description": "Human-readable name for the key (max 128 chars)"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["read", "write", "admin"],
                        "description": "Permission level for the key (capped at admin)"
                    },
                    "extra_namespaces": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Additional namespaces to grant access to (optional)"
                    },
                    "expires_in_days": {
                        "type": "integer",
                        "description": "Key expiry in days from now (optional, no expiry if omitted)"
                    }
                },
                "required": ["namespace", "name", "scope"]
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_key_list".into(),
            description: "List all API keys that include access to a namespace. \
                Returns key metadata (id, name, scope, created_at) but not the raw key values."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to list keys for"
                    }
                },
                "required": ["namespace"]
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_key_delete".into(),
            description: "Revoke a namespace-scoped API key by ID. \
                The key is immediately invalidated. Requires Admin scope on the namespace."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace the key belongs to"
                    },
                    "key_id": {
                        "type": "string",
                        "description": "ID of the API key to delete"
                    }
                },
                "required": ["namespace", "key_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_namespace_key_usage".into(),
            description: "Get usage statistics for a namespace-scoped API key. \
                Returns request counts, last used timestamp, and rate limit info."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Namespace the key belongs to"
                    },
                    "key_id": {
                        "type": "string",
                        "description": "ID of the API key to query usage for"
                    }
                },
                "required": ["namespace", "key_id"]
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
        "dakera_namespace_key_create" => Some(tool_ns_key_create(client, args).await),
        "dakera_namespace_key_list" => Some(tool_ns_key_list(client, args).await),
        "dakera_namespace_key_delete" => Some(tool_ns_key_delete(client, args).await),
        "dakera_namespace_key_usage" => Some(tool_ns_key_usage(client, args).await),
        _ => None,
    }
}

async fn tool_ns_key_create(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let name = match require_string(args, "name") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let scope = match require_string(args, "scope") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let mut body = json!({
        "name": name,
        "scope": scope
    });

    if let Some(extra) = args.get("extra_namespaces") {
        body["extra_namespaces"] = extra.clone();
    }
    if let Some(days) = args.get("expires_in_days") {
        body["expires_in_days"] = days.clone();
    }

    let path = format!("/v1/namespaces/{}/keys", urlencoding::encode(&namespace));
    match client.post_json(&path, &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_ns_key_list(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!("/v1/namespaces/{}/keys", urlencoding::encode(&namespace));
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_ns_key_delete(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let key_id = match require_string(args, "key_id") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!(
        "/v1/namespaces/{}/keys/{}",
        urlencoding::encode(&namespace),
        urlencoding::encode(&key_id),
    );
    match client.delete_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_ns_key_usage(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let namespace = match require_string(args, "namespace") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let key_id = match require_string(args, "key_id") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let path = format!(
        "/v1/namespaces/{}/keys/{}/usage",
        urlencoding::encode(&namespace),
        urlencoding::encode(&key_id),
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
        assert_eq!(definitions().len(), 4);
    }

    #[test]
    fn test_definition_names() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        assert!(names.iter().any(|n| n == "dakera_namespace_key_create"));
        assert!(names.iter().any(|n| n == "dakera_namespace_key_list"));
        assert!(names.iter().any(|n| n == "dakera_namespace_key_delete"));
        assert!(names.iter().any(|n| n == "dakera_namespace_key_usage"));
    }

    #[tokio::test]
    async fn test_create_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_namespace_key_create",
            &json!({"namespace": "myns", "name": "my-key", "scope": "write"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_list_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_namespace_key_list",
            &json!({"namespace": "myns"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_delete_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_namespace_key_delete",
            &json!({"namespace": "myns", "key_id": "key-123"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_usage_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_namespace_key_usage",
            &json!({"namespace": "myns", "key_id": "key-123"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_missing_required_param() {
        let result = execute(
            &dummy_client(),
            "dakera_namespace_key_create",
            &json!({"namespace": "myns"}), // missing name + scope
        )
        .await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("name"));
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }
}
