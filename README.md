# ⚡ dakera-mcp

[![CI](https://github.com/Dakera-AI/dakera-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Dakera-AI/dakera-mcp/actions/workflows/ci.yml) [![Crate](https://img.shields.io/crates/v/dakera-mcp?logo=rust)](https://crates.io/crates/dakera-mcp) [![npm](https://img.shields.io/npm/v/%40dakera-ai%2Fdakera-mcp?logo=npm)](https://www.npmjs.com/package/@dakera-ai/dakera-mcp) [![Downloads](https://img.shields.io/crates/d/dakera-mcp)](https://crates.io/crates/dakera-mcp) [![License: MIT](https://img.shields.io/github/license/Dakera-AI/dakera-mcp)](LICENSE) [![LoCoMo 88.2%](https://img.shields.io/badge/LoCoMo-88.2%25-22c55e?style=flat-square)](https://dakera.ai/benchmark) [![Glama](https://glama.ai/mcp/servers/Dakera-AI/dakera-mcp/badge)](https://glama.ai/mcp/servers/Dakera-AI/dakera-mcp) [![Docs](https://img.shields.io/badge/docs-dakera.ai%2Fdocs-3b82f6?style=flat-square)](https://dakera.ai/docs) [![dakera.ai](https://img.shields.io/badge/dakera.ai-website-22c55e?style=flat-square)](https://dakera.ai)

MCP server for Dakera AI. Gives any MCP-compatible AI agent persistent, queryable memory — with smart token management built in.

Works with Claude, Claude Code, and any MCP-compatible framework.

Part of [Dakera AI](https://dakera.ai) — the memory engine for AI agents.

> The Dakera memory engine scores **88.2% on LoCoMo** (1,540 questions, standard eval) — [benchmark details](https://dakera.ai/benchmark)

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
| **core** | 14 | ~2,964 | Default — always loaded |
| **admin** | 32 | ~5,975 | `DAKERA_MCP_PROFILE=admin` |
| **power** | 68 | ~13,014 | `DAKERA_MCP_PROFILE=power` |
| **all** | 86 | ~16,026 | `DAKERA_MCP_PROFILE=all` |

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

**3. Default**: `core` (14 tools, ~2,964 tokens)

---

## Run Dakera

The MCP server connects to a Dakera memory server. You need one running first:

```bash
docker run -d \
  --name dakera \
  -p 3300:3300 \
  -e DAKERA_ROOT_API_KEY=dk-mykey \
  ghcr.io/dakera-ai/dakera:latest
```

For persistent storage (recommended):

```bash
curl -sSfL https://raw.githubusercontent.com/Dakera-AI/dakera-deploy/main/docker-compose.yml \
  -o docker-compose.yml
DAKERA_API_KEY=dk-mykey docker compose up -d

curl http://localhost:3300/health  # → {"status":"ok"}
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
        "DAKERA_API_URL": "http://localhost:3300",
        "DAKERA_API_KEY": "your-key"
      }
    }
  }
}
```

To start with the power profile (exposes 68 tools):

```json
{
  "mcpServers": {
    "dakera": {
      "command": "dakera-mcp",
      "env": {
        "DAKERA_API_URL": "http://localhost:3300",
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

<sub>Part of the Dakera AI open-source ecosystem. Built with Rust. Self-hosted. Zero dependencies.</sub>
