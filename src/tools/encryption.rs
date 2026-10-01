//! SEC-3: encryption keyring tools (Dakera v0.12)
//!
//! The v0.12 keyring holds every data key (wrapped under the master key and
//! replicated to every node). A rotation adds a new key for one namespace or
//! for everything and makes it active; the previous key is kept, so reads never
//! break, and the values are re-sealed in the background.
//!
//! Both routes need a *global* admin key: a key pinned to namespaces gets 403
//! on them (v0.12), even with the admin scope.
//!
//! Tools:
//!   - `dakera_encryption_rotate_key` — POST /admin/encryption/rotate-key
//!   - `dakera_encryption_status`     — GET  /admin/encryption/status

use serde_json::json;

use super::{ok_json, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_encryption_rotate_key".into(),
            description: "Rotate the at-rest encryption key of one namespace, or of everything when namespace is omitted. \
                A random key is generated unless new_key (passphrase or 64-char hex) is given. The old key is kept; values are re-sealed in the background \
                (see dakera_encryption_status). Needs a global admin key (not namespace-pinned)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "new_key": {
                        "type": "string",
                        "description": "New key: passphrase or 64-char hex (default: generated)"
                    },
                    "namespace": {
                        "type": "string",
                        "description": "Rotate only this namespace (default: all)"
                    },
                    "wait_secs": {
                        "type": "integer",
                        "description": "Wait up to this long (max 300) for the re-seal before answering"
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_encryption_status".into(),
            description: "Show the encryption keyring (key ids, which key seals which namespace, retirement) and the background re-seal progress. \
                Never returns key material. Needs a global admin key."
                .into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
    ]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_encryption_rotate_key" => Some(tool_rotate_key(client, args).await),
        "dakera_encryption_status" => Some(tool_status(client).await),
        _ => None,
    }
}

/// The body of `POST /admin/encryption/rotate-key`: only what was given.
pub fn rotate_body(args: &serde_json::Value) -> serde_json::Value {
    let mut body = json!({});
    if let Some(key) = args.get("new_key").and_then(|v| v.as_str()) {
        body["new_key"] = json!(key);
    }
    if let Some(ns) = args.get("namespace").and_then(|v| v.as_str()) {
        body["namespace"] = json!(ns);
    }
    if let Some(secs) = args.get("wait_secs").and_then(|v| v.as_u64()) {
        body["wait_secs"] = json!(secs);
    }
    body
}

async fn tool_rotate_key(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let body = rotate_body(args);
    match client
        .post_json("/admin/encryption/rotate-key", &body)
        .await
    {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_status(client: &DakeraApiClient) -> CallToolResult {
    match client.get_json("/admin/encryption/status").await {
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
        assert_eq!(definitions().len(), 2);
    }

    #[test]
    fn test_definition_names() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        assert!(names.iter().any(|n| n == "dakera_encryption_rotate_key"));
        assert!(names.iter().any(|n| n == "dakera_encryption_status"));
    }

    #[test]
    fn test_new_key_is_optional() {
        let defs = definitions();
        let rotate = defs
            .iter()
            .find(|d| d.name == "dakera_encryption_rotate_key")
            .unwrap();
        assert_eq!(rotate.input_schema["required"], json!([]));
    }

    #[test]
    fn test_rotate_body_sends_only_what_was_given() {
        assert_eq!(rotate_body(&json!({})), json!({}));
        let body = rotate_body(&json!({"namespace": "team-a", "wait_secs": 30}));
        assert_eq!(body, json!({"namespace": "team-a", "wait_secs": 30}));
        let body = rotate_body(&json!({"new_key": "a-passphrase"}));
        assert_eq!(body, json!({"new_key": "a-passphrase"}));
    }

    #[tokio::test]
    async fn test_rotate_key_dispatches() {
        let args = json!({"new_key": "deadbeefdeadbeefdeadbeefdeadbeef"});
        let result = execute(&dummy_client(), "dakera_encryption_rotate_key", &args).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_rotate_key_with_namespace() {
        let args = json!({"new_key": "my-passphrase", "namespace": "agents"});
        let result = execute(&dummy_client(), "dakera_encryption_rotate_key", &args).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_status_dispatches() {
        let result = execute(&dummy_client(), "dakera_encryption_status", &json!({})).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }
}
