# Mikado Plan — query-http-common-v0

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.

This document is the ordered, atomic, rollback-friendly plan DELIVER
will execute to land `query-http-common-v0`. Each step ends with a
green `cargo test --workspace` (the K1 invariant); each step has an
explicit rollback that returns the workspace to the state at the end
of the previous step. The plan is the Mikado Method applied to a
refactor whose acceptance gate is byte-identical wire behaviour (K2):
small reversible moves, with the green workspace as the safety net.

The plan ALSO names the structural sub-steps of the carpaccio slicing
in DISCUSS `story-map.md`: Step A maps to US-01 walking skeleton;
Steps B-D land the new crate's surface (US-01 caps, US-03 envelope,
US-02 parser, US-04 tenant); Steps E-G rewire the three consumers
(carpaccio slices 2-4); Step H closes the loop (US-05 integration
gate).

## Mikado tree

```text
Goal: live in a workspace where the read-side HTTP scaffold has a
single source of truth in `crates/query-http-common`, with byte-
identical wire behaviour pre and post extraction.
  |
  +-- Step H: Prune dead code from the three consumer crates
  |     requires:
  |       +-- Step G: Rewire trace-query-api (two handler arms)
  |             requires:
  |               +-- Step F: Rewire log-query-api
  |                     requires:
  |                       +-- Step E: Rewire query-api (walking skeleton on the rewire side)
  |                             requires:
  |                               +-- Step D: Extract parse_time_range_seconds + resolve_tenant_or_refuse
  |                                     requires:
  |                                       +-- Step C: Extract error_response + reason text consts
  |                                             requires:
  |                                               +-- Step B: Extract cap constants
  |                                                     requires:
  |                                                       +-- Step A: Scaffold the new crate
```

The graph is strictly linear (each step depends only on its
predecessor) because the new crate must EXIST before any code can be
moved into it, and the new crate must HAVE the surface before any
consumer can call into it. The plan is therefore Mikado-flavoured
(small reversible moves with a green-bar invariant) rather than
Mikado-graph-shaped (parallel branches converging on a goal).

## Step A: Scaffold the new crate (walking skeleton)

**Action**:
1. `mkdir -p crates/query-http-common/src`.
2. Create `crates/query-http-common/Cargo.toml`:
   - `name = "query-http-common"`, `version = "0.1.0"`,
     `edition.workspace = true`, `rust-version.workspace = true`,
     `license = "AGPL-3.0-or-later"`, `publish = false`,
     `description = "Shared HTTP read-side scaffold (caps, time-range parser, error envelope, fail-closed tenant seam) consumed by query-api, log-query-api, and trace-query-api."`,
     `repository = "https://github.com/andrealaforgia/kaleidoscope"`.
   - `[lib] name = "query_http_common"`, `path = "src/lib.rs"`.
   - `[dependencies]`: `axum = { version = "0.7", default-features = false, features = ["http1", "tokio", "query", "json"] }`,
     `serde = { workspace = true }`,
     `serde_json = { workspace = true }`,
     `aegis = { path = "../aegis", version = "0.1.0" }`.
   - `[lints.rust]` `unsafe_code = "forbid"`.
   - `[lints.clippy]` `all = { level = "warn", priority = -1 }`.
3. Create `crates/query-http-common/src/lib.rs` with just the AGPL
   header, the crate-level docs, `#![forbid(unsafe_code)]`, and one
   placeholder export: `pub fn _placeholder_for_walking_skeleton() {}`.
   (The placeholder is removed in Step B; it exists ONLY to make the
   crate non-empty and to give Step A a verifiable behaviour.)
4. Edit the workspace root `Cargo.toml`: add
   `"crates/query-http-common",` to the `members` array (alphabetic
   position adjacent to `crates/query-api`).

**Verification command**:
`cargo build --workspace && cargo test --workspace`

**Expected result**: both commands exit 0; the new crate compiles
empty; the placeholder appears as a public symbol; no behaviour
change anywhere else.

**Rollback**:
`git restore Cargo.toml && rm -rf crates/query-http-common`

## Step B: Extract the cap constants

**Action**:
1. Remove the placeholder from `crates/query-http-common/src/lib.rs`.
2. Define the two pub consts with the canonical doc comments mirroring
   the consumers' current docs:
   - `pub const MAX_WINDOW_SECONDS: u64 = 86_400;` (ADR-0050 Decision 1)
   - `pub const MAX_RESULT_ROWS: usize = 100_000;` (ADR-0050 Decision 2)
3. Add `#[cfg(test)] mod tests { use super::*; #[test] fn ...
   assert_eq!(MAX_WINDOW_SECONDS, 86_400); ... #[test] fn ...
   assert_eq!(MAX_RESULT_ROWS, 100_000); ... }` mirroring the three
   consumers' existing assertions.

**Verification command**:
`cargo test -p query-http-common && cargo build --workspace`

**Expected result**: the new crate's inline tests pass; the workspace
still builds (the consumers still declare their own copies of the
constants at this step; Step E onwards rewires them).

**Rollback**:
`git restore crates/query-http-common/src/lib.rs`

## Step C: Extract `error_response` and the reason text consts

**Action**:
1. Add to `crates/query-http-common/src/lib.rs`:
   - `pub const REASON_WINDOW_TOO_LARGE: &str = "window exceeds 86400 seconds";`
   - `pub const REASON_TOO_MANY_ROWS: &str = "result exceeds 100000 rows";`
   - `pub const REASON_INVERTED_TIME_BOUNDS: &str = "invalid time bounds: end is earlier than start";`
   - `pub const REASON_MISSING_TENANT_PREFIX: &str = "no tenant resolvable: the ";`
2. Add `use axum::http::StatusCode; use axum::response::{IntoResponse, Response}; use axum::Json; use serde_json::json;` at the top of the file.
3. Add
   ```rust
   pub fn error_response(status: StatusCode, reason: &str) -> Response {
       let body = json!({"status": "error", "error": reason});
       (status, Json(body)).into_response()
   }
   ```
4. Add an inline test that builds a response and asserts the body is
   `{"status":"error","error":"window exceeds 86400 seconds"}` byte-for-byte at status 400 (mirrors the three consumers' redaction tests). Use `axum::body::to_bytes` from the `body` feature (the `tokio` feature already enables `axum::body`).

**Verification command**:
`cargo test -p query-http-common && cargo build --workspace`

**Expected result**: the new test passes; the workspace still builds.

**Rollback**:
`git restore crates/query-http-common/src/lib.rs`

## Step D: Extract `parse_time_range_seconds` and `resolve_tenant_or_refuse`

**Action**:
1. Add to `crates/query-http-common/src/lib.rs`:
   - `use aegis::TenantId;`
   - Private helper:
     ```rust
     fn parse_epoch_seconds(raw: Option<&str>, field: &str) -> Result<u64, String> {
         let raw = raw.ok_or_else(|| format!("invalid time bounds: {field} is required"))?;
         let trimmed = raw.trim();
         let parsed: f64 = trimmed.parse()
             .map_err(|_| format!("invalid time bounds: {field} is not a number"))?;
         if !parsed.is_finite() || parsed < 0.0 {
             return Err(format!("invalid time bounds: {field} is out of range"));
         }
         Ok(parsed as u64)
     }
     ```
     (Verbatim copy from `crates/trace-query-api/src/lib.rs:367-377`,
     which is the canonical `Option<&str>` shape.)
   - Public function:
     ```rust
     pub fn parse_time_range_seconds(
         start: Option<&str>,
         end: Option<&str>,
     ) -> Result<(u64, u64), String> {
         let start_secs = parse_epoch_seconds(start, "start")?;
         let end_secs = parse_epoch_seconds(end, "end")?;
         if end_secs < start_secs {
             return Err(REASON_INVERTED_TIME_BOUNDS.to_string());
         }
         Ok((start_secs, end_secs))
     }
     ```
   - Public function:
     ```rust
     pub fn resolve_tenant_or_refuse(
         tenant: &Option<TenantId>,
         service_label: &str,
     ) -> Result<TenantId, Response> {
         match tenant {
             Some(t) => Ok(t.clone()),
             None => Err(error_response(
                 StatusCode::UNAUTHORIZED,
                 &format!(
                     "{REASON_MISSING_TENANT_PREFIX}{service_label} service refuses unscoped requests"
                 ),
             )),
         }
     }
     ```
2. Add inline tests in `#[cfg(test)] mod tests`:
   - Every error path of `parse_epoch_seconds`: missing (None),
     non-numeric, negative, NaN/infinite, valid zero, valid integer,
     fractional truncation, inverted bounds rejection, equal bounds
     accepted. Mirrors the union of the three consumers' inline
     parser tests (verified against `query-api/src/lib.rs:317-356`,
     `log-query-api/src/lib.rs:313-376`,
     `trace-query-api/src/lib.rs:419-470`).
   - `resolve_tenant_or_refuse`: `Some(t)` returns `Ok(t.clone())`;
     `None` returns `Err(_)` and the response body contains
     `service_label` between the prefix and the literal suffix; the
     body has the `{"status":"error","error":...}` envelope; the
     status is `UNAUTHORIZED` (401).
   - Redaction: the returned reason for a `secretvalue` input does
     NOT contain `secretvalue`; the cap reason consts do NOT contain
     `SECRET` or `Bearer`.

**Verification command**:
`cargo test -p query-http-common && cargo build --workspace`

**Expected result**: every inline test passes; the workspace still
builds (the three consumers still hold their own copies at this step).

**Rollback**:
`git restore crates/query-http-common/src/lib.rs`

## Step E: Rewire `query-api`

**Action**:
1. `crates/query-api/Cargo.toml`: add
   `query-http-common = { path = "../query-http-common", version = "0.1.0" }`
   under `[dependencies]` (adjacent to the existing `aegis` line).
2. `crates/query-api/src/lib.rs`:
   - Remove the local `pub const MAX_WINDOW_SECONDS` and `pub const
     MAX_RESULT_ROWS` declarations (lines 73 and 82). Add
     `pub use query_http_common::{MAX_WINDOW_SECONDS, MAX_RESULT_ROWS};`
     to preserve the existing `pub` surface (US-01 AC: backward
     compatibility).
   - Remove the local `fn parse_time_range_seconds`, `fn
     parse_epoch_seconds`, and the `#[cfg(test)] fn parse_time_range`
     wrapper (lines 241-275). Replace the call site at line 177
     with `match query_http_common::parse_time_range_seconds(Some(&params.start), Some(&params.end))`.
     `seconds_to_nanos` STAYS (line 279).
   - Remove the local `fn error_response` (lines 299-305). Every call
     to `error_response(...)` in the file becomes
     `query_http_common::error_response(...)`. The cap-arm literals
     at line 188 and line 217 become
     `query_http_common::REASON_WINDOW_TOO_LARGE` and
     `query_http_common::REASON_TOO_MANY_ROWS`.
   - Replace the inline `let tenant = match &state.tenant { ... }`
     block (lines 167-175) with
     ```rust
     let tenant = match query_http_common::resolve_tenant_or_refuse(
         &state.tenant, "query"
     ) {
         Ok(t) => t,
         Err(resp) => return resp,
     };
     ```
   - Remove the inline tests that addressed the removed helpers
     directly (the ones at lines 317-413 that test
     `parse_time_range`, `parse_epoch_seconds`, `MAX_*`, cap reason
     literals). The CANONICAL versions of these tests now live in
     `query-http-common`. The acceptance suite is the byte-identity
     gate that the WIRING is correct.

**Verification command**:
`cargo test -p query-api && cargo test -p query-http-common && cargo build --workspace`

**Expected result**: `query-api` tests are green; every existing
acceptance scenario in `tests/` still passes with byte-identical
response bodies (K2 for `query-api`). The workspace builds.

**Rollback**:
`git restore crates/query-api/`

## Step F: Rewire `log-query-api`

**Action**: same shape as Step E, applied to
`crates/log-query-api/`. Service label is `"log query"`.
`parse_min_severity` and its inline tests STAY (pillar-specific, not
duplicated). `seconds_to_nanos` STAYS.

Specifically:
1. `crates/log-query-api/Cargo.toml`: add
   `query-http-common = { path = "../query-http-common", version = "0.1.0" }`.
2. `crates/log-query-api/src/lib.rs`:
   - Replace local `MAX_*` consts with `pub use`.
   - Remove local `parse_time_range_seconds`, `parse_epoch_seconds`,
     `#[cfg(test)] parse_time_range`.
   - Replace local `error_response` with calls to
     `query_http_common::error_response`. Cap reason literals at
     line 150 and 185 become the consts.
   - Replace inline tenant `match` (lines 128-136) with
     `resolve_tenant_or_refuse(&state.tenant, "log query")`.
   - Update the parser call (line 140) to pass
     `Some(&params.start), Some(&params.end)`.
   - Remove inline tests now duplicated in `query-http-common`. Keep
     all `parse_min_severity` tests.

**Verification command**:
`cargo test -p log-query-api && cargo test -p query-http-common && cargo build --workspace`

**Expected result**: `log-query-api` tests are green;
`parse_min_severity` tests untouched; K2 holds.

**Rollback**:
`git restore crates/log-query-api/`

## Step G: Rewire `trace-query-api`

**Action**: same shape as Step E, applied to
`crates/trace-query-api/`. Service label is `"trace query"` for BOTH
handler arms (`handle_traces` lines 141-149, `handle_traces_by_id`
lines 241-249).

The parser call in `handle_traces` (line 162) is already
`(params.start.as_deref(), params.end.as_deref())` — the canonical
shape. NO wrapper change needed; the call becomes
`query_http_common::parse_time_range_seconds(params.start.as_deref(), params.end.as_deref())`.

`parse_trace_id`, `read_required_service`, and their tests STAY
(pillar-specific). `seconds_to_nanos` STAYS.

**Verification command**:
`cargo test -p trace-query-api && cargo test -p query-http-common && cargo build --workspace`

**Expected result**: every existing scenario in
`tests/slice_01_traces_read.rs` and the lookup-by-id scenarios still
green; K2 holds for trace-query-api.

**Rollback**:
`git restore crates/trace-query-api/`

## Step H: Prune dead code and verify zero warnings

**Action**:
1. Remove any unused `use` statements from the three consumer crates
   (most likely `axum::Json` and `serde_json::json` are now unused
   in the consumer because the only callers were the removed
   `error_response`). Run `cargo build --workspace` with
   `RUSTFLAGS="-D warnings"` to surface them.
2. Verify the K3 scaffolding-LOC counter:
   ```bash
   grep -E "MAX_WINDOW_SECONDS *=|MAX_RESULT_ROWS *=|fn parse_time_range_seconds|fn parse_epoch_seconds|fn error_response\(|match &state\.tenant" \
     crates/query-api/src/lib.rs \
     crates/log-query-api/src/lib.rs \
     crates/trace-query-api/src/lib.rs \
     | wc -l
   ```
   Expected: ≤ 30 (K3 target). Baseline was ~90.

**Verification command**:
`RUSTFLAGS="-D warnings" cargo build --workspace && cargo test --workspace`

**Expected result**: zero warnings; full workspace test suite green;
K1 (test-count parity), K2 (byte identity, asserted by the existing
acceptance tests), K3 (LOC ≤ 30) all hold.

**Rollback**:
`git restore crates/`

## Post-Mikado: Mutation gate (K4)

After Step H, run:
```bash
cargo mutants -p query-http-common --no-shuffle --in-diff
```

Expected: 100% kill rate (per ADR-0005 Gate 5; K4). If any mutant
survives, add the killing test to `crates/query-http-common/src/lib.rs`
inside the existing `#[cfg(test)] mod tests` and re-run. The
`--in-diff` flag scopes the run to the diff (faster CI feedback);
the full run is `cargo mutants -p query-http-common --no-shuffle`.

The mutation gate is NOT a Mikado step (it is not a code move) but
is the K4 acceptance and the M-5 closure criterion per ADR-0005 and
the residuality-followups-roadmap.

## Summary

Eight Mikado steps (A through H) plus the post-Mikado mutation gate.
Every step ends with a green workspace test suite (K1). Steps E
through G each verify byte-identical wire behaviour through the
existing acceptance suite (K2). Step H verifies the K3 LOC counter.
The post-Mikado mutation gate verifies K4. The plan is linear (each
step depends only on its predecessor) and every step has a one-line
`git restore` rollback.
