//! Attachment tools (Dakera v0.12, opt-in).
//!
//! Files a memory can point at, speech to text and image indexing. The routes
//! answer `501 FEATURE_DISABLED` until the server sets `DAKERA_ATTACHMENTS`
//! (and `DAKERA_VISION` for image indexing). These tools are left out of
//! `tools/list` while `/v1/capabilities` says the feature is off, and a direct
//! call answers with the variable that turns it on.
//!
//! A memory can only reference an attachment stored in its own namespace,
//! `_dakera_agent_<agent_id>`; `namespace` defaults to that when `agent_id` is
//! given. The transcription and image jobs copy an attachment from any
//! namespace the key can read into the agent's.
//!
//! Tools:
//!   - `dakera_attachment_upload`      — POST   /v1/namespaces/{ns}/attachments
//!   - `dakera_attachment_list`        — GET    /v1/namespaces/{ns}/attachments
//!   - `dakera_attachment_download`    — GET    /v1/namespaces/{ns}/attachments/{ref}
//!   - `dakera_attachment_delete`      — DELETE /v1/namespaces/{ns}/attachments/{ref}
//!   - `dakera_attachment_transcribe`  — POST   …/{ref}/transcribe
//!   - `dakera_attachment_index_image` — POST   …/{ref}/index
//!   - `dakera_attachment_job`         — GET    …/{ref}/transcribe|index/{job_id}

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{ok_json, require_string, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

const TOOLS: &[&str] = &[
    "dakera_attachment_upload",
    "dakera_attachment_list",
    "dakera_attachment_download",
    "dakera_attachment_delete",
    "dakera_attachment_transcribe",
    "dakera_attachment_index_image",
    "dakera_attachment_job",
];

const NO_NAMESPACE: &str = "Provide namespace, or agent_id for the agent's own namespace \
     (_dakera_agent_<agent_id>)";
const NO_SOURCE: &str = "Provide file_path (a file on the machine running this MCP) or text";

/// The longest `wait_seconds` (a tool call is cut at 60 s).
const MAX_WAIT_SECS: u64 = 45;
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// The largest file `file_path` reads (the server's own limit is smaller).
const MAX_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;
/// The largest text attachment `dakera_attachment_download` returns inline.
const MAX_INLINE_BYTES: usize = 256 * 1024;

fn memory_job_properties() -> Value {
    json!({
        "namespace": { "type": "string", "description": "Namespace of the attachment (default: the agent's own)" },
        "attachment_ref": { "type": "string", "description": "sha256:<hex> from upload or list" },
        "agent_id": { "type": "string" },
        "tags": { "type": "array", "items": { "type": "string" } },
        "importance": { "type": "number", "description": "0.0-1.0" },
        "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural", "working"] },
        "session_id": { "type": "string" },
        "id": { "type": "string", "description": "Custom memory id" },
        "lang": { "type": "string", "description": "Language of the text (ISO 639-1)" },
        "ttl_seconds": { "type": "integer" },
        "wait_seconds": { "type": "integer", "description": "Wait up to this long (max 45) for the job; 0 returns the job at once" }
    })
}

pub fn definitions() -> Vec<ToolDefinition> {
    let mut index_properties = memory_job_properties();
    index_properties["content"] =
        json!({ "type": "string", "description": "Caption stored as the memory's text" });
    vec![
        ToolDefinition {
            name: "dakera_attachment_upload".into(),
            description: "Upload a file or text as an attachment (v0.12, needs DAKERA_ATTACHMENTS) and get its attachment_ref. \
                To reference it from dakera_store (attachment_ref) it must be in the agent's own namespace: pass agent_id."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "namespace": { "type": "string", "description": "Namespace (instead of agent_id)" },
                    "file_path": { "type": "string", "description": "File on the machine running this MCP server" },
                    "text": { "type": "string", "description": "Upload this text instead of a file" },
                    "content_type": { "type": "string", "description": "Media type (default: from the file extension)" }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_attachment_list".into(),
            description: "List the attachments of a namespace (references, media types, sizes). v0.12, needs DAKERA_ATTACHMENTS."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "namespace": { "type": "string" }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "dakera_attachment_download".into(),
            description: "Fetch an attachment. save_to writes it to a file; without it only small text attachments come back inline."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "namespace": { "type": "string" },
                    "attachment_ref": { "type": "string", "description": "sha256:<hex>" },
                    "save_to": { "type": "string", "description": "File to write on the machine running this MCP server" }
                },
                "required": ["attachment_ref"]
            }),
        },
        ToolDefinition {
            name: "dakera_attachment_delete".into(),
            description: "Delete an attachment. Refused (409) while a memory references it: forget the memory first."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "namespace": { "type": "string" },
                    "attachment_ref": { "type": "string", "description": "sha256:<hex>" }
                },
                "required": ["attachment_ref"]
            }),
        },
        ToolDefinition {
            name: "dakera_attachment_transcribe".into(),
            description: "Transcribe a WAV attachment (English speech to text) into a memory for agent_id; runs as a background job. \
                wait_seconds waits for it, else poll with dakera_attachment_job."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": memory_job_properties(),
                "required": ["attachment_ref", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_attachment_index_image".into(),
            description: "Index a PNG attachment as a visual memory for agent_id (needs DAKERA_VISION too); about 10 s per page on CPU, background job."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": index_properties,
                "required": ["attachment_ref", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "dakera_attachment_job".into(),
            description: "Status of a transcription or image-index job: status, progress, message, error. Jobs are lost on a server restart."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "namespace": { "type": "string", "description": "Namespace of the attachment" },
                    "attachment_ref": { "type": "string" },
                    "job_id": { "type": "string" },
                    "kind": { "type": "string", "enum": ["transcribe", "index"] }
                },
                "required": ["attachment_ref", "job_id"]
            }),
        },
    ]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    args: &serde_json::Value,
) -> Option<CallToolResult> {
    if !TOOLS.contains(&name) {
        return None;
    }
    // Switched off on the server (or a pre-v0.12 server): say so, make no call.
    if let Some(reason) = client.unavailable_reason(name).await {
        return Some(CallToolResult::error(reason));
    }
    Some(match name {
        "dakera_attachment_upload" => tool_upload(client, args).await,
        "dakera_attachment_list" => tool_list(client, args).await,
        "dakera_attachment_download" => tool_download(client, args).await,
        "dakera_attachment_delete" => tool_delete(client, args).await,
        "dakera_attachment_transcribe" => tool_start_job(client, args, "transcribe").await,
        "dakera_attachment_index_image" => tool_start_job(client, args, "index").await,
        _ => tool_job(client, args).await,
    })
}

/// The namespace of a call: `namespace`, else the agent's own.
pub fn resolve_namespace(args: &Value) -> Result<String, CallToolResult> {
    if let Some(ns) = args.get("namespace").and_then(|v| v.as_str()) {
        if !ns.is_empty() {
            return Ok(ns.to_string());
        }
    }
    match args.get("agent_id").and_then(|v| v.as_str()) {
        Some(agent) if !agent.is_empty() => Ok(format!("_dakera_agent_{agent}")),
        _ => Err(CallToolResult::error(NO_NAMESPACE.to_string())),
    }
}

fn attachments_path(namespace: &str) -> String {
    let ns = urlencoding::encode(namespace);
    format!("/v1/namespaces/{ns}/attachments")
}

fn attachment_path(namespace: &str, reference: &str) -> String {
    let base = attachments_path(namespace);
    format!("{base}/{}", urlencoding::encode(reference))
}

/// The media type to upload a file as, from its extension.
pub fn guess_content_type(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Whether a media type is text an agent can read inline.
pub fn is_text(content_type: &str) -> bool {
    let first = content_type.split(';').next().unwrap_or("");
    let essence = first.trim().to_ascii_lowercase();
    essence.starts_with("text/") || essence == "application/json"
}

async fn read_file(path: &str) -> Result<Vec<u8>, String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Cannot read {path}: {e}"))?;
    if meta.len() > MAX_UPLOAD_BYTES {
        return Err(format!("{path} is larger than {MAX_UPLOAD_BYTES} bytes"));
    }
    tokio::fs::read(path)
        .await
        .map_err(|e| format!("Cannot read {path}: {e}"))
}

/// The bytes and default media type of an upload: `file_path` or `text`.
async fn upload_source(args: &Value) -> Result<(Vec<u8>, &'static str), String> {
    let file_path = args.get("file_path").and_then(|v| v.as_str());
    let text = args.get("text").and_then(|v| v.as_str());
    match (file_path, text) {
        (Some(path), None) => {
            let data = read_file(path).await?;
            Ok((data, guess_content_type(path)))
        }
        (None, Some(text)) => Ok((text.as_bytes().to_vec(), "text/plain")),
        (Some(_), Some(_)) => Err("Provide file_path or text, not both".to_string()),
        (None, None) => Err(NO_SOURCE.to_string()),
    }
}

async fn tool_upload(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let namespace = match resolve_namespace(args) {
        Ok(ns) => ns,
        Err(e) => return e,
    };
    let (data, default_type) = match upload_source(args).await {
        Ok(source) => source,
        Err(e) => return CallToolResult::error(e),
    };
    let given = args.get("content_type").and_then(|v| v.as_str());
    let content_type = given.unwrap_or(default_type);
    let path = attachments_path(&namespace);
    match client.post_bytes(&path, content_type, data).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_list(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let namespace = match resolve_namespace(args) {
        Ok(ns) => ns,
        Err(e) => return e,
    };
    match client.get_json(&attachments_path(&namespace)).await {
        Ok(result) => ok_json(&result),
        Err(e) => CallToolResult::error(e),
    }
}

async fn tool_download(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let namespace = match resolve_namespace(args) {
        Ok(ns) => ns,
        Err(e) => return e,
    };
    let reference = match require_string(args, "attachment_ref") {
        Ok(r) => r,
        Err(e) => return e,
    };
    let path = attachment_path(&namespace, &reference);
    let (content_type, bytes) = match client.get_bytes(&path).await {
        Ok(got) => got,
        Err(e) => return CallToolResult::error(e),
    };
    let size = bytes.len();
    if let Some(save_to) = args.get("save_to").and_then(|v| v.as_str()) {
        if let Err(e) = tokio::fs::write(save_to, &bytes).await {
            return CallToolResult::error(format!("Cannot write {save_to}: {e}"));
        }
        return ok_json(&json!({
            "saved_to": save_to,
            "size_bytes": size,
            "content_type": content_type,
        }));
    }
    if is_text(&content_type) && size <= MAX_INLINE_BYTES {
        let text = String::from_utf8_lossy(&bytes);
        return ok_json(&json!({
            "content_type": content_type,
            "size_bytes": size,
            "text": text,
        }));
    }
    CallToolResult::error(format!(
        "The attachment is {size} bytes of {content_type}; pass save_to to write it to a file \
         (only small text attachments are returned inline)."
    ))
}

async fn tool_delete(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let namespace = match resolve_namespace(args) {
        Ok(ns) => ns,
        Err(e) => return e,
    };
    let reference = match require_string(args, "attachment_ref") {
        Ok(r) => r,
        Err(e) => return e,
    };
    let path = attachment_path(&namespace, &reference);
    match client.delete_empty(&path).await {
        Ok(()) => ok_json(&json!({ "deleted": reference, "namespace": namespace })),
        Err(e) => CallToolResult::error(e),
    }
}

/// The request body of a transcription or image-index job: the memory the
/// result becomes.
pub fn job_body(args: &Value, kind: &str, agent_id: &str) -> Value {
    let mut body = json!({ "agent_id": agent_id });
    let fields = [
        "tags",
        "importance",
        "memory_type",
        "session_id",
        "id",
        "lang",
        "ttl_seconds",
    ];
    for field in fields {
        if let Some(value) = args.get(field) {
            if !value.is_null() {
                body[field] = value.clone();
            }
        }
    }
    if kind == "index" {
        if let Some(content) = args.get("content") {
            body["content"] = content.clone();
        }
    }
    body
}

fn str_field<'a>(v: &'a Value, name: &str) -> &'a str {
    match v.get(name).and_then(|x| x.as_str()) {
        Some(s) => s,
        None => "",
    }
}

async fn tool_start_job(client: &DakeraApiClient, args: &Value, kind: &str) -> CallToolResult {
    let agent_id = match require_string(args, "agent_id") {
        Ok(a) => a,
        Err(e) => return e,
    };
    let reference = match require_string(args, "attachment_ref") {
        Ok(r) => r,
        Err(e) => return e,
    };
    let namespace = match resolve_namespace(args) {
        Ok(ns) => ns,
        Err(e) => return e,
    };
    let body = job_body(args, kind, &agent_id);
    let path = format!("{}/{kind}", attachment_path(&namespace, &reference));
    let accepted = match client.post_json(&path, &body).await {
        Ok(a) => a,
        Err(e) => return CallToolResult::error(e),
    };
    let wait = args.get("wait_seconds").and_then(|v| v.as_u64());
    let wait = wait.unwrap_or(0).min(MAX_WAIT_SECS);
    if wait == 0 {
        return ok_json(&accepted);
    }
    let status_path = str_field(&accepted, "status_url");
    if status_path.is_empty() {
        return ok_json(&accepted);
    }
    wait_for_job(client, status_path, wait).await
}

/// Poll a job until it ends or `wait_secs` pass. A failed job is an error
/// result carrying the job (its `error` has the status and code).
async fn wait_for_job(
    client: &DakeraApiClient,
    status_path: &str,
    wait_secs: u64,
) -> CallToolResult {
    let started = Instant::now();
    loop {
        let job = match client.get_json(status_path).await {
            Ok(job) => job,
            Err(e) => return CallToolResult::error(e),
        };
        match str_field(&job, "status") {
            "Completed" => return ok_json(&job),
            "Failed" | "Cancelled" => {
                let text = serde_json::to_string_pretty(&job).unwrap_or_default();
                return CallToolResult::error(text);
            }
            _ => {}
        }
        if started.elapsed().as_secs() >= wait_secs {
            return ok_json(&json!({
                "still_running": true,
                "job": job,
                "next": "poll with dakera_attachment_job",
            }));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn tool_job(client: &DakeraApiClient, args: &Value) -> CallToolResult {
    let namespace = match resolve_namespace(args) {
        Ok(ns) => ns,
        Err(e) => return e,
    };
    let reference = match require_string(args, "attachment_ref") {
        Ok(r) => r,
        Err(e) => return e,
    };
    let job_id = match require_string(args, "job_id") {
        Ok(j) => j,
        Err(e) => return e,
    };
    let kind = args.get("kind").and_then(|v| v.as_str());
    let kind = kind.unwrap_or("transcribe");
    if kind != "transcribe" && kind != "index" {
        return CallToolResult::error("kind must be transcribe or index".to_string());
    }
    let base = attachment_path(&namespace, &reference);
    let path = format!("{base}/{kind}/{}", urlencoding::encode(&job_id));
    match client.get_json(&path).await {
        Ok(job) => ok_json(&job),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> DakeraApiClient {
        DakeraApiClient::new("http://127.0.0.1:9".to_string(), None)
    }

    #[test]
    fn seven_tools_with_unique_names() {
        let defs = definitions();
        assert_eq!(defs.len(), 7);
        let mut seen = std::collections::HashSet::new();
        for d in &defs {
            assert!(seen.insert(d.name.clone()), "duplicate: {}", d.name);
            assert!(TOOLS.contains(&d.name.as_str()), "not in TOOLS: {}", d.name);
        }
    }

    #[test]
    fn job_tools_require_the_agent_and_the_reference() {
        let defs = definitions();
        let transcribe = defs
            .iter()
            .find(|d| d.name == "dakera_attachment_transcribe")
            .unwrap();
        assert_eq!(
            transcribe.input_schema["required"],
            json!(["attachment_ref", "agent_id"])
        );
        let index = defs
            .iter()
            .find(|d| d.name == "dakera_attachment_index_image")
            .unwrap();
        assert!(index.input_schema["properties"]["content"].is_object());
        assert!(transcribe.input_schema["properties"]["content"].is_null());
    }

    #[test]
    fn namespace_defaults_to_the_agents_own() {
        let ns = resolve_namespace(&json!({ "agent_id": "bot" })).unwrap();
        assert_eq!(ns, "_dakera_agent_bot");
        let ns = resolve_namespace(&json!({ "namespace": "uploads", "agent_id": "bot" }));
        assert_eq!(ns.unwrap(), "uploads");
    }

    #[test]
    fn namespace_or_agent_is_required() {
        let err = resolve_namespace(&json!({})).unwrap_err();
        assert_eq!(err.is_error, Some(true));
        assert!(err.content[0].text.contains("agent_id"));
    }

    #[test]
    fn paths_encode_the_namespace_and_the_reference() {
        assert_eq!(attachments_path("a b"), "/v1/namespaces/a%20b/attachments");
        let path = attachment_path("uploads", "sha256:abc");
        assert_eq!(path, "/v1/namespaces/uploads/attachments/sha256%3Aabc");
    }

    #[test]
    fn job_body_forwards_only_what_was_given() {
        let args = json!({
            "tags": ["voice"], "importance": 0.7, "lang": "de", "session_id": null,
            "wait_seconds": 30, "content": "ignored for transcribe"
        });
        let body = job_body(&args, "transcribe", "bot");
        assert_eq!(body["agent_id"], "bot");
        assert_eq!(body["tags"], json!(["voice"]));
        assert_eq!(body["importance"], json!(0.7));
        assert_eq!(body["lang"], "de");
        assert!(body.get("session_id").is_none());
        assert!(body.get("wait_seconds").is_none());
        assert!(body.get("content").is_none());
    }

    #[test]
    fn index_job_body_carries_the_caption() {
        let body = job_body(&json!({ "content": "page 3" }), "index", "bot");
        assert_eq!(body["content"], "page 3");
    }

    #[test]
    fn media_types_by_extension() {
        assert_eq!(guess_content_type("a/note.WAV"), "audio/wav");
        assert_eq!(guess_content_type("page.png"), "image/png");
        assert_eq!(guess_content_type("README"), "application/octet-stream");
    }

    #[test]
    fn text_media_types() {
        assert!(is_text("text/plain; charset=utf-8"));
        assert!(is_text("application/json"));
        assert!(!is_text("audio/wav"));
    }

    #[tokio::test]
    async fn unknown_tool_returns_none() {
        assert!(execute(&dummy_client(), "dakera_store", &json!({}))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn upload_needs_a_namespace_or_agent() {
        let args = json!({ "text": "hello" });
        let result = execute(&dummy_client(), "dakera_attachment_upload", &args).await;
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("agent_id"));
    }

    #[tokio::test]
    async fn upload_refuses_both_file_and_text() {
        let args = json!({ "agent_id": "bot", "text": "x", "file_path": "/tmp/x" });
        let result = execute(&dummy_client(), "dakera_attachment_upload", &args).await;
        assert!(result.unwrap().content[0].text.contains("not both"));
    }

    #[tokio::test]
    async fn upload_needs_a_source() {
        let args = json!({ "agent_id": "bot" });
        let result = execute(&dummy_client(), "dakera_attachment_upload", &args).await;
        assert!(result.unwrap().content[0].text.contains("file_path"));
    }

    #[tokio::test]
    async fn upload_of_a_missing_file_is_a_clear_error() {
        let args = json!({ "agent_id": "bot", "file_path": "/nonexistent/dakera/x.wav" });
        let result = execute(&dummy_client(), "dakera_attachment_upload", &args).await;
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Cannot read"));
    }

    #[tokio::test]
    async fn job_kind_is_validated() {
        let args = json!({
            "agent_id": "bot", "attachment_ref": "sha256:a", "job_id": "j", "kind": "bogus"
        });
        let result = execute(&dummy_client(), "dakera_attachment_job", &args).await;
        assert!(result.unwrap().content[0].text.contains("kind must be"));
    }

    #[tokio::test]
    async fn transcribe_requires_the_agent_id() {
        let args = json!({ "attachment_ref": "sha256:a" });
        let result = execute(&dummy_client(), "dakera_attachment_transcribe", &args).await;
        assert!(result.unwrap().content[0].text.contains("agent_id"));
    }
}
