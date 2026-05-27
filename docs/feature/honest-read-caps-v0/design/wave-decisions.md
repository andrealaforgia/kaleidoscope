# Wave Decisions: honest-read-caps-v0 (DESIGN)

British English. No em dashes. No emoji.

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.
Interaction mode: propose. Application scope.

This DESIGN wave resolves the four items DISCUSS flagged (window cap
value, result-size cap value, REFUSE vs TRUNCATE, ADR number and shape)
plus three implementation-level pins (enforcement points, caps
signature, DEVOPS handoff annotation). The feature is M-2 in the
residuality analysis (`docs/product/architecture/residuality-analysis.md`,
S13 row) and item 2 in the residuality follow-up roadmap. It closes
the read-side self-DoS surface on all three crates (`query-api`,
`log-query-api`, `trace-query-api`) in ONE walking-skeleton slice with
two compile-time caps per crate.

## Reads checklist

- [x] `docs/feature/honest-read-caps-v0/discuss/wave-decisions.md` (the
      four flags, the System Constraints, the walking-skeleton entry
      points, the OUT-of-scope list).
- [x] `docs/feature/honest-read-caps-v0/discuss/user-stories.md` (US-01
      through US-05, with the five scenarios per story and the
      redaction posture).
- [x] `docs/feature/honest-read-caps-v0/discuss/story-map.md` (the
      backbone, the carpaccio slicing rule, the priority ordering).
- [x] `docs/feature/honest-read-caps-v0/discuss/outcome-kpis.md` (the
      six KPIs across the three crates; KPI 1 + KPI 2 the north star).
- [x] `docs/product/architecture/residuality-analysis.md` (the M-2
      framing under "Resilience modifications (prioritised)"; the S13
      row of the incidence matrix; "the **S13 columns QM / QL / QT**
      all degrade"; the "capacity residues" closing summary).
- [x] `docs/residuality-followups-roadmap.md` (the feature's position
      as item 2 of 3 in the post-residuality roadmap).
- [x] `docs/product/architecture/adr-0042-query-api-contract-and-promql-subset.md`
      (the metrics query-api contract; the existing 400 arms; the
      `{status:"error", error}` envelope shape).
- [x] `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md`
      (the log read-API contract; Decision 1 reproducing the envelope
      and redaction posture).
- [x] `docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`
      (the trace read-API contract; Decision 1 with the stricter
      redaction posture; Decision 5 deferral of the
      `query-http-common` extraction).
- [x] `docs/product/architecture/adr-0049-earned-trust-honour-fsync.md`
      (the immediately prior Earned-Trust sibling; same DESIGN-time
      style; not modified, cited as the immediate precedent).
- [x] `crates/query-api/src/lib.rs` (the `handle_query_range` handler;
      `parse_time_range` at line 201; the existing 400 arms and the
      `error_response` helper at line 249).
- [x] `crates/log-query-api/src/lib.rs` (the `handle_logs` handler;
      `parse_time_range` at line 139; `error_response` at line 183).
- [x] `crates/trace-query-api/src/lib.rs` (the `handle_traces` handler;
      `read_required_service` at line 162; `parse_time_range` at line
      175; `error_response` at line 223; the stricter redaction).
- [x] Existing test fixtures across the three crates: every window used
      in `crates/query-api/tests/slice_*`, `crates/log-query-api/tests/slice_*`,
      `crates/trace-query-api/tests/slice_*` is 60 seconds wide
      (`1716200000` to `1716200060`); the widest synthetic window used
      in any test is the year-long `[0, 31536000)` ALREADY EXPECTED to
      become a cap-refusal in the new acceptance suite. Every fixture
      seeds a handful of records (three to five at most). Conclusion:
      **no existing test fixture exceeds either cap**; the 24h window
      cap and the 100_000 result cap are safe for the entire current
      test surface.

No contradiction with DISCUSS surfaced during DESIGN.

## Key Decisions

### D1. WINDOW CAP VALUE: 86_400 seconds (24 hours), uniform across the three crates (resolves FLAG 1)

`MAX_WINDOW_SECONDS = 86_400` is the slice-01 cap, identical in all
three crates. The decision is governed by three forces that DISCUSS
already named under FLAG 1:

- **6 hours is too narrow for typical analysis**. A Prometheus
  dashboard typically requests a 24h "today" panel; cutting that at 6h
  would generate false positives the operator would route around (by
  shrinking the dashboard or by raising the cap), defeating the cap
  rather than honouring it.
- **7 days is too generous against an untested lifetime**. The
  residuality analysis (S04 / S14 column P) flags the cardinality story
  as the un-defended amplification at the read side; a 7-day default
  is large enough that even with the result cap a chatty `cpu_seconds_total`
  series-fan-out comfortably saturates the listener at v0/v1. Slice 01
  is the walking skeleton, not the right time to test the lifetime.
- **24 hours is the residuality analysis's own named default**
  ("e.g. 24h default" under M-2). It is the smallest cap that does NOT
  break a normal Grafana / Prism dashboard layout, and the cap above
  which the operator's intent is "I want a long archival sweep" rather
  than "I want a live dashboard". An archival sweep is exactly the
  request that should be re-narrowed or moved to a separate service.

Per-pillar differentiation (e.g. traces 1h because `service` already
narrows; logs 6h because logs are bytes-heavier per record; metrics
24h because per-series points are bytes-light) is **DEFERRED to a
successor slice or feature, NOT slice 01**. The DISCUSS recommendation
"ONE value across the three for slice 01" is honoured. Differentiation
without measurement would be guesswork; the v0/v1 platform has no
production telemetry to calibrate from. When measurement exists, the
per-crate constant is a one-line change (`MAX_WINDOW_SECONDS` is `pub`
per crate, NOT a shared symbol; see D6).

### D2. RESULT-SIZE CAP VALUE: 100_000 (rows / records / spans), uniform across the three crates (resolves FLAG 2)

`MAX_RESULT_ROWS = 100_000` is the slice-01 cap, identical in all
three crates. The decision is governed by the same three-candidate
analysis DISCUSS named under FLAG 2:

- **10_000 is too tight**. A typical metrics query on a service with
  normal label cardinality (say 20 labels with two to ten distinct
  values each) can fan out beyond 10_000 matrix entries on a 24h
  window without the user doing anything pathological. The cap would
  bite legitimate queries.
- **1_000_000 is unsafe at v0/v1**. Even at one kilobyte per row
  (small for logs and spans; large for metric matrix entries), a
  million-row response is a gigabyte of JSON in memory in the handler
  before serialisation. The platform has no streaming JSON encoder at
  the read side today; the entire vector is allocated, then serialised,
  then sent. A gigabyte is OOM territory on small dev hardware.
- **100_000 is the sweet spot** in similar systems (the Prometheus
  remote-read protocol's typical chunk size, the Loki query-range
  default, the Tempo per-trace limit) and is what the residuality
  analysis itself named ("a result cap (e.g. 100k rows)" under M-2).
  At one kilobyte per row, 100_000 is a 100 MB response in worst case,
  still serialisable but large enough to be the right kind of
  refusal-trigger for a misconfigured request.

A reads-fixture check (see the Reads checklist above) confirms no
typical existing test fixture in any of the three crates exceeds 100k
rows / records / spans. The widest test fixture seeds five records; the
100k threshold is a factor of 20_000 above any current acceptance
test. No legacy regression risk; the new over-cap acceptance test
seeds `MAX_RESULT_ROWS + 1` records explicitly.

Per-pillar differentiation is DEFERRED for the same reason as D1.

### D3. REFUSE vs TRUNCATE: REFUSE with honest 400, NEVER truncate (resolves FLAG 3)

The result-cap breach is a 400 with the existing envelope
`{status:"error", error:"<names the cap>"}`. **No `X-Truncated`
header. No partial 200. No silent empty.** Three reasons drive the
decision against truncation:

- **Earned Trust (Principle 12 of this agent's methodology, and the
  same principle ADR-0049 made code)**: a 200 with `X-Truncated: true`
  is the read-side equivalent of a buffered fsync that lies. The
  response claims success; the client either misses the truncation
  header (most do) and treats the partial array as complete, OR the
  client honours it and now has to implement a "retry with a narrower
  window" loop the platform could simply have asked for in the first
  place. Truncate-with-partial-200 is fabricated certainty; REFUSE is
  honest doubt.
- **Consistency with the residuality analysis A-D6**: "Honest
  three-way outcomes on read" is one of the four desired attractors,
  reading: **200 (with data), 200 (with `[]`), 4xx (with named
  error), 5xx (with PersistenceFailed)**. A truncated 200 collapses
  the boundary between the first and second arms and silently shifts
  the meaning of 200 from "this is the entire matching set" to "this
  is at most the first M of the matching set". The contract becomes
  ambiguous. ADR-0042 / 0047 / 0048 ALL pin the three-way distinction;
  introducing a truncate-200 violates that pin.
- **Operational legibility**: a client that received 100_000 of a
  million rows will make wrong decisions (dashboards drawn from
  partial data, alerts triggered from missing samples). A 400 is the
  honest signal: "narrow your window, narrow your matchers, narrow
  your service, or use an archival sweep service". The 400 envelope
  is exactly what Prism's `isPromError` already handles for
  matcher-400 and bounds-400 errors (ADR-0042 lines 220-229).

A future slice MAY re-open the trade-off (e.g. a streaming endpoint or
a paginated endpoint) but it will be a SEPARATE endpoint with a
different contract, NOT a silent change to the meaning of 200 on the
existing `/api/v1/query_range` / `/api/v1/logs` / `/api/v1/traces`
routes.

### D4. ADR-0050: NEW ADR, cross-cutting refinement; ADR-0042 / 0047 / 0048 / 0049 CITED, NOT modified (resolves FLAG 4)

The cap policy is recorded in a single new ADR-0050 at
`docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`.
`ls docs/product/architecture/adr-0050*` returns no hits;
`adr-0049-earned-trust-honour-fsync.md` is the latest; 0050 is the
next free number, verified.

ADR-0050 cites:

- ADR-0042 as the originating read-side contract (the metrics
  query-api contract, the `{status:"error", error}` envelope, the
  fail-closed tenancy, the Earned-Trust probe);
- ADR-0047 as the log read-API contract (the bare JSON array, the
  redaction posture);
- ADR-0048 as the trace read-API contract (the stricter redaction,
  the `service` validation, the deferral of `query-http-common`);
- ADR-0049 as the IMMEDIATE Earned-Trust sibling (paper-becomes-code
  on the WRITE path; ADR-0050 does the same on the READ path).

**None of the four cited ADRs are modified.** The cross-cutting
refinement lives in ONE place (ADR-0050) rather than three amendments
(0042 / 0047 / 0048), in line with the immutability rule for ADRs in
this repository (ADR-0049 Context paragraph 4: "ADRs in this
repository are immutable (superseded, never edited)").

### D5. ENFORCEMENT POINTS: window check between parse and store; result check between store and serialise

The two caps live at two seams in each handler. The seams are CHOSEN
to mirror the slice-01 patterns of the matcher-400 (ADR-0046 / 0042)
and the fsync probe (ADR-0049): a check FIRST so the costly path is
never reached on a breach.

**Window check (window cap)**: immediately AFTER `parse_time_range`
SUCCEEDS and BEFORE `state.store.query(...)` is called. The placement
is the same one the existing inverted-bounds 400 occupies in the
parse function itself; the cap-check is its OWN gate in the handler,
not folded into `parse_time_range`. This separation is deliberate:
`parse_time_range` is the parser; the cap-check is a policy decision
the parser MUST NOT carry (the same logic separates `selector::parse`
from `matrix::build_filter` in `query-api`). The handler runs:

1. `parse_time_range` (the existing 400 arm: non-numeric, inverted).
2. **window cap check** (the new 400 arm: `end - start > MAX_WINDOW_SECONDS`).
3. (`query-api` only) `selector::parse` and `matrix::build_filter`
   (the existing 400 arms: bad selector, bad regex).
4. (`trace-query-api` only) `read_required_service` runs BEFORE step
   1 (preserving handler order; the missing-service 400 fires first).
5. `state.store.query(...)`.

This ordering matches the carpaccio taste-test 1 in the DISCUSS
wave-decisions: the over-window 400 is the proof that the store is
NEVER touched. A `LyingMetricStore` / `LyingLogStore` / `LyingTraceStore`
configured to return `PersistenceFailed` on any `query` call is wired
in the acceptance test; if the cap check were AFTER the store, the
500 from the lying store would surface and the test would fail. If
the cap check were INSIDE `parse_time_range`, the boundary between
parse-error (malformed input) and policy-refusal (well-formed input,
too wide) would blur.

**Result check (result-size cap)**: immediately AFTER the store
returns AND AFTER any in-handler filtering / translation that
determines the final response shape AND BEFORE
`success_response(...)`. Specifically:

- `query-api`: the cap is on the final matrix-entry count, AFTER
  `rows.retain(matrix::keep_row)` and AFTER `matrix::to_matrix(rows)`
  but BEFORE `success_response(result)`. The user observes
  `result.len()` directly in the response; the cap measures what the
  user observes, NOT what the store returned upstream of filtering.
- `log-query-api`: the cap is on `records.len()` AFTER
  `state.store.query(&tenant, range)` succeeds and BEFORE
  `success_response(records)`. There is no in-handler filtering; the
  store result is the response array.
- `trace-query-api`: the cap is on `spans.len()` AFTER
  `state.store.query(&tenant, &service, range)` succeeds and BEFORE
  `success_response(spans)`. No in-handler filtering.

A 400 fires EVEN ON a successful query whose result is too large. The
store was queried exactly once; serialisation has NOT started; the
JSON encoding cost of the over-cap result is not paid. The redaction
posture of D7 below applies.

**Three crates, three call sites, no shared crate.** The duplication
is the deliberate cost ADR-0048 Decision 5 already declared
("`query-http-common` extraction is M-5 of the residuality follow-up
roadmap, deferred"). Slice 01 honours that deferral; the rule-of-three
refactor is a SEPARATE future feature.

### D6. CAPS SIGNATURE: two `pub const` per crate's `lib.rs`, NO shared crate, NO config struct, NO env override

In each of `crates/query-api/src/lib.rs`,
`crates/log-query-api/src/lib.rs`, and `crates/trace-query-api/src/lib.rs`:

```rust
pub const MAX_WINDOW_SECONDS: u64 = 86_400;
pub const MAX_RESULT_ROWS: usize = 100_000;
```

The constants are `pub` so the acceptance suite can address them by
name (the boundary-at-exactly-N tests; the over-by-one tests). They
are CONSTANTS at compile time at slice 01. Three things explicitly
NOT done:

- **NO shared crate**. ADR-0048 Decision 5 defers `query-http-common`
  to M-5; this slice does not preempt that decision. If the rule of
  three motivates extraction later, the constants move to the shared
  crate THEN, not now.
- **NO config struct**. A `CapsConfig` struct passed into `router()`
  would be the natural shape for slice 02 (env-driven configurability),
  but at slice 01 there is no configurability: the constant is the
  configuration.
- **NO env override**. `KALEIDOSCOPE_QUERY_MAX_WINDOW_SECONDS`,
  `KALEIDOSCOPE_QUERY_MAX_RESULTS`, and their per-crate siblings are
  EXPLICITLY OUT (DISCUSS OUT list; this DESIGN preserves the
  deferral). Slice 02 (separate feature) lifts the constants to
  env-driven via the existing `composition::resolve_tenant` posture.

The `u64` type for `MAX_WINDOW_SECONDS` matches the type of
`end_secs - start_secs` after `parse_epoch_seconds` returns `u64`;
the `usize` type for `MAX_RESULT_ROWS` matches `Vec::len()`; both
types avoid casts at the cap-check call sites.

### D7. REDACTION POSTURE: SYMMETRIC with each crate's existing posture

The two new cap-400 reasons must inherit the existing redaction
posture of the crate they live in:

- **`query-api`**: no raw `start`, no raw `end`, no raw query text,
  no raw regex pattern, no forwarded Authorization / Bearer header
  value, no "SECRET". Mirror of
  `the_bounds_error_never_echoes_the_raw_value` at
  `crates/query-api/src/lib.rs:303`.
- **`log-query-api`**: no raw `start`, no raw `end`, no forwarded
  Authorization / Bearer header value, no "SECRET". Mirror of
  `the_bounds_error_never_echoes_the_raw_value` at
  `crates/log-query-api/src/lib.rs:244`.
- **`trace-query-api`**: no raw `start`, no raw `end`, no raw
  `service`, no "SECRET" anywhere in the body, no "Bearer" anywhere
  in the body, no forwarded header value. Stricter than the other
  two. Mirror of
  `the_service_error_never_echoes_the_raw_service_value_or_a_credential`
  at `crates/trace-query-api/src/lib.rs:334`.

The exact wording of each cap reason is named-by-cap, NOT echoing the
breached value:

- Window cap: `"window exceeds maximum"` (or the equivalent named
  reason the crafter chooses; the constraint is "names the cap,
  does not echo the value"). The literal `86400` is NOT included
  in the response body (so changing the cap value is a one-line edit
  with no body-text drift); the operator sees the named class of
  refusal and reads the ADR / source for the numeric value.
- Result cap: `"result exceeds maximum"`. Same shape; same
  no-numeric-echo constraint.

(The acceptance suite SHOULD assert on the stable substring
`"exceeds"` or `"too wide"` or the equivalent named class, NOT on the
exact integer. This makes the cap value tunable without a test
breakage.)

## Architecture Summary

The cap pattern is uniform across the three crates:

1. The handler validates structural pre-conditions in the existing
   order (fail-closed tenancy first, then required `service` on
   traces, then `parse_time_range`).
2. The handler then runs the **window cap check** as a NEW gate
   between parse-success and the store query. Over the cap is a 400
   with the named envelope, served via the existing `error_response`
   helper, redaction-symmetric per D7. The store is NEVER touched.
3. The handler then queries the store (unchanged). The store traits
   `pulse::MetricStore`, `lumen::LogStore`, `ray::TraceStore` are
   UNCHANGED; their callers across the rest of the workspace see no
   diff.
4. On a successful store query, the handler runs the **result-cap
   check** as a NEW gate between the store result and serialisation.
   Over the cap is a 400 with the named envelope. NO `X-Truncated`
   header, NO partial 200, NO silent empty. The JSON encoding cost of
   the over-cap result is not paid.
5. Within both caps, the response is the existing success envelope
   (matrix for metrics, bare JSON array for logs and traces).

Two compile-time constants per crate (`MAX_WINDOW_SECONDS = 86_400`,
`MAX_RESULT_ROWS = 100_000`), three crates, six total
`pub const` lines. Each crate's tests live in its own
`#[cfg(test)] mod tests` block and in its own integration suite under
`crates/<crate>/tests/`. No new crate, no new module, no new external
dependency.

The architectural posture is Rust-idiomatic per CLAUDE.md: data + free
functions, no `dyn` indirection added, no new trait. The cap-check is
two `if` statements per handler, named for what they reject. The
duplication across the three crates is the deliberate cost
ADR-0048 Decision 5 named; the deferred `query-http-common` extraction
(M-5 in the residuality follow-up roadmap) is the eventual home.

The Earned-Trust principle (Principle 12 of this agent's methodology,
encoded in ADR-0049 on the WRITE side) is honoured on the READ side:
a request that exceeds either cap is refused **out loud** with a
named envelope, never silently degraded, never partially served. The
operator sees the named class; the client narrows; the next request
serves.

## Reuse Analysis (RCA F-1 hard gate)

| Surface | Verdict | Note |
|---|---|---|
| Three read handlers (`handle_query_range`, `handle_logs`, `handle_traces`) | **EXTEND** | Two `if` statements added per handler at the two enforcement points named in D5. Existing flow, existing `error_response` helper. |
| Two `pub const` per crate's `lib.rs` (`MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS`) | **EXTEND** | Six new lines total across the three crates. No new module, no new file. |
| `error_response` helper in each crate's `lib.rs` | **REUSE** | Unchanged; called from the two new 400 arms with the named reason strings. |
| Existing `parse_time_range` in each crate | **REUSE** | Unchanged. The window cap is enforced AFTER `parse_time_range`, not inside it. |
| Existing `read_required_service` in `trace-query-api` | **REUSE** | Unchanged. Continues to fire first per the existing handler order. |
| `pulse::MetricStore`, `lumen::LogStore`, `ray::TraceStore` traits | **REUSE (untouched)** | Trait signatures are byte-identical to the prior tag. Gate 2 `cargo public-api` catches any drift. |
| `LyingMetricStore` / `LyingLogStore` / `LyingTraceStore` test doubles | **REUSE** | The over-window acceptance scenario reuses the existing lying doubles at `crates/query-api/tests/*` (or the inline pattern), `crates/log-query-api/src/composition.rs:97`, `crates/trace-query-api/src/composition.rs:106`. The cap fires BEFORE the lying store's `query` is called; the test asserts the store was never reached. |
| Existing `{status:"error", error}` envelope | **REUSE** | The cap 400 reuses the existing envelope verbatim; no new shape, no new envelope, no new event. |
| Prism's `isPromError` (ADR-0042) | **REUSE** | Already handles the envelope. No client-side change required. |
| Per-crate `gate-5-mutants-*` workflows | **REUSE** | The three workflows (`gate-5-mutants-query-api`, `gate-5-mutants-log-query-api`, `gate-5-mutants-trace-query-api`) already cover the changed files via `--in-diff`. No new CI job. |

**CREATE NEW**: zero. No new module, no new free function, no new
trait, no new crate, no new file in `src/`. The acceptance suite gains
new test files (the slice-02 acceptance file per crate) but those are
DISTILL-wave outputs, not DESIGN-wave outputs.

**Reuse verdict**: maximum REUSE. Two `pub const` + two `if`
statements per handler. The deliberate duplication across the three
crates is the cost ADR-0048 Decision 5 already declared; this slice
honours that deferral.

## Constraints

The DISCUSS wave's System Constraints carry through DESIGN unchanged:

- The caps ride OUTSIDE the storage traits. `pulse::MetricStore`,
  `lumen::LogStore`, `ray::TraceStore` signatures are UNCHANGED.
- The error envelope is the EXISTING shape:
  `{status:"error", error:"<reason>"}`. No new envelope, no new status
  code (always 400 on a cap breach), no `X-Truncated` header, no new
  event name. The cap is its own signal.
- Caps are COMPILE-TIME CONSTANTS per crate; env-driven configurability
  is OUT (slice 02 / successor feature).
- ONE window cap value (86_400 seconds) and ONE result-cap value
  (100_000) across all three crates; per-pillar tuning is OUT.
- The result-cap is enforced AFTER the store query and BEFORE
  serialisation, INSIDE the handler. No store-trait method is added
  for "limit"; the duplication is deliberate.
- Redaction symmetric with each crate's existing posture (D7).
- `trace-query-api` retains its stricter redaction posture (no
  "SECRET" or "Bearer" anywhere in the body).

Additional DESIGN-time constraints:

- **No new event name**. The refusal IS the signal; the 400 with
  envelope is what the operator sees. ADR-0049 reused
  `event=health.startup.refused` with a `substrate` payload field;
  this ADR re-uses the existing `{status:"error", error}` envelope
  with a named reason string. No metric, no dashboard, no alert
  thresholds.
- **No streaming JSON encoder**. The result-cap fires BEFORE
  serialisation precisely because the platform does not have a
  streaming encoder today; introducing one is a separate
  architectural change with its own ADR.
- **No store-side pagination, no store-side `limit` argument**.
  ADR-0048 Decision 5 deferred the cross-cutting extraction; pushing
  a `limit` into the three store traits would be a wider blast radius
  than the cap pattern. Slice 01 enforces in the handler; the store
  remains unchanged.

## DEVOPS Handoff Annotation

To `@nw-platform-architect` (Apex):

- **NO new crate**. The change is inside the three existing read-API
  crates (`crates/query-api`, `crates/log-query-api`,
  `crates/trace-query-api`). No new directory under `crates/`.
- **NO new external dependency**. The cap-check uses arithmetic on
  the existing `parse_time_range`-derived `u64` seconds value (for
  the window) and `Vec::len()` (for the result). Both are core; no
  new entry in any `Cargo.toml`.
- **NO new CI job**. The existing `gate-5-mutants-query-api`,
  `gate-5-mutants-log-query-api`, `gate-5-mutants-trace-query-api`
  workflows all cover the modified files via `--in-diff` at the 100%
  kill-rate gate (ADR-0005 Gate 5; CLAUDE.md). Primary mutation
  targets per crate: the window-cap `>` boundary (the `<=` -> `<`
  mutant must be killed by the equal-bounds boundary test); the
  result-cap `>` boundary (same shape); the two cap reasons (mutating
  the named reason strings must fail by the redaction-test
  assertions); the order-of-checks (a mutant that swaps window-cap
  and store-query order is killed by the lying-store assertion that
  `query` was NOT called on the over-window path).
- **NO new graduation tag**. The slice's surface is internal to the
  three existing crates; the public `router()` signatures are
  unchanged; the constants are `pub` informational. The existing
  `gate-2-public-api` jobs confirm the public-API surface is
  byte-identical to the prior tag (apart from the two new `pub const`
  per crate, which appear in the public API as new informational
  items, not as breaking changes; the `cargo public-api` diff lists
  them as additions).
- **NO new event, NO new metric, NO new dashboard, NO new alert**.
  The refusal envelope is the existing
  `{status:"error", error:"<reason>"}` envelope; Prism's `isPromError`
  already handles it for matcher-400 and bounds-400 errors.
- **External integrations: none new**. The three read APIs serve
  in-process from durable stores; no third-party API is consumed by
  the cap path. No consumer-driven contract test recommendation.
- **DELIVER paradigm**: Rust idiomatic per CLAUDE.md (data + free
  functions; no trait introduced; the cap-check is two `if`
  statements per handler, named for what they reject). The crafter
  owns the GREEN / REFACTOR internals; this design fixes only the
  two new constants per crate, the two enforcement points named in
  D5, the redaction posture per D7, and the named-cap response
  envelope.
- **Per-feature mutation 100%** scoped to the modified files
  (CLAUDE.md). Per-crate Gate 5 covers the three crates'
  `lib.rs` changes via `--in-diff`.

The DEVOPS surface for this feature is SLIM: no new crate, no new
dependency, no new CI gate, no new event vocabulary. The platform
gains two compile-time constants per crate, two new `if` arms per
handler, and a per-crate redaction test set inheriting the existing
posture.

## Changelog

- 2026-05-27: feature `honest-read-caps-v0` DESIGN wave artefacts
  written by Morgan. FLAG 1 resolved (window cap = 86_400 seconds).
  FLAG 2 resolved (result cap = 100_000). FLAG 3 resolved (REFUSE
  with 400, never truncate). FLAG 4 resolved (NEW ADR-0050,
  ADR-0042 / 0047 / 0048 / 0049 cited and NOT modified). Three
  implementation pins recorded (enforcement points D5; caps signature
  D6; redaction posture D7). DEVOPS handoff annotated SLIM (no new
  crate, no new dependency, no new CI job).
