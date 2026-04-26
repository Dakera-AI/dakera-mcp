---
description: Rust coding conventions for the Dakera core engine
globs: "*.rs"
---

# Rust Conventions

- Run `cargo fmt` before committing — CI enforces rustfmt
- Run `cargo clippy -- -D warnings` — CI enforces zero warnings
- Use `thiserror` for error types, not manual `impl Display`
- Prefer `tracing` over `println!` or `log` for instrumentation
- All public APIs must have doc comments
- Avoid allocations in hot loops — prefer `&str` over `String`, reuse buffers
- `crates/engine/` contains recall/ranking — benchmark changes here with full 1540Q LoCoMo bench
- Pin major versions in Cargo.toml, allow patch updates in Cargo.lock
- Any PR modifying `crates/engine/` recall/ranking or temporal logic requires full bench gate

# CE (Core Engine) PR Process (CEO directive 2026-04-24, post CE-44/45/46 revert)

- **ONE category per CE**: Each CE must target a single category (Cat1/Cat2/Cat3/Cat4) with a clear hypothesis and expected delta. No scatter-shot multi-category experiments.
- **Full 1540Q bench BEFORE merge**: Any CE touching recall/ranking/temporal must have a full 1540Q bench run that passes ALL gates before merge. Subset-100 is NOT sufficient for merge decisions.
- **NEVER lower CI baselines**: If a CE would require lowering `.bench-baseline.json` values to pass CI, it is a regression and must be reverted, not accommodated.
- **Query routing policy (founder directive 2026-04-24, corrected same day)**: Regex/keyword classification is BANNED (CE-26 to CE-42 proved ceiling ~64%). External LLM dependencies are BANNED (Dakera must remain self-contained with zero external deps). **Internal ML classifiers ARE allowed and encouraged** — founder confirmed "for ML yes that's the way to go." CE-44/CE-45/CE-46 were reverted for benchmark regressions, NOT because ML is wrong — re-approach with implementations that pass all bench gates. Acceptable improvements: BM25 tuning, vector weights, fetch_n multiplier, temporal reasoning, bench pipeline data fixes, AND internal ML-based query classification/routing.
- **BM25 search features ≠ keyword/regex (founder directive 2026-04-25, DAK-2388)**: English stemming, tokenization, stop-word filtering, and other BM25 fulltext search features are NOT "keyword matching" or "regex." They are search index quality improvements. The no-regex rule applies ONLY to query CLASSIFICATION/ROUTING (deciding what type of query it is), NOT to how the search index processes and matches text. Stemming etc. are fully allowed and encouraged.
- **No thrashing**: If a CE requires multiple fixup commits (wire → fix regression → disable → recalibrate), the original analysis was insufficient. Revert and re-analyze before recommitting.
