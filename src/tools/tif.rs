//! T-I-F (Truth-Indeterminacy-Falsity) evaluation tool — Phase 3 of the T-I-F RFC.
//!
//! See <https://github.com/Dakera-AI/dakera-deploy/issues/161>.
//!
//! Wraps the feedback-derived reliability workflow (previously 5–7 sequential REST
//! calls) into a single MCP tool call: fetch a memory's feedback history, count the
//! upvote/downvote/flag signals, compute normalised T-I-F scores, and classify the
//! memory's reuse reliability. The tool performs **pure arithmetic** on feedback
//! signals — it never calls an external LLM.
//!
//! Tools:
//!   - `dakera_tif_evaluate` — GET /v1/memories/:id/feedback + T-I-F scoring

use serde_json::json;

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "dakera_tif_evaluate".into(),
        description:
            "Evaluate T-I-F (Truth-Indeterminacy-Falsity) reliability scores for a memory \
            based on its feedback history. Returns computed scores and a classification \
            (confident_reuse, surface_contradiction, ask_clarification, verify_before_use)."
                .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "memory_id": {
                    "type": "string",
                    "description": "ID of the memory to evaluate T-I-F reliability for"
                },
                "agent_id": { "type": "string" }
            },
            "required": ["memory_id"]
        }),
    }]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_tif_evaluate" => Some(tool_tif_evaluate(client, args).await),
        _ => None,
    }
}

/// Computed T-I-F reliability scores plus the reuse classification.
#[derive(Debug, Clone, PartialEq)]
struct TifScores {
    truth: f64,
    indeterminacy: f64,
    falsity: f64,
    classification: &'static str,
}

/// Round to 4 decimal places for clean, stable JSON output.
fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// Count `(upvotes, downvotes, flags)` from a feedback-history `entries` array.
///
/// Recognises the INT-1 canonical signal names plus the backward-compatible
/// `positive`/`negative` aliases (see `FeedbackSignal` in the engine). Unknown or
/// missing signals are ignored.
fn count_signals(entries: &serde_json::Value) -> (u64, u64, u64) {
    let mut upvotes = 0;
    let mut downvotes = 0;
    let mut flags = 0;
    if let Some(arr) = entries.as_array() {
        for entry in arr {
            match entry.get("signal").and_then(|s| s.as_str()) {
                Some("upvote") | Some("positive") => upvotes += 1,
                Some("downvote") | Some("negative") => downvotes += 1,
                Some("flag") => flags += 1,
                _ => {}
            }
        }
    }
    (upvotes, downvotes, flags)
}

/// Compute T-I-F scores from raw feedback signal counts.
///
/// - `total == 0` → fully indeterminate (`indeterminacy = 1.0`), classified
///   `ask_clarification`.
/// - Otherwise truth/falsity/indeterminacy start as the upvote/downvote/flag
///   fractions. A base indeterminacy term is added when `total < 3` (thin evidence
///   should not yield high confidence), then the triple is normalised so
///   `T + I + F == 1.0`.
///
/// Classification is evaluated in this fixed order:
///   1. `falsity       >= 0.50` → `surface_contradiction`
///   2. `indeterminacy >= 0.50` → `ask_clarification`
///   3. `truth         >= 0.70` → `confident_reuse`
///   4. otherwise               → `verify_before_use`
fn compute_tif(upvotes: u64, downvotes: u64, flags: u64) -> TifScores {
    let total = upvotes + downvotes + flags;
    if total == 0 {
        return TifScores {
            truth: 0.0,
            indeterminacy: 1.0,
            falsity: 0.0,
            classification: "ask_clarification",
        };
    }

    let total_f = total as f64;
    // Thin-evidence caution: inject base indeterminacy when fewer than 3 signals exist.
    let base_indeterminacy = if total < 3 {
        (3 - total) as f64 * 0.25
    } else {
        0.0
    };

    let mut truth = upvotes as f64 / total_f;
    let mut falsity = downvotes as f64 / total_f;
    let mut indeterminacy = flags as f64 / total_f + base_indeterminacy;

    // Normalise so T + I + F == 1.0.
    let sum = truth + falsity + indeterminacy;
    truth /= sum;
    falsity /= sum;
    indeterminacy /= sum;

    let classification = if falsity >= 0.50 {
        "surface_contradiction"
    } else if indeterminacy >= 0.50 {
        "ask_clarification"
    } else if truth >= 0.70 {
        "confident_reuse"
    } else {
        "verify_before_use"
    };

    TifScores {
        truth: round4(truth),
        indeterminacy: round4(indeterminacy),
        falsity: round4(falsity),
        classification,
    }
}

async fn tool_tif_evaluate(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let agent_id = args.get("agent_id").and_then(|v| v.as_str());

    // GET /v1/memories/:id/feedback — the engine requires agent_id as a query param
    // (returns InvalidRequest without it), so forward it when the caller supplies one.
    let mut path = format!("/v1/memories/{}/feedback", urlencoding::encode(&memory_id));
    if let Some(aid) = agent_id {
        path.push_str(&format!("?agent_id={}", urlencoding::encode(aid)));
    }

    let response = match client.get_json(&path).await {
        Ok(r) => r,
        Err(e) => return CallToolResult::error(e),
    };

    let entries = response
        .get("entries")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let (upvotes, downvotes, flags) = count_signals(&entries);
    let feedback_count = upvotes + downvotes + flags;
    let scores = compute_tif(upvotes, downvotes, flags);

    ok_json(&json!({
        "memory_id": memory_id,
        "truth": scores.truth,
        "indeterminacy": scores.indeterminacy,
        "falsity": scores.falsity,
        "classification": scores.classification,
        "feedback_count": feedback_count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://localhost:9999".to_string(), None)
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-4, "expected {b}, got {a}");
    }

    #[test]
    fn test_definitions_count() {
        assert_eq!(definitions().len(), 1);
    }

    #[test]
    fn test_definition_name() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        assert!(names.iter().any(|n| n == "dakera_tif_evaluate"));
    }

    #[test]
    fn test_agent_id_has_no_description() {
        // Repo convention enforced by mod.rs test_agent_id_description_absent_from_schemas:
        // agent_id is self-documenting and must not carry a description.
        let defs = definitions();
        let agent_prop = defs[0].input_schema["properties"]["agent_id"].clone();
        assert!(agent_prop.get("description").is_none());
    }

    // ── T-I-F arithmetic — acceptance criteria 2 & 3 ──────────────────────────

    #[test]
    fn test_no_feedback_is_fully_indeterminate() {
        let s = compute_tif(0, 0, 0);
        approx(s.truth, 0.0);
        approx(s.indeterminacy, 1.0);
        approx(s.falsity, 0.0);
        assert_eq!(s.classification, "ask_clarification");
    }

    #[test]
    fn test_all_upvotes_confident_reuse() {
        let s = compute_tif(5, 0, 0);
        approx(s.truth, 1.0);
        approx(s.falsity, 0.0);
        approx(s.indeterminacy, 0.0);
        assert_eq!(s.classification, "confident_reuse");
    }

    #[test]
    fn test_all_downvotes_surface_contradiction() {
        let s = compute_tif(0, 5, 0);
        approx(s.falsity, 1.0);
        assert_eq!(s.classification, "surface_contradiction");
    }

    #[test]
    fn test_all_flags_ask_clarification() {
        let s = compute_tif(0, 0, 5);
        approx(s.indeterminacy, 1.0);
        assert_eq!(s.classification, "ask_clarification");
    }

    #[test]
    fn test_mixed_signals_normalise_to_one() {
        // 10 signals → no base indeterminacy; raw fractions are already normalised.
        let s = compute_tif(8, 1, 1);
        approx(s.truth + s.indeterminacy + s.falsity, 1.0);
        approx(s.truth, 0.8);
        approx(s.falsity, 0.1);
        approx(s.indeterminacy, 0.1);
        assert_eq!(s.classification, "confident_reuse");
    }

    #[test]
    fn test_thin_evidence_adds_base_indeterminacy() {
        // A single upvote must not reach confident_reuse — base indeterminacy holds
        // truth below the 0.70 threshold.
        let s = compute_tif(1, 0, 0);
        approx(s.truth + s.indeterminacy + s.falsity, 1.0);
        assert!(
            s.indeterminacy > 0.0,
            "thin evidence must inject indeterminacy"
        );
        assert!(s.truth < 0.70);
        assert_eq!(s.classification, "verify_before_use");
    }

    #[test]
    fn test_count_signals_parses_canonical_and_aliases() {
        let entries = json!([
            {"signal": "upvote"},
            {"signal": "positive"},
            {"signal": "downvote"},
            {"signal": "negative"},
            {"signal": "flag"},
            {"signal": "mystery"}
        ]);
        assert_eq!(count_signals(&entries), (2, 2, 1));
    }

    #[test]
    fn test_count_signals_handles_empty_and_null() {
        assert_eq!(count_signals(&serde_json::Value::Null), (0, 0, 0));
        assert_eq!(count_signals(&json!([])), (0, 0, 0));
    }

    // ── Golden vectors — canonical contract across MCP + all SDKs ───────────

    #[test]
    fn golden_no_feedback() {
        let s = compute_tif(0, 0, 0);
        approx(s.truth, 0.0);
        approx(s.indeterminacy, 1.0);
        approx(s.falsity, 0.0);
        assert_eq!(s.classification, "ask_clarification");
    }

    #[test]
    fn golden_one_upvote() {
        let s = compute_tif(1, 0, 0);
        approx(s.truth, 0.6667);
        approx(s.indeterminacy, 0.3333);
        approx(s.falsity, 0.0);
        assert_eq!(s.classification, "verify_before_use");
    }

    #[test]
    fn golden_two_upvotes() {
        let s = compute_tif(2, 0, 0);
        approx(s.truth, 0.8);
        approx(s.indeterminacy, 0.2);
        approx(s.falsity, 0.0);
        assert_eq!(s.classification, "confident_reuse");
    }

    #[test]
    fn golden_three_upvotes() {
        let s = compute_tif(3, 0, 0);
        approx(s.truth, 1.0);
        approx(s.indeterminacy, 0.0);
        approx(s.falsity, 0.0);
        assert_eq!(s.classification, "confident_reuse");
    }

    #[test]
    fn golden_two_downvotes() {
        let s = compute_tif(0, 2, 0);
        approx(s.truth, 0.0);
        approx(s.indeterminacy, 0.2);
        approx(s.falsity, 0.8);
        assert_eq!(s.classification, "surface_contradiction");
    }

    #[test]
    fn golden_two_flags() {
        let s = compute_tif(0, 0, 2);
        approx(s.truth, 0.0);
        approx(s.indeterminacy, 1.0);
        approx(s.falsity, 0.0);
        assert_eq!(s.classification, "ask_clarification");
    }

    #[test]
    fn golden_8up_1down_1flag() {
        let s = compute_tif(8, 1, 1);
        approx(s.truth, 0.8);
        approx(s.indeterminacy, 0.1);
        approx(s.falsity, 0.1);
        assert_eq!(s.classification, "confident_reuse");
    }

    #[test]
    fn golden_3down_3flag() {
        let s = compute_tif(0, 3, 3);
        approx(s.truth, 0.0);
        approx(s.indeterminacy, 0.5);
        approx(s.falsity, 0.5);
        assert_eq!(s.classification, "surface_contradiction");
    }

    // ── Dispatch ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_unknown_returns_none() {
        let result = execute(&dummy_client(), "dakera_unknown", &json!({})).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_missing_memory_id() {
        let result = execute(&dummy_client(), "dakera_tif_evaluate", &json!({})).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("memory_id"));
    }

    #[tokio::test]
    async fn test_evaluate_dispatches() {
        // Valid args → tool dispatches and attempts the API call (errors on dummy host).
        let result = execute(
            &dummy_client(),
            "dakera_tif_evaluate",
            &json!({"memory_id": "mem_123", "agent_id": "core-engine"}),
        )
        .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().is_error, Some(true));
    }
}
