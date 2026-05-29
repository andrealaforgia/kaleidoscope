# DESIGN Decisions — log-body-regex-search-v0

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-29.
Mode: propose. Scope: application.

This wave pins five flags raised by DISCUSS (Luna, commit
`2400927`) for the slice that grows `GET /api/v1/logs` by ONE
optional query-string parameter `body_regex=<pattern>`, parallel
to the `body_contains` byte-substring filter shipped under
ADR-0055 (commit `1bfa609`) and audited by the gate-5 mutants
workflow shipped under `gate-5-mutants-lumen-v0` (commit `d96a807`).

## DESIGN Decisions

### DD1 — Regex compile location: handler-side, fail-fast 400 on syntax error

The regex is compiled ONCE per request in a new
`parse_body_regex` helper inside
`crates/log-query-api/src/lib.rs`, alongside `parse_min_severity`
(ADR-0052) and `parse_body_contains` (ADR-0055). A
`regex::Error` from `Regex::new(raw)` returns HTTP 400 with the
literal envelope `{"status":"error","error":"invalid body_regex"}`
via `query_http_common::error_response`. The store is NEVER
touched on the compile-failure path.

Rationale: a compile failure is a client error, not a substrate
error; it must arrive as 400, not 500. Pushing the compile down
to lumen would (a) couple the substrate to an HTTP error
shape, (b) defer the compile cost into the store call, where the
caller has already paid an entire round-trip before learning the
pattern is invalid, and (c) duplicate the fail-fast posture that
ADR-0046 Decision 3 already pins for `query-api`'s label
matchers ("Compile the regex matchers ONCE, before the row
scan"; verified at `crates/query-api/src/lib.rs:188-195`).
Handler-side compile is the established workspace shape; this
slice continues it.

### DD2 — `lumen::Predicate` field type: `body_regex: Option<Regex>` (compiled)

The new predicate field carries the COMPILED `regex::Regex`, not
the raw pattern string. The handler compiles ONCE in
`parse_body_regex` and hands the `Regex` to the predicate via a
`body_regex(regex: Regex) -> Self` builder; `Predicate::matches`
calls `regex.is_match(&record.body)`.

Rationale: `Predicate::matches` is a HOT path — the
`InMemoryLogStore` and `FileBackedLogStore` adapters both route
EVERY in-window record through `predicate.matches(r)` (verified
at `crates/lumen/src/store.rs:159-180` and
`crates/lumen/src/file_backed.rs:229-250`, per ADR-0055
Decision 11). Carrying a raw `Option<String>` and calling
`Regex::new` per record would dominate the per-record match cost
and re-pay a wasted parse on each record, breaching the
linear-time guarantee the `regex` crate offers. The
compile-once-then-match-many shape is the same one ADR-0046
pins for `query-api`'s matchers and the same one query-api
already implements at lib.rs:192.

Consequence (load-bearing): `regex::Regex` does NOT implement
`PartialEq` or `Eq`. The existing `#[derive(PartialEq, Eq)]` on
`Predicate` (verified at `crates/lumen/src/predicate.rs:24`)
MUST be dropped. The trait is not exercised in production paths;
lumen's acceptance suites compare predicate effects via
`matches`, not via `==` on the predicate itself. This is a
public-surface change captured by ADR-0056.

### DD3 — Length cap: 1024 bytes, INCLUSIVE; mirrors `MAX_BODY_CONTAINS_LEN`

A `body_regex` value whose byte length strictly exceeds 1024
bytes is rejected with HTTP 400 and the literal envelope
`{"status":"error","error":"invalid body_regex"}`. The cap value
matches `MAX_BODY_CONTAINS_LEN` (ADR-0055 Decision 5) exactly. A
new constant `MAX_BODY_REGEX_LEN: usize = 1024` lives next to
`MAX_BODY_CONTAINS_LEN` in `crates/log-query-api/src/lib.rs`.
The boundary is INCLUSIVE: 1024 bytes is served; 1025 bytes is
refused.

Rationale: operator-facing consistency. The 1024-byte cap is the
same rule operators learnt for `body_contains`; the same rule
applies to every body-related parameter. The cap is large enough
to accommodate any honest runbook-pasted regex (10s to 100s of
bytes is typical; even an exotic 1024-byte regex covers a
realistic worst case) and small enough to refuse abuse. The
ReDoS posture is the `regex` crate's linear-time guarantee
(ADR-0046 Decision 1); the byte cap is a second-line abuse
budget, not the primary defence.

### DD4 — Mutual exclusion vs `body_contains`: 400 with named literal

When BOTH `body_contains` AND `body_regex` are present on the
same request, the handler returns HTTP 400 with the literal
envelope
`{"status":"error","error":"specify body_regex or body_contains, not both"}`
via `query_http_common::error_response`. The store is NEVER
touched on this path. The mutual-exclusion check is performed
BEFORE `parse_body_regex` (and AFTER `parse_body_contains`,
since `body_contains` is parsed first to surface its own
empty / over-cap 400 ahead of the cross-check).

Rationale: ambiguity is a symptom of client bug, not a
semantic primitive. The question "what does it mean to send BOTH
a substring filter and a regex filter on the same body?"
deserves a deliberate answer — intersection? union? error? —
not a quiet AND default. Slice 01 returns the answer "error";
future slices MAY relax once a real operator use case earns the
testing surface (the dispatch grows from 6 to 8 reachable arms
and `Predicate::matches` carries both arms simultaneously). The
new literal text differs from `"invalid body_regex"` because
neither value is syntactically invalid; they are mutually
exclusive at this slice.

Pruning: the cross product `min_severity × body_contains ×
body_regex` has 8 logical states; mutual exclusion eliminates 2
(both body filters present, with or without severity). The
handler dispatches over 6 reachable states.

### DD5 — ADR-0056: YES (new ADR records the contract growth and the lumen dep + surface diff)

The slice lands a new ADR at `docs/product/architecture/adr-0056-log-body-regex-search.md`.

Three independent triggers warrant the ADR:

1. **Lumen public surface grows.** `Predicate` gains ONE new pub
   builder method whose signature mentions `regex::Regex`. The
   `cargo public-api` baseline diff is non-empty.
2. **Lumen direct-dependency tree grows.** `regex = "1"` is
   added to `crates/lumen/Cargo.toml`'s `[dependencies]`. The
   workspace's `Cargo.lock` already pins the crate to `1.12.3`
   via `query-api`'s direct dependency (ADR-0046); the new
   direct dep on `lumen` resolves to the same lock pin with
   zero `Cargo.lock` change. The dep tree of the storage crate
   is visible to downstream consumers.
3. **HTTP read contract grows.** `GET /api/v1/logs` accepts one
   new optional parameter on the same route, parallel to
   ADR-0055; the contract change deserves the same durable
   record. ADR-0056 cites ADR-0047 (origin), ADR-0050 (caps),
   ADR-0052 (sibling severity filter), and ADR-0055 (immediate
   predecessor); none are modified.

ADR-0056 is the next free slot (`ls
docs/product/architecture/adr-0056*` returns zero hits;
ADR-0055 is the latest).

## Reuse Analysis

| Component | File | Decision | Justification |
|---|---|---|---|
| `LogsParams` | `crates/log-query-api/src/lib.rs:104-114` | EXTEND (add `body_regex: Option<String>` field) | Additive growth parallel to the `body_contains` field added by ADR-0055 Decision 9. No new struct, no new module. |
| `Predicate` | `crates/lumen/src/predicate.rs:24-33` | EXTEND (add `body_regex: Option<Regex>` field, `body_regex(re)` builder, new `matches` arm, new `is_empty` clause) | Mirrors the ADR-0055 Decision 10 shape verbatim (one field + one builder + one `matches` arm + one `is_empty` clause). The predicate IS the workspace's single source of truth for "how a record is filtered". |
| `parse_body_regex` | `crates/log-query-api/src/lib.rs` (NEW free function next to `parse_body_contains`) | NEW | Mirrors `parse_body_contains` shape PLUS a `Regex::new` compile step. Lives next to the sibling helper so the per-helper rejection contract stays adjacent. |
| `query_http_common::error_response` | `crates/query-http-common/src/lib.rs` | REUSE | Second post-extraction validation of the M-5 scaffold (the first being ADR-0055). All three new 400 reason texts are served via the existing helper; no new envelope, no new status code. |
| `query_http_common::resolve_tenant_or_refuse` | `crates/query-http-common/src/lib.rs` | REUSE | Fail-closed tenancy seam unchanged from ADR-0054. The body_regex filter runs AFTER per-tenant isolation by construction (the tenant is the first argument to `query_with`). |
| `query_http_common::parse_time_range` | `crates/query-http-common/src/lib.rs` | REUSE | Unchanged from ADR-0054; the slice does not alter window parsing. |
| `query_http_common::MAX_WINDOW_SECONDS` / `MAX_RESULT_ROWS` / `REASON_*` | `crates/query-http-common/src/lib.rs` | REUSE | Caps and reason constants unchanged from ADR-0050 + ADR-0054. KPI-K4 asserts zero new copies in `log-query-api`. |
| `lumen::LogStore::query_with(&tenant, range, &Predicate)` | `crates/lumen/src/store.rs:89` | REUSE | Trait method signature stays byte-identical (Gate 2 `cargo public-api`). Both `InMemoryLogStore::query_with` and `FileBackedLogStore::query_with` already route per record through `predicate.matches(r)`; the new `matches` arm lights up automatically with zero adapter edit. |
| `regex` crate (workspace `Cargo.lock` pin `1.12.3`) | `crates/lumen/Cargo.toml` (NEW direct dep `regex = "1"`) | NEW DEP | The workspace already carries `regex = "1"` as a direct dep of `query-api` (ADR-0046 Decision 1; verified at `crates/query-api/Cargo.toml:62`). The new direct dep on `lumen` resolves to the same lock pin with zero `Cargo.lock` change. The version specifier `"1"` mirrors `query-api`'s spelling so a coordinated bump moves both crates together. |
| `gate-5-mutants-lumen` workflow | `.github/workflows/ci.yml` (shipped in `gate-5-mutants-lumen-v0`, commit `d96a807`) | REUSE | The workflow runs `cargo mutants --in-diff origin/main` scoped to the `lumen` crate at the 100% kill-rate gate (ADR-0005 Gate 5). The new `Predicate::body_regex` field, builder, `matches` arm, and `is_empty` clause are all picked up automatically by `--in-diff`; KPI-K5 asserts the 100% kill rate. No new CI job is needed. |
| `gate-5-mutants-log-query-api` workflow | `.github/workflows/ci.yml` (shipped pre-slice) | REUSE | Picks up the new `parse_body_regex` helper, the new `LogsParams` field, the new dispatch arm, and the new mutual-exclusion check via `--in-diff`. No new CI job is needed. |

## Architecture Summary

- Pattern: ports-and-adapters (unchanged). `lumen::LogStore` is
  the driven port; `query_http_common` provides shared HTTP
  scaffolding; `log-query-api` is the HTTP driving adapter for
  the log read pillar.
- Paradigm: Rust idiomatic — data + free functions + traits
  where polymorphism is genuinely needed. Verified by the
  workspace `CLAUDE.md` Development Paradigm section and the
  existing shape of `log-query-api` and `lumen`.
- Topology: ONE new direct dep edge (lumen -> regex); ZERO new
  crate; ZERO new module; ZERO new file under
  `crates/*/src/`; ZERO change to the route, the success
  envelope, the error envelope, the cap consts, the reason
  consts, or the `LogStore` trait signatures.
- Key components extended: `LogsParams` (one field),
  `Predicate` (one field, one builder, one `matches` arm, one
  `is_empty` clause), `parse_body_regex` (new free function).
- Component touched: `crates/log-query-api/src/lib.rs` (under
  40 net new LOC, KPI-K4 budget); `crates/lumen/src/predicate.rs`
  (about 12 net new lines, plus the `#[derive(...)]` drop of
  `PartialEq, Eq`); `crates/lumen/Cargo.toml` (one new
  `[dependencies]` line).

## Technology Stack

- Language: Rust 1.88 (workspace MSRV, `Cargo.toml:52`).
- Regex engine: `regex = "1"` (RE2-derived, linear-time, no
  catastrophic backtracking). Already direct in
  `crates/query-api/Cargo.toml:62`; promoted from transitive to
  direct on `crates/lumen/Cargo.toml` under this slice. Resolves
  to the `Cargo.lock` pin `1.12.3` with zero lockfile diff.
- HTTP: axum 0.7 (unchanged); serde / serde_json (unchanged).
- Shared scaffolding: `query-http-common` 0.1.0 (unchanged);
  consumed via `query_http_common::*` paths.
- Substrate: `lumen` (unchanged trait signatures); both
  adapters light up automatically.
- License: AGPL-3.0-or-later (unchanged); the `regex` crate is
  MIT/Apache-2.0 dual-licensed and compatible.

## DEVOPS Handoff

For `nw-platform-architect` (DEVOPS wave):

- **NO new crate.** The slice adds zero workspace members.
  `Cargo.toml` workspace list is unchanged.
- **NO new CI job.** The two existing mutation-testing workflows
  (`gate-5-mutants-lumen`, `gate-5-mutants-log-query-api`) pick
  up the new code via `cargo mutants --in-diff origin/main`. No
  new GitHub Actions YAML is required. Gates 1-7 from ADR-0005
  continue to apply to both crates with no configuration change.
- **ONE new direct dep.** `regex = "1"` is added to
  `crates/lumen/Cargo.toml` `[dependencies]`. Version specifier
  spelled `"1"` to mirror `query-api`'s spelling; `Cargo.lock`
  pin is `1.12.3` already and does not change.
- **Public-API baseline.** `cargo public-api` on `lumen` shows:
  (a) `Predicate` gains one new pub method (`body_regex(re:
  Regex) -> Self`), (b) the `#[derive(PartialEq, Eq)]` on
  `Predicate` is removed (a public-surface relaxation). Both
  changes are additive in the sense that they do not break any
  existing caller in the workspace; the crafter snapshots the
  new baseline as part of the DELIVER wave (Gate 2). On
  `log-query-api` the public surface is byte-identical
  (LogsParams is private; parse_body_regex is private).
- **No new external integration.** The slice is in-process
  string matching and an in-process trait call. No third-party
  API, no consumer-driven contract test recommendation.
- **No upstream-changes propagation.** Zero DISCUSS assumptions
  changed (the DISCUSS wave-decisions Upstream Changes section
  records "None" verbatim).
- **EDD coverage.** The Earned-Trust startup probe from
  ADR-0047 continues to run unchanged: the probe issues a
  parameter-less empty-range `query`, and the slice does not
  alter the probe shape. The three orthogonal Earned-Trust
  layers (compile-time subtype on `record.body: String` and
  `Predicate.body_regex: Option<Regex>`; AST structural via
  the acceptance suite's literal references to `body_regex` and
  `invalid body_regex`; behavioural gold-tests in
  `tests/slice_01_body_regex.rs`) are picked up automatically.

## Upstream Changes

**None.** Zero DISCOVER and DISCUSS assumptions changed. The
slice composes additively on top of ADR-0046, ADR-0047,
ADR-0050, ADR-0052, ADR-0054, ADR-0055, and
`gate-5-mutants-lumen-v0` without altering any of them. No
`design/upstream-changes.md` artefact is produced.
