//! MCP Server implementation
//!
//! Reads JSON-RPC messages from stdin, processes them concurrently,
//! and writes responses to stdout with proper locking.

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::tools::{self, DakeraApiClient};

/// MCP Server that communicates over stdio
pub struct McpServer {
    client: Arc<DakeraApiClient>,
}

impl McpServer {
    pub fn new(api_url: String, api_key: Option<String>) -> Self {
        Self {
            client: Arc::new(DakeraApiClient::new(api_url, api_key)),
        }
    }

    /// Run the MCP server, reading from stdin and writing to stdout
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let stdin = tokio::io::stdin();
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            tracing::debug!(line = %line, "Received message");

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    let resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                    Self::write_response(&stdout, &resp).await;
                    continue;
                }
            };

            // MCP notifications don't get responses per JSON-RPC 2.0 spec
            if request.method.starts_with("notifications/") {
                tracing::debug!(method = %request.method, "Received notification");
                continue;
            }

            let client = Arc::clone(&self.client);
            let writer = Arc::clone(&stdout);
            tokio::spawn(async move {
                let response = handle_request(&client, &request).await;
                Self::write_response(&writer, &response).await;
            });
        }

        Ok(())
    }

    async fn write_response(stdout: &Arc<Mutex<tokio::io::Stdout>>, response: &JsonRpcResponse) {
        let out = match serde_json::to_string(response) {
            Ok(s) => s + "\n",
            Err(e) => {
                tracing::error!(error = %e, "Failed to serialize JSON-RPC response");
                return;
            }
        };

        let mut lock = stdout.lock().await;
        if let Err(e) = lock.write_all(out.as_bytes()).await {
            tracing::error!(error = %e, "stdout write failed");
            return;
        }
        if let Err(e) = lock.flush().await {
            tracing::error!(error = %e, "stdout flush failed");
        }
    }
}

async fn handle_request(client: &DakeraApiClient, request: &JsonRpcRequest) -> JsonRpcResponse {
    match request.method.as_str() {
        "initialize" => JsonRpcResponse::success(
            request.id.clone(),
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "dakera-mcp",
                    "version": "0.2.0"
                }
            }),
        ),
        "tools/list" => {
            let tool_defs = tools::tool_definitions();
            JsonRpcResponse::success(
                request.id.clone(),
                serde_json::json!({ "tools": tool_defs }),
            )
        }
        "tools/call" => handle_tools_call(client, request).await,
        "ping" => JsonRpcResponse::success(request.id.clone(), serde_json::json!({})),
        _ => JsonRpcResponse::method_not_found(request.id.clone(), &request.method),
    }
}

async fn handle_tools_call(client: &DakeraApiClient, request: &JsonRpcRequest) -> JsonRpcResponse {
    let tool_name = request
        .params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    tracing::info!(tool = %tool_name, "Executing tool");

    let result = match tokio::time::timeout(std::time::Duration::from_secs(60), async {
        tools::execute_tool(client, tool_name, &arguments).await
    })
    .await
    {
        Ok(r) => r,
        Err(_) => {
            tracing::error!(tool = %tool_name, "Tool execution timed out after 60s");
            crate::protocol::CallToolResult::error(format!(
                "Tool '{}' timed out after 60s",
                tool_name
            ))
        }
    };

    let response_value = match serde_json::to_value(&result) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(
                tool = %tool_name,
                error = %e,
                "Failed to serialize tool result"
            );
            serde_json::json!({"error": format!("serialization failed: {}", e)})
        }
    };

    JsonRpcResponse::success(request.id.clone(), response_value)
}
