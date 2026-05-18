# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.3] - 2026-05-18

### Added

- **Crate metadata for Glama.ai discoverability** — added `authors`, `homepage`, `documentation`,
  `readme`, `keywords`, and `categories` to Cargo.toml. Fixes "No maintainers listed" on Glama.ai
  registry. ([#98](https://github.com/Dakera-AI/dakera-mcp/pull/98))

## [0.10.2] - 2026-05-18

### Fixed

- **MCP pagination page size increased from 20 to 100** — all profiles (core=14, power=68, all=86)
  now return in a single page. Eliminates multi-page pagination that caused tool visibility issues
  in Claude Code and other MCP clients. ([#96](https://github.com/Dakera-AI/dakera-mcp/pull/96))

## [0.10.1] - 2026-05-18

### Added

- **MCP token/context optimizations** — 5 optimizations that cut tool description tokens by ~47%:
  description compression, property deduplication, default value removal, cursor-based pagination,
  and compact enum formatting. Core profile now ~2,400 estimated tokens (was
  ~4,500+). ([#88](https://github.com/Dakera-AI/dakera-mcp/pull/88))
- **Cross-compilation CI workflow** — automated release builds for 10 platform binaries:
  Linux (x64, ARM64, musl, ARMv7), macOS (ARM64, x64), Windows (x64, ARM64), and FreeBSD (x64).
  Triggers on GitHub Release publish.
  ([#87](https://github.com/Dakera-AI/dakera-mcp/pull/87))

### Fixed

- Updated documentation to reflect correct tool count: 14 core tools (86+ available via profiles).

## [0.10.0] - 2026-05-17

### Breaking Changes

- **Tool surface reduced from 169 to 14 default tools** — the MCP server now exposes only 14
  core memory tools by default, down from 169. Agents discover additional tools via the
  `dakera_discover_tools` meta-tool. Saves ~35K tokens per session.
  ([#83](https://github.com/Dakera-AI/dakera-mcp/pull/83))
- **Removed admin, analytics, and streaming tools** — `dakera_admin_*`, `dakera_analytics_*`,
  `dakera_streaming_*`, and redundant graph/vector tools have been permanently deleted (-1,672
  lines). If you relied on these tools, use the Dakera REST API directly.
  ([#84](https://github.com/Dakera-AI/dakera-mcp/pull/84))

### Added

- **Profile-based tool tiering** — three profiles (`minimal`, `standard`, `full`) control which
  tools are exposed. Set via `DAKERA_MCP_PROFILE` env var. `standard` is the default (14 tools).
  ([#83](https://github.com/Dakera-AI/dakera-mcp/pull/83))
- **`dakera_discover_tools` meta-tool** — agents can dynamically discover and load additional
  tools beyond the default set without exposing them all upfront.
  ([#83](https://github.com/Dakera-AI/dakera-mcp/pull/83))
- **25 integration tests against live Dakera Docker** — end-to-end verification of store, recall,
  search, session lifecycle, and profile switching against a real Dakera instance.
  ([#85](https://github.com/Dakera-AI/dakera-mcp/pull/85))
- **47 deep profile tiering validation tests** — comprehensive coverage of tool visibility,
  profile transitions, and meta-tool behavior (190 → 237 total tests).
  ([#86](https://github.com/Dakera-AI/dakera-mcp/pull/86))

### Changed

- Consolidated `graph_traverse`/`graph_path`/`graph_export` into unified graph tools.
  ([#84](https://github.com/Dakera-AI/dakera-mcp/pull/84))
- Consolidated `vector_query`/`vector_multi_search`/`vector_explain` into unified vector tools.
  ([#84](https://github.com/Dakera-AI/dakera-mcp/pull/84))

### Chore

- Applied ARM rustfmt 1.9.0 formatting.

## [0.9.8] - 2026-05-13

### Chore

- Version bump for Docker image rebuild with MCP annotation.
  ([#72](https://github.com/Dakera-AI/dakera-mcp/pull/72))

## [0.9.7] - 2026-04-29

### Changed

- Bumped `thiserror` from 1.0.69 to 2.0.18 — internal error handling improvement, no public API
  change. ([#56](https://github.com/Dakera-AI/dakera-mcp/pull/56))
- Pinned Rust toolchain to 1.95.0 for reproducible builds.
  ([#59](https://github.com/Dakera-AI/dakera-mcp/pull/59))

### Fixed

- **Docker multi-arch release builds** — replaced `push-by-digest` with platform-tagged images
  (`VERSION-amd64`, `VERSION-arm64`) followed by `imagetools create` manifest merge. Self-hosted
  ARM runners had unreliable digest availability in GHCR, causing all Docker releases since v0.9.2
  to fail.
- Added release workflow concurrency control to prevent parallel tag pushes from racing.

### Chore

- Updated Cargo.lock patch dependencies.
  ([#60](https://github.com/Dakera-AI/dakera-mcp/pull/60))
- Added Dependabot configuration for Cargo and GitHub Actions.

## [0.9.6] - 2026-04-29

### Fixed

- **Concurrent request handling and crash recovery** — wraps `stdout` in `Mutex<Stdout>` and adds
  `catch_unwind` around tool handlers to prevent a single panicking tool from crashing the MCP
  process. Critical for Claude Code which issues concurrent tool calls over stdio.
  ([#54](https://github.com/Dakera-AI/dakera-mcp/pull/54))

### Security

- Bumped `rustls-webpki` from 0.103.12 to 0.103.13 (follow-up security patch).
  ([#52](https://github.com/Dakera-AI/dakera-mcp/pull/52))

## [0.9.5] - 2026-04-29

### Fixed

- **Retry with exponential backoff, connection pooling, and tool timeout** — prevents transient
  server errors from surfacing as MCP tool failures. Adds `reqwest` connection pooling for
  keep-alive reuse and a 60-second per-tool timeout.
  ([#53](https://github.com/Dakera-AI/dakera-mcp/pull/53))
- **Nest filter params in `batch_recall` and `batch_forget`** — filters like `tags`,
  `min_importance`, `created_after` were sent as top-level fields instead of nested under the
  correct request body structure.
  ([#51](https://github.com/Dakera-AI/dakera-mcp/pull/51))

### CI

- Removed GHCR digest validation loop from release workflow (DAK-2005).
  ([#50](https://github.com/Dakera-AI/dakera-mcp/pull/50))

## [0.9.4] - 2026-04-17

### CI

- Extended GHCR digest retry window and upgraded to Node.js 24 actions.
  ([#45](https://github.com/Dakera-AI/dakera-mcp/pull/45))

### Dependencies

- Bumped `rand` from 0.9.2 to 0.9.4.
  ([#46](https://github.com/Dakera-AI/dakera-mcp/pull/46))
- **Security — rustls-webpki CVE patch**: Updated to `rustls-webpki 0.103.12` addressing
  GHSA-xgp8-3hg3-c2mh and GHSA-965h-392x-2mh5 (CVSS 2.2 LOW).
  ([#48](https://github.com/Dakera-AI/dakera-mcp/pull/48))

## [0.9.3] - 2026-04-08

### Added

**MCP-5 — Cognitive MCP Tools** (server `v0.9.6+` / `v0.9.9+`)

Three new tools for managing per-namespace memory policies and Deep Associative Recall directly from
MCP clients:

- `dakera_memory_policy_get` — read the `MemoryPolicy` for a namespace. Returns all COG-1, COG-3,
  and SEC-5 policy fields (differential TTLs, per-type decay curves, spaced-repetition settings,
  consolidation config, and rate limits). Wraps `GET /v1/namespaces/:namespace/memory_policy`.
  Requires Read scope. Server `v0.9.6+`.
- `dakera_memory_policy_set` — partial-update the `MemoryPolicy` for a namespace. Performs a
  GET-then-PUT merge internally so omitted fields are preserved; no need to supply the full schema.
  Wraps `PUT /v1/namespaces/:namespace/memory_policy`. Requires Write scope. Server `v0.9.6+`
  (SEC-5 rate-limit fields require `v0.9.9+`).
- `dakera_recall_associated` — dedicated Deep Associative Recall (KG-3). Wraps
  `POST /v1/memory/recall` with `include_associated: true` and exposes the KG-3 parameters
  directly: `associated_memories_depth` (1–3 hops, default 1) and
  `associated_memories_min_weight` (0.0–1.0, default 0.0). Response includes `memories` (primary
  hits) and `associated_memories` (KG neighbours, each with a `depth` field). Requires Read scope.
  Server `v0.9.8+`.

Total MCP tool count: **80 → 83**.

### Changed

- Updated `README.md` to reflect open-core model and current product positioning. (PR#43)
- Docs: expanded Available Tools reference from 45 → 83 tools with version badge. (PR#41)
- Docs: filled changelog history gap v0.4.1 → v0.9.1. (PR#40)

### CI

- Release workflow: added GHCR propagation validation step before manifest creation to prevent
  partial-push releases. (PR#42)

---

## [0.9.1] - 2026-04-01

### Changed

- `dakera_recall`: added `include_associated` boolean parameter (default `false`) — when `true`,
  the server traverses the knowledge graph from each primary result and includes neighbouring
  memories in the response. Backed by COG-2 (server `v0.9.6+`). (PR#36)
- `dakera_recall`: added `since` and `until` ISO-8601 string parameters for time-window recall.
  Both are optional and can be combined with other filters. Backed by CE-7 (server `v0.9.7+`).
  (PR#38)

---

## [0.9.0] - 2026-03-31

### Added

**ODE-2 — dakera-ode Entity Extraction Sidecar**

- `dakera_extract_entities` — run entity extraction via the dakera-ode sidecar. Calls
  `POST /ode/extract` on `DAKERA_ODE_URL` (default: `http://localhost:8080`). Accepts `content`,
  `agent_id`, and optional `memory_id` / `entity_types` filters. Returns
  `{ entities: [{text, label, start, end, score}], model, processing_time_ms }`.
  Requires `DAKERA_ODE_URL` to be configured. (PR#33)

**KG-2 — Knowledge Graph Query & Export** (server `v0.9.8+`, backed by `/v1/knowledge/*`)

- `dakera_kg_traverse` — BFS traversal from a root memory. Accepts `edge_type`, `min_weight`,
  `max_depth` (default 3), and `limit` filters. (PR#34)
- `dakera_kg_query` — filter-based graph query without requiring a root node; useful for
  neighbourhood exploration across an agent's full graph. (PR#34)
- `dakera_kg_export` — export the knowledge graph as `json` (structured edges) or `graphml`
  (XML compatible with Gephi / Cytoscape / yEd). (PR#34)

Total MCP tool count: **76 → 79 → 80** (KG-2 → v0.9.1 params, no new tools).

---

## [0.8.0] - 2026-03-30

### Added

**EXT-1 — Pluggable Extraction Providers** (server `v0.9.x+`)

- `dakera_extract` — run extraction via the full provider hierarchy (per-request override →
  namespace default → server default → GLiNER local). Supports `gliner`, `openai`, `anthropic`,
  `openrouter`, `ollama`, `none`. Wraps `POST /v1/extract`. (PR#29)
- `dakera_extractor_get` — read the namespace-level default extractor configuration.
  Wraps `GET /v1/namespaces/:ns/extractor`. (PR#29)
- `dakera_extractor_set` — set or clear the namespace default extractor.
  Wraps `PATCH /v1/namespaces/:ns/extractor`. (PR#29)

**SEC-3 — AES-256-GCM Encryption Key Rotation** (server `v0.9.x+`)

- `dakera_encryption_rotate_key` — zero-downtime key rotation; re-encrypts all memories under
  the new key in a single pass. Accepts an optional namespace filter. Requires SuperAdmin scope.
  Wraps `POST /admin/encryption/rotate-key`. (PR#29)

Total MCP tool count: **72 → 76**.

---

## [0.7.0] - 2026-03-30

### Added

**MCP-4 (CE-4) — Entity Extraction Tools** (server `v0.9.x+`)

- `dakera_auto_tag` — extract named entities from text via the GLiNER NER pipeline (with a
  rule-based pre-pass). Wraps `POST /v1/memories/extract`. (PR#25)
- `dakera_entity_types_set` — enable entity extraction and configure entity types per namespace.
  Wraps `PATCH /v1/namespaces/:ns/config`. (PR#25)
- `dakera_memory_entities` — retrieve the entity tags stored on a memory at write time.
  Wraps `GET /v1/memory/entities/:id`. (PR#25)

**MCP-4 (CE-5) — Memory Knowledge Graph** (server `v0.9.x+`)

- `dakera_graph_traverse` — BFS traversal from a memory node with configurable depth (1–5,
  default 3). Wraps `GET /v1/memories/:id/graph`. (PR#26)
- `dakera_graph_path` — shortest path between two memories.
  Wraps `GET /v1/memories/:id/path`. (PR#26)
- `dakera_graph_link_memory` — create an explicit `linked_by` edge between two memories.
  Wraps `POST /v1/memories/:id/links`. (PR#26)
- `dakera_graph_export` — export the full knowledge graph for an agent.
  Wraps `GET /v1/agents/:id/graph/export`. (PR#26)

**DX-1 — Memory Import / Export**

- `dakera_memory_export` — export agent memories to JSONL, CSV, Mem0, or Zep format.
  Wraps `GET /v1/agents/:id/memories/export`. (PR#28)
- `dakera_memory_import` — import memories from any supported format (auto-detected).
  Wraps `POST /v1/agents/:id/memories/import`. (PR#28)

**SEC-1 — Namespace-Scoped API Keys**

- `dakera_namespace_key_create` — create an API key scoped to a specific namespace. (PR#28)
- `dakera_namespace_key_list` — list keys with access to a namespace. (PR#28)
- `dakera_namespace_key_delete` — revoke a namespace key. (PR#28)
- `dakera_namespace_key_usage` — get usage statistics for a namespace key. (PR#28)

**INT-1 — Memory Feedback**

- `dakera_memory_feedback` — upvote, downvote, or flag a memory for quality signals. (PR#28)
- `dakera_memory_feedback_get` — retrieve the feedback history for a specific memory. (PR#28)
- `dakera_agent_feedback_summary` — aggregate feedback summary across all memories for an agent.
  (PR#28)

Total MCP tool count: **54 → 72** (16 CE-4/CE-5 tools from internal v0.5.0/v0.6.0 + DX-1/SEC-1/INT-1).

---

## [0.4.1] - 2026-03-23

### Fixed

- `dakera_hybrid_search`: `vector` parameter is now optional, enabling BM25-only full-text
  (BM25) search without requiring a pre-computed embedding vector. Fixes DAK-679. (PR#20)

---

## [0.4.0] - 2026-03-23

### Added

- `dakera_decay_config_get` — returns the current memory-decay configuration (`strategy`,
  `half_life_hours`, `min_importance`). Wraps `GET /admin/decay/config` (Admin scope). Implements DECAY-1.
- `dakera_decay_config_set` — updates decay settings at runtime without a server restart.
  Accepts optional `strategy` (`exponential` | `linear` | `step`), `half_life_hours` (> 0), and
  `min_importance` (0.0–1.0). Wraps `PUT /admin/decay/config` (Admin scope). Implements DECAY-1.
- `dakera_decay_stats` — returns cumulative decay counters (`total_decayed`, `total_deleted`,
  `cycles_run`, `last_run_at`) plus per-cycle detail for the most recent run.
  Wraps `GET /admin/decay/stats` (Admin scope). Implements DECAY-2.
- `dakera_store` schema updated — new optional `expires_at` parameter (Unix timestamp in seconds).
  Takes precedence over `ttl_seconds`; on expiry the memory is hard-deleted by the decay engine.

### Tests

- Unit tests for `dakera_decay_config_set` covering invalid strategy, zero/negative `half_life_hours`,
  and out-of-range `min_importance`; coverage for definitions length, non-empty descriptions, and
  unknown tool dispatch for the decay module.

## [0.3.2] - 2026-03-23

### Added

- `dakera_autopilot_status` — returns the live AutoPilot configuration plus last-run statistics
  (timestamps, memories deduped, clusters consolidated). Wraps `GET /admin/autopilot/status`
  (Admin scope). Implements PILOT-4.
- `dakera_autopilot_trigger` — forces an immediate AutoPilot cycle. Accepts `action` of `dedup`,
  `consolidate`, or `all`. Wraps `POST /admin/autopilot/trigger` (Admin scope). Implements PILOT-4.

### Tests

- Unit tests for `dakera_autopilot_trigger` covering invalid action, missing action, and unknown
  tool dispatch; and for `dakera_autopilot_status` / `dakera_autopilot_trigger` definitions.

## [0.3.1] - 2026-03-22

### Added

- Native binary release artifact: CI now builds and uploads a musl-linked `dakera-mcp` binary on
  every GitHub release (`ci: deploy native binary on release`, DAK-575).

### Tests

- Unit tests for `dakera_batch_recall` and `dakera_batch_forget` MCP tools, covering request
  serialization and response deserialization (test: unit tests for batch recall/forget, v0.3.0 gap).

### Security

- Pinned all `appleboy` GitHub Actions to exact SHA digests to harden CI against supply-chain
  attacks (`fix(ci): pin appleboy actions to exact versions`).

## [0.3.0] - 2026-03-22

### Added

- `dakera_batch_recall` — filter-based bulk recall without semantic search (CE-2, dakera server v0.7.0).
  Accepts `tags`, `min_importance`, `max_importance`, `created_after`, `created_before`, `memory_type`,
  and `session_id` predicates. Calls `POST /v1/memories/recall/batch`.
- `dakera_batch_forget` — filter-based bulk delete (CE-2, dakera server v0.7.0).
  Same predicate set as `dakera_batch_recall`. The server enforces at least one filter to prevent
  accidental full-namespace wipe. Calls `DELETE /v1/memories/forget/batch` with a JSON body.
- `delete_with_json` internal API client method to support DELETE requests carrying a JSON body.

## [0.2.2] - 2026-03-21

### Added

- `dakera_namespace_configure` — create-or-update (upsert) a namespace via `PUT /v1/namespaces/:namespace`.
  Added in dakera server v0.6.0. Requires Write scope.
- `dakera_knowledge_network_cross_agent` — build a cross-agent memory similarity network via
  `POST /v1/knowledge/network/cross-agent`. Added in dakera server v0.4.0 (DASH-A). Requires Admin scope.

## [0.2.1] - 2026-03-20

### Fixed

- Corrected `execute_tool` placement (moved before test module) and rustfmt formatting

### Added

- Unit tests for protocol types and tool helpers (DAK-173)

### Security

- Add explicit `GITHUB_TOKEN` permissions to CI workflow (#1)

### Chore

- Upgrade GitHub Actions runners to Node.js 24 compatible versions

## [0.2.0] - 2025-03-15

### Added

- MCP server with stdio JSON-RPC transport
- Memory tools: store, recall, search, get, update, importance, forget, consolidate
- Session tools: start, end, list, get, memories
- Agent tools: stats, memories, sessions
- Knowledge tools: graph, summarize, deduplicate
- Namespace tools: list, get, create, delete
- Vector tools: upsert, query, delete, batch query, bulk update, bulk delete, count, export, aggregate, multi-search, upsert columns, explain, warm, unified query
- Full-text search tools: index, search, delete, stats, hybrid search
- Inference tools: text query, upsert text, batch query text
- Docker support with multi-stage build
- Bearer token authentication support
