# dakera-mcp

Rust MCP (Model Context Protocol) server bridge that exposes Dakera memory operations as MCP
tools for LLM agents (Claude, Cursor, Windsurf, etc.).

## Key Commands
```bash
cargo build --release   # Build MCP server binary → target/release/dakera-mcp
cargo test              # Run tests
cargo clippy            # Lint
cargo fmt               # Format
```

## Architecture
- `src/main.rs` — Entry point; reads DAKERA_API_URL + DAKERA_API_KEY from env
- `src/server.rs` — MCP stdio transport; JSON-RPC message loop
- `src/protocol.rs` — MCP protocol types (initialize, tools/list, tools/call)
- `src/tools/` — Tool handlers mapping MCP calls to dakera REST endpoints

## Conventions
- Communicates over stdio (stdin/stdout) per MCP spec — do not add TCP/HTTP transport
- Config via env vars: `DAKERA_API_URL`, `DAKERA_API_KEY`, `DAKERA_AGENT_ID`
- Docker image published to GHCR alongside each server release
- Version tracks dakera server version
