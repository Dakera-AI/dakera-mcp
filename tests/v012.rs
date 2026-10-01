//! Tests for the Dakera v0.12 support of dakera-mcp, against a tiny in-process
//! HTTP server (no Dakera server and no feature flag needed).
//!
//! Covered: `dakera_capabilities`, graceful disabling of the attachment tools
//! from `/v1/capabilities` (in `tools/list`, `dakera_discover_tools` and direct
//! calls), the attachment tools, `lang` / `attachment_ref` forwarding, the
//! encryption / health / embed-migration tools, `Retry-After` and error hints.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dakera_mcp::protocol::{CallToolResult, JsonRpcRequest};
use dakera_mcp::server::handle_request;
use dakera_mcp::tools::{execute_tool, listed_definitions, DakeraApiClient};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// ---------------------------------------------------------------------------
// A tiny HTTP server: records every request, answers with what a closure says.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<String> {
        let found = self.headers.iter().find(|(k, _)| k == name);
        found.map(|(_, v)| v.clone())
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn json(status: u16, value: Value) -> Self {
        Reply::bytes(status, "application/json", value.to_string().as_bytes())
    }

    fn bytes(status: u16, content_type: &str, body: &[u8]) -> Self {
        Reply {
            status,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body: body.to_vec(),
        }
    }

    fn empty(status: u16) -> Self {
        Reply {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

type Handler = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

struct MockServer {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl MockServer {
    async fn start<F>(handler: F) -> MockServer
    where
        F: Fn(&Seen) -> Reply + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let log = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => return,
                };
                let handler = Arc::clone(&handler);
                let log = Arc::clone(&log);
                tokio::spawn(async move {
                    serve(stream, handler, log).await;
                });
            }
        });
        MockServer {
            url: format!("http://{addr}"),
            seen,
        }
    }

    fn client(&self) -> DakeraApiClient {
        DakeraApiClient::new(self.url.clone(), None)
    }

    fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn paths(&self) -> Vec<String> {
        let all = self.requests();
        all.iter()
            .map(|r| format!("{} {}", r.method, r.path))
            .collect()
    }

    /// The last request to `path`.
    fn last(&self, path: &str) -> Seen {
        let all = self.requests();
        let found = all.iter().rev().find(|r| r.path == path);
        found
            .cloned()
            .unwrap_or_else(|| panic!("no request to {path}: {:?}", self.paths()))
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn serve(mut stream: TcpStream, handler: Handler, log: Arc<Mutex<Vec<Seen>>>) {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        let n = match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let length = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < length {
        let n = match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    let seen = Seen {
        method,
        path,
        headers,
        body,
    };
    log.lock().unwrap().push(seen.clone());
    let reply = handler(&seen);
    let status_line = format!("HTTP/1.1 {} Status\r\n", reply.status);
    let mut out = status_line.into_bytes();
    out.extend_from_slice(format!("Content-Length: {}\r\n", reply.body.len()).as_bytes());
    out.extend_from_slice(b"Connection: close\r\n");
    for (k, v) in &reply.headers {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&reply.body);
    let _ = stream.write_all(&out).await;
    let _ = stream.shutdown().await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn caps(attachments: bool, vision: bool) -> Value {
    json!({
        "capabilities_version": 1,
        "server_version": "0.12.0",
        "default_model": "bge-large",
        "attachments": {"enabled": attachments, "max_bytes": 26214400},
        "vision": {"enabled": vision},
        "records": {"enabled": false},
        "query_languages": ["en", "de", "fr"]
    })
}

/// A server with v0.12 capabilities and `ok` for everything else.
async fn v012_server(attachments: bool, vision: bool) -> MockServer {
    MockServer::start(move |req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(attachments, vision));
        }
        Reply::json(200, json!({}))
    })
    .await
}

fn text_of(result: &CallToolResult) -> String {
    result.content[0].text.clone()
}

fn json_of(result: &CallToolResult) -> Value {
    assert!(
        result.is_error.is_none(),
        "expected success, got: {}",
        text_of(result)
    );
    serde_json::from_str(&text_of(result)).unwrap()
}

fn names(defs: &[dakera_mcp::protocol::ToolDefinition]) -> Vec<String> {
    defs.iter().map(|d| d.name.clone()).collect()
}

fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dakera-mcp-v012-{}-{name}", std::process::id()))
}

// ---------------------------------------------------------------------------
// dakera_capabilities and graceful disabling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn capabilities_tool_returns_the_document() {
    let server = v012_server(true, false).await;
    let result = execute_tool(&server.client(), "dakera_capabilities", &json!({})).await;
    let doc = json_of(&result);
    assert_eq!(doc["server_version"], "0.12.0");
    assert_eq!(doc["attachments"]["enabled"], true);
}

#[tokio::test]
async fn capabilities_tool_on_a_v011_server_says_it_is_unavailable() {
    let server = MockServer::start(|_| Reply::empty(404)).await;
    let result = execute_tool(&server.client(), "dakera_capabilities", &json!({})).await;
    let doc = json_of(&result);
    assert_eq!(doc["capabilities_available"], false);
    assert!(doc["reason"].as_str().unwrap().contains("v0.12"));
}

#[tokio::test]
async fn capabilities_tool_surfaces_a_refused_key() {
    let server = MockServer::start(|_| {
        Reply::json(
            401,
            json!({"error": "authentication_error", "code": "INVALID_API_KEY"}),
        )
    })
    .await;
    let result = execute_tool(&server.client(), "dakera_capabilities", &json!({})).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("DAKERA_API_KEY"));
}

#[tokio::test]
async fn tools_list_hides_attachment_tools_when_the_feature_is_off() {
    let server = v012_server(false, false).await;
    let defs = listed_definitions(&server.client(), "all").await;
    let names = names(&defs);
    assert!(names.contains(&"dakera_store".to_string()));
    assert!(names.contains(&"dakera_capabilities".to_string()));
    assert!(!names.iter().any(|n| n.starts_with("dakera_attachment_")));
}

#[tokio::test]
async fn tools_list_offers_attachments_but_not_image_indexing_without_vision() {
    let server = v012_server(true, false).await;
    let defs = listed_definitions(&server.client(), "all").await;
    let names = names(&defs);
    assert!(names.contains(&"dakera_attachment_upload".to_string()));
    assert!(names.contains(&"dakera_attachment_transcribe".to_string()));
    assert!(!names.contains(&"dakera_attachment_index_image".to_string()));
}

#[tokio::test]
async fn tools_list_offers_everything_with_attachments_and_vision() {
    let server = v012_server(true, true).await;
    let defs = listed_definitions(&server.client(), "all").await;
    assert!(names(&defs).contains(&"dakera_attachment_index_image".to_string()));
}

#[tokio::test]
async fn tools_list_on_a_v011_server_hides_every_opt_in_tool() {
    let server = MockServer::start(|_| Reply::empty(404)).await;
    let defs = listed_definitions(&server.client(), "all").await;
    let names = names(&defs);
    assert!(!names.iter().any(|n| n.starts_with("dakera_attachment_")));
    assert!(names.contains(&"dakera_recall".to_string()));
}

#[tokio::test]
async fn tools_list_keeps_everything_when_the_server_cannot_be_asked() {
    let server = MockServer::start(|_| Reply::json(500, json!({"error": "boom"}))).await;
    let defs = listed_definitions(&server.client(), "all").await;
    assert!(names(&defs).contains(&"dakera_attachment_upload".to_string()));
}

#[tokio::test]
async fn the_core_profile_does_not_ask_the_server() {
    let server = v012_server(false, false).await;
    let defs = listed_definitions(&server.client(), "core").await;
    assert_eq!(defs.len(), 14);
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn tools_list_over_json_rpc_follows_the_capabilities() {
    let server = v012_server(false, false).await;
    let request: JsonRpcRequest = serde_json::from_value(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {"profile": "all"}
    }))
    .unwrap();
    let response = handle_request(&server.client(), &request).await;
    let tools = response.result.unwrap()["tools"].clone();
    let listed: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(listed.contains(&"dakera_store"));
    assert!(!listed.contains(&"dakera_attachment_upload"));
}

#[tokio::test]
async fn capabilities_are_remembered_between_calls() {
    let server = v012_server(true, true).await;
    let client = server.client();
    let args = json!({"agent_id": "bot"});
    execute_tool(&client, "dakera_attachment_list", &args).await;
    execute_tool(&client, "dakera_attachment_list", &args).await;
    let probes = server
        .paths()
        .iter()
        .filter(|p| p.as_str() == "GET /v1/capabilities")
        .count();
    assert_eq!(probes, 1);
}

#[tokio::test]
async fn discover_tools_leaves_out_what_the_server_cannot_serve() {
    let server = v012_server(false, false).await;
    let args = json!({"query": "dakera_attachment_"});
    let result = execute_tool(&server.client(), "dakera_discover_tools", &args).await;
    let found = json_of(&result);
    let listed = found["tools"].as_array().unwrap();
    let hidden = listed
        .iter()
        .all(|t| !t["name"].as_str().unwrap().starts_with("dakera_attachment_"));
    assert!(hidden);

    let server = v012_server(true, true).await;
    let result = execute_tool(&server.client(), "dakera_discover_tools", &args).await;
    let found = json_of(&result);
    assert!(found["count"].as_u64().unwrap() >= 7);
}

#[tokio::test]
async fn a_disabled_feature_answers_with_the_variable_and_makes_no_attachment_call() {
    let server = v012_server(false, false).await;
    let args = json!({"agent_id": "bot"});
    let result = execute_tool(&server.client(), "dakera_attachment_list", &args).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("DAKERA_ATTACHMENTS"));
    assert_eq!(server.paths(), vec!["GET /v1/capabilities".to_string()]);
}

#[tokio::test]
async fn image_indexing_names_the_vision_variable() {
    let server = v012_server(true, false).await;
    let args = json!({"agent_id": "bot", "attachment_ref": "sha256:a"});
    let result = execute_tool(&server.client(), "dakera_attachment_index_image", &args).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("DAKERA_VISION"));
}

#[tokio::test]
async fn a_v011_server_says_attachments_need_v012() {
    let server = MockServer::start(|_| Reply::empty(404)).await;
    let args = json!({"agent_id": "bot"});
    let result = execute_tool(&server.client(), "dakera_attachment_list", &args).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("v0.12"));
}

// ---------------------------------------------------------------------------
// attachment tools
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upload_text_goes_to_the_agents_namespace() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        let created = json!({
            "attachment_ref": "sha256:abc", "content_type": "text/plain",
            "size_bytes": 5, "created": true
        });
        Reply::json(201, created)
    })
    .await;
    let args = json!({"agent_id": "bot", "text": "hello"});
    let result = execute_tool(&server.client(), "dakera_attachment_upload", &args).await;
    assert_eq!(json_of(&result)["attachment_ref"], "sha256:abc");
    let seen = server.last("/v1/namespaces/_dakera_agent_bot/attachments");
    assert_eq!(seen.method, "POST");
    assert_eq!(seen.header("content-type").unwrap(), "text/plain");
    assert_eq!(seen.body, b"hello");
}

#[tokio::test]
async fn upload_file_guesses_the_media_type() {
    let server = v012_server(true, false).await;
    let file = scratch("note.wav");
    std::fs::write(&file, b"RIFFfake").unwrap();
    let args = json!({"namespace": "uploads", "file_path": file.to_str().unwrap()});
    let result = execute_tool(&server.client(), "dakera_attachment_upload", &args).await;
    assert!(result.is_error.is_none(), "{}", text_of(&result));
    let seen = server.last("/v1/namespaces/uploads/attachments");
    assert_eq!(seen.header("content-type").unwrap(), "audio/wav");
    assert_eq!(seen.body, b"RIFFfake");
    let _ = std::fs::remove_file(&file);
}

#[tokio::test]
async fn upload_over_the_limit_carries_a_hint() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        Reply::json(
            413,
            json!({"error": "too big", "code": "PAYLOAD_TOO_LARGE", "status": 413}),
        )
    })
    .await;
    let args = json!({"agent_id": "bot", "text": "x"});
    let result = execute_tool(&server.client(), "dakera_attachment_upload", &args).await;
    assert_eq!(result.is_error, Some(true));
    let text = text_of(&result);
    assert!(text.contains("PAYLOAD_TOO_LARGE"));
    assert!(text.contains("Hint:"));
    assert!(text.contains("attachments.max_bytes"));
}

#[tokio::test]
async fn a_501_from_the_server_carries_the_feature_hint() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        let body = json!({
            "error": "attachments are disabled", "code": "FEATURE_DISABLED",
            "details": "set DAKERA_ATTACHMENTS=1"
        });
        Reply::json(501, body)
    })
    .await;
    let args = json!({"agent_id": "bot"});
    let result = execute_tool(&server.client(), "dakera_attachment_list", &args).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("switched off"));
}

#[tokio::test]
async fn list_reads_the_namespace() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        let listing = json!({"attachments": [{"attachment_ref": "sha256:abc"}]});
        Reply::json(200, listing)
    })
    .await;
    let args = json!({"namespace": "uploads"});
    let result = execute_tool(&server.client(), "dakera_attachment_list", &args).await;
    let doc = json_of(&result);
    assert_eq!(doc["attachments"][0]["attachment_ref"], "sha256:abc");
    let seen = server.last("/v1/namespaces/uploads/attachments");
    assert_eq!(seen.method, "GET");
}

#[tokio::test]
async fn download_returns_small_text_inline() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        Reply::bytes(200, "text/plain; charset=utf-8", b"hello there")
    })
    .await;
    let args = json!({"agent_id": "bot", "attachment_ref": "sha256:abc"});
    let result = execute_tool(&server.client(), "dakera_attachment_download", &args).await;
    let doc = json_of(&result);
    assert_eq!(doc["text"], "hello there");
    assert_eq!(doc["size_bytes"], 11);
    let seen = server.last("/v1/namespaces/_dakera_agent_bot/attachments/sha256%3Aabc");
    assert_eq!(seen.method, "GET");
}

#[tokio::test]
async fn download_binary_needs_save_to_and_then_writes_the_file() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        Reply::bytes(200, "audio/wav", b"RIFFfake")
    })
    .await;
    let args = json!({"namespace": "uploads", "attachment_ref": "sha256:abc"});
    let refused = execute_tool(&server.client(), "dakera_attachment_download", &args).await;
    assert_eq!(refused.is_error, Some(true));
    assert!(text_of(&refused).contains("save_to"));

    let out = scratch("download.wav");
    let args = json!({
        "namespace": "uploads", "attachment_ref": "sha256:abc", "save_to": out.to_str().unwrap()
    });
    let saved = execute_tool(&server.client(), "dakera_attachment_download", &args).await;
    let doc = json_of(&saved);
    assert_eq!(doc["size_bytes"], 8);
    assert_eq!(std::fs::read(&out).unwrap(), b"RIFFfake");
    let _ = std::fs::remove_file(&out);
}

#[tokio::test]
async fn delete_succeeds_on_204_and_reports_a_conflict() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        if req.path.ends_with("sha256%3Abusy") {
            let body = json!({"error": "referenced by 1 memory", "code": "CONFLICT"});
            return Reply::json(409, body);
        }
        Reply::empty(204)
    })
    .await;
    let ok_args = json!({"agent_id": "bot", "attachment_ref": "sha256:abc"});
    let ok = execute_tool(&server.client(), "dakera_attachment_delete", &ok_args).await;
    assert_eq!(json_of(&ok)["deleted"], "sha256:abc");
    let busy_args = json!({"agent_id": "bot", "attachment_ref": "sha256:busy"});
    let busy = execute_tool(&server.client(), "dakera_attachment_delete", &busy_args).await;
    assert_eq!(busy.is_error, Some(true));
    assert!(text_of(&busy).contains("referenced by 1 memory"));
}

fn accepted() -> Value {
    json!({
        "job_id": "job_1_0", "attachment_ref": "sha256:abc", "agent_id": "bot",
        "memory_id": "mem_1", "model": "whisper-tiny.en",
        "status_url": "/v1/namespaces/uploads/attachments/sha256:abc/transcribe/job_1_0"
    })
}

#[tokio::test]
async fn transcribe_posts_the_memory_fields_and_returns_the_job() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        Reply::json(202, accepted())
    })
    .await;
    let args = json!({
        "namespace": "uploads", "attachment_ref": "sha256:abc", "agent_id": "bot",
        "tags": ["voice"], "importance": 0.7, "lang": "de", "wait_seconds": 0
    });
    let result = execute_tool(&server.client(), "dakera_attachment_transcribe", &args).await;
    assert_eq!(json_of(&result)["job_id"], "job_1_0");
    let seen = server.last("/v1/namespaces/uploads/attachments/sha256%3Aabc/transcribe");
    assert_eq!(seen.method, "POST");
    let expected = json!({"agent_id": "bot", "tags": ["voice"], "importance": 0.7, "lang": "de"});
    assert_eq!(seen.json(), expected);
}

#[tokio::test]
async fn transcribe_with_wait_returns_the_completed_job() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        if req.method == "POST" {
            return Reply::json(202, accepted());
        }
        Reply::json(
            200,
            json!({"id": "job_1_0", "status": "Completed", "progress": 100,
                   "message": "memory mem_1 stored"}),
        )
    })
    .await;
    let args = json!({
        "namespace": "uploads", "attachment_ref": "sha256:abc", "agent_id": "bot",
        "wait_seconds": 10
    });
    let result = execute_tool(&server.client(), "dakera_attachment_transcribe", &args).await;
    let job = json_of(&result);
    assert_eq!(job["status"], "Completed");
    assert_eq!(job["message"], "memory mem_1 stored");
}

#[tokio::test]
async fn transcribe_with_wait_polls_a_running_job() {
    let polls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&polls);
    let server = MockServer::start(move |req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        if req.method == "POST" {
            return Reply::json(202, accepted());
        }
        let n = counter.fetch_add(1, Ordering::SeqCst);
        let status = if n == 0 { "Running" } else { "Completed" };
        let job = json!({"id": "job_1_0", "status": status, "progress": 50});
        Reply::json(200, job)
    })
    .await;
    let args = json!({
        "namespace": "uploads", "attachment_ref": "sha256:abc", "agent_id": "bot",
        "wait_seconds": 20
    });
    let result = execute_tool(&server.client(), "dakera_attachment_transcribe", &args).await;
    assert_eq!(json_of(&result)["status"], "Completed");
    assert_eq!(polls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_failed_job_is_an_error_result_with_the_status_and_code() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, false));
        }
        if req.method == "POST" {
            return Reply::json(202, accepted());
        }
        Reply::json(
            200,
            json!({"id": "job_1_0", "status": "Failed", "progress": 5,
                   "message": "the audio holds no speech",
                   "error": {"status": 400, "code": "INVALID_REQUEST"}}),
        )
    })
    .await;
    let args = json!({
        "namespace": "uploads", "attachment_ref": "sha256:abc", "agent_id": "bot",
        "wait_seconds": 10
    });
    let result = execute_tool(&server.client(), "dakera_attachment_transcribe", &args).await;
    assert_eq!(result.is_error, Some(true));
    let text = text_of(&result);
    assert!(text.contains("holds no speech"));
    assert!(text.contains("INVALID_REQUEST"));
}

#[tokio::test]
async fn index_image_sends_the_caption_and_job_reads_the_index_route() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, true));
        }
        if req.method == "POST" {
            return Reply::json(202, json!({"job_id": "job_2_0"}));
        }
        let job = json!({"id": "job_2_0", "status": "Running", "progress": 10});
        Reply::json(200, job)
    })
    .await;
    let client = server.client();
    let args = json!({
        "namespace": "uploads", "attachment_ref": "sha256:abc", "agent_id": "bot",
        "content": "page 3"
    });
    let started = execute_tool(&client, "dakera_attachment_index_image", &args).await;
    assert_eq!(json_of(&started)["job_id"], "job_2_0");
    let seen = server.last("/v1/namespaces/uploads/attachments/sha256%3Aabc/index");
    assert_eq!(seen.json(), json!({"agent_id": "bot", "content": "page 3"}));

    let args = json!({
        "namespace": "uploads", "attachment_ref": "sha256:abc", "job_id": "job_2_0",
        "kind": "index"
    });
    let status = execute_tool(&client, "dakera_attachment_job", &args).await;
    assert_eq!(json_of(&status)["status"], "Running");
    server.last("/v1/namespaces/uploads/attachments/sha256%3Aabc/index/job_2_0");
}

// ---------------------------------------------------------------------------
// lang and attachment_ref
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lang_and_attachment_ref_are_forwarded_only_when_given() {
    let server = MockServer::start(|_| Reply::json(200, json!({}))).await;
    let client = server.client();

    let plain_args = json!({"agent_id": "a", "content": "c"});
    execute_tool(&client, "dakera_store", &plain_args).await;
    let plain = server.last("/v1/memory/store").json();
    assert!(plain.get("lang").is_none());
    assert!(plain.get("attachment_ref").is_none());

    let args = json!({
        "agent_id": "a", "content": "c", "lang": "de", "attachment_ref": "sha256:abc"
    });
    execute_tool(&client, "dakera_store", &args).await;
    let sent = server.last("/v1/memory/store").json();
    assert_eq!(sent["lang"], "de");
    assert_eq!(sent["attachment_ref"], "sha256:abc");
}

#[tokio::test]
async fn lang_is_forwarded_by_recall_and_recall_associated() {
    let server = MockServer::start(|_| Reply::json(200, json!({}))).await;
    let client = server.client();

    let recall = json!({"agent_id": "a", "query": "q", "lang": "fr"});
    execute_tool(&client, "dakera_recall", &recall).await;
    let sent = server.last("/v1/memory/recall").json();
    assert_eq!(sent["lang"], "fr");

    let associated = json!({"agent_id": "a", "query": "q", "lang": "es"});
    execute_tool(&client, "dakera_recall_associated", &associated).await;
    let sent = server.last("/v1/memory/recall").json();
    assert_eq!(sent["lang"], "es");
}

#[tokio::test]
async fn lang_is_forwarded_by_search_update_and_extract() {
    let server = MockServer::start(|_| Reply::json(200, json!({}))).await;
    let client = server.client();

    let search = json!({"agent_id": "a", "query": "q", "lang": "it"});
    execute_tool(&client, "dakera_search", &search).await;
    let sent = server.last("/v1/memory/search").json();
    assert_eq!(sent["lang"], "it");

    let update = json!({"memory_id": "m1", "agent_id": "a", "content": "c", "lang": "pt"});
    execute_tool(&client, "dakera_memory_update", &update).await;
    let sent = server.last("/v1/memory/update/m1?agent_id=a").json();
    assert_eq!(sent["lang"], "pt");

    let extract = json!({"text": "t", "lang": "nl"});
    execute_tool(&client, "dakera_extract", &extract).await;
    let sent = server.last("/v1/extract").json();
    assert_eq!(sent["lang"], "nl");
}

#[tokio::test]
async fn recall_without_lang_sends_none() {
    let server = MockServer::start(|_| Reply::json(200, json!({}))).await;
    let args = json!({"agent_id": "a", "query": "q"});
    execute_tool(&server.client(), "dakera_recall", &args).await;
    let sent = server.last("/v1/memory/recall").json();
    assert!(sent.get("lang").is_none());
}

// ---------------------------------------------------------------------------
// encryption, health, embed migration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rotate_key_sends_only_what_was_given() {
    let server = MockServer::start(|_| Reply::json(200, json!({"key_id": "k2"}))).await;
    let client = server.client();

    execute_tool(&client, "dakera_encryption_rotate_key", &json!({})).await;
    let sent = server.last("/admin/encryption/rotate-key").json();
    assert_eq!(sent, json!({}));

    let args = json!({"namespace": "team-a", "wait_secs": 30});
    execute_tool(&client, "dakera_encryption_rotate_key", &args).await;
    let sent = server.last("/admin/encryption/rotate-key").json();
    assert_eq!(sent, json!({"namespace": "team-a", "wait_secs": 30}));
}

#[tokio::test]
async fn status_tools_read_their_routes() {
    let server = MockServer::start(|_| Reply::json(200, json!({"ok": true}))).await;
    let client = server.client();
    for (tool, path) in [
        ("dakera_encryption_status", "/admin/encryption/status"),
        ("dakera_embed_migration_status", "/admin/reembed/migration"),
        ("dakera_health", "/health"),
    ] {
        let result = execute_tool(&client, tool, &json!({})).await;
        assert_eq!(json_of(&result)["ok"], true, "{tool}");
        assert_eq!(server.last(path).method, "GET");
    }
}

#[tokio::test]
async fn a_pinned_key_gets_the_node_wide_hint() {
    let server = MockServer::start(|_| {
        Reply::json(
            403,
            json!({"error": "Access denied to namespace", "code": "NAMESPACE_ACCESS_DENIED",
                   "status": 403, "details": "namespace: *"}),
        )
    })
    .await;
    let result = execute_tool(&server.client(), "dakera_encryption_status", &json!({})).await;
    assert_eq!(result.is_error, Some(true));
    let text = text_of(&result);
    assert!(text.contains("NAMESPACE_ACCESS_DENIED"));
    assert!(text.contains("Hint: This API key is pinned to namespaces"));
}

#[tokio::test]
async fn a_503_with_retry_after_is_retried_after_that_long() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let server = MockServer::start(move |_| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            let starting = json!({"service": "dakera", "status": "starting"});
            return Reply::json(503, starting).with_header("Retry-After", "1");
        }
        Reply::json(200, json!({"service": "dakera", "status": "healthy"}))
    })
    .await;
    let started = Instant::now();
    let result = execute_tool(&server.client(), "dakera_health", &json!({})).await;
    assert_eq!(json_of(&result)["status"], "healthy");
    assert!(started.elapsed() >= Duration::from_millis(900));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_persistent_503_ends_with_the_retry_hint() {
    let server = MockServer::start(|_| {
        let body = json!({"error": "busy", "code": "SERVICE_UNAVAILABLE", "status": 503});
        Reply::json(503, body).with_header("Retry-After", "0")
    })
    .await;
    let client = server.client();
    let result = execute_tool(&client, "dakera_embed_migration_status", &json!({})).await;
    assert_eq!(result.is_error, Some(true));
    let text = text_of(&result);
    assert!(text.contains("503"));
    assert!(text.contains("Hint: The server is busy or starting; retry in 0s"));
}
