//! Dakera MCP tool definitions and execution
//!
//! Defines the tools exposed by the Dakera MCP server and handles
//! calling the Dakera API for each tool invocation.

pub mod agents;
pub mod audit;
pub mod autopilot;
pub mod decay;
pub mod encryption;
pub mod entities;
pub mod extractor;
pub mod feedback;
pub mod fulltext;
pub mod graph;
pub mod inference;
pub mod knowledge;
pub mod memory;
pub mod namespace_keys;
pub mod namespaces;
pub mod ode;
pub mod sessions;
pub mod transfer;
pub mod vectors;

use crate::protocol::{CallToolResult, ToolDefinition};

const MAX_RETRIES: u32 = 3;
const RETRY_DELAYS_MS: [u64; 3] = [100, 500, 2000];

/// API client for calling Dakera endpoints
pub struct DakeraApiClient {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

fn is_retryable_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout() || err.is_request()
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 502 | 503 | 504 | 408 | 429)
}

impl DakeraApiClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(5))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(4)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            api_key,
            client,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req
    }

    async fn send_with_retry(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(reqwest::StatusCode, String), String> {
        let mut last_err = String::new();
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = RETRY_DELAYS_MS
                    .get((attempt - 1) as usize)
                    .copied()
                    .unwrap_or(2000);
                tracing::warn!(attempt, delay_ms = delay, path, "Retrying request");
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }

            let mut req = self.request(method.clone(), path);
            if let Some(b) = body {
                req = req.json(b);
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if is_retryable_status(status) && attempt < MAX_RETRIES {
                        let text = resp.text().await.unwrap_or_default();
                        last_err = format!("API error ({}): {}", status, text);
                        tracing::warn!(attempt, status = %status, path, "Retryable status");
                        continue;
                    }
                    let text = resp
                        .text()
                        .await
                        .map_err(|e| format!("Read body failed: {}", e))?;
                    return Ok((status, text));
                }
                Err(e) => {
                    if is_retryable_error(&e) && attempt < MAX_RETRIES {
                        last_err = format!("HTTP request failed: {}", e);
                        tracing::warn!(attempt, error = %e, path, "Retryable error");
                        continue;
                    }
                    return Err(format!("HTTP request failed: {}", e));
                }
            }
        }
        Err(last_err)
    }

    fn parse_json_response(
        status: reqwest::StatusCode,
        text: &str,
    ) -> Result<serde_json::Value, String> {
        if status.is_success() {
            serde_json::from_str(text).map_err(|e| format!("JSON parse failed: {}", e))
        } else {
            Err(format!("API error ({}): {}", status, text))
        }
    }

    pub async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::POST, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::GET, path, None)
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn put_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::PUT, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn delete_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::DELETE, path, None)
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn delete_with_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::DELETE, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn patch_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::PATCH, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn get_text(&self, path: &str) -> Result<String, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::GET, path, None)
            .await?;
        if status.is_success() {
            Ok(text)
        } else {
            Err(format!("API error ({}): {}", status, text))
        }
    }

    pub async fn post_multipart_text(
        &self,
        path: &str,
        text: &str,
    ) -> Result<serde_json::Value, String> {
        let part = reqwest::multipart::Part::text(text.to_string())
            .file_name("import.jsonl")
            .mime_str("application/octet-stream")
            .map_err(|e| format!("Multipart build failed: {}", e))?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.post(&url).multipart(form);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| format!("Read body failed: {}", e))?;

        Self::parse_json_response(status, &body)
    }
}

/// Helper to extract a required string parameter, returning an error CallToolResult on failure.
pub fn require_string(args: &serde_json::Value, field: &str) -> Result<String, CallToolResult> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| CallToolResult::error(format!("Missing required parameter: {}", field)))
}

/// Format a successful JSON response.
pub fn ok_json(value: &serde_json::Value) -> CallToolResult {
    CallToolResult::text(serde_json::to_string_pretty(value).unwrap_or_default())
}

/// Return all tool definitions aggregated from every module.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    let mut defs = Vec::new();
    defs.extend(memory::definitions());
    defs.extend(sessions::definitions());
    defs.extend(agents::definitions());
    defs.extend(knowledge::definitions());
    defs.extend(namespaces::definitions());
    defs.extend(namespace_keys::definitions());
    defs.extend(vectors::definitions());
    defs.extend(inference::definitions());
    defs.extend(fulltext::definitions());
    defs.extend(autopilot::definitions());
    defs.extend(decay::definitions());
    defs.extend(entities::definitions());
    defs.extend(graph::definitions());
    defs.extend(audit::definitions());
    defs.extend(transfer::definitions());
    defs.extend(feedback::definitions());
    defs.extend(extractor::definitions());
    defs.extend(encryption::definitions());
    defs.extend(ode::definitions());
    defs
}

/// Execute a tool call by dispatching to the appropriate module.
pub async fn execute_tool(
    client: &DakeraApiClient,
    name: &str,
    arguments: &serde_json::Value,
) -> CallToolResult {
    if let Some(result) = memory::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = sessions::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = agents::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = knowledge::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = namespaces::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = vectors::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = inference::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = fulltext::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = autopilot::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = decay::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = entities::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = graph::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = audit::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = namespace_keys::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = transfer::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = feedback::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = extractor::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = encryption::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = ode::execute(client, name, arguments).await {
        return result;
    }
    CallToolResult::error(format!("Unknown tool: {}", name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_require_string_present() {
        let args = json!({"agent_id": "test-agent"});
        assert_eq!(require_string(&args, "agent_id").unwrap(), "test-agent");
    }

    #[test]
    fn test_require_string_missing_field() {
        let args = json!({});
        let err = require_string(&args, "agent_id").unwrap_err();
        assert_eq!(err.is_error, Some(true));
        assert!(err.content[0].text.contains("agent_id"));
    }

    #[test]
    fn test_require_string_wrong_type_number() {
        let args = json!({"count": 42});
        let err = require_string(&args, "count").unwrap_err();
        assert_eq!(err.is_error, Some(true));
        assert!(err.content[0].text.contains("count"));
    }

    #[test]
    fn test_require_string_wrong_type_bool() {
        let args = json!({"flag": true});
        let err = require_string(&args, "flag").unwrap_err();
        assert_eq!(err.is_error, Some(true));
    }

    #[test]
    fn test_require_string_null_value() {
        let args = json!({"key": null});
        let err = require_string(&args, "key").unwrap_err();
        assert_eq!(err.is_error, Some(true));
    }

    #[test]
    fn test_ok_json_text_is_pretty() {
        let value = json!({"key": "value", "num": 1});
        let result = ok_json(&value);
        assert!(result.is_error.is_none());
        let text = &result.content[0].text;
        assert!(
            text.contains('\n'),
            "Expected pretty-printed JSON with newlines"
        );
        assert!(text.contains("\"key\""));
    }

    #[test]
    fn test_ok_json_empty_object() {
        let result = ok_json(&json!({}));
        assert!(result.is_error.is_none());
        assert!(!result.content[0].text.is_empty());
    }

    #[test]
    fn test_tool_definitions_not_empty() {
        let defs = tool_definitions();
        assert!(!defs.is_empty());
    }

    #[test]
    fn test_tool_definitions_have_unique_names() {
        let defs = tool_definitions();
        let mut seen = std::collections::HashSet::new();
        for def in &defs {
            assert!(
                seen.insert(def.name.clone()),
                "Duplicate tool name: {}",
                def.name
            );
        }
    }

    #[test]
    fn test_tool_definitions_all_have_descriptions() {
        for def in tool_definitions() {
            assert!(!def.name.is_empty(), "Tool has empty name");
            assert!(
                !def.description.is_empty(),
                "Tool '{}' has empty description",
                def.name
            );
        }
    }

    #[test]
    fn test_tool_definitions_contain_expected_tools() {
        let defs = tool_definitions();
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains("dakera_store"));
        assert!(names.contains("dakera_recall"));
        assert!(names.contains("dakera_session_start"));
        assert!(names.contains("dakera_session_end"));
        // v0.2.2 tools
        assert!(
            names.contains("dakera_namespace_configure"),
            "dakera_namespace_configure missing from tool definitions"
        );
        assert!(
            names.contains("dakera_knowledge_network_cross_agent"),
            "dakera_knowledge_network_cross_agent missing from tool definitions"
        );
        // v0.3.0 CE-2 tools
        assert!(
            names.contains("dakera_batch_recall"),
            "dakera_batch_recall missing from tool definitions"
        );
        assert!(
            names.contains("dakera_batch_forget"),
            "dakera_batch_forget missing from tool definitions"
        );
        // v0.3.2 PILOT-4 tools
        assert!(
            names.contains("dakera_autopilot_status"),
            "dakera_autopilot_status missing from tool definitions"
        );
        assert!(
            names.contains("dakera_autopilot_trigger"),
            "dakera_autopilot_trigger missing from tool definitions"
        );
        // v0.4.0 DECAY tools
        assert!(
            names.contains("dakera_decay_config_get"),
            "dakera_decay_config_get missing from tool definitions"
        );
        assert!(
            names.contains("dakera_decay_config_set"),
            "dakera_decay_config_set missing from tool definitions"
        );
        assert!(
            names.contains("dakera_decay_stats"),
            "dakera_decay_stats missing from tool definitions"
        );
        // v0.5.0 MCP-4 / CE-4 entity tools
        assert!(
            names.contains("dakera_auto_tag"),
            "dakera_auto_tag missing from tool definitions"
        );
        assert!(
            names.contains("dakera_entity_types_set"),
            "dakera_entity_types_set missing from tool definitions"
        );
        assert!(
            names.contains("dakera_entity_types_get"),
            "dakera_entity_types_get missing from tool definitions"
        );
        assert!(
            names.contains("dakera_memory_entities"),
            "dakera_memory_entities missing from tool definitions"
        );
        // v0.6.0 MCP-4 / CE-5 graph tools
        assert!(
            names.contains("dakera_graph_traverse"),
            "dakera_graph_traverse missing from tool definitions"
        );
        assert!(
            names.contains("dakera_graph_path"),
            "dakera_graph_path missing from tool definitions"
        );
        assert!(
            names.contains("dakera_graph_link_memory"),
            "dakera_graph_link_memory missing from tool definitions"
        );
        assert!(
            names.contains("dakera_graph_export"),
            "dakera_graph_export missing from tool definitions"
        );
        // v0.7.0 OBS-1 audit tool
        assert!(
            names.contains("dakera_audit_query"),
            "dakera_audit_query missing from tool definitions"
        );
        // v0.9.0 ODE-2 entity extraction tool
        assert!(
            names.contains("dakera_extract_entities"),
            "dakera_extract_entities missing from tool definitions"
        );
        // v0.9.6 KG-2 graph query tools
        assert!(
            names.contains("dakera_kg_traverse"),
            "dakera_kg_traverse missing from tool definitions"
        );
        assert!(
            names.contains("dakera_kg_query"),
            "dakera_kg_query missing from tool definitions"
        );
        assert!(
            names.contains("dakera_kg_export"),
            "dakera_kg_export missing from tool definitions"
        );
        // v0.9.2 MCP-5 cognitive tools
        assert!(
            names.contains("dakera_memory_policy_get"),
            "dakera_memory_policy_get missing from tool definitions"
        );
        assert!(
            names.contains("dakera_memory_policy_set"),
            "dakera_memory_policy_set missing from tool definitions"
        );
        assert!(
            names.contains("dakera_recall_associated"),
            "dakera_recall_associated missing from tool definitions"
        );
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_error() {
        let client = DakeraApiClient::new("http://localhost:9999".to_string(), None);
        let result = execute_tool(&client, "nonexistent_tool_xyz", &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("nonexistent_tool_xyz"));
    }
}
