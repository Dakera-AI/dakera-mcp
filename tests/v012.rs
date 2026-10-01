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
    let hidden = listed.iter().all(|t| {
        !t["name"]
            .as_str()
            .unwrap()
            .starts_with("dakera_attachment_")
    });
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
    let out = json_of(&result);
    assert_eq!(out["job"]["status"], "Completed");
    assert_eq!(out["job"]["message"], "memory mem_1 stored");
    // The 202's identifiers come back with the finished job.
    assert_eq!(out["job_id"], "job_1_0");
    assert_eq!(out["memory_id"], "mem_1");
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
    assert_eq!(json_of(&result)["job"]["status"], "Completed");
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
    assert!(text.contains("job_1_0"));
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

    // The wait is capped below the 30 s HTTP timeout: a rotation is never re-sent.
    let args = json!({"namespace": "team-a", "wait_secs": 30});
    execute_tool(&client, "dakera_encryption_rotate_key", &args).await;
    let sent = server.last("/admin/encryption/rotate-key").json();
    assert_eq!(sent, json!({"namespace": "team-a", "wait_secs": 20}));
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

// ---------------------------------------------------------------------------
// Live contract sweep against ghcr.io/dakera-ai/dakera:0.12.0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graph_traverse_agent_only_queries_the_whole_agent_graph() {
    // KgQueryParams.root_id is optional; agent_id alone used to fail with
    // "Missing required parameter: memory_id".
    let server = MockServer::start(|_| Reply::json(200, json!({"edges": []}))).await;
    let args = json!({"agent_id": "a b", "edge_type": "linked_by", "limit": 5});
    let r = execute_tool(&server.client(), "dakera_graph_traverse", &args).await;
    assert_ne!(r.is_error, Some(true), "{}", r.content[0].text);
    let paths = server.paths();
    assert_eq!(
        paths.last().unwrap(),
        "GET /v1/knowledge/query?agent_id=a%20b&edge_type=linked_by&limit=5"
    );
}

#[tokio::test]
async fn graph_traverse_with_root_sends_root_and_depth() {
    let server = MockServer::start(|_| Reply::json(200, json!({"edges": []}))).await;
    let args = json!({"agent_id": "a", "root_id": "m1", "depth": 9});
    execute_tool(&server.client(), "dakera_graph_traverse", &args).await;
    assert_eq!(
        server.paths().last().unwrap(),
        "GET /v1/knowledge/query?agent_id=a&root_id=m1&max_depth=5"
    );
}

#[tokio::test]
async fn graph_traverse_memory_anchored_with_agent_uses_the_memory_graph() {
    let server = MockServer::start(|_| Reply::json(200, json!({"nodes": []}))).await;
    let args = json!({"agent_id": "a", "memory_id": "m1", "depth": 2});
    execute_tool(&server.client(), "dakera_graph_traverse", &args).await;
    assert_eq!(
        server.paths().last().unwrap(),
        "GET /v1/memories/m1/graph?depth=2"
    );
}

#[tokio::test]
async fn tif_evaluate_requires_agent_id_and_sends_it() {
    let server = MockServer::start(|_| Reply::json(200, json!({"entries": []}))).await;
    let client = server.client();
    let r = execute_tool(&client, "dakera_tif_evaluate", &json!({"memory_id": "m1"})).await;
    assert_eq!(r.is_error, Some(true));
    assert!(r.content[0].text.contains("agent_id"));
    assert!(server.paths().is_empty(), "no request without agent_id");

    let args = json!({"memory_id": "m1", "agent_id": "a"});
    let r = execute_tool(&client, "dakera_tif_evaluate", &args).await;
    assert_ne!(r.is_error, Some(true), "{}", r.content[0].text);
    assert_eq!(
        server.paths().last().unwrap(),
        "GET /v1/memories/m1/feedback?agent_id=a"
    );
}

#[tokio::test]
async fn extract_sends_agent_id_and_no_entity_types() {
    // POST /v1/extract has no entity_types field; it was silently dropped.
    let server = MockServer::start(|_| Reply::json(200, json!({"entities": []}))).await;
    let args = json!({"text": "t", "agent_id": "a", "entity_types": ["person"]});
    execute_tool(&server.client(), "dakera_extract", &args).await;
    let sent = server.last("/v1/extract").json();
    assert_eq!(sent, json!({"text": "t", "agent_id": "a"}));
}

#[tokio::test]
async fn extract_schema_offers_no_entity_types() {
    let server = MockServer::start(|_| Reply::json(200, json!({}))).await;
    let defs = listed_definitions(&server.client(), "all").await;
    let extract = defs.iter().find(|d| d.name == "dakera_extract").unwrap();
    let props = &extract.input_schema["properties"];
    assert!(props.get("entity_types").is_none());
    assert!(props.get("agent_id").is_some());
    let tif = defs
        .iter()
        .find(|d| d.name == "dakera_tif_evaluate")
        .unwrap();
    assert_eq!(
        tif.input_schema["required"],
        json!(["memory_id", "agent_id"])
    );
}

// ---------------------------------------------------------------------------
// Audit fixes (PR #155 review)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_forget_never_sends_an_empty_filter() {
    let server = MockServer::start(|_| Reply::json(200, json!({"deleted_count": 0}))).await;
    let client = server.client();
    // `tags: []` used to reach the server, which counts it as a filter that
    // matches every memory: the whole agent was deleted.
    for args in [
        json!({"agent_id": "a"}),
        json!({"agent_id": "a", "tags": []}),
        json!({"agent_id": "a", "tags": [], "memory_type": null}),
    ] {
        let result = execute_tool(&client, "dakera_batch_forget", &args).await;
        assert_eq!(result.is_error, Some(true), "{args}");
        assert!(text_of(&result).contains("nothing was deleted"));
    }
    assert!(server.requests().is_empty(), "{:?}", server.paths());

    let args = json!({"agent_id": "a", "tags": [], "max_importance": 0.2});
    execute_tool(&client, "dakera_batch_forget", &args).await;
    let sent = server.last("/v1/memories/forget/batch");
    assert_eq!(sent.method, "DELETE");
    assert_eq!(sent.json()["filter"], json!({"max_importance": 0.2}));
}

#[tokio::test]
async fn batch_recall_drops_empty_filters_and_sends_limit() {
    let server = MockServer::start(|_| Reply::json(200, json!({"memories": []}))).await;
    let args = json!({"agent_id": "a", "tags": [], "session_id": null, "limit": 7});
    execute_tool(&server.client(), "dakera_batch_recall", &args).await;
    let sent = server.last("/v1/memories/recall/batch").json();
    assert_eq!(sent, json!({"agent_id": "a", "filter": {}, "limit": 7}));
}

#[tokio::test]
async fn store_and_recall_forward_the_new_optional_fields_only_when_given() {
    let server = MockServer::start(|_| Reply::json(200, json!({}))).await;
    let client = server.client();
    let args = json!({"agent_id": "a", "content": "c", "ttl_seconds": 60, "metadata": {"k": 1}});
    execute_tool(&client, "dakera_store", &args).await;
    let sent = server.last("/v1/memory/store").json();
    assert_eq!(sent["ttl_seconds"], 60);
    assert_eq!(sent["metadata"], json!({"k": 1}));
    assert!(sent.get("expires_at").is_none());

    let plain = json!({"agent_id": "a", "query": "q"});
    execute_tool(&client, "dakera_recall", &plain).await;
    let sent = server.last("/v1/memory/recall").json();
    for field in ["tags", "memory_type", "session_id", "lang"] {
        assert!(sent.get(field).is_none(), "{field} sent: {sent}");
    }
    let filtered = json!({"agent_id": "a", "query": "q", "tags": ["x"], "memory_type": "semantic", "session_id": "s1"});
    execute_tool(&client, "dakera_recall", &filtered).await;
    let sent = server.last("/v1/memory/recall").json();
    assert_eq!(sent["tags"], json!(["x"]));
    assert_eq!(sent["memory_type"], "semantic");
    assert_eq!(sent["session_id"], "s1");
}

#[tokio::test]
async fn import_lets_the_server_detect_the_format() {
    let server = MockServer::start(|_| Reply::json(200, json!({"imported": 1}))).await;
    let args = json!({"agent_id": "a", "data": "content,importance\nhello,0.5\n"});
    execute_tool(&server.client(), "dakera_memory_import", &args).await;
    let sent = server.last("/v1/import?agent_id=a");
    let body = String::from_utf8_lossy(&sent.body).to_string();
    assert!(body.contains("filename=\"import\""), "{body}");
    assert!(!body.contains("import.jsonl"));
}

#[tokio::test]
async fn memory_policy_set_drops_the_read_only_count_and_uses_the_server_enum() {
    let server = MockServer::start(|req| {
        if req.method == "GET" {
            return Reply::json(
                200,
                json!({"working_decay": "exponential", "consolidated_count": 3, "dedup_on_store": false}),
            );
        }
        Reply::json(200, req.json())
    })
    .await;
    let args = json!({"namespace": "ns", "working_decay": "step_function", "dedup_on_store": true});
    let result = execute_tool(&server.client(), "dakera_memory_policy_set", &args).await;
    assert!(result.is_error.is_none(), "{}", text_of(&result));
    let sent = server.last("/v1/namespaces/ns/memory_policy");
    assert_eq!(sent.method, "PUT");
    let body = sent.json();
    assert!(body.get("consolidated_count").is_none());
    assert_eq!(body["working_decay"], "step_function");
    assert_eq!(body["dedup_on_store"], true);

    let defs = dakera_mcp::tools::filtered_definitions("all");
    let policy = defs
        .iter()
        .find(|d| d.name == "dakera_memory_policy_set")
        .unwrap();
    let decay = &policy.input_schema["properties"]["working_decay"]["enum"];
    assert!(decay.as_array().unwrap().contains(&json!("step_function")));
    assert!(!decay.as_array().unwrap().contains(&json!("step")));
}

#[tokio::test]
async fn v012_only_status_tools_on_a_v011_server() {
    let server = MockServer::start(|_| Reply::empty(404)).await;
    let client = server.client();
    let defs = listed_definitions(&client, "admin").await;
    let listed = names(&defs);
    assert!(!listed.contains(&"dakera_encryption_status".to_string()));
    assert!(!listed.contains(&"dakera_embed_migration_status".to_string()));
    assert!(listed.contains(&"dakera_encryption_rotate_key".to_string()));
    for tool in ["dakera_encryption_status", "dakera_embed_migration_status"] {
        let result = execute_tool(&client, tool, &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("v0.12"), "{}", text_of(&result));
    }
    // Only the capability probe was sent: no call to the missing routes.
    assert!(
        server.paths().iter().all(|p| p == "GET /v1/capabilities"),
        "{:?}",
        server.paths()
    );
}

#[tokio::test]
async fn rotate_key_on_a_v011_server_says_new_key_is_needed() {
    let server = MockServer::start(|_| {
        Reply::bytes(
            422,
            "text/plain",
            b"Failed to deserialize the JSON body into the target type: missing field `new_key` at line 1 column 2",
        )
    })
    .await;
    let result = execute_tool(&server.client(), "dakera_encryption_rotate_key", &json!({})).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("Hint: Dakera servers before v0.12 require new_key"));
}

#[tokio::test]
async fn wake_up_reads_the_agents_startup_context() {
    let server = MockServer::start(|_| Reply::json(200, json!({"memories": []}))).await;
    let client = server.client();
    let args = json!({"agent_id": "bot", "top_n": 5, "min_importance": 0.3});
    let result = execute_tool(&client, "dakera_wake_up", &args).await;
    json_of(&result);
    let sent = server.last("/v1/agents/bot/wake-up?top_n=5&min_importance=0.3");
    assert_eq!(sent.method, "GET");
    execute_tool(&client, "dakera_wake_up", &json!({"agent_id": "bot"})).await;
    server.last("/v1/agents/bot/wake-up");
}

#[tokio::test]
async fn session_and_agent_lists_page() {
    let server = MockServer::start(|_| Reply::json(200, json!([]))).await;
    let client = server.client();
    let args = json!({"session_id": "s1", "limit": 100, "offset": 50});
    execute_tool(&client, "dakera_session_memories", &args).await;
    server.last("/v1/sessions/s1/memories?&limit=100&offset=50");
    let args = json!({"agent_id": "bot", "limit": 10});
    execute_tool(&client, "dakera_agent_sessions", &args).await;
    server.last("/v1/agents/bot/sessions?&limit=10");
    execute_tool(&client, "dakera_session_list", &args).await;
    server.last("/v1/sessions?agent_id=bot&active_only=false&limit=10");
}

#[tokio::test]
async fn a_rate_limit_is_not_called_a_busy_server() {
    let server = MockServer::start(|_| {
        Reply::json(
            429,
            json!({"error": "rate limit", "code": "RATE_LIMIT_EXCEEDED"}),
        )
        .with_header("Retry-After", "0")
    })
    .await;
    let args = json!({"agent_id": "a", "query": "q"});
    let result = execute_tool(&server.client(), "dakera_recall", &args).await;
    assert_eq!(result.is_error, Some(true));
    assert!(
        text_of(&result).contains("Rate limit reached"),
        "{}",
        text_of(&result)
    );
}

#[tokio::test]
async fn initialize_reports_the_crate_version() {
    let server = v012_server(false, false).await;
    let req: JsonRpcRequest =
        serde_json::from_value(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"})).unwrap();
    let resp = handle_request(&server.client(), &req).await;
    let info = &resp.result.unwrap()["serverInfo"];
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
}

// ---------------------------------------------------------------------------
// Method-aware route audit: every tool, called with arguments made from its
// own input schema, may only send requests the v0.12.0 router serves (method
// AND path). `tests/route_audit.rs` checks the path literals; this checks
// what is actually sent.
// ---------------------------------------------------------------------------

fn server_routes() -> Vec<(String, Vec<String>)> {
    include_str!("server_routes_v0.12.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_once(' '))
        .map(|(m, p)| (m.to_string(), p.split('/').map(str::to_string).collect()))
        .collect()
}

fn served(routes: &[(String, Vec<String>)], method: &str, target: &str) -> bool {
    let path = target.split('?').next().unwrap_or("");
    let segments: Vec<&str> = path.split('/').collect();
    routes.iter().any(|(m, pattern)| {
        m == method
            && pattern.len() == segments.len()
            && pattern
                .iter()
                .zip(&segments)
                .all(|(p, s)| p == "*" || p == s)
    })
}

/// A value of the shape `schema` asks for.
fn sample(schema: &Value) -> Value {
    if let Some(first) = schema.get("enum").and_then(|e| e.get(0)) {
        return first.clone();
    }
    let ty = match &schema["type"] {
        Value::Array(types) => types[0].as_str().unwrap_or("string").to_string(),
        other => other.as_str().unwrap_or("string").to_string(),
    };
    match ty.as_str() {
        "integer" => json!(1),
        "number" => json!(0.5),
        "boolean" => json!(true),
        "array" => json!([sample(&schema["items"])]),
        "object" => {
            let mut obj = serde_json::Map::new();
            let props = schema.get("properties").and_then(|p| p.as_object());
            let required = schema.get("required").and_then(|r| r.as_array());
            if let (Some(props), Some(required)) = (props, required) {
                for name in required.iter().filter_map(|n| n.as_str()) {
                    obj.insert(name.to_string(), sample(&props[name]));
                }
            }
            Value::Object(obj)
        }
        _ => json!("x"),
    }
}

#[tokio::test]
async fn every_tool_sends_only_methods_and_paths_the_v012_router_serves() {
    let server = MockServer::start(|req| {
        if req.path == "/v1/capabilities" {
            return Reply::json(200, caps(true, true));
        }
        if req.path.starts_with("/v1/export") || req.path.ends_with("/ops/metrics") {
            return Reply::bytes(200, "text/plain", b"x");
        }
        Reply::json(200, json!({}))
    })
    .await;
    let client = server.client();
    let routes = server_routes();
    // Tools whose schema has no required field naming what they act on.
    let extra: std::collections::HashMap<&str, Value> = [
        (
            "dakera_attachment_upload",
            json!({"agent_id": "x", "text": "t"}),
        ),
        ("dakera_attachment_list", json!({"agent_id": "x"})),
        (
            "dakera_attachment_download",
            json!({"agent_id": "x", "save_to": scratch("audit.bin").to_string_lossy()}),
        ),
        ("dakera_attachment_delete", json!({"agent_id": "x"})),
        ("dakera_attachment_job", json!({"agent_id": "x"})),
        ("dakera_batch_forget", json!({"tags": ["x"]})),
        (
            "dakera_knowledge_summarize",
            json!({"memory_ids": ["a", "b"]}),
        ),
        ("dakera_graph_traverse", json!({"agent_id": "x"})),
    ]
    .into_iter()
    .collect();
    // Tools that make no request to the Dakera API besides the capability probe.
    let local = [
        "dakera_discover_tools",
        "dakera_load_tools",
        "dakera_capabilities",
    ];

    let mut silent = Vec::new();
    let mut unserved = Vec::new();
    for def in dakera_mcp::tools::filtered_definitions("all") {
        // Calls the separate ODE sidecar (DAKERA_ODE_URL), not the Dakera API.
        if def.name == "dakera_extract_entities" {
            continue;
        }
        let mut args = sample(&def.input_schema);
        if let Some(Value::Object(more)) = extra.get(def.name.as_str()) {
            for (k, v) in more {
                args[k] = v.clone();
            }
        }
        let before = server.requests().len();
        execute_tool(&client, &def.name, &args).await;
        let sent: Vec<Seen> = server.requests().into_iter().skip(before).collect();
        let sent: Vec<Seen> = sent
            .into_iter()
            .filter(|r| r.path != "/v1/capabilities")
            .collect();
        if sent.is_empty() && !local.contains(&def.name.as_str()) {
            silent.push(def.name.clone());
        }
        for r in sent {
            if !served(&routes, &r.method, &r.path) {
                unserved.push(format!("{}: {} {}", def.name, r.method, r.path));
            }
        }
    }
    assert!(
        unserved.is_empty(),
        "requests the v0.12 router does not serve: {unserved:#?}"
    );
    assert!(
        silent.is_empty(),
        "tools that sent nothing with schema-made arguments: {silent:#?}"
    );
}

#[tokio::test]
async fn an_import_that_imported_nothing_is_an_error() {
    let server = MockServer::start(|_| {
        Reply::json(
            200,
            json!({"status": "failed", "imported": 0, "skipped": 1, "errors": ["bad id"]}),
        )
    })
    .await;
    let args = json!({"agent_id": "a", "data": "content\nx\n"});
    let result = execute_tool(&server.client(), "dakera_memory_import", &args).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("bad id"));
}

#[test]
fn every_request_is_answered_when_input_ends() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(env!("CARGO_BIN_EXE_dakera-mcp"))
        .env("DAKERA_API_URL", "http://127.0.0.1:9")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for (id, method) in [(1, "initialize"), (2, "tools/list"), (3, "ping")] {
        let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}});
        writeln!(stdin, "{req}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 3, "{text}");
}
