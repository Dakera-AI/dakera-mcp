# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
