[![Docs](https://img.shields.io/badge/docs-dakera.ai-D4A843)](https://dakera.ai/docs)
# ⚡ dakera-mcp



[![CI](https://github.com/Dakera-AI/dakera-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Dakera-AI/dakera-mcp/actions/workflows/ci.yml) [![Crate](https://img.shields.io/crates/v/dakera-mcp?logo=rust)](https://crates.io/crates/dakera-mcp) [![License: MIT](https://img.shields.io/github/license/Dakera-AI/dakera-mcp)](LICENSE) [![Glama](https://glama.ai/mcp/servers/Dakera-AI/dakera-mcp/badge)](https://glama.ai/mcp/servers/Dakera-AI/dakera-mcp)
[![Docs](https://img.shields.io/badge/docs-dakera.ai-D4A843)](https://dakera.ai/docs)

MCP server for Dakera AI. 83 tools. Gives any MCP-compatible AI agent persistent, queryable memory in minutes.

Works with Claude, Claude Code, and any MCP-compatible framework.

Part of [Dakera AI](https://dakera.ai) — the memory engine for AI agents.

> The Dakera memory engine scores **87.6% on LoCoMo** (1,540 questions, standard eval) — [benchmark details](https://dakera.ai/benchmark)

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

```bash
cargo install dakera-mcp
```

Or with Docker:

```bash
docker pull ghcr.io/dakera-ai/dakera-mcp:latest
```

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

## What You Get

83 tools across 15 categories:

- **Memory** — store, recall, search, decay, importance scoring
- **Sessions** — create and manage agent sessions
- **Agents** — namespaces, stats, memory health
- **Knowledge** — graph construction, entity extraction, clustering, cross-agent network
- **Vectors** — upsert, query, hybrid search, batch operations
- **Full-Text** — BM25 index, search, stats
- **Operations** — health, metrics, backup, audit

## Why This Exists

AI agents forget everything when the session ends. Dakera fixes that. This MCP server gives your agent a persistent memory layer with zero infrastructure overhead — point it at a Dakera instance and it works.

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

*Part of the Dakera AI open core. The engine is proprietary. The tools are yours.*
