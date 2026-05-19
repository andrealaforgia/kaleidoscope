# DESIGN — cli-get-tier-subcommand-v0

Author: orchestrator (Crafty / Atlas quota exhausted at the agent
layer; orchestrator-direct DESIGN as fallback)
Date: 2026-05-19
Mode: Propose, application/components scope

## Pre-decided

- Application/components scope. The change is wholly inside
  `crates/kaleidoscope-cli`. No new system or domain layer.
- Rust idiomatic per `CLAUDE.md`.
- No stress analysis (trivial scope).

## DD1 — Function shape

`pub fn get_tier(tenant: &TenantId, data_dir: &Path, item_id: &str,
mut writer: impl Write) -> Result<(), Error>` in `lib.rs`. Mirror
of `list_items`'s shape minus the tier arg. The function:

1. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
   Box::new(NoopRecorder))`.
2. Calls `cinder.get_tier(tenant, &ItemId::new(item_id))`.
3. On `Some(tier)`, writes `tier=<lowercase>\n` to `writer` and
   returns `Ok(())`.
4. On `None`, returns `Err(Error::CinderMigrate(MigrateError::
   UnknownItem { tenant: tenant.clone(), item: ItemId::new(...)
   }))`. The same `UnknownItem` variant migrate already uses —
   no new Error variant added. The stderr line operators see is
   byte-identical to migrate's unknown-item line, which is the
   sibling experience.

## DD2 — Output shape

`tier=<lowercase>\n` (key=value, matches the stats subcommand's
convention for tier lines). Operators script with `cut -d= -f2`
if they want the raw value.

## DD3 — Error reuse

`Error::CinderOpen` for store-open failures. `Error::CinderMigrate
(MigrateError::UnknownItem)` for missing item. No new Error
variant.

## DD4 — Reuse Analysis

| Construct | Source | Reuse |
|-----------|--------|-------|
| `FileBackedTieringStore::open` | `cinder::store` | REUSE |
| `NoopRecorder` | `cinder::store` | REUSE |
| `ItemId::new` | `cinder::id` | REUSE |
| `Tier` + `tier_lowercase` | `kaleidoscope-cli::lib` | REUSE |
| `cinder_base(data_dir)` | `kaleidoscope-cli::lib` | REUSE |
| `Error::{CinderOpen, CinderMigrate}` | local | REUSE |
| `MigrateError::UnknownItem` | `cinder::store` | REUSE |
| `parse_positional` | `main.rs` | REUSE |

Zero CREATE NEW. No new public type. No new trait. No new
external dependency.

## DD5 — Out-of-scope confirmations

- Bulk get-tier (multiple item ids in one call) — deferred.
- JSON output — out of scope.
- `--observe-otlp` — `get_tier` is a read; Cinder's
  `MetricsRecorder` has no `record_get` method. Nothing to
  record. Not added.
- `get_entry` full output (placed_at, migrated_at) — deferred to
  a future feature; today's scope is current tier only.

## No new ADR

Reuses ADR-0001's free-function shape. No new public type, no
new abstraction. Place the new function alongside the existing
read-side functions (`list_items`) in `lib.rs`.

## DEVOPS handoff annotation

- Paradigm: Rust idiomatic
- External integrations: NONE
- Dependency footprint: ZERO new external crates
- CI gates: 5 existing inherit unchanged;
  `gate-5-mutants-kaleidoscope-cli` auto-covers via `--in-diff`
  on `crates/kaleidoscope-cli/**`
- Workspace changes: one new `[[test]]` block in `Cargo.toml`
- Mutation scope: `lib.rs` (new `get_tier` fn) + `main.rs`
  (`run_get_tier` + `run_get_tier_with` + print_usage update),
  100% kill rate target
