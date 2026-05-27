# Wave Decisions - pulse-cardinality-watermark-v0 / DEVOPS

British English. No em dashes.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-27
- **Mode**: slim DEVOPS. This wave confirms that the existing CI
  contract already covers the four modified files in `crates/pulse/src/`
  and the one new file in `crates/self-observe/src/`, and that no new
  infrastructure is warranted by a refinement that adds one `pub const`,
  one additive receipt field, one additive default trait method, one
  additive `apply_ingest` parameter, one additive shadow counter, and
  one new bridge file. The decision to run slim, and its shape, are
  Apex's own judgement from the DESIGN handoff, not pre-taken.

## Why this wave is slim

The feature is a focused refinement that completes the Earned-Trust
trilogy on the write side at the cardinality boundary. ADR-0051 records
the refinement. The deliverable is:

- `pub const MAX_SERIES_PER_TENANT: usize = 10_000;` in
  `crates/pulse/src/lib.rs`.
- An additive `series_refused: usize` field on `IngestReceipt`
  (`crates/pulse/src/store.rs`).
- A shadow per-tenant counter `HashMap<TenantId, usize>` inside both
  `InnerState` (in-memory adapter) and `Inner` (file-backed adapter),
  under the same Mutex as the existing series map.
- An additive `enforce_cap: bool` parameter on the private
  `apply_ingest` function in `crates/pulse/src/file_backed.rs`; the
  WAL-replay call site passes `false`, the live-ingest call site
  passes `true`.
- One additive default-method `record_series_refused` on the existing
  `MetricsRecorder` trait (`crates/pulse/src/metrics.rs`) with a no-op
  default body, plus one additive `RecordedEvent::SeriesRefused`
  variant on the test-helper `CapturingRecorder`.
- One new file `crates/self-observe/src/pulse_cardinality_bridge.rs`
  defining `PulseCardinalityToPulseRecorder`, mirroring the existing
  `LumenToPulseRecorder` and `CinderToPulseRecorder` template; the
  bridge emits `pulse.series.refused.count` via `MetricStore::ingest`
  on a second pulse store.
- A one-line `mod` declaration and a one-line `pub use` re-export in
  `crates/self-observe/src/lib.rs`.

There is NO new workspace crate, NO new external dependency (the cap
is in-process arithmetic on a `usize` shadow counter; the bridge reuses
the existing `aegis` + `pulse` dependencies already in
`crates/self-observe/Cargo.toml`), NO new public event name beyond the
existing `pulse.<event>.count` self-observe convention, NO new CI gate,
NO new graduation tag (neither pulse nor self-observe is in Gate 2 /
Gate 3's locked set; the locked set is harness / spark / sieve / codex
per ci.yml lines 326-347), and NO new external integration. The DESIGN
"Handoff to DEVOPS" section
(`../design/wave-decisions.md` lines 400-424) anticipated every DEVOPS
conclusion; the job of this wave is to VERIFY those conclusions against
the live CI workflow and `deny.toml`, record the verification, and hand
off, not to re-litigate the design.

This follows the `honest-read-caps-v0` slim-DEVOPS precedent, which
itself followed `earned-trust-fsync-probe-v0`. That precedent produced
two files (`environments.yaml`, `wave-decisions.md`); this wave
produces the same two, for the same reasons recorded there under
"Artefacts judged N/A".

## Inputs read (in dependency order)

1. `CLAUDE.md` - paradigm (Rust idiomatic) and the per-feature mutation
   testing strategy at 100% kill rate (declared; not modified here).
2. `../discuss/wave-decisions.md` and `../discuss/user-stories.md` -
   the four DISCUSS flags Morgan resolved at DESIGN, and the US-01
   through US-05 acceptance scenarios.
3. `../design/wave-decisions.md` - DESIGN decisions D1-D7 and the
   explicit "Handoff to DEVOPS" section (no new crate, no new
   dependency, `gate-5-mutants-pulse` covers the modified pulse files
   via `--in-diff`, `gate-5-mutants-self-observe` covers the new
   bridge file, no new event name, three orthogonal Earned-Trust
   enforcement layers reproduced from ADR-0049 and ADR-0050).
4. `../design/application-architecture.md` - C4 L2, the per-metric
   decision flow, the Changes Per File table (four files in pulse, one
   new file in self-observe, one re-export).
5. `docs/product/architecture/adr-0051-pulse-per-tenant-cardinality-watermark.md`
   - the cardinality-watermark ADR (Accepted) and its Verification
   section.
6. `docs/feature/honest-read-caps-v0/devops/{environments.yaml,wave-decisions.md}`
   - the immediate slim-DEVOPS sibling shape this wave mirrors.
7. `.github/workflows/ci.yml` - the existing five-gate workflow, read
   to CONFIRM (not modify) the `gate-5-mutants-pulse` and
   `gate-5-mutants-self-observe` jobs' existence and `--in-diff`
   scopes (see "Verification against ci.yml" below).
8. `deny.toml` - read to CONFIRM no licence / ban / advisory policy
   change is needed (no new dependency).

## Pre-wave decisions (carried in from project convention, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| P1 | `deployment_target` | None new (pulse and self-observe stay library-only; no new binary, no new container) | DESIGN handoff + ADR-0051 |
| P2 | `container_orchestration` | N/A (slice 01 produces no container image; the pre-existing kaleidoscope-cli Dockerfile is untouched) | environments.yaml |
| P3 | `cicd_platform` | GitHub Actions (existing, unchanged) | ADR-0005 |
| P4 | `existing_infrastructure` | Yes (workspace + five-gate CI; `gate-5-mutants-pulse` already present at ci.yml line 1297; `gate-5-mutants-self-observe` already present at ci.yml line 862) | ci.yml |
| P5 | `git_branching_strategy` | Trunk-based, pure (main has no required-status-checks; CI is feedback, not a gate) | memory `project_kaleidoscope_pure_trunk_based` |
| P6 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md, ADR-0005 Gate 5 |

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] No new CI gate; ADR-0005's five gates inherited unchanged

The change touches four files in `crates/pulse/src/` (lib.rs, store.rs,
file_backed.rs, metrics.rs), adds one new file under
`crates/self-observe/src/` (pulse_cardinality_bridge.rs), adds a one-
line mod declaration and a one-line re-export in
`crates/self-observe/src/lib.rs`, and adds new test files under
`crates/pulse/tests/` and `crates/self-observe/tests/`. Each gate is
satisfied by existing machinery:

- **Gate 1 (`cargo test --workspace`)**: runs the new acceptance
  suites (`crates/pulse/tests/slice_01_*.rs` covering US-01 through
  US-05, and `crates/self-observe/tests/` covering the bridge
  emission) with zero workflow edit. This is the KPI collection
  surface for every behavioural KPI (within-cap, at-cap, boundary,
  per-tenant isolation, WAL replay, partial-apply, receipt
  observability, recorder hook, bridge emission).
- **Gate 2 (`cargo public-api`)** and **Gate 3 (`cargo
  semver-checks`)**: scope to harness / spark / sieve / codex only;
  pulse and self-observe are not in the locked set (verified below).
  No diff applies at the gate level. The `pulse::MetricStore` trait
  method signatures (ingest, query, query_with) are byte-identical to
  the prior tag regardless (ADR-0051 Decision 6: no trait method-
  signature change). The additive items (the `pub const`, the
  receipt field, the default trait method, the new variant, the new
  bridge struct, the re-export) are additions if either crate ever
  graduates.
- **Gate 4 (`cargo deny`)**: no new external dependency, so no scan
  change. VERIFIED in `deny.toml`: no edit required (A4 below).
- **Gate 5 (`cargo mutants`)**: covered by the existing
  `gate-5-mutants-pulse` job (for the four modified pulse files) and
  the existing `gate-5-mutants-self-observe` job (for the new bridge
  file); no new job (A2).

No new or amended gate is warranted. No new CI workflow file is
created; no existing gate is added to, removed, or modified by this
feature.

### [A2] Mutation testing: the two existing gate-5 jobs cover the modified files; no workflow edit

**Options considered**:

1. **Rely on the existing `gate-5-mutants-pulse` and
   `gate-5-mutants-self-observe` jobs** (each runs `cargo mutants
   --package <crate> --in-diff` against `crates/<crate>/**`).
2. Add a new file-scoped job pinned to the four modified pulse files
   plus the new bridge file.
3. Add a new combined `gate-5-mutants-pulse-cardinality-watermark`
   job covering both crates' touched files.

**Decision**: Option 1.

**Rationale**: Both existing jobs exist in `ci.yml`. The
`gate-5-mutants-pulse` job is defined at line 1297 with diff filter
`git diff "$BASELINE" HEAD -- 'crates/pulse/**'` at line 1351 and
invocation `cargo mutants --package pulse --in-diff "$DIFF_FILE"
--no-shuffle --jobs 2` at lines 1359-1363. The
`gate-5-mutants-self-observe` job is defined at line 862 with diff
filter `git diff "$BASELINE" HEAD -- 'crates/self-observe/**'` at
line 920 and invocation `cargo mutants --package self-observe
--in-diff "$DIFF_FILE" --no-shuffle --jobs 2` at lines 928-932. Both
use the `origin/main -> HEAD~1 -> full` baseline cascade with an
empty-diff short-circuit to a zero-second exit.

Because this feature modifies
`crates/pulse/src/{lib.rs, store.rs, file_backed.rs, metrics.rs}` and
adds `crates/self-observe/src/pulse_cardinality_bridge.rs` (plus a
one-line `mod` declaration and a one-line `pub use` re-export in
`crates/self-observe/src/lib.rs`), the `crates/pulse/**` diff filter
naturally picks up the four modified pulse files and the
`crates/self-observe/**` diff filter naturally picks up the new
bridge file and the re-export changes in lib.rs. The two jobs run in
parallel by design, providing per-crate mutation coverage at the 100%
kill-rate gate (CLAUDE.md, ADR-0005 Gate 5). Option 2 would duplicate
the existing jobs' behaviour for no benefit. Option 3 would conflate
two independent crate-level mutation budgets into one job, REJECTED
on proportionality grounds: the two jobs already exist precisely so
each crate's mutation feedback stays attributed and parallel.

**Mutation scope** (per ADR-0051 Verification and the DESIGN
"Earned-Trust verification layers" section):

For `gate-5-mutants-pulse` over `crates/pulse/**`:

- The cap-arm `>=` boundary (a `>=` to `>` mutant must be killed by
  the boundary scenario at exactly N; a `>=` to `<` mutant must be
  killed by the over-by-one scenario at N+1; US-01 Scenario 4).
- The shadow-counter increment on a new-key insert (a mutant that
  fails to increment is killed by the at-cap refusal scenario, which
  would fire earlier than expected; US-01 Scenario 2).
- The shadow-counter post-replay initialisation in `open()` (a mutant
  that skips the initialisation pass is killed by the post-replay
  live-ingest refusal scenario; US-04 Scenario 2).
- The `enforce_cap=false` on the WAL-replay call site (a mutant that
  flips it to `true` is killed by the tightened-cap replay scenario;
  replay would refuse the surplus and retroactively un-accept
  already-accepted data; US-04).
- The per-metric loop continue-vs-break (a mutant that aborts the
  loop on first refusal is killed by US-05 Scenario 1; the existing-
  series points in the same batch would not land).
- The `MetricsRecorder::record_series_refused` invocation (a mutant
  that elides the call when refused > 0 is killed by US-03 Scenario
  2; the `CapturingRecorder` events vector would be empty).
- The global-vs-per-tenant counter (a mutant that uses `series.len()`
  instead of the per-tenant projection is killed by US-02 Scenarios
  1, 2, 3; tenant B's count would include tenant A's).

For `gate-5-mutants-self-observe` over `crates/self-observe/**`:

- The bridge emission (a mutant that elides the `MetricStore::ingest`
  call on the bridge target store is killed by the bridge integration
  test asserting the `pulse.series.refused.count` points land with
  the expected value, kind, and `{tenant}` attribute).
- The bridge's metric-name string, kind, value, and attribute (mutants
  that empty or alter any of these are killed by the integration
  test's expected-points assertions).

### [A3] No new public event name; no new dashboard

Refusal rides on two surfaces: the existing `IngestReceipt` struct
gains one additive field (`series_refused: usize`), and the existing
`MetricsRecorder` trait gains one additive default-method
(`record_series_refused`). The bridge emits a self-observe metric
named `pulse.series.refused.count` following the existing
self-observe naming convention (`<source>.<event>.count`,
established by `lumen.<event>.count` and `cinder.<event>.count` per
ADR-0038). There is no new event vocabulary at the wire level; the
OTLP partial-success path on aperture is the natural translation of
the per-call receipt, and that translation is aperture's own slice,
not this one (ADR-0051 Decision 7). No metric counter beyond the
bridge's emission, no dashboard, no alert threshold. At v0/v1 the
platform's self-observe is the entire observability stack; the
receipt field and the bridge-emitted points ARE the signal. Recorded
so DELIVER does not invent an alert routing story.

### [A4] No new external dependency; Gate 4 unaffected (deny.toml unchanged)

The cap-check uses arithmetic on a `usize` shadow counter and a
`HashMap<TenantId, usize>` lookup. The bridge holds `Arc<dyn
pulse::MetricStore + Send + Sync>` and calls `MetricStore::ingest`
with a `MetricBatch`, the same shape as the existing
`LumenToPulseRecorder` and `CinderToPulseRecorder`. The
`crates/self-observe/Cargo.toml` already lists `aegis` and `pulse` as
dependencies (the two bridges' authority). No new crate is needed.
VERIFIED by reading `deny.toml`:

- The `[graph].targets` (x86_64-linux-gnu, aarch64-linux-gnu,
  x86_64-darwin, aarch64-darwin) already cover the supported
  platforms.
- The `[licenses].allow` list is unaffected (no new transitive crate
  is added).
- The `[bans]` list is unaffected (no new dependency, so no new
  duplicate-version concern; the `multiple-versions = "allow"`
  relaxation is unchanged).
- The `[advisories]` and `[sources]` policies are unaffected.

**Verdict**: zero change to `deny.toml`.

### [A5] No new graduation tag; no per-crate release

Neither pulse nor self-observe is a graduated crate (neither appears
in Gate 2 or Gate 3's `--package` lists; the locked set is
harness / spark / sieve / codex per ci.yml lines 326-347). The
change lands as a `git commit` on `main` (pure trunk-based per P5)
under the existing crate manifests. No `pulse-vX.Y.Z` or
`self-observe-vX.Y.Z` tag is created by this slice. Recorded so
DELIVER does not invent a release story.

### [A6] No observability / monitoring / alerting instrumentation beyond the two refusal surfaces

The receipt field (`series_refused`) and the bridge emission of
`pulse.series.refused.count` are the entire observability surface for
slice 01. There is no new bridge-latency counter, no caps-breached
gauge, no histogram. For a focused refinement with no new deployment
artefact, the CI gates ARE the alerting surface: a regression fails
Gate 1 (test) on one of the new acceptance suites, or Gate 5
(`gate-5-mutants-pulse` or `gate-5-mutants-self-observe`) on the
modified files at the next push.

### [A7] No deployment / rollback procedure beyond git revert

There is no new deployment artefact, so there is nothing to roll back
at the deployment layer. pulse and self-observe remain library-only;
their consumers (kaleidoscope-gateway, aperture-storage-sink, any
future composition root) compile against the unchanged
`pulse::MetricStore` trait method signatures (ADR-0051 Decision 6);
the additive items (the `pub const`, the receipt field, the default
trait method, the new variant, the new bridge struct, the re-export)
are additions, not breaking changes. The project is pure trunk-based
with no merge gate (memory `project_kaleidoscope_pure_trunk_based`);
the recovery is fix-forward on `main`. This satisfies the
rollback-first principle vacuously for the deployment layer: the only
"rollback" available and needed is a git revert of the slice commit,
and because the WAL on-disk shape is unchanged (`WalRecord::Ingest`
is byte-identical per ADR-0051 Decision 6) and the cap adds no
on-disk state-format change, a revert has no data consequence (an
existing FileBackedMetricStore on disk continues to load under the
pre-cap code path with no migration). Any in-flight client that
received a `series_refused > 0` receipt on the reverted branch sees
only a `series_refused = 0` on subsequent receipts (the field would
no longer exist, which a structural diff would catch at the receipt
construction site); the within-cap path is untouched by the slice
and untouched by the revert.

## Verification against ci.yml (CONFIRM, not modify)

Read of `.github/workflows/ci.yml` in this wave confirmed:

| Claim | Verified location | Result |
|-------|-------------------|--------|
| Gate 2 (`cargo public-api`) scopes to harness / spark / sieve / codex; pulse and self-observe excluded | lines 326-347 (`-p otlp-conformance-harness`, `-p spark`, `-p sieve`, `-p codex`) | CONFIRMED, neither pulse nor self-observe is present |
| Gate 3 (`cargo semver-checks`) scopes to the same four; pulse and self-observe excluded | same `--package` set as Gate 2 | CONFIRMED, neither is present |
| `gate-5-mutants-pulse` job exists and runs `cargo mutants --in-diff` over `crates/pulse/**` | line 1297; invocation `cargo mutants --package pulse --in-diff "$DIFF_FILE" --no-shuffle --jobs 2` (lines 1359-1363) with `origin/main -> HEAD~1 -> full` cascade (lines 1340-1370) and empty-diff short-circuit (lines 1352-1355); diff filter is `git diff "$BASELINE" HEAD -- 'crates/pulse/**'` (line 1351) | CONFIRMED present, covers `crates/pulse/src/{lib.rs, store.rs, file_backed.rs, metrics.rs}` |
| `gate-5-mutants-self-observe` job exists and runs `cargo mutants --in-diff` over `crates/self-observe/**` | line 862; invocation `cargo mutants --package self-observe --in-diff "$DIFF_FILE" --no-shuffle --jobs 2` (lines 928-932) with the same cascade (lines 909-939) and short-circuit; diff filter `git diff "$BASELINE" HEAD -- 'crates/self-observe/**'` (line 920) | CONFIRMED present, covers `crates/self-observe/src/pulse_cardinality_bridge.rs` (new file) and the one-line edit to `crates/self-observe/src/lib.rs` (the mod declaration and re-export) |

Both crate paths (`crates/pulse/**`, `crates/self-observe/**`) include
the respective `src/` directories, which contain all the files this
feature modifies and adds. No new mutation job is needed; the existing
two jobs cover the modified files via `--in-diff` at the 100% kill
rate.

No workflow file was modified by this wave. No gate was added, removed,
or amended.

## Verification against deny.toml (CONFIRM, not modify)

Read of `deny.toml` in this wave confirmed that no policy change is
needed:

- `[graph].targets`: the four supported triples (linux x86_64,
  linux aarch64, darwin x86_64, darwin aarch64) are unchanged.
- `[licenses].allow`: zero new transitive licence (no new dependency
  introduced; the cap is core arithmetic; the bridge reuses
  `aegis` + `pulse`, both already in `crates/self-observe/Cargo.toml`).
- `[bans]`: zero new dependency, so the `multiple-versions = "allow"`
  relaxation and the `deny = [{ name = "openssl", ... }]` clauses are
  unaffected.
- `[advisories]`: `yanked = "deny"` policy is unaffected.
- `[sources]`: `unknown-registry = "deny"`, `unknown-git = "deny"`
  policies are unaffected.

**Verdict**: zero change to `deny.toml`. The Gate 4 (`cargo deny`) run
on the slice's commit will pass with the same policy that passed at
`honest-read-caps-v0` close.

## KPI to gate mapping

All outcome KPIs are correctness indicators collected by green
acceptance tests under **Gate 1** (`cargo test --workspace`) running
the new acceptance suites, with Gate 5 (`gate-5-mutants-pulse` and
`gate-5-mutants-self-observe`) guarding the test-suite strength
behind the cap-check assertions in the modified pulse files and the
bridge emission in the new self-observe file. The trait-signature KPI
is additionally collected by the compile of existing consumers under
Gate 1.

| KPI (from outcome-kpis.md) | Target | Gate | Collection |
|----------------------------|--------|------|------------|
| North star: write-side cardinality Earned-Trust claim is honest under overreach | within-cap pass + boundary + per-tenant isolation + replay coherence + partial-apply + two observability surfaces | Gate 1 | the new `slice_01_*.rs` files plus the self-observe bridge integration test |
| Within-cap happy path: receipt.count = points, receipt.series_refused = 0 | 1 IngestReceipt with the expected pair of values | Gate 1 | within_cap scenario (US-01 Scenario 1) |
| At-cap refusal: the (N+1)-th new SeriesKey is refused; receipt.series_refused = 1; index width unchanged | refused = 1, points_stored = 0 for that metric, no insert | Gate 1 | at_cap_refuses scenario (US-01 Scenario 2) |
| Existing-series continues post-cap | a SeriesKey already in the index keeps receiving points | Gate 1 | existing_series_continues scenario (US-01 Scenario 3) |
| Boundary at exactly MAX_SERIES_PER_TENANT served; at +1 refused | the N-th accepted, the (N+1)-th refused | Gate 1 | boundary_inclusive_and_exclusive scenarios (US-01 Scenario 4) |
| Per-tenant isolation: tenant A at cap; tenant B unaffected | tenant B's count unchanged; tenant B's new keys insert | Gate 1 | per_tenant_isolation scenarios (US-02 Scenarios 1, 2, 3) |
| Receipt-field observability: receipt.series_refused is honest on every call | zero on within-cap; positive on at-cap; accumulated across a partial-apply batch | Gate 1 | receipt_observability scenarios (US-03 Scenarios 1, 3) |
| Recorder-hook observability: CapturingRecorder accumulates RecordedEvent::SeriesRefused | events vector contains SeriesRefused { tenant, count } when refused > 0; absent when refused = 0 | Gate 1 | recorder_observability scenarios (US-03 Scenarios 2, 4) |
| Bridge emits pulse.series.refused.count via the second pulse store | a point with metric name "pulse.series.refused.count", value=count as f64, kind Sum, point attribute {tenant} lands in the bridge target store | Gate 1 | bridge integration test in crates/self-observe/tests/ |
| WAL replay rebuilds existing series past the cap (enforce_cap=false) | the rebuilt index width equals the WAL-seeded width regardless of MAX_SERIES_PER_TENANT | Gate 1 | wal_replay_rebuilds scenarios (US-04 Scenarios 1, 2) |
| Post-replay live ingest refuses for tenants at or above the cap | the first new live ingest after replay is refused | Gate 1 | post_replay_live_refusal scenario (US-04 Scenario 2) |
| Partial-apply: the per-metric loop never aborts | existing-series points land, new-below-cap inserts, new-above-cap refused, all in the same batch | Gate 1 | partial_apply_batch scenarios (US-05 Scenarios 1, 2, 3) |
| pulse::MetricStore trait method signatures unchanged | 0 signature changes on ingest, query, query_with | Gate 1 | compile of kaleidoscope-gateway, aperture-storage-sink, self-observe under Gate 1 |
| Cap-arm boundary (>=) not deletable | 0 surviving mutants on the cap arm in store.rs and file_backed.rs | Gate 5 | gate-5-mutants-pulse --in-diff over the four modified pulse files |
| Shadow-counter increment on insert not deletable | 0 surviving mutants on the `+= 1` line in either adapter | Gate 5 | gate-5-mutants-pulse --in-diff |
| Shadow-counter post-replay initialisation in open() not deletable | 0 surviving mutants on the initialisation pass in file_backed.rs | Gate 5 | gate-5-mutants-pulse --in-diff (file_backed.rs) |
| enforce_cap=false on the WAL-replay call site not flippable | 0 surviving mutants on the `enforce_cap=false` argument | Gate 5 | gate-5-mutants-pulse --in-diff (file_backed.rs) |
| Per-metric loop continue-vs-break (partial-apply) not breakable | 0 surviving mutants that swap continue for break | Gate 5 | gate-5-mutants-pulse --in-diff (store.rs, file_backed.rs) |
| MetricsRecorder::record_series_refused invocation not elidable | 0 surviving mutants that elide the call when refused > 0 | Gate 5 | gate-5-mutants-pulse --in-diff (store.rs, file_backed.rs) |
| Bridge emission of pulse.series.refused.count not elidable | 0 surviving mutants that elide the MetricStore::ingest call on the bridge target | Gate 5 | gate-5-mutants-self-observe --in-diff over the new bridge file |

## Infrastructure summary

- **Deployment**: none new (pulse and self-observe stay library-only;
  no new binary, no new container).
- **CI/CD**: GitHub Actions, ADR-0005 five gates, inherited unchanged.
  `gate-5-mutants-pulse` already present at ci.yml line 1297 covering
  `crates/pulse/**` via `--in-diff`; `gate-5-mutants-self-observe`
  already present at ci.yml line 862 covering `crates/self-observe/**`
  via `--in-diff`. No new or amended job.
- **Branching**: pure trunk-based (project default, unchanged).
- **Mutation testing**: per-feature, 100% kill rate, scoped by
  `--in-diff` to the four modified files in `crates/pulse/src/` (one
  job) and the new bridge file plus the one-line lib.rs edit in
  `crates/self-observe/src/` (one job).
- **External integrations**: none. No contract tests apply.
- **External dependencies**: none new. `deny.toml` unchanged.
- **Observability**: two pulse-internal surfaces (the additive
  `IngestReceipt::series_refused` field; the additive
  `MetricsRecorder::record_series_refused` default method) plus the
  bridge emission of `pulse.series.refused.count` via the existing
  self-observe naming convention. No new event vocabulary at the wire
  level; CI gates are the alerting surface (A6).
- **Public surface (pulse)**: three additive items
  (`MAX_SERIES_PER_TENANT` `pub const`,
  `IngestReceipt::series_refused` field,
  `MetricsRecorder::record_series_refused` default method); the three
  `MetricStore` trait method signatures (ingest, query, query_with)
  are byte-identical to the prior tag.
- **Public surface (self-observe)**: two additive items
  (`PulseCardinalityToPulseRecorder` struct, re-export from lib.rs);
  no existing item changed.
- **Graduation tag**: none (neither crate is in Gate 2 / Gate 3's
  locked set).
- **Docker**: out of scope for slice 01; the pre-existing
  kaleidoscope-cli Dockerfile is untouched.

## Artefacts produced by this wave

| Artefact | Path |
|----------|------|
| Environment inventory (clean target environment, in-process integration tests, no external services) | `docs/feature/pulse-cardinality-watermark-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/pulse-cardinality-watermark-v0/devops/wave-decisions.md` |

## Artefacts judged N/A (with reason)

| Skipped artefact | Reason |
|------------------|--------|
| `kpi-instrumentation.md` | Every KPI maps to Gate 1 on the new acceptance suites (plus Gate 5 for suite strength on the modified files of each crate); no instrumentation to design beyond the two observability surfaces already mandated by ADR-0051. A separate file would only restate the KPI to gate mapping table above. |
| `ci-cd-pipeline.md` | This feature adds no job and edits no workflow; the existing `gate-5-mutants-pulse` and `gate-5-mutants-self-observe` jobs cover the modified files as-is. The "Verification against ci.yml" section above is the entire pipeline content for this feature; a separate addendum would be empty. |
| `platform-architecture.md` | No platform infrastructure to architect (no cloud, no orchestration, no service mesh). Morgan's `../design/application-architecture.md` is sufficient. |
| `observability-design.md` / `monitoring-alerting.md` | No runtime monitoring beyond the two pulse-internal surfaces and the bridge emission, all designed in ADR-0051 (A3, A6); CI gates are the alerting surface. |
| `infrastructure-integration.md` | No external integrations at runtime (DESIGN: external integrations = none). |
| `branching-strategy.md` | Pure trunk-based is the project default; no per-feature deviation (P5). |
| `deployment-strategy.md` / `rollback.md` | No new deployment artefact; recovery is git revert with no data-format consequence (A7: WAL on-disk shape is byte-identical per ADR-0051 Decision 6). |
| `docker.md` / `containers.md` | Out of scope for slice 01 (environments.yaml); the pre-existing kaleidoscope-cli Dockerfile is untouched. |

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Write the new acceptance suites at `crates/pulse/tests/slice_01_*.rs` covering US-01 (within-cap, at-cap, existing-series-continues, boundary inclusive/exclusive), US-02 (per-tenant isolation), US-03 (receipt and recorder observability), US-04 (WAL replay coherence and post-replay live refusal), US-05 (partial-apply); and a bridge integration test at `crates/self-observe/tests/` asserting the bridge emits `pulse.series.refused.count` with value=count as f64, kind Sum, point attribute {tenant} via `MetricStore::ingest` on a second pulse store | The `clean` environment with in-process construction of FileBackedMetricStore (per-test tempdir for the WAL) and InMemoryMetricStore is the only environment to parametrise over (environments.yaml); the cap and the bridge are the contract |
| At DISTILL | DO NOT edit `.github/workflows/ci.yml` | No new gate; Gate 1 auto-discovers the new test files, and the existing `gate-5-mutants-pulse` and `gate-5-mutants-self-observe` jobs already cover mutation on the modified files via `--in-diff` (A1, A2) |
| At DISTILL | DO NOT add pulse or self-observe to Gate 2 or Gate 3 | They are not graduated crates; the locked set scopes to harness / spark / sieve / codex (A1) |
| At DISTILL | DO NOT propose a new combined `gate-5-mutants-pulse-cardinality-watermark` job | The two existing per-crate jobs already cover the modified files via `--in-diff`; collapsing them would conflate two independent mutation budgets for no benefit (A2) |
| At DELIVER | Declare `MAX_SERIES_PER_TENANT: usize = 10_000` as `pub const` in `crates/pulse/src/lib.rs` | ADR-0051 Decision 1 and DESIGN D1, D6; the constant is intended public so the acceptance suite can address it by name (boundary tests stay stable across a future cap-value re-tune) |
| At DELIVER | Extend `IngestReceipt` with `pub series_refused: usize`; update the two construction sites (in-memory adapter at `crates/pulse/src/store.rs`; file-backed adapter at `crates/pulse/src/file_backed.rs`, both the empty-batch path and the normal-ingest path) | ADR-0051 Decision 2 and DESIGN D2, D6 |
| At DELIVER | Add the shadow per-tenant counter `series_count_per_tenant: HashMap<TenantId, usize>` inside both `InnerState` (in-memory) and `Inner` (file-backed), under the same Mutex as the existing series map; in `FileBackedMetricStore::open()`, after WAL replay completes, populate by counting rebuilt series per tenant in one pass | ADR-0051 Decision 5 and DESIGN D7; the same Mutex serialises the cap-check, the shadow-counter increment, and the series-map insert; the three are atomic per metric |
| At DELIVER | Add the `enforce_cap: bool` parameter to the private `apply_ingest` function in `crates/pulse/src/file_backed.rs`; pass `false` from the WAL-replay call site (inside `open()` at line 158 of the current code) and `true` from the live-ingest call site (inside `FileBackedMetricStore::ingest` at line 273) | ADR-0051 Decision 4 and DESIGN D5; the cap NEVER fires during WAL replay; the cap is a forward-looking gate |
| At DELIVER | Add `fn record_series_refused(&self, _tenant: &TenantId, _count: usize) {}` to `MetricsRecorder` with a default no-op body; add `RecordedEvent::SeriesRefused { tenant: TenantId, count: usize }`; override `record_series_refused` on `CapturingRecorder` to push the variant | ADR-0051 Decision 2 and DESIGN D6; the default body keeps the trait addition non-breaking |
| At DELIVER | Place the cap arm inside `apply_ingest`'s per-metric for-loop, BEFORE `series.entry(key).or_insert_with(...)` is called for a key that DOES NOT already exist; mirror the same arm in `InMemoryMetricStore::ingest` | ADR-0051 Decision 5 and DESIGN D7; the cap rides in the store implementation, not the trait; the two adapters' semantics stay in lockstep |
| At DELIVER | The per-metric loop NEVER aborts on a refused metric; continue to the next metric and accumulate `refused` locally; `record_series_refused(tenant, refused)` is called once per ingest call only when `refused > 0` | ADR-0051 Decision 3 and DESIGN D3; partial-apply, not reject-whole |
| At DELIVER | Create `crates/self-observe/src/pulse_cardinality_bridge.rs` with one struct `PulseCardinalityToPulseRecorder` holding `Arc<dyn pulse::MetricStore + Send + Sync>`; impl `pulse::MetricsRecorder` whose `record_series_refused(tenant, count)` emits a one-point `MetricBatch` named `pulse.series.refused.count`, value=`count as f64`, kind `Sum`, point attribute `{tenant}`; `record_ingest` and `record_query` are no-ops | ADR-0051 Decision 2 (final paragraph); mirrors `LumenToPulseRecorder` and `CinderToPulseRecorder` |
| At DELIVER | Add the `mod pulse_cardinality_bridge;` declaration and `pub use pulse_cardinality_bridge::PulseCardinalityToPulseRecorder;` re-export to `crates/self-observe/src/lib.rs` | DESIGN Changes Per File |
| At DELIVER | Turn the four modified pulse files' mutants AND the new self-observe bridge file's mutants 100% killed before close | CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5 (A2) |
| At DELIVER | Do not change the `pulse::MetricStore` trait method signatures (ingest, query, query_with); do not change the `WalRecord::Ingest` on-disk shape | ADR-0051 Decision 6: the cap rides in the implementation, not on the trait or on the WAL record |
| At DELIVER | Do not introduce a separate per-tenant cumulative refused-since-start counter inside `Inner`; the longitudinal view lives in the recorder emission seam | ADR-0051 Decision 5 (final paragraph); avoids duplicating state that is already a derived view of the recorder events |
| At DELIVER | Do not invent a structured event log, a new metric envelope, a Prism panel, a beacon rule, or an alert threshold beyond the receipt and the recorder hook | ADR-0051 Decision 7; the two surfaces ARE the signal |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**What DISTILL receives**: the mandatory `environments.yaml` for
Mandate 4 (the `clean` environment, in-process integration tests, no
external services); the confirmation that no CI edit is needed (A1,
A2); the confirmation that `deny.toml` is unchanged (A4); the
per-crate mutation coverage map (`gate-5-mutants-pulse --in-diff` over
`crates/pulse/**` covers the four modified files;
`gate-5-mutants-self-observe --in-diff` over `crates/self-observe/**`
covers the new bridge file and the one-line `lib.rs` edit); the
constraint that `MAX_SERIES_PER_TENANT` is a `pub` informational item
in `crates/pulse/src/lib.rs`, that `IngestReceipt::series_refused` is
an additive public field, that `MetricsRecorder::record_series_refused`
is an additive default method, and that
`PulseCardinalityToPulseRecorder` is the new bridge struct re-exported
from `crates/self-observe/src/lib.rs`, all alongside the unchanged
`pulse::MetricStore` trait method signatures and the unchanged
`WalRecord::Ingest` on-disk shape; and the KPI to gate mapping above.

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt
of this wave's outputs.
