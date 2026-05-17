//! Health probe tools — ready, live, Prometheus metrics

use serde_json::json;

use super::DakeraApiClient;
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_health_ready".into(),
            description: "Readiness probe — returns 200 when the server is ready to serve requests (storage and inference loaded). No auth required.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_health_live".into(),
            description: "Liveness probe — returns 200 when the server process is alive. No auth required.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_ops_metrics".into(),
            description: "Get Prometheus-format metrics (counters, histograms, gauges) for the Dakera instance. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
    ]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    _args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_health_ready" => {
            Some(match client.get_text("/health/ready").await {
                Ok(text) => CallToolResult::text(
                    if text.is_empty() { "ready".into() } else { text },
                ),
                Err(e) => CallToolResult::error(e),
            })
        }
        "dakera_health_live" => {
            Some(match client.get_text("/health/live").await {
                Ok(text) => CallToolResult::text(
                    if text.is_empty() { "alive".into() } else { text },
                ),
                Err(e) => CallToolResult::error(e),
            })
        }
        "dakera_ops_metrics" => Some(match client.get_text("/v1/ops/metrics").await {
            Ok(text) => CallToolResult::text(text),
            Err(e) => CallToolResult::error(e),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::DakeraApiClient;
    use serde_json::json;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[tokio::test]
    async fn test_unknown_returns_none() {
        assert!(
            execute(&dummy_client(), "not_health", &json!({}))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_definitions_unique() {
        let defs = definitions();
        let mut seen = std::collections::HashSet::new();
        for d in &defs {
            assert!(seen.insert(d.name.as_str()), "duplicate: {}", d.name);
        }
        assert_eq!(defs.len(), 3);
    }
}
