//! MCP Server implementation
//!
//! Reads JSON-RPC messages from stdin, processes them, and writes responses to stdout.
//! This implements the MCP protocol over stdio transport.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::tools::{self, DakeraApiClient};

/// MCP Server that communicates over stdio
pub struct McpServer {
    client: DakeraApiClient,
}

impl McpServer {
    pub fn new(api_url: String, api_key: Option<String>) -> Self {
        Self {
            client: DakeraApiClient::new(api_url, api_key),
        }
    }

    /// Run the MCP server, reading from stdin and writing to stdout
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
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
                    let resp = JsonRpcResponse::error(
                        None,
                        -32700,
                        format!("Parse error: {}", e),
                    );
                    let out = serde_json::to_string(&resp)? + "\n";
                    stdout.write_all(out.as_bytes()).await?;
                    stdout.flush().await?;
                    continue;
                }
            };

            let response = self.handle_request(&request).await;
            let out = serde_json::to_string(&response)? + "\n";
            stdout.write_all(out.as_bytes()).await?;
            stdout.flush().await?;
        }

        Ok(())
    }

    async fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request),
            "notifications/initialized" => {
                // Client acknowledgement, no response needed for notifications
                // but since we read it as a line, return empty success
                JsonRpcResponse::success(request.id.clone(), serde_json::json!({}))
            }
            "tools/list" => self.handle_tools_list(request),
            "tools/call" => self.handle_tools_call(request).await,
            "ping" => JsonRpcResponse::success(request.id.clone(), serde_json::json!({})),
            _ => JsonRpcResponse::method_not_found(request.id.clone(), &request.method),
        }
    }

    fn handle_initialize(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse::success(
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
        )
    }

    fn handle_tools_list(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let tool_defs = tools::tool_definitions();
        JsonRpcResponse::success(
            request.id.clone(),
            serde_json::json!({
                "tools": tool_defs
            }),
        )
    }

    async fn handle_tools_call(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
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

        let result = tools::execute_tool(&self.client, tool_name, &arguments).await;

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

        JsonRpcResponse::success(
            request.id.clone(),
            response_value,
        )
    }
}
