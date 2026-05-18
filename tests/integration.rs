//! Integration tests for dakera-mcp against a live dakera docker instance.
//!
//! # Running
//!
//! ```bash
//! # Start dakera docker first:
//! docker run -d --name dakera-test -p 3300:3300 \
//!   -e DAKERA_ROOT_API_KEY=test-key ghcr.io/dakera-ai/dakera:latest
//!
//! # Run integration tests:
//! DAKERA_URL=http://localhost:3300 \
//! DAKERA_API_KEY=test-key \
//! cargo test --features integration -- --test-threads=1
//! ```
//!
//! The entire file is gated on `--features integration` so `cargo test`
//! (without the feature) never requires a live server.

#![cfg(feature = "integration")]

use dakera_mcp::protocol::JsonRpcRequest;
use dakera_mcp::server::handle_request;
use dakera_mcp::tools::{execute_tool, filtered_definitions, DakeraApiClient};
use serde_json::{json, Value};

/// Tag applied to every memory written by these tests — enables clean teardown.
const TEST_TAG: &str = "integration-test";

fn client() -> DakeraApiClient {
    let url = std::env::var("DAKERA_URL").unwrap_or_else(|_| "http://localhost:3300".to_string());
    let key = std::env::var("DAKERA_API_KEY").ok();
    DakeraApiClient::new(url, key)
}

fn agent(name: &str) -> String {
    format!("inttest-{name}")
}

/// Assert success and parse the JSON text from a tool result.
fn ok(r: &dakera_mcp::protocol::CallToolResult) -> Value {
    let text = r
        .content
        .first()
        .map(|c| c.text.as_str())
        .unwrap_or("<no content>");
    assert!(
        !r.is_error.unwrap_or(false),
        "Expected tool success, got error: {text}"
    );
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("Invalid JSON in tool result: {e}\nText: {text}"))
}

/// Delete all integration-test memories for the given agent.
async fn cleanup(c: &DakeraApiClient, agent_id: &str) {
    let _ = execute_tool(
        c,
        "dakera_batch_forget",
        &json!({
            "agent_id": agent_id,
            "tags": [TEST_TAG],
        }),
    )
    .await;
}

// ── Profile tests (pure local — no live server required) ──────────────────────

#[test]
fn test_profile_core_returns_14_tools() {
    let defs = filtered_definitions("core");
    assert_eq!(
        defs.len(),
        14,
        "core profile must expose exactly 12 core + 2 meta = 14 tools"
    );
}

#[test]
fn test_profile_power_is_superset_of_core() {
    let core = filtered_definitions("core");
    let power = filtered_definitions("power");
    assert!(
        power.len() > core.len(),
        "power ({}) must have more tools than core ({})",
        power.len(),
        core.len()
    );
}

#[test]
fn test_profile_all_is_superset_of_power() {
    let power = filtered_definitions("power");
    let all = filtered_definitions("all");
    assert!(
        all.len() > power.len(),
        "all ({}) must have more tools than power ({})",
        all.len(),
        power.len()
    );
}

#[test]
fn test_profile_unknown_falls_back_to_core() {
    let bogus = filtered_definitions("unknown_profile_xyz");
    let core = filtered_definitions("core");
    assert_eq!(
        bogus.len(),
        core.len(),
        "unknown profile must fall back to core behaviour"
    );
}

#[test]
fn test_core_profile_contains_all_12_core_tools() {
    let defs = filtered_definitions("core");
    let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
    for tool in &[
        "dakera_store",
        "dakera_recall",
        "dakera_search",
        "dakera_session_start",
        "dakera_session_end",
        "dakera_batch_recall",
        "dakera_forget",
        "dakera_hybrid_search",
        "dakera_fulltext_search",
        "dakera_knowledge_graph",
        "dakera_extract",
        "dakera_batch_forget",
    ] {
        assert!(names.contains(tool), "core profile missing: {tool}");
    }
}

#[test]
fn test_core_profile_contains_meta_tools() {
    let defs = filtered_definitions("core");
    let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(
        names.contains("dakera_discover_tools"),
        "core profile missing dakera_discover_tools"
    );
    assert!(
        names.contains("dakera_load_tools"),
        "core profile missing dakera_load_tools"
    );
}

// ── Meta-tool tests (pure local — no live server required) ────────────────────

#[tokio::test]
async fn test_discover_tools_returns_full_catalog() {
    let c = client();
    let r = execute_tool(&c, "dakera_discover_tools", &json!({})).await;
    let v = ok(&r);
    let count = v["count"].as_u64().expect("response must include count");
    assert!(count > 12, "full catalog must have >12 tools, got {count}");
    assert!(v["tools"].is_array(), "response must include tools array");
    assert!(v["hint"].is_string(), "response must include usage hint");
}

#[tokio::test]
async fn test_discover_tools_tier_core_returns_12() {
    let c = client();
    let r = execute_tool(&c, "dakera_discover_tools", &json!({"tier": "core"})).await;
    let v = ok(&r);
    assert_eq!(
        v["count"].as_u64().unwrap(),
        12,
        "tier=core must return exactly 12 tools"
    );
}

#[tokio::test]
async fn test_discover_tools_tier_meta_returns_2() {
    let c = client();
    let r = execute_tool(&c, "dakera_discover_tools", &json!({"tier": "meta"})).await;
    let v = ok(&r);
    assert_eq!(
        v["count"].as_u64().unwrap(),
        2,
        "tier=meta must return exactly 2 tools (discover + load)"
    );
    let tools = v["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"dakera_discover_tools"));
    assert!(names.contains(&"dakera_load_tools"));
}

#[tokio::test]
async fn test_discover_tools_keyword_filter() {
    let c = client();
    let r = execute_tool(&c, "dakera_discover_tools", &json!({"query": "session"})).await;
    let v = ok(&r);
    let tools = v["tools"].as_array().unwrap();
    assert!(
        !tools.is_empty(),
        "keyword filter 'session' must match at least one tool"
    );
    for t in tools {
        let name = t["name"].as_str().unwrap_or("").to_lowercase();
        let desc = t["description"].as_str().unwrap_or("").to_lowercase();
        assert!(
            name.contains("session") || desc.contains("session"),
            "tool '{name}' does not match keyword 'session'"
        );
    }
}

#[tokio::test]
async fn test_discover_tools_result_has_tier_field() {
    let c = client();
    let r = execute_tool(&c, "dakera_discover_tools", &json!({})).await;
    let v = ok(&r);
    let tools = v["tools"].as_array().unwrap();
    for t in tools {
        assert!(
            t["tier"].is_string(),
            "every tool must expose a tier field, missing in: {}",
            t["name"]
        );
    }
}

#[tokio::test]
async fn test_load_tools_returns_full_schema() {
    let c = client();
    let r = execute_tool(&c, "dakera_load_tools", &json!({"tools": ["dakera_store"]})).await;
    let v = ok(&r);
    let tools = v["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"].as_str().unwrap(), "dakera_store");
    assert!(
        tools[0]["inputSchema"].is_object(),
        "load_tools must return inputSchema"
    );
    assert!(
        v["not_found"].as_array().unwrap().is_empty(),
        "not_found must be empty for a known tool"
    );
}

#[tokio::test]
async fn test_load_tools_unknown_tool_in_not_found() {
    let c = client();
    let r = execute_tool(
        &c,
        "dakera_load_tools",
        &json!({"tools": ["dakera_does_not_exist_xyz"]}),
    )
    .await;
    let v = ok(&r);
    let not_found = v["not_found"].as_array().unwrap();
    assert_eq!(not_found.len(), 1);
    assert_eq!(not_found[0].as_str().unwrap(), "dakera_does_not_exist_xyz");
    assert!(v["tools"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_load_tools_mixed_known_unknown() {
    let c = client();
    let r = execute_tool(
        &c,
        "dakera_load_tools",
        &json!({"tools": ["dakera_recall", "dakera_nonexistent_abc"]}),
    )
    .await;
    let v = ok(&r);
    assert_eq!(v["tools"].as_array().unwrap().len(), 1);
    assert_eq!(v["not_found"].as_array().unwrap().len(), 1);
}

// ── Live dakera tests — require DAKERA_URL + DAKERA_API_KEY ──────────────────

#[tokio::test]
async fn test_session_lifecycle() {
    let c = client();
    let agent_id = agent("session");

    // Start session
    let r1 = execute_tool(
        &c,
        "dakera_session_start",
        &json!({
            "agent_id": agent_id,
            "metadata": {"source": "integration-test"}
        }),
    )
    .await;
    let v1 = ok(&r1);
    let session_id = v1["session"]["id"]
        .as_str()
        .expect("session_start must return session.id");

    // End session
    let r2 = execute_tool(
        &c,
        "dakera_session_end",
        &json!({
            "session_id": session_id,
            "summary": "integration test session completed"
        }),
    )
    .await;
    ok(&r2);
}

#[tokio::test]
async fn test_store_and_recall() {
    let c = client();
    let agent_id = agent("store-recall");
    cleanup(&c, &agent_id).await;

    let content = "Integration test: the sky turns indigo just before dawn.";
    let r1 = execute_tool(
        &c,
        "dakera_store",
        &json!({
            "agent_id": agent_id,
            "content": content,
            "importance": 0.8,
            "tags": [TEST_TAG, "recall-test"],
        }),
    )
    .await;
    let stored = ok(&r1);
    assert!(
        stored["memory"]["id"].is_string(),
        "store must return memory.id; got: {stored}"
    );

    // Verify storage via deterministic batch_recall (tag filter, no embedding needed)
    let r_verify = execute_tool(
        &c,
        "dakera_batch_recall",
        &json!({
            "agent_id": agent_id,
            "tags": [TEST_TAG, "recall-test"],
        }),
    )
    .await;
    let verified = ok(&r_verify);
    let stored_memories = verified["memories"]
        .as_array()
        .expect("batch_recall must return memories array");
    assert!(
        !stored_memories.is_empty(),
        "batch_recall must confirm the memory was stored"
    );

    // Test semantic recall endpoint returns valid response
    let r2 = execute_tool(
        &c,
        "dakera_recall",
        &json!({
            "agent_id": agent_id,
            "query": "sky color before dawn",
            "top_k": 5,
        }),
    )
    .await;
    let recalled = ok(&r2);
    assert!(
        recalled["memories"].is_array(),
        "recall must return a memories array"
    );

    cleanup(&c, &agent_id).await;
}

#[tokio::test]
async fn test_search() {
    let c = client();
    let agent_id = agent("search");
    cleanup(&c, &agent_id).await;

    execute_tool(
        &c,
        "dakera_store",
        &json!({
            "agent_id": agent_id,
            "content": "Search test: the capital of France is Paris, a city on the Seine.",
            "importance": 0.7,
            "tags": [TEST_TAG],
        }),
    )
    .await;

    let r = execute_tool(
        &c,
        "dakera_search",
        &json!({
            "agent_id": agent_id,
            "query": "Paris capital France",
            "top_k": 5,
        }),
    )
    .await;
    let v = ok(&r);
    assert!(
        v["memories"].is_array(),
        "search must return memories array"
    );

    cleanup(&c, &agent_id).await;
}

#[tokio::test]
async fn test_batch_recall_by_tag() {
    let c = client();
    let agent_id = agent("batch-recall");
    cleanup(&c, &agent_id).await;

    for i in 0..2 {
        execute_tool(
            &c,
            "dakera_store",
            &json!({
                "agent_id": agent_id,
                "content": format!("Batch recall test memory {i}"),
                "importance": 0.6,
                "tags": [TEST_TAG, "batch-recall-tag"],
            }),
        )
        .await;
    }

    let r = execute_tool(
        &c,
        "dakera_batch_recall",
        &json!({
            "agent_id": agent_id,
            "tags": [TEST_TAG, "batch-recall-tag"],
            "min_importance": 0.5,
        }),
    )
    .await;
    let v = ok(&r);
    let memories = v["memories"]
        .as_array()
        .expect("batch_recall must return memories array");
    assert!(
        memories.len() >= 2,
        "batch_recall must return at least 2 tagged memories, got {}",
        memories.len()
    );

    cleanup(&c, &agent_id).await;
}

#[tokio::test]
async fn test_forget_by_id() {
    let c = client();
    let agent_id = agent("forget");
    cleanup(&c, &agent_id).await;

    let r1 = execute_tool(
        &c,
        "dakera_store",
        &json!({
            "agent_id": agent_id,
            "content": "This memory will be individually forgotten.",
            "importance": 0.5,
            "tags": [TEST_TAG],
        }),
    )
    .await;
    let stored = ok(&r1);
    let memory_id = stored["memory"]["id"]
        .as_str()
        .expect("store must return memory.id");

    let r2 = execute_tool(
        &c,
        "dakera_forget",
        &json!({
            "agent_id": agent_id,
            "memory_ids": [memory_id],
        }),
    )
    .await;
    ok(&r2);
}

#[tokio::test]
async fn test_batch_forget() {
    let c = client();
    let agent_id = agent("batch-forget");
    cleanup(&c, &agent_id).await;

    for i in 0..3 {
        execute_tool(
            &c,
            "dakera_store",
            &json!({
                "agent_id": agent_id,
                "content": format!("Batch forget memory {i}"),
                "importance": 0.6,
                "tags": [TEST_TAG, "to-delete"],
            }),
        )
        .await;
    }

    let r = execute_tool(
        &c,
        "dakera_batch_forget",
        &json!({
            "agent_id": agent_id,
            "tags": [TEST_TAG, "to-delete"],
        }),
    )
    .await;
    ok(&r);

    // Verify all tagged memories are gone
    let r2 = execute_tool(
        &c,
        "dakera_batch_recall",
        &json!({
            "agent_id": agent_id,
            "tags": [TEST_TAG, "to-delete"],
        }),
    )
    .await;
    let v2 = ok(&r2);
    let remaining = v2["memories"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(
        remaining, 0,
        "all tagged memories must be deleted after batch_forget"
    );
}

#[tokio::test]
async fn test_hybrid_search() {
    let c = client();
    // hybrid_search operates on raw vector namespaces, not agent memory
    let ns = "inttest-hybrid-ns";

    // Setup: upsert a text document so the namespace has data to search
    let setup = execute_tool(
        &c,
        "dakera_upsert_text",
        &json!({
            "namespace": ns,
            "documents": [{
                "id": "qc-test-doc",
                "text": "quantum computing uses qubits for superposition and entanglement"
            }]
        }),
    )
    .await;
    // Setup may fail if namespace creation is restricted — skip gracefully
    if setup.is_error.unwrap_or(false) {
        return;
    }

    let r = execute_tool(
        &c,
        "dakera_hybrid_search",
        &json!({
            "namespace": ns,
            "text": "quantum computing qubits",
            "top_k": 5,
        }),
    )
    .await;
    let v = ok(&r);
    // Response is a JSON object (results may be empty but must be valid)
    assert!(v.is_object(), "hybrid_search must return a JSON object");
}

#[tokio::test]
async fn test_fulltext_search() {
    let c = client();
    // fulltext_search operates on raw vector namespaces (BM25 index), not agent memory
    let ns = "inttest-fulltext-ns";

    // Setup: index a document for BM25 search
    let setup = execute_tool(
        &c,
        "dakera_fulltext_index",
        &json!({
            "namespace": ns,
            "documents": [{
                "id": "platypus-doc",
                "text": "the platypus is a semi-aquatic egg-laying mammal native to Australia"
            }]
        }),
    )
    .await;
    // Setup may fail if namespace creation is restricted — skip gracefully
    if setup.is_error.unwrap_or(false) {
        return;
    }

    // Small delay for BM25 index to be ready
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let r = execute_tool(
        &c,
        "dakera_fulltext_search",
        &json!({
            "namespace": ns,
            "query": "platypus mammal Australia",
            "top_k": 5,
        }),
    )
    .await;
    let v = ok(&r);
    assert!(v.is_object(), "fulltext_search must return a JSON object");
}

#[tokio::test]
async fn test_knowledge_graph() {
    let c = client();
    let agent_id = agent("kg");
    cleanup(&c, &agent_id).await;

    // Store a seed memory and get its ID
    let r1 = execute_tool(
        &c,
        "dakera_store",
        &json!({
            "agent_id": agent_id,
            "content": "Knowledge graph test: Alice collaborates with Bob on the Rust engine.",
            "importance": 0.8,
            "tags": [TEST_TAG],
        }),
    )
    .await;
    let stored = ok(&r1);
    let memory_id = stored["memory"]["id"]
        .as_str()
        .expect("store must return memory.id");

    let r2 = execute_tool(
        &c,
        "dakera_knowledge_graph",
        &json!({
            "agent_id": agent_id,
            "memory_id": memory_id,
        }),
    )
    .await;
    let v = ok(&r2);
    assert!(v.is_object(), "knowledge_graph must return an object");

    cleanup(&c, &agent_id).await;
}

#[tokio::test]
async fn test_extract() {
    let c = client();
    // dakera_extract takes free-form text, not agent_id
    let r = execute_tool(
        &c,
        "dakera_extract",
        &json!({
            "text": "Alice Smith is a senior engineer at Acme Corp in San Francisco. \
                     She works on distributed systems with her colleague Bob Jones.",
        }),
    )
    .await;
    let v = ok(&r);
    assert!(v.is_object(), "extract must return an object");
}

// ── Admin profile tests (gap #1 — profile tiering completeness) ──────────────

#[test]
fn test_profile_admin_includes_namespace_tools() {
    let defs = filtered_definitions("admin");
    let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(
        names.contains("dakera_namespace_list"),
        "admin profile must include dakera_namespace_list (Admin tier)"
    );
    assert!(
        names.contains("dakera_namespace_create"),
        "admin profile must include dakera_namespace_create (Admin tier)"
    );
    assert!(
        names.contains("dakera_audit_query"),
        "admin profile must include dakera_audit_query (Admin tier)"
    );
}

#[test]
fn test_profile_admin_excludes_power_tools() {
    let defs = filtered_definitions("admin");
    let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(
        !names.contains("dakera_agent_stats"),
        "admin profile must NOT include power tool dakera_agent_stats"
    );
    assert!(
        !names.contains("dakera_consolidate"),
        "admin profile must NOT include power tool dakera_consolidate"
    );
}

#[test]
fn test_profile_admin_larger_than_core() {
    let core = filtered_definitions("core");
    let admin = filtered_definitions("admin");
    assert!(
        admin.len() > core.len(),
        "admin ({}) must have more tools than core ({})",
        admin.len(),
        core.len()
    );
}

#[test]
fn test_profile_all_larger_than_admin() {
    let admin = filtered_definitions("admin");
    let all = filtered_definitions("all");
    assert!(
        all.len() > admin.len(),
        "all ({}) must have more tools than admin ({})",
        all.len(),
        admin.len()
    );
}

// ── Total tool count regression test (gap #8) ────────────────────────────────

#[test]
fn test_total_tool_count_regression() {
    let all = filtered_definitions("all");
    // 86 tools after PR#84 prune. Update this constant after intentional catalog changes.
    assert_eq!(
        all.len(),
        86,
        "Total tool count must be exactly 86. Actual: {}. \
         If you intentionally added/removed tools, update this constant.",
        all.len()
    );
}

// ── Tier assignment spot-check (gap #9) ──────────────────────────────────────

#[test]
fn test_tier_assignment_integration_spot_check() {
    use dakera_mcp::tools::full_catalog;
    let catalog = full_catalog();
    let tier_map: std::collections::HashMap<_, _> = catalog
        .iter()
        .map(|e| (e.def.name.clone(), e.tier))
        .collect();

    // Power tier tools must NOT appear in core profile
    let core_names: std::collections::HashSet<_> = filtered_definitions("core")
        .into_iter()
        .map(|d| d.name)
        .collect();
    for power_tool in &[
        "dakera_agent_stats",
        "dakera_consolidate",
        "dakera_autopilot_status",
    ] {
        assert!(
            !core_names.contains(*power_tool),
            "Power tool '{power_tool}' must NOT be in core profile"
        );
        assert_eq!(
            tier_map.get(*power_tool).map(|t| t.as_str()),
            Some("power"),
            "'{power_tool}' must be classified as Power tier"
        );
    }

    // Admin tier tools must appear in admin profile but NOT in power profile
    let admin_names: std::collections::HashSet<_> = filtered_definitions("admin")
        .into_iter()
        .map(|d| d.name)
        .collect();
    let power_names: std::collections::HashSet<_> = filtered_definitions("power")
        .into_iter()
        .map(|d| d.name)
        .collect();
    for admin_tool in &[
        "dakera_namespace_create",
        "dakera_namespace_delete",
        "dakera_audit_query",
    ] {
        assert!(
            admin_names.contains(*admin_tool),
            "Admin tool '{admin_tool}' must appear in admin profile"
        );
        assert!(
            !power_names.contains(*admin_tool),
            "Admin tool '{admin_tool}' must NOT appear in power profile"
        );
    }
}

// ── Protocol-level JSON-RPC tests (gap #3 — validates server.rs lines 106-114) ──

/// Build a fake DakeraApiClient pointing nowhere — tools/list never makes HTTP calls.
fn no_http_client() -> DakeraApiClient {
    DakeraApiClient::new("http://localhost:0".to_string(), None)
}

fn rpc_request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
    serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    }))
    .expect("test JSON-RPC request must deserialize cleanly")
}

#[tokio::test]
async fn test_protocol_tools_list_core_returns_14() {
    let c = no_http_client();
    let req = rpc_request("tools/list", serde_json::json!({"profile": "core"}));
    let resp = handle_request(&c, &req).await;
    let result = resp.result.expect("tools/list must return a result");
    let tools = result["tools"]
        .as_array()
        .expect("result must contain tools array");
    assert_eq!(
        tools.len(),
        14,
        "tools/list profile=core must return 14 tools (12 core + 2 meta)"
    );
}

#[tokio::test]
async fn test_protocol_tools_list_power_larger_than_core() {
    let c = no_http_client();
    let core_req = rpc_request("tools/list", serde_json::json!({"profile": "core"}));
    let power_req = rpc_request("tools/list", serde_json::json!({"profile": "power"}));
    let core_resp = handle_request(&c, &core_req).await;
    let power_resp = handle_request(&c, &power_req).await;
    let core_count = core_resp.result.unwrap()["tools"].as_array().unwrap().len();
    let power_count = power_resp.result.unwrap()["tools"]
        .as_array()
        .unwrap()
        .len();
    assert!(
        power_count > core_count,
        "tools/list profile=power ({power_count}) must have more tools than core ({core_count})"
    );
}

#[tokio::test]
async fn test_protocol_tools_list_admin_profile() {
    let c = no_http_client();
    let req = rpc_request("tools/list", serde_json::json!({"profile": "admin"}));
    let resp = handle_request(&c, &req).await;
    let result = resp.result.expect("tools/list must return a result");
    let tools = result["tools"]
        .as_array()
        .expect("result must contain tools array");
    let names: std::collections::HashSet<_> =
        tools.iter().filter_map(|t| t["name"].as_str()).collect();
    // Admin profile must include admin-tier namespace tools
    assert!(
        names.contains("dakera_namespace_list"),
        "tools/list profile=admin must include dakera_namespace_list"
    );
    // Admin profile must include core tools
    assert!(
        names.contains("dakera_store"),
        "tools/list profile=admin must include dakera_store"
    );
    // Admin profile must NOT include power tools
    assert!(
        !names.contains("dakera_agent_stats"),
        "tools/list profile=admin must NOT include power tool dakera_agent_stats"
    );
    assert!(
        tools.len() > 14,
        "tools/list profile=admin must have more than 14 tools (admin > core), got {}",
        tools.len()
    );
}

#[tokio::test]
async fn test_protocol_tools_list_all_returns_86() {
    let c = no_http_client();
    let mut cursor: Option<String> = None;
    let mut total = 0usize;
    loop {
        let params = match &cursor {
            None => serde_json::json!({"profile": "all"}),
            Some(cur) => serde_json::json!({"profile": "all", "cursor": cur}),
        };
        let req = rpc_request("tools/list", params);
        let resp = handle_request(&c, &req).await;
        let result = resp.result.expect("tools/list must return a result");
        total += result["tools"].as_array().expect("tools array").len();
        cursor = result
            .get("nextCursor")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        total, 86,
        "tools/list profile=all must return exactly 86 tools across all pages"
    );
}

#[tokio::test]
async fn test_protocol_tools_list_no_params_defaults_to_core() {
    let c = no_http_client();
    // Ensure env var is not set so we exercise the true default
    unsafe {
        std::env::remove_var("DAKERA_MCP_PROFILE");
    }
    let req = rpc_request("tools/list", serde_json::json!({}));
    let resp = handle_request(&c, &req).await;
    let result = resp.result.expect("tools/list must return a result");
    let tools = result["tools"]
        .as_array()
        .expect("result must contain tools array");
    assert_eq!(
        tools.len(),
        14,
        "tools/list with no profile param and no env var must default to core (14 tools)"
    );
}

// ── DAKERA_MCP_PROFILE env var fallback (gap #4) ──────────────────────────────
// NOTE: must run with --test-threads=1 (already enforced in CI) since env mutation is global.

#[tokio::test]
async fn test_protocol_env_var_profile_power() {
    let c = no_http_client();
    unsafe {
        std::env::set_var("DAKERA_MCP_PROFILE", "power");
    }
    let req = rpc_request("tools/list", serde_json::json!({}));
    let resp = handle_request(&c, &req).await;
    unsafe {
        std::env::remove_var("DAKERA_MCP_PROFILE");
    }
    let result = resp.result.expect("tools/list must return a result");
    let count = result["tools"].as_array().unwrap().len();
    let core_count = filtered_definitions("core").len();
    assert!(
        count > core_count,
        "DAKERA_MCP_PROFILE=power must expose more tools than core ({count} vs {core_count})"
    );
}

#[tokio::test]
async fn test_protocol_env_var_profile_admin() {
    let c = no_http_client();
    unsafe {
        std::env::set_var("DAKERA_MCP_PROFILE", "admin");
    }
    let req = rpc_request("tools/list", serde_json::json!({}));
    let resp = handle_request(&c, &req).await;
    unsafe {
        std::env::remove_var("DAKERA_MCP_PROFILE");
    }
    let result = resp.result.expect("tools/list must return a result");
    let tools = result["tools"].as_array().unwrap();
    let names: std::collections::HashSet<_> =
        tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(
        names.contains("dakera_namespace_list"),
        "DAKERA_MCP_PROFILE=admin must expose admin tools like dakera_namespace_list"
    );
    assert!(
        !names.contains("dakera_agent_stats"),
        "DAKERA_MCP_PROFILE=admin must NOT expose power tools like dakera_agent_stats"
    );
}

#[tokio::test]
async fn test_protocol_request_profile_overrides_env_var() {
    let c = no_http_client();
    // Env var says "all" but request param says "core" — request wins
    unsafe {
        std::env::set_var("DAKERA_MCP_PROFILE", "all");
    }
    let req = rpc_request("tools/list", serde_json::json!({"profile": "core"}));
    let resp = handle_request(&c, &req).await;
    unsafe {
        std::env::remove_var("DAKERA_MCP_PROFILE");
    }
    let result = resp.result.expect("tools/list must return a result");
    let count = result["tools"].as_array().unwrap().len();
    assert_eq!(
        count, 14,
        "Request profile param must override DAKERA_MCP_PROFILE env var: got {count}"
    );
}

// ── Pruned tool regression tests (gap #7 — deleted tools return errors) ──────

#[tokio::test]
async fn test_pruned_admin_health_full_returns_unknown() {
    let c = client();
    let r = execute_tool(&c, "dakera_admin_health_full", &serde_json::json!({})).await;
    assert_eq!(
        r.is_error,
        Some(true),
        "Pruned tool dakera_admin_health_full must return is_error=true"
    );
    assert!(
        r.content[0].text.contains("Unknown tool"),
        "Expected 'Unknown tool' error for dakera_admin_health_full, got: {}",
        r.content[0].text
    );
}

#[tokio::test]
async fn test_pruned_analytics_usage_returns_unknown() {
    let c = client();
    let r = execute_tool(&c, "dakera_analytics_usage", &serde_json::json!({})).await;
    assert_eq!(r.is_error, Some(true));
    assert!(
        r.content[0].text.contains("Unknown tool"),
        "Expected 'Unknown tool' error for dakera_analytics_usage, got: {}",
        r.content[0].text
    );
}

#[tokio::test]
async fn test_pruned_stream_events_returns_unknown() {
    let c = client();
    let r = execute_tool(&c, "dakera_stream_events", &serde_json::json!({})).await;
    assert_eq!(r.is_error, Some(true));
    assert!(
        r.content[0].text.contains("Unknown tool"),
        "Expected 'Unknown tool' error for dakera_stream_events, got: {}",
        r.content[0].text
    );
}

// ── Power-tier live tool invocations (gap #5 — validates ~75% of tool catalog) ──

#[tokio::test]
async fn test_power_agent_stats_live() {
    let c = client();
    let r = execute_tool(
        &c,
        "dakera_agent_stats",
        &serde_json::json!({"agent_id": "inttest-power-stats"}),
    )
    .await;
    // May return 404 if agent doesn't exist — that's still a valid API response, not a tool error
    let text = &r.content[0].text;
    // If it's an error, it should be an API error (404/etc), not "Unknown tool"
    if r.is_error.unwrap_or(false) {
        assert!(
            !text.contains("Unknown tool"),
            "dakera_agent_stats must not return 'Unknown tool': {text}"
        );
    } else {
        // Success path: response should be a JSON object
        assert!(
            !text.is_empty(),
            "dakera_agent_stats must return non-empty response"
        );
    }
}

#[tokio::test]
async fn test_power_autopilot_status_live() {
    let c = client();
    let r = execute_tool(
        &c,
        "dakera_autopilot_status",
        &serde_json::json!({"agent_id": "inttest-power-autopilot"}),
    )
    .await;
    let text = &r.content[0].text;
    if r.is_error.unwrap_or(false) {
        assert!(
            !text.contains("Unknown tool"),
            "dakera_autopilot_status must not return 'Unknown tool': {text}"
        );
    } else {
        assert!(
            !text.is_empty(),
            "dakera_autopilot_status must return non-empty response"
        );
    }
}

#[tokio::test]
async fn test_power_session_list_live() {
    let c = client();
    let agent_id = agent("power-sessionlist");
    let r = execute_tool(
        &c,
        "dakera_session_list",
        &serde_json::json!({"agent_id": agent_id}),
    )
    .await;
    let text = &r.content[0].text;
    if r.is_error.unwrap_or(false) {
        assert!(
            !text.contains("Unknown tool"),
            "dakera_session_list must not be unknown: {text}"
        );
    } else {
        // sessions list for a fresh test agent should return an array (possibly empty)
        let v: serde_json::Value = serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("dakera_session_list response is not JSON: {e}\n{text}"));
        assert!(
            v.is_object(),
            "dakera_session_list must return a JSON object"
        );
    }
}

#[tokio::test]
async fn test_power_memory_importance_live() {
    let c = client();
    let agent_id = agent("power-importance");
    cleanup(&c, &agent_id).await;

    // Store a memory first to get a valid ID
    let r_store = execute_tool(
        &c,
        "dakera_store",
        &serde_json::json!({
            "agent_id": agent_id,
            "content": "Power tier test: importance boost candidate memory.",
            "importance": 0.5,
            "tags": [TEST_TAG],
        }),
    )
    .await;
    let stored = ok(&r_store);
    let memory_id = stored["memory"]["id"]
        .as_str()
        .expect("store must return memory.id");

    // Invoke the power tool dakera_memory_importance
    let r = execute_tool(
        &c,
        "dakera_memory_importance",
        &serde_json::json!({
            "agent_id": agent_id,
            "memory_id": memory_id,
            "importance": 0.9,
        }),
    )
    .await;
    let text = &r.content[0].text;
    assert!(
        !text.contains("Unknown tool"),
        "dakera_memory_importance must not be unknown: {text}"
    );

    cleanup(&c, &agent_id).await;
}

#[tokio::test]
async fn test_power_consolidate_live() {
    let c = client();
    let agent_id = agent("power-consolidate");
    cleanup(&c, &agent_id).await;

    // Consolidate on an agent with no memories — should return cleanly, not "Unknown tool"
    let r = execute_tool(
        &c,
        "dakera_consolidate",
        &serde_json::json!({"agent_id": agent_id}),
    )
    .await;
    let text = &r.content[0].text;
    assert!(
        !text.contains("Unknown tool"),
        "dakera_consolidate must not be unknown: {text}"
    );
}

#[tokio::test]
async fn test_power_graph_traverse_live() {
    let c = client();
    let agent_id = agent("power-graph");
    cleanup(&c, &agent_id).await;

    // Store a memory and traverse its graph
    let r_store = execute_tool(
        &c,
        "dakera_store",
        &serde_json::json!({
            "agent_id": agent_id,
            "content": "Graph traverse test: node A connects to node B in the knowledge graph.",
            "importance": 0.7,
            "tags": [TEST_TAG],
        }),
    )
    .await;
    let stored = ok(&r_store);
    let memory_id = stored["memory"]["id"]
        .as_str()
        .expect("store must return memory.id");

    let r = execute_tool(
        &c,
        "dakera_graph_traverse",
        &serde_json::json!({
            "agent_id": agent_id,
            "memory_id": memory_id,
            "depth": 1,
        }),
    )
    .await;
    let text = &r.content[0].text;
    assert!(
        !text.contains("Unknown tool"),
        "dakera_graph_traverse must not be unknown: {text}"
    );

    cleanup(&c, &agent_id).await;
}

// ── Meta-tool power-tier round-trip (gap #6 — discover power → load → invoke) ──

#[tokio::test]
async fn test_meta_roundtrip_power_tier() {
    let c = client();

    // Step 1: discover power-tier tools via meta-tool
    let r1 = execute_tool(
        &c,
        "dakera_discover_tools",
        &serde_json::json!({"tier": "power"}),
    )
    .await;
    let v1 = ok(&r1);
    let tools = v1["tools"].as_array().unwrap();
    assert!(!tools.is_empty(), "power tier must have discoverable tools");

    // Confirm dakera_agent_stats appears as a power tool
    let agent_stats_entry = tools
        .iter()
        .find(|t| t["name"].as_str() == Some("dakera_agent_stats"))
        .expect("dakera_agent_stats must appear in power tier catalog");
    assert_eq!(
        agent_stats_entry["tier"].as_str().unwrap(),
        "power",
        "dakera_agent_stats must be classified as power"
    );

    // Step 2: load full schema for the power-tier tool via meta-tool
    let r2 = execute_tool(
        &c,
        "dakera_load_tools",
        &serde_json::json!({"tools": ["dakera_agent_stats"]}),
    )
    .await;
    let v2 = ok(&r2);
    let schema = &v2["tools"][0];
    assert_eq!(schema["name"].as_str().unwrap(), "dakera_agent_stats");
    assert!(
        schema["inputSchema"].is_object(),
        "load_tools must return inputSchema for power tool"
    );
    assert!(
        schema["inputSchema"]["properties"]["agent_id"].is_object(),
        "dakera_agent_stats schema must define agent_id parameter"
    );
    assert_eq!(
        schema["tier"].as_str().unwrap(),
        "power",
        "loaded schema must include tier=power for dakera_agent_stats"
    );

    // Step 3: invoke the power-tier tool using the schema contract
    let r3 = execute_tool(
        &c,
        "dakera_agent_stats",
        &serde_json::json!({"agent_id": "inttest-meta-power-roundtrip"}),
    )
    .await;
    let text = &r3.content[0].text;
    assert!(
        !text.contains("Unknown tool"),
        "Power-tier round-trip: dakera_agent_stats invocation must not return 'Unknown tool': {text}"
    );
}

// ── Full round-trip: discover → load schema → invoke tool (core tier) ────────

#[tokio::test]
async fn test_meta_roundtrip_discover_load_invoke() {
    let c = client();

    // Step 1: discover tools in core tier
    let r1 = execute_tool(&c, "dakera_discover_tools", &json!({"tier": "core"})).await;
    let v1 = ok(&r1);
    let tools = v1["tools"].as_array().unwrap();
    assert!(!tools.is_empty(), "core tier must list tools");

    // Confirm dakera_store is in the catalog with tier=core
    let store_entry = tools
        .iter()
        .find(|t| t["name"].as_str() == Some("dakera_store"))
        .expect("dakera_store must appear in core tier catalog");
    assert_eq!(store_entry["tier"].as_str().unwrap(), "core");

    // Step 2: load full schema for dakera_store via the meta-tool
    let r2 = execute_tool(&c, "dakera_load_tools", &json!({"tools": ["dakera_store"]})).await;
    let v2 = ok(&r2);
    let schema = &v2["tools"][0];
    assert!(
        schema["inputSchema"]["properties"]["agent_id"].is_object(),
        "loaded schema must contain agent_id property definition"
    );
    assert!(
        schema["inputSchema"]["properties"]["content"].is_object(),
        "loaded schema must contain content property definition"
    );

    // Step 3: invoke dakera_store using the loaded schema's contract
    let agent_id = agent("meta-roundtrip");
    cleanup(&c, &agent_id).await;

    let r3 = execute_tool(
        &c,
        "dakera_store",
        &json!({
            "agent_id": agent_id,
            "content": "Meta round-trip verified: discovered in catalog, schema loaded, tool invoked.",
            "importance": 0.6,
            "tags": [TEST_TAG],
        }),
    )
    .await;
    let v3 = ok(&r3);
    assert!(
        v3["memory"]["id"].is_string(),
        "round-trip store must return a memory ID"
    );

    cleanup(&c, &agent_id).await;
}

// ── DAK-5216: Pagination tests (pure local — no live server required) ─────────

#[tokio::test]
async fn test_pagination_core_profile_single_page() {
    // Core profile has 14 tools which fit in one page (page_size=100).
    // No nextCursor should be returned.
    use dakera_mcp::server::handle_request;
    use dakera_mcp::tools::DakeraApiClient;
    let c = DakeraApiClient::new("http://127.0.0.1:9".to_string(), None);
    let req: dakera_mcp::protocol::JsonRpcRequest = serde_json::from_str(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"profile":"core"}}"#,
    )
    .unwrap();
    let resp = handle_request(&c, &req).await;
    let result = resp.result.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(
        tools.len(),
        14,
        "core profile must return all 14 tools in one page"
    );
    assert!(
        result.get("nextCursor").is_none(),
        "core profile (14 tools) fits in one page — no nextCursor expected"
    );
}

#[tokio::test]
async fn test_pagination_all_profile_first_page() {
    // all profile (86 tools) with page_size=100 fits in one page — no nextCursor.
    use dakera_mcp::server::handle_request;
    use dakera_mcp::tools::DakeraApiClient;
    let c = DakeraApiClient::new("http://127.0.0.1:9".to_string(), None);
    let req: dakera_mcp::protocol::JsonRpcRequest = serde_json::from_str(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"profile":"all"}}"#,
    )
    .unwrap();
    let resp = handle_request(&c, &req).await;
    let result = resp.result.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(
        tools.len(),
        86,
        "all profile must return all 86 tools in one page"
    );
    assert!(
        result.get("nextCursor").is_none(),
        "'all' profile (86 tools) fits in one page — no nextCursor expected"
    );
}

#[tokio::test]
async fn test_pagination_cursor_advances_page() {
    // Cursor past total returns empty page with no nextCursor.
    use dakera_mcp::server::handle_request;
    use dakera_mcp::tools::DakeraApiClient;
    let c = DakeraApiClient::new("http://127.0.0.1:9".to_string(), None);
    let req: dakera_mcp::protocol::JsonRpcRequest =
        serde_json::from_str(r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"profile":"all","cursor":"86"}}"#).unwrap();
    let resp = handle_request(&c, &req).await;
    let result = resp.result.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 0, "cursor past total must return empty page");
    assert!(
        result.get("nextCursor").is_none(),
        "cursor past total must have no nextCursor"
    );
}

#[tokio::test]
async fn test_pagination_all_tools_across_pages() {
    // Paginating through all pages of 'all' profile must cover all 86 tools exactly once.
    use dakera_mcp::server::handle_request;
    use dakera_mcp::tools::DakeraApiClient;
    let c = DakeraApiClient::new("http://127.0.0.1:9".to_string(), None);
    let mut cursor: Option<String> = None;
    let mut all_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut pages = 0usize;

    loop {
        let params = match &cursor {
            None => r#"{"profile":"all"}"#.to_string(),
            Some(cur) => format!(r#"{{"profile":"all","cursor":"{}"}}"#, cur),
        };
        let raw = format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"tools/list","params":{}}}"#,
            pages, params
        );
        let req: dakera_mcp::protocol::JsonRpcRequest = serde_json::from_str(&raw).unwrap();
        let resp = handle_request(&c, &req).await;
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        for t in tools {
            all_names.insert(t["name"].as_str().unwrap_or("").to_string());
        }
        pages += 1;
        cursor = result
            .get("nextCursor")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if cursor.is_none() {
            break;
        }
        assert!(
            pages <= 10,
            "Pagination took more than 10 pages — likely infinite loop"
        );
    }

    assert_eq!(
        all_names.len(),
        86,
        "Paginating through 'all' profile must yield all 86 tools, got: {}",
        all_names.len()
    );
}

// ── DAK-5216: Token count measurement (pure local — no live server required) ──

#[test]
fn test_token_count_before_after_measurement() {
    // Measure JSON byte size of each profile's tools/list response.
    // Token estimate: bytes / 3.5 (conservative avg chars/token for JSON).
    // Pre-optimization all-profile baseline: ~72000 bytes (~20571 tokens).
    let profiles = [("core", 14usize, 0usize), ("power", 0, 0), ("all", 86, 0)];
    for (profile, expected_count, _) in &profiles {
        let defs = dakera_mcp::tools::filtered_definitions(profile);
        if *expected_count > 0 {
            assert_eq!(
                defs.len(),
                *expected_count,
                "profile={} tool count mismatch",
                profile
            );
        }
        let json = serde_json::json!({"tools": defs});
        let bytes = serde_json::to_string(&json).unwrap().len();
        let est_tokens = bytes as f64 / 3.5;
        println!(
            "[DAK-5216] profile={} tools={} bytes={} est_tokens={:.0}",
            profile,
            defs.len(),
            bytes,
            est_tokens
        );
    }
    // Hard gate: core profile must stay under 9000 bytes (~2571 tokens).
    let core = dakera_mcp::tools::filtered_definitions("core");
    let core_bytes = serde_json::to_string(&serde_json::json!({"tools": core}))
        .unwrap()
        .len();
    assert!(
        core_bytes < 9000,
        "Core profile tools/list JSON is {} bytes (est {:.0} tokens) — exceeds budget",
        core_bytes,
        core_bytes as f64 / 3.5
    );
}
