//! Session tools — start, end, touch, list, get, memories
//!
//! Since Dakera v0.12.2 the server ends a session after a period of inactivity
//! (4 h by default, `idle_timeout_secs` per session) with `ended_reason: "idle"`.
//! Storing into an ended session still succeeds, and the store answer says
//! `session_state: "ended"`; the tools add a note telling the agent to start a
//! new session.

use serde_json::{json, Value};

use super::{ok_json, require_string, route_missing, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_session_start".into(),
            description:
                "Open a session; its session_id groups the memories you store. The server ends a session idle for 4 h by default (v0.12.2+).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "metadata": { "type": "object", "description": "Optional session metadata" },
                    "idle_timeout_secs": { "type": "integer", "description": "End after this many idle seconds; 0 = never (v0.12.2+)" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_end".into(),
            description: "Close a session with an optional summary. Call at run end, even on error. A session the server already ended (idle) keeps its state; your summary is then not saved.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session ID to end" },
                    "summary": { "type": "string", "description": "Optional session summary" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_list".into(),
            description: "List sessions for an agent, newest first, one page (default 50, max 1000; use limit/offset). Set active_only=true to find an open session to resume. v0.12.2+ shows last_activity_at and ended_reason (client or idle)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "active_only": { "type": "boolean", "description": "Only return active sessions" },
                    "limit": { "type": "integer", "description": "Page size" },
                    "offset": { "type": "integer", "description": "Pagination offset" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_get".into(),
            description:
                "Fetch a session: metadata, summary, timestamps, memory count; v0.12.2+ adds last_activity_at, ended_reason (client or idle) and idle_since. Use to review a run or check it is still open.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session ID to retrieve" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_memories".into(),
            description: "Return the memories stored under a session, one page at a time (default 50, max 500; use limit/offset). Content is cut to content_preview_chars (default 500; content_truncated marks a cut one, read it with dakera_memory_get).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session ID" },
                    "limit": { "type": "integer", "description": "Page size" },
                    "offset": { "type": "integer", "description": "Pagination offset" },
                    "content_preview_chars": { "type": "integer", "description": "Characters of content per memory (1-10000, 0 = full)" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_session_touch".into(),
            description: "Keep a session open while you work without storing or recalling (v0.12.2+). Answers session_state active (with idle_deadline_at) or ended: then start a new session.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session ID to keep open" }
                },
                "required": ["session_id"]
            }),
        },
    ]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_session_start" => Some(tool_session_start(client, args).await),
        "dakera_session_end" => Some(tool_session_end(client, args).await),
        "dakera_session_list" => Some(tool_session_list(client, args).await),
        "dakera_session_get" => Some(tool_session_get(client, args).await),
        "dakera_session_memories" => Some(tool_session_memories(client, args).await),
        "dakera_session_touch" => Some(tool_session_touch(client, args).await),
        _ => None,
    }
}

async fn tool_session_start(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut body = json!({
        "agent_id": agent_id,
        "metadata": args.get("metadata").cloned().unwrap_or(json!(null)),
    });
    // Sent only when given: a server before v0.12.2 does not read it (its
    // sessions are never ended for inactivity).
    if let Some(secs) = args.get("idle_timeout_secs").filter(|v| !v.is_null()) {
        match secs.as_u64() {
            Some(n) => body["idle_timeout_secs"] = json!(n),
            None => {
                return CallToolResult::error(format!(
                    "idle_timeout_secs must be a non-negative integer (seconds; 0 = never \
                     ended for inactivity); got {secs}"
                ))
            }
        }
    }
    match client.post_json("/v1/sessions/start", &body).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_session_end(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let session_id = match require_string(args, "session_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let body = json!({
        "summary": args.get("summary").and_then(|v| v.as_str()),
    });
    let encoded = urlencoding::encode(&session_id);
    let path = format!("/v1/sessions/{}/end", encoded);
    match client.post_json(&path, &body).await {
        Ok(result) => {
            let answer = ok_json(&result);
            match end_note(&result) {
                Some(note) => answer.with_note(note),
                None => answer,
            }
        }
        Err(e) => CallToolResult::error(e),
    }
}

/// A note on a `POST /v1/sessions/{id}/end` answer that did not end the session
/// now: the server had ended it already for inactivity (v0.12.2), or there is
/// no such session the key can reach (the idempotent answer with an empty
/// `agent_id`).
pub fn end_note(result: &Value) -> Option<String> {
    let session = result.get("session")?;
    let id = session.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if session.get("ended_reason").and_then(|v| v.as_str()) == Some("idle") {
        return Some(format!(
            "Note: the server had already ended session {id} after inactivity (ended_reason \
             \"idle\"); it was not ended again and a summary passed now was not saved. Store \
             the summary as a memory if it matters, and start a new session for further work."
        ));
    }
    let agent = session.get("agent_id").and_then(|v| v.as_str());
    let started = session.get("started_at").and_then(|v| v.as_u64());
    if agent == Some("") && started == Some(0) {
        return Some(format!(
            "Note: no session {id} was found for this key (unknown id, or a session of an agent \
             the key cannot reach); nothing was ended."
        ));
    }
    None
}

/// A note for an answer that says the session it names has ended
/// (`session_state: "ended"`, Dakera v0.12.2): from a store or a touch.
pub fn ended_session_note(result: &Value) -> Option<String> {
    if result.get("session_state").and_then(|v| v.as_str()) != Some("ended") {
        return None;
    }
    let id = result
        .pointer("/memory/session_id")
        .or_else(|| result.pointer("/session/id"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Some(format!(
        "Note: session {id} has ended (by dakera_session_end, or by the server after a period of \
         inactivity, 4 h by default). Memories stored with it are kept, but it is closed: start a \
         new session with dakera_session_start and use its session_id from now on."
    ))
}

async fn tool_session_list(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let active_only = args
        .get("active_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let encoded = urlencoding::encode(&agent_id);
    let path = format!(
        "/v1/sessions?agent_id={}&active_only={}{}",
        encoded,
        active_only,
        page_query(args)
    );
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

/// `&limit=N&offset=M` for the paging arguments that were given (empty when none).
pub fn page_query(args: &serde_json::Value) -> String {
    let mut query = String::new();
    for name in ["limit", "offset"] {
        if let Some(n) = args.get(name).and_then(|v| v.as_u64()) {
            query.push_str(&format!("&{name}={n}"));
        }
    }
    query
}

async fn tool_session_get(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let session_id = match require_string(args, "session_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&session_id);
    let path = format!("/v1/sessions/{}", encoded);
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_session_memories(
    client: &DakeraApiClient,
    args: &serde_json::Value,
) -> CallToolResult {
    let session_id = match require_string(args, "session_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let preview = match super::content_preview_chars(args) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let encoded = urlencoding::encode(&session_id);
    let mut path = format!("/v1/sessions/{}/memories?{}", encoded, page_query(args));
    if let Some(n) = preview {
        path.push_str(&format!("&content_preview_chars={n}"));
    }
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

/// The answer of `dakera_session_touch` on a server without the route.
pub const TOUCH_UNSUPPORTED: &str = "This Dakera server predates session keep-alive \
     (POST /v1/sessions/{id}/touch, v0.12.2). It does not end sessions for inactivity, so the \
     session stays open until dakera_session_end; nothing to do.";

async fn tool_session_touch(client: &DakeraApiClient, args: &serde_json::Value) -> CallToolResult {
    let session_id = match require_string(args, "session_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!("/v1/sessions/{}/touch", urlencoding::encode(&session_id));
    let (status, text) = match client
        .send_raw(reqwest::Method::POST, &path, Some(&json!({})))
        .await
    {
        Ok(answer) => answer,
        Err(e) => return CallToolResult::error(e),
    };
    if route_missing(status, &text) {
        return ok_json(&json!({
            "session_id": session_id,
            "touch_supported": false,
            "note": TOUCH_UNSUPPORTED,
        }));
    }
    if !status.is_success() {
        return CallToolResult::error(format!("API error ({}): {}", status, text));
    }
    let result: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return CallToolResult::error(format!("JSON parse failed: {}", e)),
    };
    let answer = ok_json(&result);
    match ended_session_note(&result) {
        Some(note) => answer.with_note(note),
        None => answer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[test]
    fn test_ended_session_note_from_a_store_answer() {
        let store = json!({"memory": {"id": "m1", "session_id": "s1"}, "session_state": "ended"});
        let note = ended_session_note(&store).unwrap();
        assert!(note.contains("session s1 has ended"));
        assert!(note.contains("dakera_session_start"));
        let active = json!({"memory": {"session_id": "s1"}, "session_state": "active"});
        assert!(ended_session_note(&active).is_none());
        // A server before v0.12.2 sends no session_state.
        assert!(ended_session_note(&json!({"memory": {"session_id": "s1"}})).is_none());
    }

    #[test]
    fn test_ended_session_note_from_a_touch_answer() {
        let touch =
            json!({"session": {"id": "s2", "ended_reason": "idle"}, "session_state": "ended"});
        assert!(ended_session_note(&touch).unwrap().contains("s2"));
    }

    #[test]
    fn test_end_note() {
        let idle = json!({"session": {"id": "s1", "agent_id": "a", "started_at": 5, "ended_reason": "idle"}});
        assert!(end_note(&idle).unwrap().contains("not saved"));
        let unknown =
            json!({"session": {"id": "s9", "agent_id": "", "started_at": 0, "ended_at": 0}});
        assert!(end_note(&unknown).unwrap().contains("nothing was ended"));
        let client = json!({"session": {"id": "s1", "agent_id": "a", "started_at": 5, "ended_reason": "client"}});
        assert!(end_note(&client).is_none());
        // v0.12.0 / v0.12.1: no ended_reason.
        let old = json!({"session": {"id": "s1", "agent_id": "a", "started_at": 5, "ended_at": 9}});
        assert!(end_note(&old).is_none());
    }

    #[tokio::test]
    async fn test_touch_requires_session_id() {
        let result = execute(&dummy_client(), "dakera_session_touch", &json!({}))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("session_id"));
    }

    #[tokio::test]
    async fn test_session_memories_refuses_a_bad_preview() {
        let args = json!({"session_id": "s1", "content_preview_chars": 20000});
        let result = execute(&dummy_client(), "dakera_session_memories", &args)
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("content_preview_chars"));
    }
}
