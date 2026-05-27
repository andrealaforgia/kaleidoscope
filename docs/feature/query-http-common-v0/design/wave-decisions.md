# DESIGN Decisions — query-http-common-v0

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.

This document closes the six DISCUSS flags raised by `@nw-product-owner`
(Luna) in `docs/feature/query-http-common-v0/discuss/wave-decisions.md`,
pins the public API surface of the new workspace crate, names the
Mikado plan that DELIVER will execute, and prepares the handoff to
`@nw-platform-architect` (Apex) and `@nw-acceptance-designer` (DISTILL).

The feature is a CROSS-CUTTING REFACTOR (DISCUSS D1). No new endpoints,
no behaviour change, no impact on external clients. The acceptance gate
is byte-identical response bodies pre and post extraction (K2) and a
green workspace test suite at every Mikado step (K1).

## Key decisions

### DD1: One new workspace-internal crate `query-http-common`

Closes DISCUSS Flag 1. PIN: new top-level workspace crate at
`crates/query-http-common/`, library-only (no binary, no test
integration harness), `#![forbid(unsafe_code)]`, `0.1.0`,
AGPL-3.0-or-later, `publish = false`, added to the workspace `members`
array in the root `Cargo.toml`.

Rationale: ADR-0048 Decision 5 / Placement B and ADR-0053 Decision 5
both name `query-http-common` as the deferred extraction. A sub-module
of one of the three consumer crates would force the other two to
depend on the host's domain vocabulary (`pulse::TimeRange`,
`lumen::LogStore`, `ray::TraceStore`) and re-introduce the coupling
the ADRs explicitly warned against. The workspace crate is the
textbook shape and is the recorded intent.

### DD2: Public API surface (five items, minimum necessary)

Closes DISCUSS Flag 2. PIN: five exported items, no more.

| Symbol | Kind | Signature | Doc |
|--------|------|-----------|-----|
| `MAX_WINDOW_SECONDS` | `pub const u64` | `= 86_400` | maximum half-open window in whole seconds (ADR-0050 Decision 1) |
| `MAX_RESULT_ROWS` | `pub const usize` | `= 100_000` | maximum response-vector length, REFUSE-not-TRUNCATE (ADR-0050 Decision 2) |
| `parse_time_range_seconds` | `pub fn` | `fn(start: Option<&str>, end: Option<&str>) -> Result<(u64, u64), String>` | the canonical epoch-seconds parser; float-tolerant; rejects missing, non-numeric, out-of-range, inverted; redaction-symmetric (never echoes raw values) |
| `error_response` | `pub fn` | `fn(status: StatusCode, reason: &str) -> Response` | returns `{status:"error", error:"<reason>"}` JSON body at the given status; reason is borrowed (interpolated formats flow through unchanged) |
| `resolve_tenant_or_refuse` | `pub fn` | `fn(tenant: &Option<TenantId>, service_label: &str) -> Result<TenantId, Response>` | returns `Ok(t.clone())` on `Some(t)`; `Err(error_response(UNAUTHORIZED, "no tenant resolvable: the <label> service refuses unscoped requests"))` on `None`; the `service_label` interpolates into the reason text |

Plus four `pub const` reason text literals (see DD4 below).

Rationale: each item maps 1:1 to a duplicated surface in the three
consumer crates and a discovered maintainer pain. Adding more (a
structured `ErrorBody` newtype, a generic `TimeRange` builder, a
`FromRequestParts` Axum extractor) would be speculative; YAGNI applies
and the surface can grow when a fourth consumer joins.

The canonical parser shape is `Option<&str>` (the strictly more
permissive variant used today by `trace-query-api`). The two consumers
whose handlers extract `String` (`query-api`, `log-query-api`) wrap
their values with `Some(&...)` at the call site — a one-line mechanical
change per call site. `seconds_to_nanos` STAYS per-consumer because each
consumer builds its pillar-specific `TimeRange` (`pulse::TimeRange`,
`lumen::TimeRange`, `ray::TimeRange`); sharing it would force one of
those types into `query-http-common`, breaching the ADR-0048 Decision
5 cautioning.

### DD3: Error helper stays at `(StatusCode, &str) -> Response`

Closes DISCUSS Flag 3. PIN: keep the helper-function shape. No
`thiserror`, no custom `Error` enum, no `From` impls, no structured
`ErrorBody` newtype. The wire shape is the contract; the implementation
detail (`serde_json::json!`) is hidden behind the helper.

Rationale: the three consumer crates today already use this shape; a
structured `ErrorBody` would force a migration on every error arm
without enabling a maintainer decision. A future story can promote
the helper to a builder when a use case appears (e.g. a `request_id`
field), and the present signature is forward-compatible because the
extra field would be a new optional parameter rather than a breaking
shape change.

The reason parameter is `&str` rather than `&'static str` because the
parsers emit interpolated reasons (e.g. `format!("invalid time bounds:
{field} is not a number")`) that own their `String`s. A `&'static
str` parameter would force every caller through a redundant `.as_str()`
indirection or a parallel helper for the static-literal case.

### DD4: Cap reason texts as `pub const` literals

Closes DISCUSS Flag 4. PIN: promote the four reason texts to
`pub const` literals in `query-http-common`, callable as
`error_response(StatusCode::BAD_REQUEST, query_http_common::REASON_WINDOW_TOO_LARGE)`.

| Const name | Literal value | Pre-extraction call-site count |
|------------|---------------|--------------------------------|
| `REASON_WINDOW_TOO_LARGE` | `"window exceeds 86400 seconds"` | 3 (one per consumer crate) |
| `REASON_TOO_MANY_ROWS` | `"result exceeds 100000 rows"` | 4 (query-api x1, log-query-api x1, trace-query-api x2 — one per arm) |
| `REASON_INVERTED_TIME_BOUNDS` | `"invalid time bounds: end is earlier than start"` | inside `parse_time_range_seconds`, will collapse to one site post-extraction |
| `REASON_MISSING_TENANT_PREFIX` | `"no tenant resolvable: the "` | inside `resolve_tenant_or_refuse`; suffix interpolated from `service_label` parameter |

Rationale: the user-instruction brief on this DESIGN wave overrides the
DISCUSS Flag 4 recommendation (which had kept them at the call site)
because once the helpers are extracted, the cap-check arm itself ALSO
collapses to a one-line call at each consumer. Promoting the reason
text is the same edit and pre-empts the drift the K2 byte-identity
gate would otherwise be the only thing catching. The
`REASON_INVERTED_TIME_BOUNDS` and `REASON_MISSING_TENANT_PREFIX`
literals live INSIDE the helpers and never need to be referenced by
the consumer; they are exported for documentation and inline-test
addressability, NOT for direct consumption.

Redaction posture: every `pub const` is a static literal that NEVER
interpolates a request-derived value. `REASON_MISSING_TENANT_PREFIX` is
joined inside `resolve_tenant_or_refuse` with `service_label`, which is
itself a `&'static str` passed by each handler (`"query"`, `"log
query"`, `"trace query"`); no untrusted input flows into the body.

### DD5: ADR-0054 is written alongside this feature

Closes DISCUSS Flag 5. PIN: a small ADR-0054 lands with the slice.
~150 lines, Status Accepted, Date 2026-05-27, sections Context /
Decision / Consequences / Alternatives / References. Cross-references
ADR-0048 Decision 5 (the recorded "its own ADR" intent), ADR-0050 (the
caps origin), ADR-0052 (the sibling style), and ADR-0053 (the rule-of-
three-and-a-bit pin). All four cited ADRs remain UNCHANGED (ADR
immutability convention).

The next free ADR number is 0054 (verified: `ls
docs/product/architecture/adr-0054*` returns no hits;
`adr-0053-trace-lookup-by-id.md` is the latest).

Rationale: ADR-0048 Decision 5 explicitly named this as "its own
ADR"; landing the feature without one would leave the recorded plan
half-honoured. M-5 in `docs/residuality-followups-roadmap.md` ALSO
flags the ADR alongside the crate. A small new ADR is the lighter lift
and is the recorded intent.

### DD6: Tenant divergence is service-label-only

Closes DISCUSS Flag 6. VERIFIED: the four inline `match &state.tenant`
blocks differ ONLY on the service-label suffix:

| Site | File | Lines | Label |
|------|------|-------|-------|
| 1 | `crates/query-api/src/lib.rs` | 167-175 | `"query"` |
| 2 | `crates/log-query-api/src/lib.rs` | 128-136 | `"log query"` |
| 3 | `crates/trace-query-api/src/lib.rs` (`handle_traces`) | 141-149 | `"trace query"` |
| 4 | `crates/trace-query-api/src/lib.rs` (`handle_traces_by_id`) | 241-249 | `"trace query"` |

The envelope and the prefix are identical; the suffix differs.
`resolve_tenant_or_refuse(&state.tenant, service_label)` accepts the
suffix as a parameter, interpolates inside the helper, and emits the
shared envelope. The `service_label` values are static literals passed
by the handler (NOT request-derived), so the interpolation has no
redaction surface.

Rationale: this is the minimum useful extraction. A future story
could promote the helper to a proper Axum `FromRequestParts`
extractor; that is a larger change (it would touch the `ApiState`
shape and the router wiring) and is out of scope for this slice.

## Architecture Summary

- **Pattern**: hexagonal / ports-and-adapters preserved unchanged.
  `query-http-common` is a shared LIBRARY consumed by three adapter
  crates; it does NOT carry a port (no trait) and does NOT depend on
  a store. Pure data + free functions.
- **Paradigm**: Rust idiomatic (data + free functions; no inheritance,
  no `dyn` where generics suffice). Matches the project's existing
  paradigm pin in `CLAUDE.md`.
- **Key components**: one new crate (`query-http-common`); three
  existing crates rewired (`query-api`, `log-query-api`,
  `trace-query-api`); zero new external dependencies.
- **C4 view**: see `application-architecture.md`. L1 System Context
  shows kaleidoscope-gateway routing to the three read APIs which now
  share `query-http-common`. L2 Container zooms into the shared crate
  and its public API. No L3 needed: the shared crate is 60 lines of
  flat free functions.

## Reuse Analysis

| Existing Component | File | Overlap | Decision | Justification |
|--------------------|------|---------|----------|---------------|
| `MAX_RESULT_ROWS` const | `crates/query-api/src/lib.rs:82`, `crates/log-query-api/src/lib.rs:77`, `crates/trace-query-api/src/lib.rs:84` | identical const (three copies, same value) | EXTRACT to `query-http-common`, `pub use` in consumers | single source of truth; mutation kill rate becomes meaningful only with one site; rule of three reached |
| `MAX_WINDOW_SECONDS` const | `crates/query-api/src/lib.rs:73`, `crates/log-query-api/src/lib.rs:70`, `crates/trace-query-api/src/lib.rs:77` | identical const (three copies, same value) | EXTRACT | same |
| `parse_time_range_seconds` fn | `crates/query-api/src/lib.rs:254`, `crates/log-query-api/src/lib.rs:221`, `crates/trace-query-api/src/lib.rs:354` | duplicated body; `&str` vs `Option<&str>` parameter divergence | EXTRACT with the more permissive `Option<&str>` shape; the two `&str` callers wrap with `Some(&...)` at the call site | DISCUSS US-02 acceptance criteria; trace-query-api already uses the canonical shape |
| `parse_epoch_seconds` fn | `crates/query-api/src/lib.rs:266`, `crates/log-query-api/src/lib.rs:233`, `crates/trace-query-api/src/lib.rs:367` | private helper of `parse_time_range_seconds`; same divergence | EXTRACT as private helper of the new crate | follows its parent |
| `error_response` fn | `crates/query-api/src/lib.rs:299`, `crates/log-query-api/src/lib.rs:294`, `crates/trace-query-api/src/lib.rs:399` | identical body (`{"status":"error","error":reason}`) | EXTRACT | byte-identical bodies; K2 |
| inline `match &state.tenant` (4 sites) | `crates/query-api/src/lib.rs:167-175`, `crates/log-query-api/src/lib.rs:128-136`, `crates/trace-query-api/src/lib.rs:141-149` and `:241-249` | duplicated `match` block; differ only on `<label>` suffix in the 401 reason | EXTRACT `resolve_tenant_or_refuse(tenant, service_label)` | rule of three and a bit; identical envelope; same redaction posture |
| `seconds_to_nanos` fn | `crates/query-api/src/lib.rs:279`, `crates/log-query-api/src/lib.rs:246`, `crates/trace-query-api/src/lib.rs:381` | identical body | KEEP per consumer | each consumer builds its pillar-specific `TimeRange`; sharing would force `pulse::TimeRange`/`lumen::TimeRange`/`ray::TimeRange` into `query-http-common`, breaching ADR-0048 Decision 5 |
| cap reason text literals | x4 sites (3 windows + 4 results in 3 crates) | identical string literals | EXTRACT as `pub const` (DD4) | pre-empt drift the K2 gate would otherwise catch on a per-test basis |
| `parse_min_severity`, `parse_trace_id`, `read_required_service`, `success_response` | per-crate | pillar-specific, not duplicated | KEEP per consumer | each is wired to a pillar-specific type (`lumen::SeverityNumber`, `ray::TraceId`, `ray::ServiceName`, `Vec<ray::Span>` / `Vec<lumen::LogRecord>` / `Vec<matrix::PromMatrixEntry>`); no shared surface possible without crossing the dependency boundary |

## Technology Stack

NO new external dependencies. The new crate depends only on substrate
already in the workspace:

- `axum = "0.7"` (with the `http1`, `tokio`, `query`, `json` features
  the three consumer crates already use) — for `StatusCode`,
  `Response`, `IntoResponse`, `Json`. Workspace pin unchanged.
- `serde = { workspace = true, features = ["derive"] }` — workspace
  pin unchanged.
- `serde_json = { workspace = true }` — workspace pin unchanged.
- `aegis = { path = "../aegis", version = "0.1.0" }` — for `TenantId`.

License: AGPL-3.0-or-later (mirrors the three consumer crates; the new
crate is consumed only by them, so the licence aligns).

## Constraints Established

- The new crate is `publish = false`; it is workspace-internal and
  carries no public SemVer commitment beyond `0.1.0`.
- NO `1.0.0` bump on any crate (project policy; SemVer 1.0 is
  Andrea's call, not the agent's).
- `#![forbid(unsafe_code)]` mirrors the three consumers.
- The new crate does NOT depend on `pulse`, `lumen`, or `ray` (the
  pillar stores). Crossing this boundary would force a domain type
  into the shared crate and break the symmetry that justifies the
  extraction.
- The mutation kill-rate gate per ADR-0005 Gate 5 is 100% on the new
  crate after Mikado completes (K4).
- Mikado plan execution order is fixed (A through H, see
  `mikado-plan.md`). The order is dependency-driven: scaffold first,
  extract pure-data next (consts), then helpers (error envelope,
  parser, tenant), then rewire consumers one at a time, then prune.
- Each Mikado step must end with `cargo test --workspace` green
  before the next step starts.
- Architectural enforcement (Rust-idiomatic): the
  `#![forbid(unsafe_code)]` lint is the workspace-default policy
  marker; the `regex` import boundary is preserved by NOT pulling it
  into the shared crate. A Rust-idiomatic enforcement equivalent of
  ArchUnit is not adopted here — the shared crate is small enough
  (one file, ~60 lines) that grep-based assertions in the slice
  brief plus the K3 LOC counter are the proportional gate.

## Earned Trust posture (Principle 12)

The new crate carries NO driven adapter (no filesystem, no network,
no subprocess, no vendor SDK, no clock, no configuration source). It
is pure data + free functions over `&str`, `Option<&str>`, and
`TenantId`. There is NO substrate that can lie at this boundary;
therefore there is no `probe()` to define. The Earned Trust
discipline lands ONE level deeper: at the three CONSUMER binaries
(`query-api`, `log-query-api`, `trace-query-api`), whose composition
roots already run their own startup probes (ADR-0042 metrics probe,
the lumen and ray probes per ADR-0047 / ADR-0048). Those probes are
UNCHANGED by this refactor; the consumer composition roots continue
to own the wire-then-probe-then-use invariant. The K2 byte-identity
gate (every 400 and 401 acceptance test green pre and post) is the
runtime evidence that the new crate has not silently changed the
contract under the existing probes.

## DEVOPS Handoff

For `@nw-platform-architect` (Apex). Reads:
`docs/feature/query-http-common-v0/discuss/outcome-kpis.md`,
`docs/feature/query-http-common-v0/design/application-architecture.md`,
`docs/feature/query-http-common-v0/design/mikado-plan.md`,
`docs/product/architecture/adr-0054-query-http-common-extraction.md`.

- **Paradigm**: Rust idiomatic. No paradigm change.
- **New crate**: `query-http-common`, library-only, workspace
  member, AGPL-3.0-or-later, `publish = false`, version `0.1.0`.
  External dependencies: NONE new; `axum 0.7`, `serde`, `serde_json`,
  `aegis` are all already in the workspace.
- **Workspace member**: ADD `"crates/query-http-common"` to the
  `members` array in the root `Cargo.toml`.
- **Consumer Cargo.toml deltas**: `query-api`, `log-query-api`,
  `trace-query-api` each gain ONE dependency line:
  `query-http-common = { path = "../query-http-common", version = "0.1.0" }`.
  NO `1.0.0` bump on the consumer crates.
- **CI gates**: ADR-0005 five gates inherited verbatim. A NEW per-crate
  job `gate-5-mutants-query-http-common` is required at the
  `gate-5-mutants` level, following the precedent set by ADR-0048
  (`gate-5-mutants-trace-query-api`) and ADR-0052 (the per-crate tag
  at graduation). The job scope is `cargo mutants -p query-http-common
  --no-shuffle`; target kill rate is 100% (K4 / ADR-0005 Gate 5).
- **External integrations**: NONE. The shared crate has no network
  surface, no third-party API, no webhook, no OAuth/OIDC provider.
  Contract testing recommendations are not applicable.
- **Tag**: at DELIVER closure, the consumer release tag pattern
  (`query-api/v0.x`, `log-query-api/v0.x`, `trace-query-api/v0.x`)
  is extended by ONE new tag `query-http-common/v0.1.0`. The new
  crate's first release ships with this feature; tagging is a
  DELIVER step, not a DESIGN step.
- **Observability**: NO new dashboards, NO new alerts. The four
  KPIs (K1 test regressions, K2 byte identity, K3 LOC reduction,
  K4 mutation kill rate) are measured at build time, not at runtime.

## Upstream Changes

NONE. The DISCUSS assumptions hold verbatim. All six DISCUSS flags
are resolved with the recommended pins, with two annotations:

1. DD4 promotes the cap reason texts to `pub const` literals
   (the user-instruction brief overrode the DISCUSS Flag 4
   recommendation to keep them at the call site). This is a tighter
   posture than DISCUSS suggested; it does not contradict any
   DISCUSS acceptance criterion (the K2 byte-identity gate still
   holds — the literal value is preserved).
2. DD2 pins `resolve_tenant_or_refuse` to return `Result<TenantId,
   Response>` (cloned) rather than `Result<&TenantId, Response>`
   (borrowed) as the user-instruction brief speculated. The four
   call sites today already do `t.clone()` immediately after the
   match, so returning the cloned value preserves byte-identity
   semantics without forcing a lifetime change on `ApiState`.
   Documented for the record; this is the minimum-friction shape.

No `docs/feature/query-http-common-v0/design/upstream-changes.md`
file is required.

## Handoff readiness

- DESIGN deliverables produced in
  `docs/feature/query-http-common-v0/design/`: `wave-decisions.md`
  (this file), `application-architecture.md`, `mikado-plan.md`.
- New ADR produced at
  `docs/product/architecture/adr-0054-query-http-common-extraction.md`.
- SSOT `docs/product/architecture/brief.md` extended with the
  `## Application Architecture — query-http-common-v0` section.
- DISTILL handoff (acceptance designer): no new acceptance scenarios
  are required because the feature has no wire-observable behaviour
  change. The K2 byte-identity gate runs against the EXISTING
  acceptance suite verbatim. DISTILL may add ONE inline harness
  test per Mikado step asserting `cargo test --workspace` is green
  if it considers that disciplinary; the slice brief does not
  require it.
- DEVOPS handoff (platform architect): see DEVOPS Handoff section
  above.
