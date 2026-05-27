# DISCUSS Decisions — query-http-common-v0

## Key decisions

- **D1 Feature type**: Cross-cutting refactor. The feature touches three
  read-side HTTP crates and introduces one new workspace-internal crate.
  Rationale: the duplicated scaffolding is genuinely cross-cutting; the
  surface lives at the boundary of three otherwise-separate domain crates.
  (Decision 1 = Cross-cutting.)
- **D2 Walking skeleton**: No greenfield walking skeleton. Brownfield
  refactor with an existing acceptance suite as the regression net. US-01
  is the analogue of a walking skeleton at the slicing level (it ships the
  new crate and the simplest shared surface, with the workspace test suite
  as the gate). (Decision 2 = No.)
- **D3 Research depth**: Lightweight. The maintainer's job is named in
  one line per story; no multi-persona research. (Decision 3 = Lightweight.)
- **D4 JTBD**: Skipped. This is an internal refactor with no user-facing
  behaviour change. The maintainer's job is named in each story's
  `Decision enabled` line. (Decision 4 = No.)
- **D5 Slicing model**: Elephant-carpaccio, one extraction per slice plus an
  integration gate slice. Each slice is ≤ 1 day, has a named learning
  hypothesis, and ships end-to-end (the workspace builds and tests green
  after each slice).
- **D6 Story framing for `@infrastructure`**: All five stories carry the
  `@infrastructure` tag because none of them adds user-observable behaviour.
  Per the LeanUX template's `Elevator Pitch` discipline, every story still
  carries a `Decision enabled` line so the maintainer's job-to-be-done is
  visible. The feature is INTERNAL plumbing with a clear maintainer outcome,
  not orphaned `@infrastructure` (which would block slice release per the
  carpaccio rules).

## Requirements summary

- **Primary jobs / user needs**: When the read-side HTTP scaffold needs to
  change (cap value, time-range parser, error envelope shape, fail-closed
  tenancy behaviour), the read-side maintainer must be able to make the
  change in ONE source file and trust it to propagate to all three pillar
  read APIs. Today the maintainer must edit three places in lockstep and
  rely on the acceptance suite to catch drift; this feature collapses the
  scaffold to a single source location.
- **Walking skeleton scope**: US-01 (cap constants extracted into the new
  crate; three consumer crates re-export). This is the thinnest end-to-end
  slice that proves the extraction model works.
- **Feature type**: Cross-cutting refactor; brownfield; internal plumbing.

## Constraints established

- The new crate `query-http-common` is workspace-internal, AGPL-3.0-or-later,
  starts at `0.1.0`, depends only on existing workspace deps (`axum`,
  `serde`, `serde_json`, `aegis`), and does NOT depend on the pillar stores
  (`pulse`, `lumen`, `ray`).
- The cap reason texts (`"window exceeds 86400 seconds"`,
  `"result exceeds 100000 rows"`) are preserved byte-for-byte.
- The error envelope JSON shape (`{"status":"error","error":"<reason>"}`) is
  preserved byte-for-byte.
- The 401 fail-closed reason text retains its pillar-specific suffix
  (`"the query service"`, `"the log query service"`, `"the trace query
  service"`); this is achieved by accepting a `service_label: &str`
  parameter on the shared helper.
- The mutation kill-rate gate per ADR-0005 Gate 5 is 100% on the new crate.
- The three consumer crates retain their existing public surface
  (`MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS` re-exported via `pub use`).
- `seconds_to_nanos` stays in each consumer crate; the new crate returns
  `(u64, u64)` seconds and each consumer builds its pillar-specific
  `TimeRange` on top. This avoids forcing one of
  `pulse::TimeRange`/`lumen::TimeRange`/`ray::TimeRange` into the shared
  crate (the explicit caution from ADR-0048 Decision 5 / Placement B).
- No `1.0.0` bump on any crate.

## Flags to DESIGN (six)

Each flag includes a recommended pin so DESIGN can either accept the
recommendation or override with rationale.

### Flag 1: Workspace crate vs sub-module

- **Question**: Extract into a new top-level workspace crate `query-http-common`
  or expose the shared surface as a sub-module of one of the three existing
  crates?
- **Recommended pin**: NEW workspace crate `crates/query-http-common`.
- **Rationale**: ADR-0048 Decision 5 / Placement B and ADR-0053 Decision 5
  both name `query-http-common` as the deferred extraction. A sub-module of
  one consumer crate would force the other two to depend on its host's
  domain types and would re-introduce the coupling the ADRs explicitly
  warned against. A new workspace crate is the textbook shape and matches
  the recorded intent.

### Flag 2: Public API surface scope

- **Question**: What is exported from `query-http-common`?
- **Recommended pin**: Minimum necessary surface, four items only:
  - `pub const MAX_WINDOW_SECONDS: u64 = 86_400`
  - `pub const MAX_RESULT_ROWS: usize = 100_000`
  - `pub fn parse_time_range_seconds(start: Option<&str>, end: Option<&str>) -> Result<(u64, u64), String>`
  - `pub fn error_response(status: StatusCode, reason: &str) -> Response`
  - `pub fn resolve_tenant_or_refuse(tenant: &Option<TenantId>, service_label: &str) -> Result<TenantId, Response>`
- **Rationale**: Each item maps 1:1 to a duplicated surface in the three
  consumer crates and a discovered maintainer pain. Adding more (e.g. a
  structured `ErrorBody` newtype, a generic `TimeRange` builder) would be
  speculative; YAGNI applies and the surface can grow when a fourth
  consumer joins.

### Flag 3: Error type design

- **Question**: Should the shared error helper return a structured
  `ErrorBody` type or stay at the `(StatusCode, &str)` -> `Response`
  helper-function shape today?
- **Recommended pin**: Stay at the helper-function shape.
- **Rationale**: The three consumer crates today already use the
  helper-function shape; a structured `ErrorBody` would force a migration
  on every error arm and add ceremony with no decision enabled. The
  current shape is byte-for-byte equivalent and is the smallest move that
  collapses the duplication. A structured `ErrorBody` (with builder
  pattern, optional fields, etc.) is a candidate for a future slice if a
  use case appears.

### Flag 4: Cap reason texts as const literals

- **Question**: Should the cap reason texts (`"window exceeds 86400
  seconds"`, `"result exceeds 100000 rows"`) live as
  `pub const`s in `query-http-common`, or stay as string literals at the
  call site?
- **Recommended pin**: Keep them as string literals at the call site for
  this feature. Verify the exact strings against the three consumer
  crates' current code (`crates/{query,log-query,trace-query}-api/src/lib.rs`,
  lines around 187-188, 150, 174, 188, 217 respectively).
- **Rationale**: Promoting them to `pub const`s in the new crate is a
  defensible follow-on but is NOT required to single-source the scaffold:
  the cap-check arm itself (the `if end - start > MAX_WINDOW_SECONDS { ...
  error_response(BAD_REQUEST, "window exceeds 86400 seconds") }` block)
  stays in each consumer crate because each consumer wires it into its
  own handler control flow. The byte-identity gate (K2) catches any drift.
  If DESIGN prefers to also promote the reason texts to `pub const` to
  pre-empt drift, recommend that as a follow-on slice.

### Flag 5: ADR-0054 — write one alongside this feature

- **Question**: Should DESIGN produce an ADR-0054 documenting the
  extraction decision and the four surfaces, or treat this as a refactor
  covered by ADR-0048 Decision 5 / Placement B without a new ADR?
- **Recommended pin**: YES, write a small ADR-0054. Scope: ~150 lines,
  one Context paragraph, one Decision section listing the four extracted
  surfaces, one Consequences section pinning the "scaffold-lives-in-one-place"
  property, one Alternatives paragraph re-stating why the surface is
  minimum-necessary (Flag 2). Do NOT modify ADR-0048 or ADR-0053 (ADR
  immutability).
- **Rationale**: ADR-0048 Decision 5 explicitly named this as "its own
  ADR"; landing the feature without one would leave the recorded plan
  half-honoured. A small new ADR is the lighter lift and is the
  recorded intent.

### Flag 6: Tenant pattern — verify the three (now four) are truly identical

- **Question**: The brief framed this as a "tenant extractor (Axum
  `FromRequestParts`)". The actual code is an inline `match` block. Are
  the four call sites truly identical up to the pillar-label suffix?
- **Recommended pin**: VERIFIED IDENTICAL. The four call sites:
  - `crates/query-api/src/lib.rs:167-175` — `"the query service"`
  - `crates/log-query-api/src/lib.rs:128-136` — `"the log query service"`
  - `crates/trace-query-api/src/lib.rs:141-149` — `"the trace query service"`
  - `crates/trace-query-api/src/lib.rs:241-249` — `"the trace query service"`
    (same suffix as the prior arm; the two trace arms are byte-identical
    on this block, as ADR-0053 Decision 1 / ADR-0048 Decision 2 redaction
    extended explicitly pins).
- **Rationale**: They differ ONLY on the pillar-label suffix and the
  envelope is identical. The recommended extraction (`resolve_tenant_or_refuse`
  taking `service_label: &str`) preserves the byte-identity property
  while collapsing four call sites into one helper. The "extractor"
  framing in the original brief is corrected in US-04's body.

## Upstream changes

No DISCOVER or DIVERGE artefacts exist for this feature (it was raised
directly off ADR-0048 Decision 5 / Placement B and ADR-0053 Decision 5,
both of which are SSOT). No DISCOVER assumptions are changed.

## Handoff readiness

The five DISCUSS deliverables exist under
`docs/feature/query-http-common-v0/discuss/`:

- `user-stories.md` — five `@infrastructure` stories with elevator pitches
- `story-map.md` — backbone (Extract → Rewire → Verify), US-01 as walking
  skeleton, five carpaccio slices with learning hypotheses, priority
  rationale, scope assessment PASS
- `outcome-kpis.md` — four numeric KPIs (K1 test regressions, K2 byte-identity,
  K3 LOC reduction, K4 mutation kill rate) with baselines, targets, and
  measurement methods
- `dor-validation.md` — 9/9 DoR items PASS per story; risks surfaced;
  anti-patterns checked
- `wave-decisions.md` — this file

DESIGN-wave handoff to `nw-solution-architect` is unblocked.
DEVOPS-wave handoff to `nw-platform-architect` is unblocked
(reads `outcome-kpis.md`).
