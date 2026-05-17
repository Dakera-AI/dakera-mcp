//! Analytics tools — overview, latency, throughput, storage, KPIs

use serde_json::json;

use super::{ok_json, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_analytics_overview".into(),
            description: "Get an analytics overview: total vectors, namespaces, memory ops, and recall rates over the last 24h. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_analytics_latency".into(),
            description: "Get p50/p95/p99 latency histograms for recall, store, and vector query operations. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_analytics_throughput".into(),
            description: "Get operations-per-second throughput metrics (store, recall, vector query) over time. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_analytics_storage".into(),
            description: "Get storage analytics: bytes used per namespace, growth rate, and tier distribution. Requires Admin scope.".into(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        },
        ToolDefinition {
            name: "dakera_kpis".into(),
            description: "Get product KPI snapshot: MAU, DAU, API call totals, and memory health scores. Requires Admin scope.".into(),
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
        "dakera_analytics_overview" => Some(client.get_json("/v1/analytics/overview").await.map(|v| ok_json(&v)).unwrap_or_else(CallToolResult::error)),
        "dakera_analytics_latency" => Some(client.get_json("/v1/analytics/latency").await.map(|v| ok_json(&v)).unwrap_or_else(CallToolResult::error)),
        "dakera_analytics_throughput" => Some(client.get_json("/v1/analytics/throughput").await.map(|v| ok_json(&v)).unwrap_or_else(CallToolResult::error)),
        "dakera_analytics_storage" => Some(client.get_json("/v1/analytics/storage").await.map(|v| ok_json(&v)).unwrap_or_else(CallToolResult::error)),
        "dakera_kpis" => Some(client.get_json("/v1/kpis").await.map(|v| ok_json(&v)).unwrap_or_else(CallToolResult::error)),
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
        assert!(execute(&dummy_client(), "not_analytics", &json!({})).await.is_none());
    }

    #[tokio::test]
    async fn test_definitions_unique() {
        let defs = definitions();
        let mut seen = std::collections::HashSet::new();
        for d in &defs {
            assert!(seen.insert(d.name.as_str()), "duplicate: {}", d.name);
        }
    }

    #[tokio::test]
    async fn test_known_tools_dispatch() {
        for tool in &["dakera_analytics_overview", "dakera_analytics_latency", "dakera_analytics_throughput", "dakera_analytics_storage", "dakera_kpis"] {
            let r = execute(&dummy_client(), tool, &json!({})).await;
            assert!(r.is_some(), "{} should dispatch", tool);
        }
    }
}
