//! MCP-8: Discovery meta-tools — dakera_discover_tools, dakera_load_tools
//!
//! These tools are always exposed (ToolTier::Meta) and let callers explore
//! the full 169-tool catalog without loading all schemas upfront. Callers
//! can first discover tools by keyword or tier, then load only the schemas
//! they actually need, saving ~35K tokens versus loading everything by default.

use serde_json::json;

use super::{ok_json, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_discover_tools".into(),
            description: "Search the Dakera tool catalog by keyword or tier (core/power/admin/meta) and return names and one-line summaries without loading full schemas. Call this first to find relevant tools, then use dakera_load_tools to fetch only the schemas you need — avoids loading the full catalog upfront."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keyword to search tool names/descriptions."
                    },
                    "tier": {
                        "type": "string",
                        "enum": ["core", "power", "admin", "meta", "all"]
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_load_tools".into(),
            description: "Fetch the full inputSchema for one or more named tools. Use after dakera_discover_tools. Returns schemas for found tools and a not_found list for unrecognized names."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tool names to load schemas for",
                        "minItems": 1
                    }
                },
                "required": ["tools"]
            }),
        },
    ]
}

pub async fn execute(
    _client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_discover_tools" => Some(discover_tools(args)),
        "dakera_load_tools" => Some(load_tools(args)),
        _ => None,
    }
}

fn discover_tools(args: &serde_json::Value) -> CallToolResult {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());
    let tier_filter = args.get("tier").and_then(|v| v.as_str()).unwrap_or("all");

    let catalog = super::full_catalog();
    let tools: Vec<serde_json::Value> = catalog
        .into_iter()
        .filter(|entry| {
            let tier_ok = match tier_filter {
                "core" => entry.tier == crate::protocol::ToolTier::Core,
                "power" => entry.tier == crate::protocol::ToolTier::Power,
                "admin" => entry.tier == crate::protocol::ToolTier::Admin,
                "meta" => entry.tier == crate::protocol::ToolTier::Meta,
                _ => true,
            };
            if !tier_ok {
                return false;
            }
            if let Some(ref q) = query {
                let name_lc = entry.def.name.to_lowercase();
                let desc_lc = entry.def.description.to_lowercase();
                name_lc.contains(q.as_str()) || desc_lc.contains(q.as_str())
            } else {
                true
            }
        })
        .map(|entry| {
            json!({
                "name": entry.def.name,
                "description": entry.def.description,
                "tier": entry.tier.as_str(),
            })
        })
        .collect();

    let count = tools.len();
    ok_json(&json!({
        "tools": tools,
        "count": count,
        "hint": "Use dakera_load_tools with tool names to get full inputSchema definitions",
    }))
}

fn load_tools(args: &serde_json::Value) -> CallToolResult {
    let tool_names = match args.get("tools").and_then(|v| v.as_array()) {
        Some(arr) => arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>(),
        None => {
            return CallToolResult::error("Missing required parameter: tools".to_string());
        }
    };

    if tool_names.is_empty() {
        return CallToolResult::error(
            "Parameter 'tools' must contain at least one tool name".to_string(),
        );
    }

    let catalog = super::full_catalog();
    let mut found = Vec::new();
    let mut not_found = Vec::new();

    for name in &tool_names {
        match catalog.iter().find(|e| e.def.name == *name) {
            Some(entry) => {
                found.push(json!({
                    "name": entry.def.name,
                    "description": entry.def.description,
                    "inputSchema": entry.def.input_schema,
                    "tier": entry.tier.as_str(),
                }));
            }
            None => not_found.push(*name),
        }
    }

    ok_json(&json!({
        "tools": found,
        "not_found": not_found,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_definitions_returns_two_tools() {
        let defs = definitions();
        assert_eq!(defs.len(), 2);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dakera_discover_tools"));
        assert!(names.contains(&"dakera_load_tools"));
    }

    #[test]
    fn test_definitions_have_descriptions() {
        for def in definitions() {
            assert!(
                !def.description.is_empty(),
                "{} has empty description",
                def.name
            );
        }
    }

    #[test]
    fn test_discover_tools_no_filter_returns_all() {
        let result = discover_tools(&json!({}));
        assert!(result.is_error.is_none());
        let text = &result.content[0].text;
        let val: serde_json::Value = serde_json::from_str(text).unwrap();
        let count = val["count"].as_u64().unwrap();
        assert!(
            count > 12,
            "Expected more than 12 tools in full catalog, got {count}"
        );
    }

    #[test]
    fn test_discover_tools_tier_core_returns_12() {
        let result = discover_tools(&json!({"tier": "core"}));
        assert!(result.is_error.is_none());
        let val: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(
            val["count"].as_u64().unwrap(),
            12,
            "Core tier should have exactly 12 tools"
        );
    }

    #[test]
    fn test_discover_tools_tier_meta_returns_2() {
        let result = discover_tools(&json!({"tier": "meta"}));
        assert!(result.is_error.is_none());
        let val: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(
            val["count"].as_u64().unwrap(),
            2,
            "Meta tier should have exactly 2 tools"
        );
    }

    #[test]
    fn test_discover_tools_query_filters_by_name() {
        let result = discover_tools(&json!({"query": "session"}));
        let val: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        let tools = val["tools"].as_array().unwrap();
        assert!(tools.iter().all(|t| {
            let name = t["name"].as_str().unwrap().to_lowercase();
            let desc = t["description"].as_str().unwrap().to_lowercase();
            name.contains("session") || desc.contains("session")
        }));
        assert!(!tools.is_empty(), "Expected at least one session tool");
    }

    #[test]
    fn test_discover_tools_response_has_hint() {
        let result = discover_tools(&json!({}));
        let val: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(
            val["hint"].as_str().is_some(),
            "Response should include a hint field"
        );
    }

    #[test]
    fn test_load_tools_returns_schema_for_known_tool() {
        let result = load_tools(&json!({"tools": ["dakera_store"]}));
        assert!(result.is_error.is_none());
        let val: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        let tools = val["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"].as_str().unwrap(), "dakera_store");
        assert!(
            tools[0]["inputSchema"].is_object(),
            "Should include inputSchema"
        );
        let not_found = val["not_found"].as_array().unwrap();
        assert!(not_found.is_empty());
    }

    #[test]
    fn test_load_tools_unknown_tool_in_not_found() {
        let result = load_tools(&json!({"tools": ["dakera_nonexistent_xyz"]}));
        assert!(result.is_error.is_none());
        let val: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        let not_found = val["not_found"].as_array().unwrap();
        assert!(not_found
            .iter()
            .any(|v| v.as_str() == Some("dakera_nonexistent_xyz")));
    }

    #[test]
    fn test_load_tools_missing_param_returns_error() {
        let result = load_tools(&json!({}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn test_load_tools_empty_array_returns_error() {
        let result = load_tools(&json!({"tools": []}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn test_load_tools_mixed_known_and_unknown() {
        let result = load_tools(&json!({"tools": ["dakera_recall", "dakera_does_not_exist"]}));
        let val: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(val["tools"].as_array().unwrap().len(), 1);
        assert_eq!(val["not_found"].as_array().unwrap().len(), 1);
    }
}
