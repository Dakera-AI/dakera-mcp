# ⚡ dakera-mcp

[![CI](https://github.com/Dakera-AI/dakera-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Dakera-AI/dakera-mcp/actions/workflows/ci.yml) [![Crate](https://img.shields.io/crates/v/dakera-mcp?logo=rust)](https://crates.io/crates/dakera-mcp) [![npm](https://img.shields.io/npm/v/%40dakera-ai%2Fdakera-mcp?logo=npm)](https://www.npmjs.com/package/@dakera-ai/dakera-mcp) [![Downloads](https://img.shields.io/crates/d/dakera-mcp)](https://crates.io/crates/dakera-mcp) [![License: MIT](https://img.shields.io/github/license/Dakera-AI/dakera-mcp)](LICENSE) [![LoCoMo 88.2%](https://img.shields.io/badge/LoCoMo-88.2%25-22c55e?style=flat-square)](https://dakera.ai/benchmark) [![Glama](https://glama.ai/mcp/servers/Dakera-AI/dakera-mcp/badge)](https://glama.ai/mcp/servers/Dakera-AI/dakera-mcp) [![Docs](https://img.shields.io/badge/docs-dakera.ai%2Fdocs-3b82f6?style=flat-square)](https://dakera.ai/docs) [![dakera.ai](https://img.shields.io/badge/dakera.ai-website-22c55e?style=flat-square)](https://dakera.ai) [![Playground](https://img.shields.io/badge/playground-try%20it-ff6b35?style=flat-square)](https://dakera.ai/playground)

MCP server for Dakera AI. Gives any MCP-compatible AI agent persistent, queryable memory — with smart token management built in.

Works with Claude, Claude Code, and any MCP-compatible framework.

Part of [Dakera AI](https://dakera.ai) — the memory engine for AI agents.

> The Dakera memory engine scores **88.2% Recall@20 on LoCoMo** (1,536 evaluated questions · LLM-judged retrieval recall) — [benchmark details](https://dakera.ai/benchmark)

---

## Architecture: 14 core tools + on-demand discovery

Starting every agent session with 60+ tool schemas wastes ~15K tokens before you write a single message. dakera-mcp solves this with **hybrid tool exposure**:

- **14 tools loaded by default** — the 12 highest-frequency memory operations + 2 meta-discovery tools
- **On-demand expansion** — use `dakera_discover_tools` and `dakera_load_tools` to fetch additional tool schemas only when you need them

### Default tool set (core profile)

| Tool | Purpose |
|---|---|
| `dakera_store` | Store a memory with importance, tags, and type |
| `dakera_recall` | Semantic recall by query text |
| `dakera_search` | Advanced memory search with tag/type filters |
| `dakera_session_start` | Start a session to group related memories |
| `dakera_session_end` | End a session with optional summary |
| `dakera_batch_recall` | Bulk filter-based recall (by tags, importance, time) |
| `dakera_forget` | Delete specific memories by ID |
| `dakera_hybrid_search` | Combined vector + BM25 search |
| `dakera_fulltext_search` | BM25 full-text search |
| `dakera_knowledge_graph` | Build a knowledge graph from a seed memory |
| `dakera_extract` | Extract entities and structure from free-form text |
| `dakera_batch_forget` | Bulk delete by tags, type, or time range |
| `dakera_discover_tools` | Search the full tool catalog by keyword or tier |
| `dakera_load_tools` | Load full schemas for specific tools on demand |

### Profiles & token cost

| Profile | Tools | ~Tokens | How to enable |
|---|---|---|---|
| **core** | 14 | ~3,350 | Default — always loaded |
| **admin** | 34 | ~6,700 | `DAKERA_MCP_PROFILE=admin` |
| **power** | 79 | ~16,600 | `DAKERA_MCP_PROFILE=power` |
| **all** | 99 | ~19,950 | `DAKERA_MCP_PROFILE=all` |

Token figures are estimates (JSON bytes / 3). The attachment tools below count in `power` and `all`,
but only appear while the connected server has the feature on (see [Dakera v0.12](#dakera-v012)).

### Accessing additional tools

```
# In your agent: discover what's available
dakera_discover_tools(tier="power")
→ returns names + descriptions, no schemas loaded

# Load schemas for the tools you want
dakera_load_tools(tools=["dakera_consolidate", "dakera_agent_stats"])
→ returns full inputSchema for each tool
```

### Profile selection

The profile controls which tools appear in `tools/list`. Three ways to set it:

**1. Per-request** (in `tools/list` params):
```json
{"profile": "power"}
```

**2. Environment variable** (applies to all requests):
```bash
DAKERA_MCP_PROFILE=power
```

**3. Default**: `core` (14 tools, ~3,350 tokens)

---

## Dakera v0.12

dakera-mcp 0.11 works against **Dakera v0.11.108 and v0.12.0** servers. Every v0.11 tool keeps
its name and arguments; the v0.12 additions are optional arguments that are sent only when you
supply them, and tools that call v0.12 routes.

| Dakera server | dakera-mcp 0.11 |
|---|---|
| v0.12.0 | every tool; the attachment tools while `DAKERA_ATTACHMENTS` (and `DAKERA_VISION` for images) is on |
| v0.11.108 | every v0.11 tool unchanged; the attachment tools, `dakera_encryption_status` and `dakera_embed_migration_status` are not listed (a direct call says they need v0.12); `dakera_encryption_rotate_key` needs `new_key` |
| older | not tested |

### What is new

| Tool | Tier | Needs | What it does |
|---|---|---|---|
| `dakera_capabilities` | power | v0.12 | `GET /v1/capabilities`: active model, search mode, scoring strategy, accepted `lang` values, which opt-in features are on. On a v0.11 server it answers `capabilities_available: false` |
| `dakera_health` | power | any | `GET /health`: status and version; on v0.12 also `degraded`, `config_warnings`, `embed_migration` |
| `dakera_wake_up` | power | any | an agent's startup context in one call: its top memories by importance x recency, no query, no embedding |
| `dakera_embed_migration_status` | admin | v0.12 | progress of the one-time background re-embed after the upgrade |
| `dakera_encryption_status` | admin | v0.12 | the encryption keyring and the background re-seal (never key material) |
| `dakera_encryption_rotate_key` | admin | v0.11+ | `new_key` is optional on v0.12 once encryption is on (the server generates a key; with encryption off, `new_key` turns it on); `wait_secs` (at most 20); `namespace` rotates one namespace |
| `dakera_attachment_upload` / `_list` / `_download` / `_delete` | power | v0.12 + `DAKERA_ATTACHMENTS` | files (or text) a memory can reference with `dakera_store` `attachment_ref` |
| `dakera_attachment_transcribe` | power | v0.12 + `DAKERA_ATTACHMENTS` | WAV speech to text in any language the model knows (`lang` forces one) into a memory; a background job, `wait_seconds` (at most 45) waits for it |
| `dakera_attachment_index_image` | power | v0.12 + `DAKERA_ATTACHMENTS` + `DAKERA_VISION` | PNG page as a visual memory (use an agent dedicated to images: the visual lane stores page vectors) |
| `dakera_attachment_job` | power | v0.12 + `DAKERA_ATTACHMENTS` | status of a transcription / index job |

Per-request **`lang`** (`en`, `de`, `fr`, `es`, `it`, `pt`, `nl`; v0.12) is accepted by `dakera_store`,
`dakera_recall`, `dakera_recall_associated`, `dakera_search`, `dakera_memory_update`, `dakera_extract`,
`dakera_auto_tag` and the attachment jobs;
`dakera_store` also takes `attachment_ref` (`sha256:<hex>` of an attachment in the agent's own namespace,
`_dakera_agent_<agent_id>`). Neither is sent unless given, so the same calls work on a v0.11.108 server.

Other optional arguments (all servers): `ttl_seconds` and `metadata` on `dakera_store`; `tags`,
`memory_type` and `session_id` filters on `dakera_recall`; `limit` on `dakera_batch_recall`;
`limit` / `offset` on `dakera_session_list`, `dakera_session_memories` and `dakera_agent_sessions`
(the server pages at 50); `memory_type` on `dakera_knowledge_deduplicate`; `dedup_on_store` /
`dedup_threshold` on `dakera_memory_policy_set`.

### Features the server has off are left out

The opt-in features (attachments, speech to text, image indexing) are off by default on the server.
dakera-mcp asks `GET /v1/capabilities` (once a minute, 3 s timeout) before it lists tools:

* `attachments` off, or a server without `/v1/capabilities` (v0.11): the `dakera_attachment_*` tools are not
  listed and not returned by `dakera_discover_tools`; a direct call answers with the variable to set
  (`DAKERA_ATTACHMENTS=1`) and makes no request.
* `vision` off: `dakera_attachment_index_image` is left out (`DAKERA_VISION=1` turns it on).
* A server without `/v1/capabilities` (v0.11) also hides `dakera_encryption_status` and
  `dakera_embed_migration_status`, whose routes are new in v0.12.
* The server cannot be asked (down, starting, key refused): nothing is hidden (asked again after 10 s).

The default `core` profile has no opt-in tools, so it never makes that request.

### Errors

Error answers keep the server's text and add a `Hint:` line for the v0.12 cases: a key **pinned to
namespaces** gets `403` on node-wide `/admin` routes (backups, encryption, quotas, config); backup
download, upload and restore need `super_admin`; `413` (body over a limit, or a hard quota), `501`
(feature off), `503` (`Retry-After`, which the retry logic now honours, up to 8 s) and `429` (rate
limit). A route the server lacks (an older server) and a v0.11 rotation without `new_key` get a hint
too. A request that timed out is retried only when it is safe to repeat (GET, PUT, DELETE): a store,
an import or a key rotation is never sent twice.

---

## Run Dakera

The MCP server connects to a Dakera memory server. You need one running first:

```bash
docker run -d \
  --name dakera \
  -p 3000:3000 \
  -e DAKERA_ROOT_API_KEY=dk-mykey \
  ghcr.io/dakera-ai/dakera:latest
```

For persistent storage (recommended):

```bash
curl -sSfL https://raw.githubusercontent.com/Dakera-AI/dakera-deploy/main/docker-compose.yml \
  -o docker-compose.yml
DAKERA_API_KEY=dk-mykey docker compose up -d

curl http://localhost:3000/health  # → {"status":"ok"}
```

Full deployment guide (Docker Compose, Kubernetes, Helm): [dakera-deploy](https://github.com/Dakera-AI/dakera-deploy)

---

## Install

### npm / npx (Node.js 18+)

```bash
# Global install
npm install -g @dakera-ai/dakera-mcp

# Or run directly without installing
npx @dakera-ai/dakera-mcp
```

### Homebrew (macOS / Linux)

```bash
brew install dakera-ai/tap/dakera-mcp
```

### Cargo

```bash
cargo install dakera-mcp
```

### Docker

```bash
docker pull ghcr.io/dakera-ai/dakera-mcp:latest
```

### Binary download

Pre-built binaries for macOS, Linux, and Windows are available on the [releases page](https://github.com/Dakera-AI/dakera-mcp/releases).

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `dakera-mcp-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `dakera-mcp-x86_64-apple-darwin.tar.gz` |
| Linux x64 | `dakera-mcp-x86_64-unknown-linux-musl.tar.gz` |
| Linux arm64 | `dakera-mcp-aarch64-unknown-linux-musl.tar.gz` |
| Windows x64 | `dakera-mcp-x86_64-pc-windows-msvc.zip` |

---

## Connect

Add to `.mcp.json` (Claude Code) or `claude_desktop_config.json` (Claude Desktop):

```json
{
  "mcpServers": {
    "dakera": {
      "command": "dakera-mcp",
      "env": {
        "DAKERA_API_URL": "http://localhost:3000",
        "DAKERA_API_KEY": "your-key"
      }
    }
  }
}
```

To start with the power profile (exposes up to 79 tools):

```json
{
  "mcpServers": {
    "dakera": {
      "command": "dakera-mcp",
      "env": {
        "DAKERA_API_URL": "http://localhost:3000",
        "DAKERA_API_KEY": "your-key",
        "DAKERA_MCP_PROFILE": "power"
      }
    }
  }
}
```

## Why This Exists

AI agents forget everything when the session ends. Dakera fixes that. This MCP server gives your agent a persistent memory layer with zero infrastructure overhead — point it at a Dakera instance and it works.

The 14-tool default keeps your context window lean. The meta-tools let you expand on demand when you need advanced operations like bulk vector upsert, knowledge graph traversal, or memory federation.

→ [dakera.ai](https://dakera.ai) for hosted instance  
→ Self-host with [dakera-deploy](https://github.com/dakera-ai/dakera-deploy)

## Documentation

→ [Full docs](https://dakera.ai/docs)  
→ [MCP reference](https://dakera.ai/docs/mcp)

## Related

| Repo | What it is |
|---|---|
| [dakera-py](https://github.com/dakera-ai/dakera-py) | Python SDK |
| [dakera-js](https://github.com/dakera-ai/dakera-js) | TypeScript SDK |
| [dakera-cli](https://github.com/dakera-ai/dakera-cli) | CLI |
| [dakera-deploy](https://github.com/dakera-ai/dakera-deploy) | Self-host Dakera |

---

**[dakera.ai](https://dakera.ai)** · [Documentation](https://dakera.ai/docs) · [Request Early Access](https://dakera.ai#cta)

<sub>Part of the Dakera AI open-core ecosystem. Built with Rust. Self-hosted. Zero dependencies.</sub>
