//! SEC-3: Encryption key rotation tool
//!
//! Covers the zero-downtime key rotation endpoint. Requires `SuperAdmin` scope.
//!
//! Tools:
//!   - `dakera_encryption_rotate_key` — POST /admin/encryption/rotate-key

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "dakera_encryption_rotate_key".into(),
        description: "Rotate the AES-256-GCM encryption key used for memory content at rest. \
            Re-encrypts all stored memories under the new key in a single atomic pass. \
            The server begins using the new key immediately — existing clients continue \
            operating without restart (zero-downtime). Requires SuperAdmin scope. \
            new_key accepts either a 64-char hex string (raw 256-bit key) or a passphrase \
            (stretched via PBKDF2-HMAC-SHA256). Returns the number of memories rotated \
            and the namespaces affected."
            .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "new_key": {
                    "type": "string",
                    "description": "New encryption key: 64-char hex string or passphrase"
                },
                "namespace": {
                    "type": "string",
                    "description": "Rotate only this namespace. If omitted, all namespaces are rotated."
                }
            },
            "required": ["new_key"]
        }),
    }]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_encryption_rotate_key" => Some(tool_rotate_key(client, args).await),
        _ => None,
    }
}

async fn tool_rotate_key(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let new_key = match require_string(args, "new_key") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let mut body = json!({ "new_key": new_key });
    if let Some(ns) = args.get("namespace").and_then(|v| v.as_str()) {
        body["namespace"] = json!(ns);
    }

    match client
        .post_json("/admin/encryption/rotate-key", &body)
        .await
    {
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
        assert_eq!(definitions().len(), 1);
    }

    #[test]
    fn test_definition_name() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        assert!(names.iter().any(|n| n == "dakera_encryption_rotate_key"));
    }

    #[tokio::test]
    async fn test_rotate_key_dispatches() {
        let result = execute(
            &dummy_client(),
            "dakera_encryption_rotate_key",
            &json!({"new_key": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_rotate_key_missing_new_key() {
        let result = execute(&dummy_client(), "dakera_encryption_rotate_key", &json!({})).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("new_key"));
    }

    #[tokio::test]
    async fn test_rotate_key_with_namespace() {
        let result = execute(
            &dummy_client(),
            "dakera_encryption_rotate_key",
            &json!({"new_key": "my-passphrase", "namespace": "agents"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }
}
