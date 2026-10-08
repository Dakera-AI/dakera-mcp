//! Dakera MCP tool definitions and execution
//!
//! Defines the tools exposed by the Dakera MCP server and handles
//! calling the Dakera API for each tool invocation.

pub mod agents;
pub mod attachments;
pub mod audit;
pub mod autopilot;
pub mod capabilities;
pub mod decay;
pub mod discovery;
pub mod encryption;
pub mod entities;
pub mod extractor;
pub mod feedback;
pub mod fulltext;
pub mod graph;
pub mod health;
pub mod hints;
pub mod identity;
pub mod inference;
pub mod knowledge;
pub mod memory;
pub mod namespace_keys;
pub mod namespaces;
pub mod ode;
pub mod ops;
pub mod sessions;
pub mod tif;
pub mod transfer;
pub mod vectors;

use crate::protocol::{CallToolResult, ToolDefinition, ToolTier};

/// A tool definition paired with its tier classification (MCP-8).
pub struct ToolCatalogEntry {
    pub tier: ToolTier,
    pub def: ToolDefinition,
}

/// Assign a tier to a tool by name.
///
/// Core (12): high-frequency recall/store/search tools always exposed by default.
/// Admin: namespace management, encryption, bulk ops, audit — management-plane.
/// Meta: discovery tools themselves.
/// Power: everything else (advanced but not surfaced by default).
fn assign_tier(name: &str) -> ToolTier {
    // Must remain sorted alphabetically for binary_search.
    const CORE_TOOLS: &[&str] = &[
        "dakera_batch_forget",
        "dakera_batch_recall",
        "dakera_extract",
        "dakera_forget",
        "dakera_fulltext_search",
        "dakera_hybrid_search",
        "dakera_knowledge_graph",
        "dakera_recall",
        "dakera_search",
        "dakera_session_end",
        "dakera_session_start",
        "dakera_store",
    ];

    if CORE_TOOLS.binary_search(&name).is_ok() {
        return ToolTier::Core;
    }

    if name.starts_with("dakera_namespace")
        || name.starts_with("dakera_encryption")
        || name.starts_with("dakera_decay")
        || name.starts_with("dakera_audit")
        || name == "dakera_memory_export"
        || name == "dakera_memory_import"
        || name == "dakera_embed_migration_status"
        || name.contains("_bulk_")
    {
        return ToolTier::Admin;
    }

    ToolTier::Power
}

const MAX_RETRIES: u32 = 3;
const RETRY_DELAYS_MS: [u64; 3] = [100, 500, 2000];

/// The longest a `Retry-After` from the server is honoured between retries.
const MAX_RETRY_AFTER_MS: u64 = 8_000;

/// How long `/v1/capabilities` is remembered.
const CAPABILITY_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// How long a failed probe (server down, starting, key refused) is remembered,
/// so a down server does not cost every `tools/list` the probe timeout.
const CAPABILITY_RETRY_TTL: std::time::Duration = std::time::Duration::from_secs(10);

/// Timeout of the capability probe: it runs before `tools/list` is answered.
const CAPABILITY_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Timeout of an attachment upload / download (up to 25 MiB by default),
/// inside the 60 s a tool call may take.
const TRANSFER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(55);

/// When `/v1/capabilities` was last read, and what it said.
type CapabilityCache =
    std::sync::Mutex<Option<(std::time::Instant, capabilities::CapabilityState)>>;

/// API client for calling Dakera endpoints
pub struct DakeraApiClient {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
    capabilities: CapabilityCache,
}

/// The `Retry-After` header of a response, in seconds.
fn retry_after_secs(resp: &reqwest::Response) -> Option<u64> {
    let value = resp.headers().get("retry-after")?;
    value.to_str().ok()?.trim().parse::<u64>().ok()
}

/// An error answer's text with a `Hint:` line when v0.12 gives it a known meaning.
fn add_hint(status: reqwest::StatusCode, text: String, retry_after: Option<u64>) -> String {
    if status.is_success() {
        return text;
    }
    match hints::error_hint(status.as_u16(), &text, retry_after) {
        Some(hint) => format!("{text}\nHint: {hint}"),
        None => text,
    }
}

/// Whether a failed send may be repeated. A connection that was never made is
/// always safe to retry; a timeout (or a request that broke after it was sent)
/// only for an idempotent method — the server may have acted on a POST/PATCH
/// already (a store, an import, a key rotation), and resending it would act twice.
fn is_retryable_error(err: &reqwest::Error, method: &reqwest::Method) -> bool {
    if err.is_connect() {
        return true;
    }
    let idempotent = matches!(
        *method,
        reqwest::Method::GET | reqwest::Method::PUT | reqwest::Method::DELETE
    );
    idempotent && (err.is_timeout() || err.is_request())
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 502 | 503 | 504 | 408 | 429)
}

impl DakeraApiClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(5))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(4)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .user_agent(format!("dakera-mcp/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            api_key,
            client,
            capabilities: std::sync::Mutex::new(None),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req
    }

    async fn send_with_retry(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(reqwest::StatusCode, String), String> {
        let mut last_err = String::new();
        let mut retry_after_ms: Option<u64> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let default_ms = RETRY_DELAYS_MS
                    .get((attempt - 1) as usize)
                    .copied()
                    .unwrap_or(2000);
                // A v0.12 server says when to come back (`Retry-After` on every 503).
                let delay = match retry_after_ms.take() {
                    Some(ms) => ms.clamp(default_ms, MAX_RETRY_AFTER_MS),
                    None => default_ms,
                };
                tracing::warn!(attempt, delay_ms = delay, path, "Retrying request");
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }

            let mut req = self.request(method.clone(), path);
            if let Some(b) = body {
                req = req.json(b);
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let retry_after = retry_after_secs(&resp);
                    if is_retryable_status(status) && attempt < MAX_RETRIES {
                        let text = resp.text().await.unwrap_or_default();
                        last_err = format!("API error ({}): {}", status, text);
                        retry_after_ms = retry_after.map(|s| s.saturating_mul(1000));
                        tracing::warn!(attempt, status = %status, path, "Retryable status");
                        continue;
                    }
                    let text = resp
                        .text()
                        .await
                        .map_err(|e| format!("Read body failed: {}", e))?;
                    return Ok((status, add_hint(status, text, retry_after)));
                }
                Err(e) => {
                    if is_retryable_error(&e, &method) && attempt < MAX_RETRIES {
                        last_err = format!("HTTP request failed: {}", e);
                        tracing::warn!(attempt, error = %e, path, "Retryable error");
                        continue;
                    }
                    return Err(format!("HTTP request failed: {}", e));
                }
            }
        }
        Err(last_err)
    }

    fn parse_json_response(
        status: reqwest::StatusCode,
        text: &str,
    ) -> Result<serde_json::Value, String> {
        if status.is_success() {
            serde_json::from_str(text).map_err(|e| format!("JSON parse failed: {}", e))
        } else {
            Err(format!("API error ({}): {}", status, text))
        }
    }

    /// Send a request (with the usual retries) and return the status and the
    /// body text, without turning an error status into `Err`: a tool calling a
    /// route that is new in Dakera v0.12.2 tells "this server has no such
    /// route" (see [`route_missing`]) apart from other refusals. `Err` only
    /// when no answer was received.
    pub async fn send_raw(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(reqwest::StatusCode, String), String> {
        self.send_with_retry(method, path, body).await
    }

    pub async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::POST, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::GET, path, None)
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn put_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::PUT, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn delete_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::DELETE, path, None)
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn delete_with_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::DELETE, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn patch_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::PATCH, path, Some(body))
            .await?;
        Self::parse_json_response(status, &text)
    }

    pub async fn get_text(&self, path: &str) -> Result<String, String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::GET, path, None)
            .await?;
        if status.is_success() {
            Ok(text)
        } else {
            Err(format!("API error ({}): {}", status, text))
        }
    }

    /// POST raw bytes (an attachment) with a `Content-Type` and parse the JSON answer.
    pub async fn post_bytes(
        &self,
        path: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .request(reqwest::Method::POST, path)
            .timeout(TRANSFER_TIMEOUT)
            .header("Content-Type", content_type)
            .body(data)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        let status = resp.status();
        let retry_after = retry_after_secs(&resp);
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read body failed: {}", e))?;
        Self::parse_json_response(status, &add_hint(status, text, retry_after))
    }

    /// GET the bytes of an attachment: its media type and content.
    pub async fn get_bytes(&self, path: &str) -> Result<(String, Vec<u8>), String> {
        let resp = self
            .request(reqwest::Method::GET, path)
            .timeout(TRANSFER_TIMEOUT)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let retry_after = retry_after_secs(&resp);
            let text = resp.text().await.unwrap_or_default();
            let text = add_hint(status, text, retry_after);
            return Err(format!("API error ({}): {}", status, text));
        }
        let default_type = "application/octet-stream";
        let content_type = match resp.headers().get("content-type") {
            Some(v) => v.to_str().unwrap_or(default_type).to_string(),
            None => default_type.to_string(),
        };
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Read body failed: {}", e))?;
        Ok((content_type, bytes.to_vec()))
    }

    /// DELETE a resource whose success answer has no body (`204`).
    pub async fn delete_empty(&self, path: &str) -> Result<(), String> {
        let (status, text) = self
            .send_with_retry(reqwest::Method::DELETE, path, None)
            .await?;
        if status.is_success() {
            Ok(())
        } else {
            Err(format!("API error ({}): {}", status, text))
        }
    }

    /// Ask `/v1/capabilities` once, without retries. `Ok` when the server
    /// answered (a document, or "no such route": a pre-v0.12 server); `Err`
    /// with the reason when it could not be read.
    pub async fn probe_capabilities(&self) -> Result<capabilities::CapabilityState, String> {
        let req = self
            .request(reqwest::Method::GET, "/v1/capabilities")
            .timeout(CAPABILITY_PROBE_TIMEOUT);
        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND
            || status == reqwest::StatusCode::METHOD_NOT_ALLOWED
        {
            return Ok(capabilities::CapabilityState::NotSupported);
        }
        let retry_after = retry_after_secs(&resp);
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read body failed: {}", e))?;
        if !status.is_success() {
            let text = add_hint(status, text, retry_after);
            return Err(format!("API error ({}): {}", status, text));
        }
        match serde_json::from_str(&text) {
            Ok(doc) => Ok(capabilities::CapabilityState::Known(doc)),
            Err(e) => Err(format!("JSON parse failed: {}", e)),
        }
    }

    /// What the server reports it supports, remembered for a minute. A server
    /// that cannot be asked is `Unknown` (nothing is hidden because of it).
    pub async fn capability_state(&self) -> capabilities::CapabilityState {
        if let Ok(guard) = self.capabilities.lock() {
            if let Some((at, state)) = guard.as_ref() {
                let ttl = match state {
                    capabilities::CapabilityState::Unknown => CAPABILITY_RETRY_TTL,
                    _ => CAPABILITY_TTL,
                };
                if at.elapsed() < ttl {
                    return state.clone();
                }
            }
        }
        match self.probe_capabilities().await {
            Ok(state) => {
                if let Ok(mut guard) = self.capabilities.lock() {
                    *guard = Some((std::time::Instant::now(), state.clone()));
                }
                state
            }
            Err(e) => {
                tracing::debug!(error = %e, "capabilities unavailable");
                let state = capabilities::CapabilityState::Unknown;
                if let Ok(mut guard) = self.capabilities.lock() {
                    *guard = Some((std::time::Instant::now(), state.clone()));
                }
                state
            }
        }
    }

    /// The explanation to answer a call of `tool` with when the server has the
    /// feature it needs switched off (or predates v0.12); `None` to go ahead.
    pub async fn unavailable_reason(&self, tool: &str) -> Option<String> {
        if capabilities::required_features(tool).is_empty() {
            return None;
        }
        let state = self.capability_state().await;
        capabilities::unavailable_reason(tool, &state)
    }

    pub async fn post_multipart_text(
        &self,
        path: &str,
        text: &str,
    ) -> Result<serde_json::Value, String> {
        let part = reqwest::multipart::Part::text(text.to_string())
            // No extension: the server picks the format from the file name
            // before it looks at the bytes, so `import.jsonl` made every
            // CSV / Mem0 / Zep payload sent without `format` parse as JSONL.
            .file_name(IMPORT_FILE_NAME)
            .mime_str("application/octet-stream")
            .map_err(|e| format!("Multipart build failed: {}", e))?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.post(&url).multipart(form);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = resp.status();
        let retry_after = retry_after_secs(&resp);
        let body = resp
            .text()
            .await
            .map_err(|e| format!("Read body failed: {}", e))?;

        Self::parse_json_response(status, &add_hint(status, body, retry_after))
    }
}

/// The file name of an import upload: no extension, so the server detects the
/// format from the content when the call names none.
pub const IMPORT_FILE_NAME: &str = "import";

/// Whether an answer says the server has no such route or method — an older
/// server than the one a route is new in — as opposed to a refusal by a route
/// it has (such as `404` "Session not found"). Covers `405` (the path exists
/// with other methods: `POST /v1/agents` before v0.12.2), a bodiless `404`
/// (v0.11) and the JSON `ROUTE_NOT_FOUND` / `METHOD_NOT_ALLOWED` answers of
/// v0.12.0 and later. `text` may carry the `Hint:` line [`add_hint`] adds.
pub fn route_missing(status: reqwest::StatusCode, text: &str) -> bool {
    match status.as_u16() {
        405 => true,
        404 => {
            let body = text.split("\nHint: ").next().unwrap_or("").trim();
            if body.is_empty() || body.starts_with("Hint: ") {
                return true;
            }
            let parsed: serde_json::Value =
                serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
            matches!(
                parsed.get("code").and_then(|c| c.as_str()),
                Some("ROUTE_NOT_FOUND") | Some("METHOD_NOT_ALLOWED")
            )
        }
        _ => false,
    }
}

/// The preview size the list-style tools ask for when the caller names none:
/// enough to tell memories apart, without pulling up to 100 kB per memory into
/// the context. `dakera_memory_get` returns a memory in full.
pub const DEFAULT_CONTENT_PREVIEW_CHARS: u64 = 500;

/// The largest `content_preview_chars` the server accepts.
pub const MAX_CONTENT_PREVIEW_CHARS: u64 = 10_000;

/// The `content_preview_chars` to send for a list-style tool: the caller's value
/// (`0` = full content, nothing sent), else [`DEFAULT_CONTENT_PREVIEW_CHARS`].
/// A server before v0.12.2 ignores the parameter and returns full content.
pub fn content_preview_chars(args: &serde_json::Value) -> Result<Option<u64>, CallToolResult> {
    let Some(value) = args.get("content_preview_chars").filter(|v| !v.is_null()) else {
        return Ok(Some(DEFAULT_CONTENT_PREVIEW_CHARS));
    };
    match value.as_u64() {
        Some(0) => Ok(None),
        Some(n) if n <= MAX_CONTENT_PREVIEW_CHARS => Ok(Some(n)),
        _ => Err(CallToolResult::error(format!(
            "content_preview_chars must be an integer from 1 to {MAX_CONTENT_PREVIEW_CHARS}, \
             or 0 for the full content; got {value}"
        ))),
    }
}

/// Helper to extract a required string parameter, returning an error CallToolResult on failure.
pub fn require_string(args: &serde_json::Value, field: &str) -> Result<String, CallToolResult> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| CallToolResult::error(format!("Missing required parameter: {}", field)))
}

/// Format a successful JSON response.
pub fn ok_json(value: &serde_json::Value) -> CallToolResult {
    CallToolResult::text(serde_json::to_string_pretty(value).unwrap_or_default())
}

/// Return the full tool catalog with tier assignments for every tool (MCP-8).
///
/// Aggregates definitions from all modules including discovery meta-tools and
/// pairs each with its `ToolTier` classification.
pub fn full_catalog() -> Vec<ToolCatalogEntry> {
    let mut raw = Vec::new();
    raw.extend(memory::definitions());
    raw.extend(sessions::definitions());
    raw.extend(agents::definitions());
    raw.extend(knowledge::definitions());
    raw.extend(namespaces::definitions());
    raw.extend(namespace_keys::definitions());
    raw.extend(vectors::definitions());
    raw.extend(inference::definitions());
    raw.extend(fulltext::definitions());
    raw.extend(autopilot::definitions());
    raw.extend(decay::definitions());
    raw.extend(entities::definitions());
    raw.extend(graph::definitions());
    raw.extend(audit::definitions());
    raw.extend(transfer::definitions());
    raw.extend(feedback::definitions());
    raw.extend(tif::definitions());
    raw.extend(extractor::definitions());
    raw.extend(encryption::definitions());
    raw.extend(ode::definitions());
    raw.extend(health::definitions());
    raw.extend(identity::definitions());
    raw.extend(ops::definitions());
    raw.extend(capabilities::definitions());
    raw.extend(attachments::definitions());
    // Meta-tools are always exposed regardless of profile.
    raw.extend(discovery::definitions());

    raw.into_iter()
        .map(|def| {
            let tier = if def.name == "dakera_discover_tools" || def.name == "dakera_load_tools" {
                ToolTier::Meta
            } else {
                assign_tier(&def.name)
            };
            ToolCatalogEntry { tier, def }
        })
        .collect()
}

/// Return tool definitions filtered by profile (MCP-8 hybrid exposure).
///
/// - `"core"` (default): Core (12) + Meta (2) = 14 tools
/// - `"power"`: Core + Power + Meta (excludes Admin management-plane tools)
/// - `"admin"`: Core + Admin + Meta (namespace/encryption/bulk ops — no Power tools)
/// - `"all"`: every tool (backwards-compatible escape hatch)
pub fn filtered_definitions(profile: &str) -> Vec<ToolDefinition> {
    full_catalog()
        .into_iter()
        .filter(|entry| match profile {
            "power" => matches!(
                entry.tier,
                ToolTier::Core | ToolTier::Power | ToolTier::Meta
            ),
            "admin" => matches!(
                entry.tier,
                ToolTier::Core | ToolTier::Admin | ToolTier::Meta
            ),
            "all" => true,
            _ => matches!(entry.tier, ToolTier::Core | ToolTier::Meta),
        })
        .map(|entry| entry.def)
        .collect()
}

/// Drop the tools the server cannot serve: those that need an opt-in feature
/// (attachments, image indexing) its `/v1/capabilities` reports as off, or any
/// feature at all on a server that predates v0.12.
pub fn available_definitions(
    defs: Vec<ToolDefinition>,
    state: &capabilities::CapabilityState,
) -> Vec<ToolDefinition> {
    defs.into_iter()
        .filter(|def| capabilities::is_available(&def.name, state))
        .collect()
}

/// The definitions `tools/list` answers with: the profile's tools, minus the
/// ones the server cannot serve. The server is asked (once a minute) only when
/// the profile holds a tool that needs an opt-in feature.
pub async fn listed_definitions(client: &DakeraApiClient, profile: &str) -> Vec<ToolDefinition> {
    let defs = filtered_definitions(profile);
    let gated = defs
        .iter()
        .any(|d| !capabilities::required_features(&d.name).is_empty());
    if !gated {
        return defs;
    }
    let state = client.capability_state().await;
    available_definitions(defs, &state)
}

/// Return all tool definitions across every tier (test helper).
#[cfg(test)]
pub fn tool_definitions() -> Vec<ToolDefinition> {
    full_catalog().into_iter().map(|e| e.def).collect()
}

/// Execute a tool call by dispatching to the appropriate module.
///
/// # Profile enforcement
/// This function dispatches to ALL tools regardless of the caller's active profile.
/// Profile selection via `filtered_definitions()` controls which tools appear in
/// `tools/list` responses (i.e. what the MCP client *sees*), not which tools can
/// be *invoked* via `tools/call`. This mirrors the Claude Code ToolSearch pattern
/// where listing and invocation are independent operations. The security boundary
/// is the Dakera API key, not the MCP profile tier.
pub async fn execute_tool(
    client: &DakeraApiClient,
    name: &str,
    arguments: &serde_json::Value,
) -> CallToolResult {
    // Discovery meta-tools are pure local catalog lookups — no API call needed.
    if let Some(result) = discovery::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = memory::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = sessions::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = agents::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = knowledge::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = namespaces::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = vectors::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = inference::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = fulltext::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = autopilot::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = decay::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = entities::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = graph::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = audit::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = namespace_keys::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = transfer::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = feedback::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = tif::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = extractor::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = encryption::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = ode::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = health::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = identity::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = ops::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = capabilities::execute(client, name, arguments).await {
        return result;
    }
    if let Some(result) = attachments::execute(client, name, arguments).await {
        return result;
    }
    CallToolResult::error(format!("Unknown tool: {}", name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_require_string_present() {
        let args = json!({"agent_id": "test-agent"});
        assert_eq!(require_string(&args, "agent_id").unwrap(), "test-agent");
    }

    #[test]
    fn test_require_string_missing_field() {
        let args = json!({});
        let err = require_string(&args, "agent_id").unwrap_err();
        assert_eq!(err.is_error, Some(true));
        assert!(err.content[0].text.contains("agent_id"));
    }

    #[test]
    fn test_require_string_wrong_type_number() {
        let args = json!({"count": 42});
        let err = require_string(&args, "count").unwrap_err();
        assert_eq!(err.is_error, Some(true));
        assert!(err.content[0].text.contains("count"));
    }

    #[test]
    fn test_require_string_wrong_type_bool() {
        let args = json!({"flag": true});
        let err = require_string(&args, "flag").unwrap_err();
        assert_eq!(err.is_error, Some(true));
    }

    #[test]
    fn test_require_string_null_value() {
        let args = json!({"key": null});
        let err = require_string(&args, "key").unwrap_err();
        assert_eq!(err.is_error, Some(true));
    }

    #[test]
    fn test_route_missing_tells_an_older_server_from_a_refusal() {
        use reqwest::StatusCode;
        let coded = r#"{"error":"No route","code":"ROUTE_NOT_FOUND","status":404}"#;
        assert!(route_missing(StatusCode::NOT_FOUND, coded));
        assert!(route_missing(StatusCode::METHOD_NOT_ALLOWED, "{}"));
        // A bodiless 404 (v0.11), with or without the hint line added to it.
        assert!(route_missing(StatusCode::NOT_FOUND, ""));
        assert!(route_missing(
            StatusCode::NOT_FOUND,
            "\nHint: This server has no such route"
        ));
        let session =
            r#"{"error":"Session not found: s1","code":"VECTOR_NOT_FOUND","resource":"session"}"#;
        assert!(!route_missing(StatusCode::NOT_FOUND, session));
        assert!(!route_missing(StatusCode::FORBIDDEN, coded));
        assert!(!route_missing(StatusCode::OK, ""));
    }

    #[test]
    fn test_content_preview_chars_defaults_and_bounds() {
        assert_eq!(
            content_preview_chars(&json!({})).unwrap(),
            Some(DEFAULT_CONTENT_PREVIEW_CHARS)
        );
        assert_eq!(
            content_preview_chars(&json!({"content_preview_chars": null})).unwrap(),
            Some(DEFAULT_CONTENT_PREVIEW_CHARS)
        );
        assert_eq!(
            content_preview_chars(&json!({"content_preview_chars": 0})).unwrap(),
            None
        );
        assert_eq!(
            content_preview_chars(&json!({"content_preview_chars": 10000})).unwrap(),
            Some(10000)
        );
        for bad in [json!(10001), json!(-3), json!(1.5), json!("200")] {
            let err = content_preview_chars(&json!({"content_preview_chars": bad})).unwrap_err();
            assert_eq!(err.is_error, Some(true));
        }
    }

    #[test]
    fn test_ok_json_text_is_pretty() {
        let value = json!({"key": "value", "num": 1});
        let result = ok_json(&value);
        assert!(result.is_error.is_none());
        let text = &result.content[0].text;
        assert!(
            text.contains('\n'),
            "Expected pretty-printed JSON with newlines"
        );
        assert!(text.contains("\"key\""));
    }

    #[test]
    fn test_ok_json_empty_object() {
        let result = ok_json(&json!({}));
        assert!(result.is_error.is_none());
        assert!(!result.content[0].text.is_empty());
    }

    #[test]
    fn test_tool_definitions_not_empty() {
        let defs = tool_definitions();
        assert!(!defs.is_empty());
    }

    // ── MCP-8 hybrid exposure tests ────────────────────────────────────────

    #[test]
    fn test_core_profile_returns_14_tools() {
        let defs = filtered_definitions("core");
        assert_eq!(
            defs.len(),
            14,
            "Core profile must expose exactly 12 core + 2 meta = 14 tools"
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
            assert!(names.contains(tool), "Core profile missing: {tool}");
        }
    }

    #[test]
    fn test_core_profile_contains_meta_tools() {
        let defs = filtered_definitions("core");
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains("dakera_discover_tools"),
            "Core profile missing dakera_discover_tools"
        );
        assert!(
            names.contains("dakera_load_tools"),
            "Core profile missing dakera_load_tools"
        );
    }

    #[test]
    fn test_all_profile_matches_tool_definitions() {
        let all_profile = filtered_definitions("all");
        let all_defs = tool_definitions();
        assert_eq!(
            all_profile.len(),
            all_defs.len(),
            "profile=all must return same count as tool_definitions()"
        );
    }

    #[test]
    fn test_power_profile_larger_than_core() {
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
    fn test_all_profile_larger_than_power() {
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
    fn test_unknown_profile_defaults_to_core() {
        let unknown = filtered_definitions("bogus_profile");
        let core = filtered_definitions("core");
        assert_eq!(
            unknown.len(),
            core.len(),
            "Unknown profile must default to core behaviour"
        );
    }

    #[test]
    fn test_full_catalog_exactly_12_core_tier_tools() {
        let catalog = full_catalog();
        let core_names: std::collections::HashSet<_> = catalog
            .iter()
            .filter(|e| e.tier == ToolTier::Core)
            .map(|e| e.def.name.as_str())
            .collect();
        assert!(core_names.contains("dakera_store"));
        assert!(core_names.contains("dakera_recall"));
        assert!(core_names.contains("dakera_batch_recall"));
        assert_eq!(core_names.len(), 12, "Expected exactly 12 Core-tier tools");
    }

    #[test]
    fn test_full_catalog_exactly_2_meta_tier_tools() {
        let catalog = full_catalog();
        let meta: Vec<_> = catalog
            .iter()
            .filter(|e| e.tier == ToolTier::Meta)
            .collect();
        assert_eq!(meta.len(), 2, "Expected exactly 2 Meta-tier tools");
        let meta_names: std::collections::HashSet<_> =
            meta.iter().map(|e| e.def.name.as_str()).collect();
        assert!(meta_names.contains("dakera_discover_tools"));
        assert!(meta_names.contains("dakera_load_tools"));
    }

    #[test]
    fn test_full_catalog_namespace_tools_are_admin_tier() {
        let catalog = full_catalog();
        for entry in &catalog {
            if entry.def.name.starts_with("dakera_namespace") {
                assert_eq!(
                    entry.tier,
                    ToolTier::Admin,
                    "{} must be Admin tier",
                    entry.def.name
                );
            }
        }
    }

    #[test]
    fn test_tool_definitions_includes_discovery_tools() {
        let defs = tool_definitions();
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains("dakera_discover_tools"),
            "tool_definitions() missing dakera_discover_tools"
        );
        assert!(
            names.contains("dakera_load_tools"),
            "tool_definitions() missing dakera_load_tools"
        );
    }

    #[test]
    fn test_tool_definitions_have_unique_names() {
        let defs = tool_definitions();
        let mut seen = std::collections::HashSet::new();
        for def in &defs {
            assert!(
                seen.insert(def.name.clone()),
                "Duplicate tool name: {}",
                def.name
            );
        }
    }

    #[test]
    fn test_tool_definitions_all_have_descriptions() {
        for def in tool_definitions() {
            assert!(!def.name.is_empty(), "Tool has empty name");
            assert!(
                !def.description.is_empty(),
                "Tool '{}' has empty description",
                def.name
            );
        }
    }

    #[test]
    fn test_tool_definitions_contain_expected_tools() {
        let defs = tool_definitions();
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains("dakera_store"));
        assert!(names.contains("dakera_recall"));
        assert!(names.contains("dakera_session_start"));
        assert!(names.contains("dakera_session_end"));
        // v0.2.2 tools
        assert!(
            names.contains("dakera_namespace_configure"),
            "dakera_namespace_configure missing from tool definitions"
        );
        assert!(
            names.contains("dakera_knowledge_network_cross_agent"),
            "dakera_knowledge_network_cross_agent missing from tool definitions"
        );
        // v0.3.0 CE-2 tools
        assert!(
            names.contains("dakera_batch_recall"),
            "dakera_batch_recall missing from tool definitions"
        );
        assert!(
            names.contains("dakera_batch_forget"),
            "dakera_batch_forget missing from tool definitions"
        );
        // v0.3.2 PILOT-4 tools
        assert!(
            names.contains("dakera_autopilot_status"),
            "dakera_autopilot_status missing from tool definitions"
        );
        assert!(
            names.contains("dakera_autopilot_trigger"),
            "dakera_autopilot_trigger missing from tool definitions"
        );
        // v0.4.0 DECAY tools
        assert!(
            names.contains("dakera_decay_config_get"),
            "dakera_decay_config_get missing from tool definitions"
        );
        assert!(
            names.contains("dakera_decay_config_set"),
            "dakera_decay_config_set missing from tool definitions"
        );
        assert!(
            names.contains("dakera_decay_stats"),
            "dakera_decay_stats missing from tool definitions"
        );
        // v0.5.0 MCP-4 / CE-4 entity tools
        assert!(
            names.contains("dakera_auto_tag"),
            "dakera_auto_tag missing from tool definitions"
        );
        assert!(
            names.contains("dakera_entity_types_set"),
            "dakera_entity_types_set missing from tool definitions"
        );
        assert!(
            names.contains("dakera_entity_types_get"),
            "dakera_entity_types_get missing from tool definitions"
        );
        assert!(
            names.contains("dakera_memory_entities"),
            "dakera_memory_entities missing from tool definitions"
        );
        // v0.6.0 MCP-4 / CE-5 graph tools
        assert!(
            names.contains("dakera_graph_traverse"),
            "dakera_graph_traverse missing from tool definitions"
        );
        assert!(
            names.contains("dakera_graph_path"),
            "dakera_graph_path missing from tool definitions"
        );
        assert!(
            names.contains("dakera_graph_link_memory"),
            "dakera_graph_link_memory missing from tool definitions"
        );
        assert!(
            names.contains("dakera_graph_export"),
            "dakera_graph_export missing from tool definitions"
        );
        // v0.7.0 OBS-1 audit tool
        assert!(
            names.contains("dakera_audit_query"),
            "dakera_audit_query missing from tool definitions"
        );
        // v0.9.0 ODE-2 entity extraction tool
        assert!(
            names.contains("dakera_extract_entities"),
            "dakera_extract_entities missing from tool definitions"
        );
        // v0.9.2 MCP-5 cognitive tools
        assert!(
            names.contains("dakera_memory_policy_get"),
            "dakera_memory_policy_get missing from tool definitions"
        );
        assert!(
            names.contains("dakera_memory_policy_set"),
            "dakera_memory_policy_set missing from tool definitions"
        );
        assert!(
            names.contains("dakera_recall_associated"),
            "dakera_recall_associated missing from tool definitions"
        );
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_error() {
        let client = DakeraApiClient::new("http://localhost:9999".to_string(), None);
        let result = execute_tool(&client, "nonexistent_tool_xyz", &json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("nonexistent_tool_xyz"));
    }

    // ── Admin profile tests (gap #1 fix) ─────────────────────────────────────

    #[test]
    fn test_admin_profile_includes_core_tools() {
        let defs = filtered_definitions("admin");
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        for tool in &["dakera_store", "dakera_recall", "dakera_batch_recall"] {
            assert!(
                names.contains(tool),
                "admin profile missing core tool: {tool}"
            );
        }
    }

    #[test]
    fn test_admin_profile_includes_meta_tools() {
        let defs = filtered_definitions("admin");
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains("dakera_discover_tools"),
            "admin profile must include meta tool dakera_discover_tools"
        );
        assert!(
            names.contains("dakera_load_tools"),
            "admin profile must include meta tool dakera_load_tools"
        );
    }

    #[test]
    fn test_admin_profile_includes_admin_tier_tools() {
        let defs = filtered_definitions("admin");
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        // Namespace tools are Admin tier — must appear in admin profile
        assert!(
            names.contains("dakera_namespace_list"),
            "admin profile missing dakera_namespace_list"
        );
        assert!(
            names.contains("dakera_namespace_create"),
            "admin profile missing dakera_namespace_create"
        );
        assert!(
            names.contains("dakera_audit_query"),
            "admin profile missing dakera_audit_query"
        );
    }

    #[test]
    fn test_admin_profile_excludes_power_tier_tools() {
        let defs = filtered_definitions("admin");
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        // Power-tier tools must NOT appear in admin profile
        assert!(
            !names.contains("dakera_agent_stats"),
            "admin profile must NOT include power tool dakera_agent_stats"
        );
        assert!(
            !names.contains("dakera_consolidate"),
            "admin profile must NOT include power tool dakera_consolidate"
        );
        assert!(
            !names.contains("dakera_autopilot_status"),
            "admin profile must NOT include power tool dakera_autopilot_status"
        );
    }

    #[test]
    fn test_admin_profile_larger_than_core() {
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
    fn test_all_profile_larger_than_admin() {
        let admin = filtered_definitions("admin");
        let all = filtered_definitions("all");
        assert!(
            all.len() > admin.len(),
            "all ({}) must have more tools than admin ({})",
            all.len(),
            admin.len()
        );
    }

    // ── Total tool count regression test (gap #8 fix) ────────────────────────

    #[test]
    fn test_total_tool_count() {
        let all = filtered_definitions("all");
        // Exact count: 102 tools total (86 after PR#84 prune + dakera_tif_evaluate, DAK-6561,
        // + 12 for Dakera v0.12: capabilities, health, embed migration status, encryption
        // status, the 7 attachment tools and dakera_wake_up; + 3 for Dakera v0.12.2:
        // dakera_session_touch, dakera_agent_create and dakera_whoami).
        // If this fails, run filtered_definitions("all").len() to discover the new count
        // and update this assertion. Catches accidental add/remove.
        assert_eq!(
            all.len(),
            102,
            "Expected exactly 102 tools in 'all' profile. Actual: {}. \
             Update this constant after intentional catalog changes.",
            all.len()
        );
    }

    // ── Tier assignment correctness spot-check (gap #9 fix) ──────────────────

    #[test]
    fn test_tier_assignment_spot_check() {
        let catalog = full_catalog();
        let tier_map: std::collections::HashMap<_, _> = catalog
            .iter()
            .map(|e| (e.def.name.as_str(), e.tier))
            .collect();

        // Core tier — high-frequency tools always in default profile
        for name in &[
            "dakera_store",
            "dakera_recall",
            "dakera_search",
            "dakera_batch_recall",
            "dakera_batch_forget",
            "dakera_forget",
            "dakera_session_start",
            "dakera_session_end",
            "dakera_hybrid_search",
            "dakera_fulltext_search",
            "dakera_knowledge_graph",
            "dakera_extract",
        ] {
            assert_eq!(
                tier_map.get(name).copied(),
                Some(ToolTier::Core),
                "{name} must be Core tier"
            );
        }

        // Admin tier — namespace/encryption/bulk/audit/decay management plane
        for name in &[
            "dakera_namespace_list",
            "dakera_namespace_create",
            "dakera_namespace_delete",
            "dakera_namespace_configure",
            "dakera_namespace_key_create",
            "dakera_audit_query",
            "dakera_memory_export",
            "dakera_memory_import",
            "dakera_encryption_rotate_key",
            "dakera_encryption_status",
            "dakera_embed_migration_status",
        ] {
            assert_eq!(
                tier_map.get(name).copied(),
                Some(ToolTier::Admin),
                "{name} must be Admin tier"
            );
        }

        // Power tier — advanced tools not in default profile
        for name in &[
            "dakera_agent_stats",
            "dakera_consolidate",
            "dakera_autopilot_status",
            "dakera_session_list",
            "dakera_memory_get",
            "dakera_graph_traverse",
            "dakera_vector_unified_query",
            "dakera_capabilities",
            "dakera_health",
            "dakera_attachment_upload",
            "dakera_attachment_transcribe",
            "dakera_session_touch",
            "dakera_agent_create",
            "dakera_whoami",
        ] {
            assert_eq!(
                tier_map.get(name).copied(),
                Some(ToolTier::Power),
                "{name} must be Power tier"
            );
        }

        // Meta tier — always exposed alongside Core
        for name in &["dakera_discover_tools", "dakera_load_tools"] {
            assert_eq!(
                tier_map.get(name).copied(),
                Some(ToolTier::Meta),
                "{name} must be Meta tier"
            );
        }
    }

    // ── Pruned tool regression test (gap #7 fix — unit level) ────────────────

    #[tokio::test]
    async fn test_pruned_tools_not_in_execute_catalog() {
        let client = DakeraApiClient::new("http://localhost:9999".to_string(), None);
        // These tools were deleted in PR#84 (DAK-5182). They must return "Unknown tool".
        for pruned in &[
            "dakera_admin_health_full",
            "dakera_analytics_usage",
            "dakera_stream_events",
        ] {
            let result = execute_tool(&client, pruned, &json!({})).await;
            assert_eq!(
                result.is_error,
                Some(true),
                "Pruned tool '{pruned}' must return an error, not silently succeed"
            );
            assert!(
                result.content[0].text.contains("Unknown tool"),
                "Pruned tool '{pruned}' error must say 'Unknown tool', got: {}",
                result.content[0].text
            );
        }
    }

    // ── Token count / schema compression tests (DAK-5216) ────────────────────

    #[test]
    fn test_no_internal_refs_in_descriptions() {
        // All tool descriptions must be free of internal codenames (COG-N, CE-N, KG-N, SEC-N, etc.)
        let internal_ref_pattern = regex_pattern_internal_refs();
        for def in tool_definitions() {
            assert!(
                !has_internal_ref(&def.description),
                "Tool '{}' description contains internal ref: {:?}",
                def.name,
                &def.description
            );
            // Check property descriptions inside inputSchema
            if let Some(props) = def
                .input_schema
                .get("properties")
                .and_then(|p| p.as_object())
            {
                for (prop_name, prop_val) in props {
                    if let Some(desc) = prop_val.get("description").and_then(|d| d.as_str()) {
                        assert!(
                            !has_internal_ref(desc),
                            "Tool '{}' property '{}' description contains internal ref: {:?}",
                            def.name,
                            prop_name,
                            desc
                        );
                    }
                }
            }
            let _ = internal_ref_pattern; // suppress unused warning
        }
    }

    fn has_internal_ref(s: &str) -> bool {
        // Match COG-N, CE-N, KG-N, SEC-N, ODE-N, PILOT-N, INT-N as standalone words
        let patterns = ["COG-", "SEC-", "ODE-", "PILOT-", "INT-"];
        for p in &patterns {
            if s.contains(p) {
                return true;
            }
        }
        false
    }

    fn regex_pattern_internal_refs() -> &'static str {
        "COG-|SEC-|ODE-|PILOT-|INT-"
    }

    #[test]
    fn test_no_default_fields_in_schemas() {
        // JSON schema "default" fields are redundant (server applies defaults server-side)
        // and waste tokens. Verify they are absent.
        for def in tool_definitions() {
            let schema_str = serde_json::to_string(&def.input_schema).unwrap();
            assert!(
                !schema_str.contains("\"default\":"),
                "Tool '{}' input_schema still contains a \"default\" field — remove it",
                def.name
            );
        }
    }

    #[test]
    fn test_token_size_core_profile_within_budget() {
        // Core profile (14 tools) after description compression, default removal,
        // and agent_id dedup. Budget: 3500 estimated tokens (JSON bytes / 3).
        // Pre-optimization baseline was ~4500+ estimated tokens at full 86 tools.
        let defs = filtered_definitions("core");
        let json_bytes = serde_json::to_string(&defs).unwrap().len();
        let estimated_tokens = json_bytes / 3;
        assert!(
            estimated_tokens < 3500,
            "Core profile estimated tokens {} exceeds 3500 budget (JSON bytes: {}). \
             Compress tool descriptions further.",
            estimated_tokens,
            json_bytes
        );
    }

    #[test]
    fn test_token_size_all_profile_within_budget() {
        // All-profile (102 tools) after description compression. Budget: 21000 estimated
        // tokens (JSON bytes / 3; it was 17000 for 87 tools before the v0.12 tools and 20000
        // for 99 before the v0.12.2 session, agent and identity tools). With MCP pagination
        // at 128 tools/page, one page holds the whole catalog.
        let defs = filtered_definitions("all");
        let json_bytes = serde_json::to_string(&defs).unwrap().len();
        let estimated_tokens = json_bytes / 3;
        assert!(
            estimated_tokens < 21000,
            "All profile estimated tokens {} exceeds 21000 budget (JSON bytes: {})",
            estimated_tokens,
            json_bytes
        );
    }

    #[test]
    fn test_agent_id_description_absent_from_schemas() {
        // agent_id property must not have a verbose description — the name is self-documenting.
        // This checks that property description dedup was applied.
        for def in tool_definitions() {
            if let Some(props) = def
                .input_schema
                .get("properties")
                .and_then(|p| p.as_object())
            {
                if let Some(agent_prop) = props.get("agent_id") {
                    assert!(
                        agent_prop.get("description").is_none(),
                        "Tool '{}' agent_id property has a description — omit it (self-documenting param)",
                        def.name
                    );
                }
            }
        }
    }
}
