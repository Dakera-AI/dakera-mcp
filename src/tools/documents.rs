//! Document import (Dakera v0.12.3, Dakera-AI/dakera#923).
//!
//! A document becomes chunk memories of the agent (heading breadcrumb,
//! provenance in `_dakera_document`), imported by a background job.
//!
//! Tools:
//!   - `dakera_import_document`  — `POST /v1/memory/documents` (text, Markdown,
//!     HTML given as `content`), or `POST /v1/namespaces/{ns}/attachments/{ref}/document`
//!     for an uploaded attachment (PDF, DOCX, ...); optional bounded wait
//!   - `dakera_document_status`  — `GET /v1/memory/documents/jobs/{job_id}?agent_id=`
//!   - `dakera_list_documents`   — `GET /v1/memory/documents?agent_id=&cursor=&limit=`
//!   - `dakera_delete_document`  — `DELETE /v1/memory/documents/{document_id}?agent_id=`
//!
//! The routes are new in Dakera v0.12.3: the tools are hidden (and a direct
//! call explains why) when `/v1/capabilities` has no enabled `documents`
//! section.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

/// The longest `wait_seconds` (a tool call is cut at 60 s).
pub const MAX_WAIT_SECS: u64 = 45;
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// The media types the text route accepts.
pub const TEXT_MEDIA_TYPES: &[&str] = &["text/plain", "text/markdown", "text/html"];

/// The largest page of the document listing.
pub const MAX_LIST_LIMIT: u64 = 500;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dakera_import_document".into(),
            description: "Import a document as chunk memories of an agent (v0.12.3+): content as text, Markdown or HTML, or attachment_ref of an uploaded PDF/DOCX. Chunks keep their heading path and page, so recall can cite them. Runs as a job: wait_seconds waits, else poll dakera_document_status.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "content": { "type": "string", "description": "The document text" },
                    "media_type": { "type": "string", "enum": TEXT_MEDIA_TYPES, "description": "Of content (text/plain if omitted)" },
                    "attachment_ref": { "type": "string", "description": "sha256:<hex> of an uploaded attachment, instead of content" },
                    "namespace": { "type": "string", "description": "Namespace of the attachment (the agent's own if omitted)" },
                    "title": { "type": "string" },
                    "document_id": { "type": "string", "description": "Re-importing the same id replaces the document" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "session_id": { "type": "string" },
                    "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"] },
                    "importance": { "type": "number" },
                    "metadata": { "type": "object" },
                    "lang": { "type": "string" },
                    "max_tokens": { "type": "integer", "description": "Chunk budget in tokens" },
                    "overlap_tokens": { "type": "integer" },
                    "strategy": { "type": "string", "enum": ["structure", "fixed"] },
                    "decompose": { "type": "boolean", "description": "Also derive sentence sub-memories per chunk" },
                    "wait_seconds": { "type": "integer", "description": "Wait up to this long (max 45) for the job" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_document_status".into(),
            description: "Status of a document import job (v0.12.3+): Pending, Running (progress), Completed or Failed with error {status, code}.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "job_id": { "type": "string" }
                },
                "required": ["agent_id", "job_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_list_documents".into(),
            description: "List an agent's imported documents (v0.12.3+): id, title, media type, chunk count, pages. Pass next_cursor as cursor for the next page.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "limit": { "type": "integer", "description": "1-500 (50 if omitted)" },
                    "cursor": { "type": "string" }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_delete_document".into(),
            description: "Delete an imported document: every chunk memory of it (v0.12.3+). Irreversible.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "document_id": { "type": "string" }
                },
                "required": ["agent_id", "document_id"]
            }),
        },
    ]
}

pub async fn execute(client: &DakeraApiClient, name: &str, args: &Value) -> Option<CallToolResult> {
    let run = matches!(
        name,
        "dakera_import_document"
            | "dakera_document_status"
            | "dakera_list_documents"
            | "dakera_delete_document"
    );
    if !run {
        return None;
    }
    if let Some(reason) = client.unavailable_reason(name).await {
        return Some(CallToolResult::error(reason));
    }
    Some(match name {
        "dakera_import_document" => tool_import(client, args).await,
        "dakera_document_status" => tool_status(client, args).await,
        "dakera_list_documents" => tool_list(client, args).await,
        _ => tool_delete(client, args).await,
    })
}

/// The body of an import request: the fields the server knows, only those
/// given (the server refuses unknown `chunking` fields). `content` and
/// `media_type` go to the text route only.
pub fn import_body(args: &Value, agent_id: &str, text: bool) -> Result<Value, CallToolResult> {
    let mut body = json!({ "agent_id": agent_id });
    if text {
        let content = require_string(args, "content")?;
        body["content"] = json!(content);
        if let Some(media_type) = args.get("media_type").and_then(|v| v.as_str()) {
            if !TEXT_MEDIA_TYPES.contains(&media_type) {
                return Err(CallToolResult::error(format!(
                    "media_type must be one of {}; got {media_type:?} (import a PDF or DOCX by \
                     uploading it with dakera_attachment_upload and passing attachment_ref)",
                    TEXT_MEDIA_TYPES.join(", ")
                )));
            }
            body["media_type"] = json!(media_type);
        }
    }
    for field in ["title", "document_id", "session_id", "memory_type", "lang"] {
        if let Some(v) = args.get(field).and_then(|v| v.as_str()) {
            body[field] = json!(v);
        }
    }
    if let Some(v) = args.get("importance").and_then(|v| v.as_f64()) {
        body["importance"] = json!(v);
    }
    if let Some(tags) = args.get("tags").and_then(|v| v.as_array()) {
        body["tags"] = json!(tags);
    }
    if let Some(meta) = args.get("metadata").filter(|v| v.is_object()) {
        body["metadata"] = meta.clone();
    }
    if let Some(v) = args.get("decompose").and_then(|v| v.as_bool()) {
        body["decompose"] = json!(v);
    }
    let mut chunking = serde_json::Map::new();
    for field in ["max_tokens", "overlap_tokens"] {
        match args.get(field).filter(|v| !v.is_null()) {
            None => {}
            Some(v) => match v.as_u64() {
                Some(n) => {
                    chunking.insert(field.into(), json!(n));
                }
                None => {
                    return Err(CallToolResult::error(format!(
                        "{field} must be a non-negative integer; got {v}"
                    )))
                }
            },
        }
    }
    if let Some(strategy) = args.get("strategy").and_then(|v| v.as_str()) {
        if !matches!(strategy, "structure" | "fixed") {
            return Err(CallToolResult::error(format!(
                "strategy must be structure or fixed; got {strategy:?}"
            )));
        }
        chunking.insert("strategy".into(), json!(strategy));
    }
    if !chunking.is_empty() {
        body["chunking"] = Value::Object(chunking);
    }
    Ok(body)
}

/// The job-status route of an agent's import job.
pub fn job_path(agent_id: &str, job_id: &str) -> String {
    format!(
        "/v1/memory/documents/jobs/{}?agent_id={}",
        urlencoding::encode(job_id),
        urlencoding::encode(agent_id)
    )
}

async fn tool_import(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let started = Instant::now();
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let reference = args
        .get("attachment_ref")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let has_content = args.get("content").is_some_and(|v| !v.is_null());
    if reference.is_some() == has_content {
        return CallToolResult::error(
            "Give exactly one of content (text, Markdown or HTML) and attachment_ref (an \
             uploaded file such as a PDF or DOCX)."
                .to_string(),
        );
    }
    let body = match import_body(args, &agent_id, reference.is_none()) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let path = match reference {
        None => "/v1/memory/documents".to_string(),
        Some(reference) => {
            let namespace = match super::attachments::resolve_namespace(args) {
                Ok(ns) => ns,
                Err(e) => return e,
            };
            format!(
                "/v1/namespaces/{}/attachments/{}/document",
                urlencoding::encode(&namespace),
                urlencoding::encode(reference)
            )
        }
    };
    let accepted = match client.post_json(&path, &body).await {
        Ok(a) => a,
        Err(e) => return CallToolResult::error(e),
    };
    let wait = args
        .get("wait_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .min(MAX_WAIT_SECS);
    let job_id = accepted
        .get("job_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if wait == 0 || job_id.is_empty() {
        return ok_json(&accepted);
    }
    let status = job_path(&agent_id, job_id);
    wait_for_job(
        client,
        &accepted,
        &status,
        started + Duration::from_secs(wait),
    )
    .await
}

/// The `202` identifiers, the job once polled, and what to do next.
pub fn import_result(accepted: &Value, job: Option<&Value>, still_running: bool) -> Value {
    let mut out = json!({});
    for field in ["job_id", "document_id", "agent_id", "status_url"] {
        if let Some(v) = accepted.get(field) {
            out[field] = v.clone();
        }
    }
    if let Some(job) = job {
        out["job"] = job.clone();
    }
    if still_running {
        out["still_running"] = json!(true);
        out["next"] = json!("poll with dakera_document_status (agent_id, job_id above)");
    }
    out
}

async fn wait_for_job(
    client: &DakeraApiClient,
    accepted: &Value,
    status_path: &str,
    deadline: Instant,
) -> CallToolResult {
    loop {
        let job = match client.get_json(status_path).await {
            Ok(job) => job,
            Err(e) => {
                let ids = import_result(accepted, None, false);
                return CallToolResult::error(format!("{e}\nThe import was started: {ids}"));
            }
        };
        match job.get("status").and_then(|v| v.as_str()).unwrap_or("") {
            "Completed" => return ok_json(&import_result(accepted, Some(&job), false)),
            "Failed" | "Cancelled" => {
                let result = import_result(accepted, Some(&job), false);
                return CallToolResult::error(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                );
            }
            _ => {}
        }
        if Instant::now() + POLL_INTERVAL >= deadline {
            return ok_json(&import_result(accepted, Some(&job), true));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn tool_status(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let job_id = match require_string(args, "job_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.get_json(&job_path(&agent_id, &job_id)).await {
        Ok(job) => ok_json(&job),
        Err(e) => CallToolResult::error(e),
    }
}

/// The listing route with its query.
pub fn list_path(args: &Value, agent_id: &str) -> Result<String, CallToolResult> {
    let mut path = format!(
        "/v1/memory/documents?agent_id={}",
        urlencoding::encode(agent_id)
    );
    if let Some(v) = args.get("limit").filter(|v| !v.is_null()) {
        match v.as_u64() {
            Some(n) if (1..=MAX_LIST_LIMIT).contains(&n) => path.push_str(&format!("&limit={n}")),
            _ => {
                return Err(CallToolResult::error(format!(
                    "limit must be an integer from 1 to {MAX_LIST_LIMIT}; got {v}"
                )))
            }
        }
    }
    if let Some(cursor) = args.get("cursor").and_then(|v| v.as_str()) {
        path.push_str(&format!("&cursor={}", urlencoding::encode(cursor)));
    }
    Ok(path)
}

async fn tool_list(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = match list_path(args, &agent_id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    match client.get_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_delete(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let document_id = match require_string(args, "document_id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = format!(
        "/v1/memory/documents/{}?agent_id={}",
        urlencoding::encode(&document_id),
        urlencoding::encode(&agent_id)
    );
    match client.delete_json(&path).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

/// The citation of a recall result that is a document chunk: its
/// `memory.metadata._dakera_document`, reduced to what a citation needs.
pub fn citation(result: &Value) -> Option<Value> {
    let doc = result
        .get("memory")?
        .get("metadata")?
        .get("_dakera_document")?;
    let id = doc.get("id")?.as_str()?;
    let mut out = json!({ "document_id": id });
    for field in [
        "title",
        "page_start",
        "page_end",
        "heading_path",
        "chunk_index",
        "chunk_count",
        "media_type",
    ] {
        if let Some(v) = doc.get(field).filter(|v| !v.is_null()) {
            out[field] = v.clone();
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_body_sends_only_given_fields_and_known_chunking() {
        let args = json!({
            "agent_id": "a", "content": "# T\n\nx", "media_type": "text/markdown",
            "title": "T", "max_tokens": 200, "strategy": "fixed", "decompose": false,
            "tags": ["k"], "wait_seconds": 5, "namespace": "ignored"
        });
        let body = import_body(&args, "a", true).unwrap();
        assert_eq!(
            body,
            json!({
                "agent_id": "a", "content": "# T\n\nx", "media_type": "text/markdown",
                "title": "T", "tags": ["k"], "decompose": false,
                "chunking": {"max_tokens": 200, "strategy": "fixed"}
            })
        );
        let attachment = import_body(&json!({"agent_id": "a", "title": "M"}), "a", false).unwrap();
        assert_eq!(attachment, json!({"agent_id": "a", "title": "M"}));
    }

    #[test]
    fn import_body_refuses_bad_values() {
        let bad = [
            json!({"agent_id": "a", "content": "x", "media_type": "application/pdf"}),
            json!({"agent_id": "a", "content": "x", "strategy": "semantic"}),
            json!({"agent_id": "a", "content": "x", "max_tokens": -1}),
            json!({"agent_id": "a"}),
        ];
        for args in bad {
            assert!(import_body(&args, "a", true).is_err(), "{args}");
        }
    }

    #[test]
    fn list_path_checks_the_limit() {
        let path = list_path(&json!({"limit": 10, "cursor": "a b"}), "x/y").unwrap();
        assert_eq!(
            path,
            "/v1/memory/documents?agent_id=x%2Fy&limit=10&cursor=a%20b"
        );
        assert!(list_path(&json!({"limit": 0}), "x").is_err());
        assert!(list_path(&json!({"limit": 501}), "x").is_err());
    }

    #[test]
    fn citation_reads_the_chunk_metadata() {
        let result = json!({"memory": {"id": "doc_1", "metadata": {"_dakera_document": {
            "id": "manual", "title": "Field Manual", "media_type": "application/pdf",
            "chunk_index": 41, "chunk_count": 212, "page_start": 39, "page_end": 40,
            "heading_path": ["Field Manual", "Install"], "char_start": 1, "char_end": 2,
            "sha256": "ab"
        }}}});
        let c = citation(&result).unwrap();
        assert_eq!(c["document_id"], "manual");
        assert_eq!(c["page_start"], 39);
        assert_eq!(c["heading_path"], json!(["Field Manual", "Install"]));
        assert!(c.get("sha256").is_none());
        assert!(citation(&json!({"memory": {"id": "m", "metadata": {}}})).is_none());
    }
}
