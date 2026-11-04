//! Forgetting (Dakera v0.12.4): the body of `dakera_forget` (by id, by
//! description with `about`, by a user's request with `request`), reading
//! messages for requests to forget, and the agent's trash of restorable
//! deletions.
//!
//! Tools:
//!   - `dakera_forget_detect`  — POST /v1/memory/forget/detect
//!   - `dakera_memory_trash`   — GET  /v1/memory/deduplicated?agent_id=
//!   - `dakera_memory_restore` — POST /v1/memory/deduplicated/{id}/restore?agent_id=

use serde_json::{json, Value};

use super::capabilities::{self, SERVER_V0124};
use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

/// The most messages one detection reads.
pub const MAX_DETECT_TEXTS: usize = 64;

/// The longest message (`about`, `request`, a detected text), in characters.
pub const MAX_TEXT_CHARS: usize = 8_000;

/// The most memories one `about` / `request` forget removes.
pub const MAX_FORGET_LIMIT: u64 = 50;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_forget_detect".into(),
            description: "Check which of a user's messages ask to forget something said, in any language (v0.12.4+): probability and is_request per message. Reads no memory. Act on a request with dakera_forget request=<the message>.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "texts": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Messages as said (1-64, each at most 8000 characters)"
                    }
                },
                "required": ["texts"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_trash".into(),
            description: "List an agent's restorable deletions with their content: memories forgotten with restorable=true (kind deletion, v0.12.4+), deduplicated or consolidated ones. Restore one with dakera_memory_restore.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "agent_id": { "type": "string" } },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_memory_restore".into(),
            description: "Put back a memory from the agent's trash (dakera_memory_trash), exactly as it was stored. 404 when it has no restorable copy; 409 when the agent holds that id again.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "memory_id": { "type": "string" }
                },
                "required": ["agent_id", "memory_id"]
            }),
        },
    ]
}

pub async fn execute(client: &DakeraApiClient, name: &str, args: &Value) -> Option<CallToolResult> {
    let run = match name {
        "dakera_forget_detect" | "dakera_memory_trash" | "dakera_memory_restore" => name,
        _ => return None,
    };
    if let Some(reason) = client.unavailable_reason(run).await {
        return Some(CallToolResult::error(reason));
    }
    Some(match run {
        "dakera_forget_detect" => tool_forget_detect(client, args).await,
        "dakera_memory_trash" => tool_trash(client, args).await,
        _ => tool_restore(client, args).await,
    })
}

/// A text argument's length check.
fn check_text(name: &str, text: &str) -> Result<(), CallToolResult> {
    if text.trim().is_empty() {
        return Err(CallToolResult::error(format!("{name} is empty")));
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(CallToolResult::error(format!(
            "{name} is longer than {MAX_TEXT_CHARS} characters"
        )));
    }
    Ok(())
}

/// The body of `POST /v1/memory/forget` from `dakera_forget`'s arguments,
/// checked. Empty lists are left out: the server reads `tags: []` as a
/// filter.
pub fn forget_body(args: &Value) -> Result<Value, CallToolResult> {
    let agent_id = require_string(args, "agent_id")?;
    let mut body = json!({ "agent_id": agent_id });
    for field in ["memory_ids", "tags"] {
        if let Some(list) = args
            .get(field)
            .and_then(|v| v.as_array())
            .filter(|a| !a.is_empty())
        {
            body[field] = json!(list);
        }
    }
    let about = args.get("about").and_then(|v| v.as_str());
    let request = args.get("request").and_then(|v| v.as_str());
    if about.is_some() && request.is_some() {
        return Err(CallToolResult::error(
            "Send about (what was said) or request (the user's message), not both.".to_string(),
        ));
    }
    if let Some(about) = about {
        check_text("about", about)?;
        body["about"] = json!(about);
    }
    if let Some(request) = request {
        check_text("request", request)?;
        body["request"] = json!(request);
    }
    let described = about.is_some() || request.is_some();
    if body.get("memory_ids").is_none() && body.get("tags").is_none() && !described {
        return Err(CallToolResult::error(
            "Say what to forget: memory_ids, tags, about or request. Nothing was deleted."
                .to_string(),
        ));
    }
    if let Some(until) = args.get("until").filter(|v| !v.is_null()) {
        let Some(until) = until.as_u64() else {
            return Err(CallToolResult::error(format!(
                "until must be Unix seconds; got {until}"
            )));
        };
        if !described {
            return Err(CallToolResult::error(
                "until goes with about or request.".to_string(),
            ));
        }
        body["until"] = json!(until);
    }
    if let Some(score) = args.get("min_score").filter(|v| !v.is_null()) {
        let score = score.as_f64().filter(|s| (0.0..=1.0).contains(s));
        match (score, about.is_some()) {
            (Some(score), true) => body["min_score"] = json!(score),
            (None, _) => {
                return Err(CallToolResult::error(
                    "min_score is a relevance between 0 and 1.".to_string(),
                ))
            }
            (_, false) => {
                return Err(CallToolResult::error(
                    "min_score goes with about.".to_string(),
                ))
            }
        }
    }
    if let Some(limit) = args.get("limit").filter(|v| !v.is_null()) {
        let limit = limit
            .as_u64()
            .filter(|l| (1..=MAX_FORGET_LIMIT).contains(l));
        match (limit, described) {
            (Some(limit), true) => body["limit"] = json!(limit),
            (None, _) => {
                return Err(CallToolResult::error(format!(
                    "limit is between 1 and {MAX_FORGET_LIMIT}."
                )))
            }
            (_, false) => {
                return Err(CallToolResult::error(
                    "limit goes with about or request.".to_string(),
                ))
            }
        }
    }
    for flag in ["restorable", "dry_run"] {
        if args.get(flag).and_then(|v| v.as_bool()) == Some(true) {
            body[flag] = json!(true);
        }
    }
    Ok(body)
}

/// The `dakera_forget` arguments a server before v0.12.4 ignores: it would
/// act on the rest of the request (delete the ids although `dry_run` was
/// asked, for good although `restorable` was).
pub fn needs_v0124(body: &Value) -> Vec<&'static str> {
    ["about", "request", "dry_run", "restorable"]
        .into_iter()
        .filter(|f| body.get(*f).is_some())
        .collect()
}

/// `dakera_forget`: refuse the v0.12.4 arguments on an older server, then
/// forget, and say how a `request` was read.
pub async fn tool_forget(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let body = match forget_body(args) {
        Ok(body) => body,
        Err(e) => return e,
    };
    let newer = needs_v0124(&body);
    if !newer.is_empty() {
        let state = client.capability_state().await;
        if capabilities::server_at_least(&state, SERVER_V0124) == Some(false) {
            return CallToolResult::error(format!(
                "{} need(s) Dakera server v0.12.4 or later; an older server ignores them and \
                 would act on the rest of the request. Nothing was sent.",
                newer.join(", ")
            ));
        }
    }
    match client.post_json("/v1/memory/forget", &body).await {
        Ok(result) => {
            let answer = ok_json(&result);
            match forget_note(&result) {
                Some(note) => answer.with_note(note),
                None => answer,
            }
        }
        Err(e) => CallToolResult::error(e),
    }
}

/// A note on a forget answer: a `request` that did not read as a request to
/// forget (nothing was forgotten), or a dry run.
pub fn forget_note(result: &Value) -> Option<String> {
    if let Some(intent) = result.get("intent") {
        if intent.get("is_request").and_then(|v| v.as_bool()) == Some(false) {
            return Some(
                "Note: the message does not read as a request to forget (intent.is_request is \
                 false), so nothing was forgotten."
                    .to_string(),
            );
        }
    }
    None
}

/// What to tell the agent when a stored text reads as a request to forget
/// (`forget_request` of a store answer, Dakera v0.12.4).
pub fn store_forget_note(result: &Value) -> Option<String> {
    let intent = result.get("forget_request")?;
    if intent.get("is_request").and_then(|v| v.as_bool()) != Some(true) {
        return None;
    }
    Some(
        "Note: this text reads as a request to forget something said (forget_request). If the \
         user asked to forget, call dakera_forget with request set to their message (and until \
         set to now) — unless the server forgets such requests itself \
         (DAKERA_FORGET_REQUESTS=apply)."
            .to_string(),
    )
}

async fn tool_forget_detect(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let Some(texts) = args.get("texts").and_then(|v| v.as_array()) else {
        return CallToolResult::error("Missing required parameter: texts (array)".to_string());
    };
    if texts.is_empty() || texts.len() > MAX_DETECT_TEXTS {
        return CallToolResult::error(format!(
            "texts holds between 1 and {MAX_DETECT_TEXTS} messages; got {}",
            texts.len()
        ));
    }
    let mut checked = Vec::with_capacity(texts.len());
    for (i, text) in texts.iter().enumerate() {
        let Some(text) = text.as_str() else {
            return CallToolResult::error(format!("texts[{i}] is not a string"));
        };
        if text.chars().count() > MAX_TEXT_CHARS {
            return CallToolResult::error(format!(
                "texts[{i}] is longer than {MAX_TEXT_CHARS} characters"
            ));
        }
        checked.push(text);
    }
    let body = json!({ "texts": checked });
    match client.post_json("/v1/memory/forget/detect", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_trash(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!(
        "/v1/memory/deduplicated?agent_id={}",
        urlencoding::encode(&agent_id)
    );
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_restore(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let memory_id = match require_string(args, "memory_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!(
        "/v1/memory/deduplicated/{}/restore?agent_id={}",
        urlencoding::encode(&memory_id),
        urlencoding::encode(&agent_id)
    );
    match client.post_json(&path, &json!({})).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_text(result: CallToolResult) -> String {
        assert_eq!(result.is_error, Some(true));
        result.content[0].text.clone()
    }

    #[test]
    fn forget_by_ids_keeps_the_old_body() {
        let body =
            forget_body(&json!({"agent_id": "a", "memory_ids": ["m1"], "tags": []})).unwrap();
        assert_eq!(body, json!({"agent_id": "a", "memory_ids": ["m1"]}));
        assert!(needs_v0124(&body).is_empty());
    }

    #[test]
    fn forget_by_request_sends_its_options() {
        let body = forget_body(&json!({
            "agent_id": "a",
            "request": "Oublie mon adresse",
            "until": 1_700_000_000u64,
            "limit": 5,
            "restorable": true,
            "dry_run": false
        }))
        .unwrap();
        assert_eq!(
            body,
            json!({
                "agent_id": "a",
                "request": "Oublie mon adresse",
                "until": 1_700_000_000u64,
                "limit": 5,
                "restorable": true
            })
        );
        assert_eq!(needs_v0124(&body), vec!["request", "restorable"]);
    }

    #[test]
    fn forget_refuses_what_the_server_would_refuse_or_misread() {
        let cases = [
            json!({"agent_id": "a"}),
            json!({"agent_id": "a", "about": "x", "request": "y"}),
            json!({"agent_id": "a", "about": "  "}),
            json!({"agent_id": "a", "memory_ids": ["m"], "until": 5}),
            json!({"agent_id": "a", "request": "r", "min_score": 0.5}),
            json!({"agent_id": "a", "about": "x", "min_score": 2.0}),
            json!({"agent_id": "a", "about": "x", "limit": 51}),
            json!({"agent_id": "a", "about": "x".repeat(8_001)}),
        ];
        for args in cases {
            assert!(forget_body(&args).is_err(), "{args}");
        }
    }

    #[test]
    fn notes_for_a_request_that_is_not_one_and_a_stored_request() {
        let not = json!({"deleted_count": 0, "intent": {"probability": 0.1, "threshold": 0.5, "is_request": false}});
        assert!(forget_note(&not).unwrap().contains("nothing was forgotten"));
        let yes = json!({"deleted_count": 2, "intent": {"probability": 0.9, "threshold": 0.5, "is_request": true}});
        assert!(forget_note(&yes).is_none());
        let stored = json!({"memory": {"id": "m"}, "forget_request": {"probability": 0.9, "threshold": 0.5, "is_request": true}});
        assert!(store_forget_note(&stored)
            .unwrap()
            .contains("dakera_forget"));
        assert!(store_forget_note(&json!({"memory": {"id": "m"}})).is_none());
    }

    #[tokio::test]
    async fn detect_checks_its_texts_before_sending() {
        let client = DakeraApiClient::new("http://127.0.0.1:1".into(), None);
        let none = tool_forget_detect(&client, &json!({"texts": []})).await;
        assert!(error_text(none).contains("between 1 and 64"));
        let many: Vec<String> = (0..65).map(|i| i.to_string()).collect();
        let too_many = tool_forget_detect(&client, &json!({ "texts": many })).await;
        assert!(error_text(too_many).contains("got 65"));
        let not_text = tool_forget_detect(&client, &json!({"texts": [1]})).await;
        assert!(error_text(not_text).contains("not a string"));
    }
}
