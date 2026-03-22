# Dakera MCP Server

[![CI](https://github.com/dakera-ai/dakera-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/dakera-ai/dakera-mcp/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange)](https://www.rust-lang.org/)

MCP (Model Context Protocol) server that gives AI agents persistent memory through the [Dakera](https://dakera.ai) vector database. Connect Claude Desktop, Cursor, Windsurf, or any MCP-compatible client to store, recall, and search memories across sessions.

## Features

- **Agent Memory** -- Store, recall, search, and consolidate semantic memories with importance scoring and tagging
- **Vector Operations** -- Full vector CRUD with similarity search, batch queries, multi-vector search, aggregations, and hybrid ranking
- **Full-Text Search** -- BM25-scored document search with hybrid vector+text mode
- **Knowledge Graph** -- Build relationship graphs from memories, summarize, and deduplicate
- **Session Management** -- Group related memories into sessions with metadata and summaries
- **Inference** -- Upsert and query using natural language text with server-side embedding
- **Namespace Isolation** -- Create and manage isolated vector collections with configurable dimensions and distance metrics

## Installation

### From source

```bash
cargo build --release
```

The binary is at `target/release/dakera-mcp`.

### Docker

Pull from GitHub Container Registry (recommended):

```bash
docker pull ghcr.io/dakera-ai/dakera-mcp:latest
```

Or build from source:

```bash
docker build -t dakera-mcp .
```

Run (macOS / Windows — `host.docker.internal` resolves natively):

```bash
docker run -i --rm \
  -e DAKERA_API_URL=http://host.docker.internal:3300 \
  -e DAKERA_API_KEY=your-key \
  ghcr.io/dakera-ai/dakera-mcp
```

Run (Linux — `host.docker.internal` requires `--add-host`):

```bash
docker run -i --rm \
  --add-host=host.docker.internal:host-gateway \
  -e DAKERA_API_URL=http://host.docker.internal:3300 \
  -e DAKERA_API_KEY=your-key \
  ghcr.io/dakera-ai/dakera-mcp
```

## Configuration

| Variable | Description | Default |
|---|---|---|
| `DAKERA_API_URL` | Dakera API base URL | `http://localhost:3300` |
| `DAKERA_API_KEY` | API key for authentication (optional) | none |
| `RUST_LOG` | Log level (`dakera_mcp=debug`, etc.) | `dakera_mcp=info` |

## Usage

### Claude Desktop

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dakera": {
      "command": "/path/to/dakera-mcp",
      "env": {
        "DAKERA_API_URL": "http://localhost:3300",
        "DAKERA_API_KEY": "your-api-key"
      }
    }
  }
}
```

### Claude Code

Add to your `.claude/settings.json`:

```json
{
  "mcpServers": {
    "dakera": {
      "command": "/path/to/dakera-mcp",
      "env": {
        "DAKERA_API_URL": "http://localhost:3300",
        "DAKERA_API_KEY": "your-api-key"
      }
    }
  }
}
```

### Cursor

Add to your Cursor MCP settings:

```json
{
  "mcpServers": {
    "dakera": {
      "command": "/path/to/dakera-mcp",
      "env": {
        "DAKERA_API_URL": "http://localhost:3300",
        "DAKERA_API_KEY": "your-api-key"
      }
    }
  }
}
```

### Docker with MCP clients

Point the command to Docker instead of a local binary. Use `--add-host` to make `host.docker.internal` resolvable on Linux (it is a no-op on macOS and Windows):

```json
{
  "mcpServers": {
    "dakera": {
      "command": "docker",
      "args": ["run", "-i", "--rm",
        "--add-host=host.docker.internal:host-gateway",
        "-e", "DAKERA_API_URL=http://host.docker.internal:3300",
        "-e", "DAKERA_API_KEY=your-api-key",
        "ghcr.io/dakera-ai/dakera-mcp"
      ]
    }
  }
}
```

## Available Tools

### Memory (8 tools)

| Tool | Description |
|---|---|
| `dakera_store` | Store a memory with content, type (episodic/semantic/procedural/working), importance score, and tags |
| `dakera_recall` | Recall memories by semantic similarity to a query |
| `dakera_search` | Advanced memory search with tag and type filters |
| `dakera_memory_get` | Retrieve a specific memory by ID |
| `dakera_memory_update` | Update a memory's content, importance, or tags (re-embeds on content change) |
| `dakera_memory_importance` | Batch-update importance scores for multiple memories |
| `dakera_forget` | Delete memories by ID or tag filter |
| `dakera_consolidate` | Consolidate related memories into a single summary |

### Sessions (5 tools)

| Tool | Description |
|---|---|
| `dakera_session_start` | Start a new session for an agent with optional metadata |
| `dakera_session_end` | End a session with an optional summary |
| `dakera_session_list` | List sessions for an agent, optionally active-only |
| `dakera_session_get` | Get session details including metadata and summary |
| `dakera_session_memories` | List all memories associated with a session |

### Agents (3 tools)

| Tool | Description |
|---|---|
| `dakera_agent_stats` | Get agent statistics: memory count, session count, storage usage, top tags |
| `dakera_agent_memories` | List all memories for an agent with pagination |
| `dakera_agent_sessions` | List all sessions for an agent |

### Knowledge (3 tools)

| Tool | Description |
|---|---|
| `dakera_knowledge_graph` | Build a knowledge graph from a seed memory via embedding similarity |
| `dakera_knowledge_summarize` | Summarize multiple memories into a consolidated memory |
| `dakera_knowledge_deduplicate` | Find and optionally merge duplicate memories |

### Namespaces (4 tools)

| Tool | Description |
|---|---|
| `dakera_namespace_list` | List all namespaces |
| `dakera_namespace_get` | Get namespace details (vector count, dimensions, index stats) |
| `dakera_namespace_create` | Create a namespace with dimensions and distance metric (cosine/euclidean/dot) |
| `dakera_namespace_delete` | Delete a namespace and all its vectors |

### Vectors (14 tools)

| Tool | Description |
|---|---|
| `dakera_vector_upsert` | Upsert vectors with IDs, float arrays, and optional metadata |
| `dakera_vector_upsert_columns` | Upsert vectors in column format for efficient batch operations |
| `dakera_vector_query` | Query by similarity, returning nearest neighbors |
| `dakera_vector_batch_query` | Run multiple similarity searches in parallel |
| `dakera_vector_multi_search` | Multi-vector search with positive/negative vectors, MMR diversity, and score thresholds |
| `dakera_vector_unified_query` | Unified query with flexible ranking (vector ANN, text BM25, attribute ordering, combined) |
| `dakera_vector_delete` | Delete vectors by ID |
| `dakera_vector_bulk_update` | Update metadata on vectors matching a filter |
| `dakera_vector_bulk_delete` | Delete all vectors matching a filter |
| `dakera_vector_count` | Count vectors in a namespace with optional filter |
| `dakera_vector_export` | Export vectors with pagination |
| `dakera_vector_aggregate` | Compute aggregations (Count, Sum, Avg, Min, Max) with optional grouping |
| `dakera_vector_explain` | Explain a query execution plan with cost estimates and optimization hints |
| `dakera_vector_warm` | Pre-load vectors into cache for faster queries |

### Full-Text Search (5 tools)

| Tool | Description |
|---|---|
| `dakera_fulltext_index` | Index documents for full-text search |
| `dakera_fulltext_search` | Search documents with BM25 scoring |
| `dakera_fulltext_delete` | Delete documents from the full-text index |
| `dakera_fulltext_stats` | Get index statistics (document count, unique terms, avg doc length) |
| `dakera_hybrid_search` | Hybrid search combining vector similarity and BM25 with configurable weighting |

### Inference (3 tools)

| Tool | Description |
|---|---|
| `dakera_text_query` | Query using natural language text (server-side embedding) |
| `dakera_upsert_text` | Upsert text documents with automatic embedding generation |
| `dakera_batch_query_text` | Batch query using multiple text queries with automatic embedding |

## Architecture

```
┌─────────────────────┐    stdio (JSON-RPC)    ┌──────────────────┐
│   MCP Client        │◄──────────────────────►│  dakera-mcp      │
│ (Claude, Cursor...) │                         │                  │
└─────────────────────┘                         │  ┌────────────┐  │
                                                │  │ Protocol   │  │
                                                │  │ (JSON-RPC) │  │
                                                │  └─────┬──────┘  │
                                                │        │         │
                                                │  ┌─────▼──────┐  │
                                                │  │  Server     │  │
                                                │  │  (dispatch) │  │
                                                │  └─────┬──────┘  │
                                                │        │         │
                                                │  ┌─────▼──────┐  │     HTTP/REST
                                                │  │   Tools     │  │◄──────────────►  Dakera API
                                                │  │  (45 tools) │  │
                                                │  └────────────┘  │
                                                └──────────────────┘
```

The server communicates over **stdio** using the MCP JSON-RPC protocol. Each tool call translates to one or more HTTP requests against the Dakera REST API. No local state is kept -- the Dakera API is the single source of truth.

## Related Repositories

| Repository | Description |
|------------|-------------|
| [dakera](https://github.com/dakera-ai/dakera) | Core vector database engine (Rust) |
| [dakera-py](https://github.com/dakera-ai/dakera-py) | Python SDK |
| [dakera-js](https://github.com/dakera-ai/dakera-js) | TypeScript/JavaScript SDK |
| [dakera-go](https://github.com/dakera-ai/dakera-go) | Go SDK |
| [dakera-rs](https://github.com/dakera-ai/dakera-rs) | Rust SDK |
| [dakera-cli](https://github.com/dakera-ai/dakera-cli) | Command-line interface |
| [dakera-dashboard](https://github.com/dakera-ai/dakera-dashboard) | Admin dashboard (Leptos/WASM) |
| [dakera-docs](https://github.com/dakera-ai/dakera-docs) | Documentation |
| [dakera-deploy](https://github.com/dakera-ai/dakera-deploy) | Deployment configs and Docker Compose |
| [dakera-cortex](https://github.com/dakera-ai/dakera-cortex) | Flagship demo with AI agents |

## License

MIT -- see [LICENSE](LICENSE).
