//! Identity tool (Dakera v0.12.2): `GET /v1/auth/whoami`.
//!
//! Says which key the MCP server authenticates with, as the Dakera server reads
//! it: its scope, the namespaces it reaches (`null` = all; `p*` prefix patterns
//! since v0.12.2), whether it is unrestricted, when it expires, and the entries
//! that grant nothing (`inert_namespaces`). The first thing to look at when a
//! call gets `403`. Any valid key may call it; no scope is needed.
//!
//! Tools:
//!   - `dakera_whoami` — GET /v1/auth/whoami

use serde_json::json;

use super::{ok_json, route_missing, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

/// The answer to `dakera_whoami` on a server without the route.
pub const WHOAMI_UNSUPPORTED: &str = "dakera_whoami needs Dakera server v0.12.2 or later: this \
     server has no GET /v1/auth/whoami. dakera_health shows its version; ask the operator which \
     scope and namespaces DAKERA_API_KEY was created with.";

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "dakera_whoami".into(),
        description:
            "Show the API key this MCP server uses, as Dakera reads it (v0.12.2+): scope, \
            namespaces it reaches (null = all; agent memory lives in _dakera_agent_<agent_id>), \
            expires_at and inert_namespaces (entries that grant nothing). Call it when a tool \
            gets 403."
                .into(),
        input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
    }]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    _args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_whoami" => Some(tool_whoami(client).await),
        _ => None,
    }
}

async fn tool_whoami(client: &DakeraApiClient) -> CallToolResult {
    let (status, text) = match client
        .send_raw(reqwest::Method::GET, "/v1/auth/whoami", None)
        .await
    {
        Ok(answer) => answer,
        Err(e) => return CallToolResult::error(e),
    };
    if route_missing(status, &text) {
        return CallToolResult::error(WHOAMI_UNSUPPORTED.to_string());
    }
    if !status.is_success() {
        return CallToolResult::error(format!("API error ({}): {}", status, text));
    }
    match serde_json::from_str(&text) {
        Ok(doc) => ok_json(&doc),
        Err(e) => CallToolResult::error(format!("JSON parse failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        assert!(execute(&dummy_client(), "not_whoami", &json!({}))
            .await
            .is_none());
    }

    #[test]
    fn test_definitions() {
        let defs = definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "dakera_whoami");
    }

    #[tokio::test]
    async fn test_unreachable_server_is_an_error() {
        let result = execute(&dummy_client(), "dakera_whoami", &json!({}))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }
}
