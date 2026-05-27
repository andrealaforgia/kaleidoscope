# Wave Decisions - honest-read-caps-v0 / DEVOPS

British English. No em dashes.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-27
- **Mode**: slim DEVOPS. This wave confirms that the existing CI
  contract already covers the three read-API crates' lib.rs changes,
  and that no new infrastructure is warranted by a six-pub-const,
  six-if-arm cross-cutting refinement. The decision to run slim, and
  its shape, are Apex's own judgement from the DESIGN handoff, not
  pre-taken.

## Why this wave is slim

The feature is a cross-cutting refinement that makes the read-side
Earned-Trust claim code. ADR-0050 records the refinement. The
deliverable is:

- Two `pub const` per crate (`MAX_WINDOW_SECONDS = 86_400`,
  `MAX_RESULT_ROWS = 100_000`) in each of the three existing read-API
  crates' `lib.rs` (`crates/query-api/src/lib.rs`,
  `crates/log-query-api/src/lib.rs`,
  `crates/trace-query-api/src/lib.rs`).
- Two new `if` arms per handler (`handle_query_range`, `handle_logs`,
  `handle_traces`): a window-cap check between `parse_time_range`
  success and the store query, and a result-cap check between the
  store result and `success_response`.

There is NO new workspace crate, NO new external dependency (the
cap-checks are arithmetic over an already-parsed `u64` window and
`Vec::len()`; both are core), NO new public event name (the existing
`{status:"error", error:"<reason>"}` envelope is reused with two new
named reason strings), NO new CI gate, NO new graduation tag (no new
crate to tag; the three read-API crates are not in Gate 2 / Gate 3's
locked set), and NO new external integration. The DESIGN "DEVOPS
Handoff Annotation"
(`../design/wave-decisions.md` lines 448-501) anticipated every DEVOPS
conclusion; the job of this wave is to VERIFY those conclusions against
the live CI workflow and `deny.toml`, record the verification, and
hand off, not to re-litigate the design.

This follows the `earned-trust-fsync-probe-v0` slim-DEVOPS precedent.
That precedent produced two files (`environments.yaml`,
`wave-decisions.md`); this wave produces the same two, for the same
reasons recorded there under "Artefacts judged N/A".

## Inputs read (in dependency order)

1. `CLAUDE.md` - paradigm (Rust idiomatic) and the per-feature mutation
   testing strategy at 100% kill rate (declared; not modified here).
2. `../discuss/wave-decisions.md` and `../discuss/user-stories.md` -
   the four DISCUSS flags Morgan resolved at DESIGN, and the US-01
   through US-05 acceptance scenarios.
3. `../design/wave-decisions.md` - DESIGN decisions D1-D7 and the
   explicit DEVOPS Handoff Annotation (no new crate, no new
   dependency, the three existing `gate-5-mutants-*-query-api` jobs
   cover the modified lib.rs of each crate via `--in-diff`, no new
   event name, three orthogonal Earned-Trust enforcement layers
   reproduced from ADR-0049).
4. `../design/application-architecture.md` - C4 L2, the Changes Per
   File table (the exact line loci in the three lib.rs files).
5. `docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`
   - the cross-cutting refinement ADR (Accepted).
6. `docs/feature/earned-trust-fsync-probe-v0/devops/{environments.yaml,wave-decisions.md}`
   - the immediate slim-DEVOPS sibling shape this wave mirrors.
7. `.github/workflows/ci.yml` - the existing five-gate workflow, read
   to CONFIRM (not modify) the three `gate-5-mutants-*-query-api`
   jobs' existence and `--in-diff` scopes (see "Verification against
   ci.yml" below).
8. `deny.toml` - read to CONFIRM no licence / ban / advisory policy
   change is needed (no new dependency).

## Pre-wave decisions (carried in from project convention, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| P1 | `deployment_target` | None new (the three read-API crates stay library-only; no new binary, no new container) | DESIGN handoff + ADR-0050 |
| P2 | `container_orchestration` | N/A (slice 01 produces no container image; the pre-existing kaleidoscope-cli Dockerfile is untouched) | environments.yaml |
| P3 | `cicd_platform` | GitHub Actions (existing, unchanged) | ADR-0005 |
| P4 | `existing_infrastructure` | Yes (workspace + five-gate CI; all three `gate-5-mutants-*-query-api` jobs already present at ci.yml lines 1036, 1123, 1210) | ci.yml |
| P5 | `git_branching_strategy` | Trunk-based, pure (main has no required-status-checks; CI is feedback, not a gate) | memory `project_kaleidoscope_pure_trunk_based` |
| P6 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md, ADR-0005 Gate 5 |

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] No new CI gate; ADR-0005's five gates inherited unchanged

The change touches one file in each of the three read-API crates
(`lib.rs`), adds zero files in `src/`, and adds three new test files
under `crates/<crate>/tests/`. Each gate is satisfied by existing
machinery:

- **Gate 1 (`cargo test --workspace`)**: runs the three new acceptance
  suites
  (`crates/query-api/tests/slice_05_honest_caps.rs`,
  `crates/log-query-api/tests/slice_02_honest_caps.rs`,
  `crates/trace-query-api/tests/slice_02_honest_caps.rs`) with zero
  workflow edit. This is the KPI collection surface for every
  behavioural KPI (within-cap happy path, over-window 400 before
  store, over-result 400 before serialisation, boundary at exactly the
  cap, redaction, trace handler order).
- **Gate 2 (`cargo public-api`)** and **Gate 3 (`cargo semver-checks`)**:
  scope to harness / spark / sieve / codex only; query-api,
  log-query-api, and trace-query-api are not in the locked set
  (verified below). No diff applies. `pulse::MetricStore`,
  `lumen::LogStore`, and `ray::TraceStore` trait signatures are
  unchanged regardless (ADR-0050 Decision 5: no store-trait change).
- **Gate 4 (`cargo deny`)**: no new external dependency, so no scan
  change. VERIFIED in `deny.toml`: no edit required (A4 below).
- **Gate 5 (`cargo mutants`)**: covered by the existing three
  `gate-5-mutants-*-query-api` jobs for the modified `lib.rs` of each
  crate; no new job (A2).

No new or amended gate is warranted. No new CI workflow file is
created; no existing gate is added to, removed, or modified by this
feature.

### [A2] Mutation testing: the three existing `gate-5-mutants-*-query-api` jobs cover the modified `lib.rs` files; no workflow edit

**Options considered**:

1. **Rely on the three existing `gate-5-mutants-query-api`,
   `gate-5-mutants-log-query-api`, `gate-5-mutants-trace-query-api`
   jobs** (which each run `cargo mutants --package <crate> --in-diff`
   against `crates/<crate>/**`).
2. Add a new file-scoped job pinned to the three modified `lib.rs`
   files.
3. Add a new combined `gate-5-mutants-honest-read-caps` job covering
   all three.

**Decision**: Option 1.

**Rationale**: All three existing jobs exist in `ci.yml`
(`gate-5-mutants-query-api` at line 1036, `gate-5-mutants-log-query-api`
at line 1123, `gate-5-mutants-trace-query-api` at line 1210) and each
runs the `--in-diff` cascade against its own crate path with the
`origin/main -> HEAD~1 -> full` baseline, short-circuiting to a
zero-second exit on an empty diff. Because this feature modifies
`crates/query-api/src/lib.rs`, `crates/log-query-api/src/lib.rs`, and
`crates/trace-query-api/src/lib.rs` exclusively, each crate's diff
filter (`crates/<crate>/**`; verified at ci.yml lines 1094, 1181,
1268) naturally picks up the modified `lib.rs` of its own crate. The
three jobs run in parallel by design, providing per-crate mutation
coverage at the 100% kill-rate gate (CLAUDE.md, ADR-0005 Gate 5).
Option 2 would duplicate the existing jobs' behaviour for no benefit.
Option 3 would conflate three independent crate-level mutation
budgets into one job, REJECTED on proportionality grounds: the three
jobs already exist precisely so each crate's mutation feedback stays
attributed and parallel.

**Mutation scope (per DESIGN ADR-0050 Verification)**: the modified
`lib.rs` of each of the three crates. Primary mutation targets per
crate per the DESIGN DEVOPS Handoff Annotation:

- The window-cap `>` boundary (a `>` -> `>=` mutant must be killed by
  the `MAX_WINDOW_SECONDS` boundary inclusive test; a `>` -> `<`
  mutant must be killed by the over-by-one test).
- The result-cap `>` boundary (same shape).
- The order-of-checks (a mutant that swaps the window-cap check and
  `state.store.query(...)` order is killed by the lying-store
  assertion that `query()` was NOT called on the over-window path).
- The two cap reason strings (a mutant that empties or alters the
  named reason is killed by the redaction tests and the
  reason-substring assertions; `trace-query-api`'s stricter posture
  additionally kills mutants that introduce "SECRET" or "Bearer" or
  echo the raw `service`).

### [A3] No new public event name; no new dashboard

Refusal rides on the existing `{status:"error", error:"<reason>"}`
envelope. The two new reason strings ("window exceeds maximum",
"result exceeds maximum" within the named-class constraint of D7) are
inside the existing envelope shape; Prism's `isPromError` (ADR-0042
lines 220-229) already handles them as the same error class as the
matcher-400 and bounds-400 errors. There is no new event vocabulary,
no metric counter, no dashboard, no alert threshold. At v0/v1 the
platform has no live observability stack of its own; the 400 IS the
signal. Recorded so DELIVER does not invent an alert routing story.

### [A4] No new external dependency; Gate 4 unaffected (`deny.toml` unchanged)

The cap-check uses arithmetic on the `u64` window value
`parse_time_range` already returns (or on the `range.start_unix_nano`
/ `range.end_unix_nano` divided by 1e9, at the crafter's choice) and
`Vec::len()` on the store result. Both are core. No `nix`, no
`libc`, no new crate. VERIFIED by reading `deny.toml`:

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

There is no new crate, so there is no new per-crate tag at graduation.
The change lands as a `git commit` on `main` (pure trunk-based per P5)
under the three existing read-API crate manifests. No
`query-api-vX.Y.Z`, `log-query-api-vX.Y.Z`, or
`trace-query-api-vX.Y.Z` tag is created by this slice (none of those
crates is in Gate 2 / Gate 3's locked set; they are not graduated
crates). Recorded so DELIVER does not invent a release story.

### [A6] No observability / monitoring / alerting instrumentation beyond the refusal envelope

The 400 with the existing `{status:"error", error:"<reason>"}`
envelope is the entire observability surface for slice 01. There is
no new bridge-latency counter, no caps-breached gauge, no histogram.
For a no-new-deployment cross-cutting refinement, the CI gates ARE
the alerting surface: a regression fails Gate 1 (test) on one of the
three new acceptance suites, or Gate 5 (the three
`gate-5-mutants-*-query-api` jobs) on the modified `lib.rs` of the
affected crate at the next push.

### [A7] No deployment / rollback procedure beyond git revert

There is no new deployment artefact, so there is nothing to roll back
at the deployment layer. The three read-API crates remain
library-only; their `router()` signatures are byte-identical to the
prior tag (ADR-0050 Decision 5); consumers
(`kaleidoscope-gateway` and any future composition root) compile
without diff. The project is pure trunk-based with no merge gate
(memory `project_kaleidoscope_pure_trunk_based`); the recovery is
fix-forward on `main`. This satisfies the rollback-first principle
vacuously for the deployment layer: the only "rollback" available and
needed is a git revert of the slice commit, and because the caps add
no on-disk state-format change and no new public event vocabulary, a
revert has no data consequence (any in-flight client that received a
cap-400 on the reverted branch sees only a "named class disappeared"
on the next request; the within-cap and existing-error-class paths
are untouched by the slice and untouched by the revert).

## Verification against ci.yml (CONFIRM, not modify)

Read of `.github/workflows/ci.yml` in this wave confirmed:

| Claim | Verified location | Result |
|-------|-------------------|--------|
| Gate 2 (`cargo public-api`) scopes to harness / spark / sieve / codex; query-api, log-query-api, trace-query-api excluded | lines 326-347 (`-p otlp-conformance-harness`, `-p spark`, `-p sieve`, `-p codex`) | CONFIRMED, none of the three read-API crates is present |
| Gate 3 (`cargo semver-checks`) scopes to the same four; query-api, log-query-api, trace-query-api excluded | lines 420-433 (`--package` for the same four) | CONFIRMED, none of the three is present |
| `gate-5-mutants-query-api` job exists and runs `cargo mutants --in-diff` over `crates/query-api/**` | line 1036; invocation `cargo mutants --package query-api --in-diff "$DIFF_FILE"` (lines 1102-1106) with `origin/main -> HEAD~1 -> full` cascade (lines 1086-1098) and empty-diff short-circuit (lines 1095-1098); diff filter is `git diff "$BASELINE" HEAD -- 'crates/query-api/**'` (line 1094) | CONFIRMED present, covers `crates/query-api/src/lib.rs` |
| `gate-5-mutants-log-query-api` job exists and runs `cargo mutants --in-diff` over `crates/log-query-api/**` | line 1123; invocation `cargo mutants --package log-query-api --in-diff "$DIFF_FILE"` (lines 1189-1193) with the same cascade (lines 1173-1185); diff filter `git diff "$BASELINE" HEAD -- 'crates/log-query-api/**'` (line 1181) | CONFIRMED present, covers `crates/log-query-api/src/lib.rs` |
| `gate-5-mutants-trace-query-api` job exists and runs `cargo mutants --in-diff` over `crates/trace-query-api/**` | line 1210; invocation `cargo mutants --package trace-query-api --in-diff "$DIFF_FILE"` (lines 1276-1280) with the same cascade (lines 1260-1272); diff filter `git diff "$BASELINE" HEAD -- 'crates/trace-query-api/**'` (line 1268) | CONFIRMED present, covers `crates/trace-query-api/src/lib.rs` |

All three crate paths (`crates/query-api/**`, `crates/log-query-api/**`,
`crates/trace-query-api/**`) include the respective `src/lib.rs`,
which is the only file modified per crate. No new mutation job is
needed; the existing three jobs cover the modified files via
`--in-diff` at the 100% kill rate.

No workflow file was modified by this wave. No gate was added, removed,
or amended.

## Verification against deny.toml (CONFIRM, not modify)

Read of `deny.toml` in this wave confirmed that no policy change is
needed:

- `[graph].targets`: the four supported triples (linux x86_64,
  linux aarch64, darwin x86_64, darwin aarch64) are unchanged.
- `[licenses].allow`: zero new transitive licence (no new dependency
  introduced; core is unlicenced for purposes of `cargo deny`).
- `[bans]`: zero new dependency, so the `multiple-versions = "allow"`
  relaxation and the `deny = [{ name = "openssl", ... }]` clauses are
  unaffected.
- `[advisories]`: `yanked = "deny"` policy is unaffected.
- `[sources]`: `unknown-registry = "deny"`, `unknown-git = "deny"`
  policies are unaffected.

**Verdict**: zero change to `deny.toml`. The Gate 4 (`cargo deny`) run
on the slice's commit will pass with the same policy that passed at
`earned-trust-fsync-probe-v0` close.

## KPI to gate mapping

All outcome KPIs (`../discuss/outcome-kpis.md`) are correctness
indicators collected by green acceptance tests under **Gate 1**
(`cargo test --workspace`) running the three new acceptance suites,
with Gate 5 (the three `gate-5-mutants-*-query-api` jobs) guarding
the test-suite strength behind the cap-check assertions in each
crate's `lib.rs`. The trait-signature KPI is additionally collected
by the compile of existing consumers under Gate 1.

| KPI (from outcome-kpis.md) | Target | Gate | Collection |
|----------------------------|--------|------|------------|
| North star: read-side Earned-Trust claim is honest under overreach | within-cap pass + 4 cap classes refused across 3 crates | Gate 1 | the three `slice_*_honest_caps.rs` files (within-cap + over-window + over-result + boundary inclusive + boundary exclusive) |
| Within-cap happy path serves 200 (per crate) | 1 200 with matrix / bare JSON array | Gate 1 | within_cap scenario in each crate's new acceptance suite |
| Over-window 400 fires BEFORE store.query (per crate) | LyingStore.query() NEVER called | Gate 1 | the LyingMetricStore / LyingLogStore / LyingTraceStore scenario in each suite |
| Over-result 400 fires BEFORE serialisation (per crate) | cap-400 with no X-Truncated, no partial 200, no silent empty | Gate 1 | the real FileBack...Store seeded with MAX_RESULT_ROWS + 1 records, in each suite |
| Boundary at MAX_WINDOW_SECONDS served; +1 refused (per crate) | inclusive 200, exclusive 400 | Gate 1 | window_boundary_inclusive and window_boundary_exclusive scenarios |
| Boundary at MAX_RESULT_ROWS served; +1 refused (per crate) | inclusive 200, exclusive 400 | Gate 1 | result_boundary_inclusive and result_boundary_exclusive scenarios |
| Redaction posture symmetric per crate on the two new cap reasons (D7) | no raw start / end / service / SECRET / Bearer / Authorization in cap body | Gate 1 | mirror redaction scenarios in each suite; trace-query-api strictest |
| trace-query-api handler order preserved | missing-service 400 fires BEFORE window-cap 400 | Gate 1 | the_missing_service_400_still_fires_before_the_window_cap_400 scenario |
| pulse::MetricStore / lumen::LogStore / ray::TraceStore trait signatures unchanged | 0 signature changes | Gate 1 | compile of kaleidoscope-gateway and any consumer under Gate 1 |
| Window-cap `>` boundary not deletable (per crate) | 0 surviving mutants on the `>` check in each lib.rs | Gate 5 | `gate-5-mutants-query-api` / `-log-query-api` / `-trace-query-api` --in-diff over the modified lib.rs |
| Result-cap `>` boundary not deletable (per crate) | 0 surviving mutants on the `>` check in each lib.rs | Gate 5 | same three jobs |
| Order of checks (window-cap BEFORE store.query) not swappable | 0 surviving mutants on the cap-then-store ordering | Gate 5 | same three jobs; killed by the lying-store assertion |
| Named cap reason strings not deletable / alterable | 0 surviving mutants on the reason strings | Gate 5 | same three jobs; killed by the redaction tests and substring assertions |

## Infrastructure summary

- **Deployment**: none new (the three read-API crates stay
  library-only; no new binary, no new container).
- **CI/CD**: GitHub Actions, ADR-0005 five gates, inherited unchanged.
  All three `gate-5-mutants-*-query-api` jobs already present at
  ci.yml lines 1036, 1123, 1210; each covers the modified `lib.rs` of
  its crate via `--in-diff` over `crates/<crate>/**`. No new or
  amended job.
- **Branching**: pure trunk-based (project default, unchanged).
- **Mutation testing**: per-feature, 100% kill rate, scoped by
  `--in-diff` to `crates/query-api/src/lib.rs`,
  `crates/log-query-api/src/lib.rs`,
  `crates/trace-query-api/src/lib.rs` (one file per existing job).
- **External integrations**: none. No contract tests apply.
- **External dependencies**: none new. `deny.toml` unchanged.
- **Observability**: no new instrumentation beyond the reused
  `{status:"error", error:"<reason>"}` envelope with two new named
  reason strings; CI gates are the alerting surface.
- **Public surface (query-api, log-query-api, trace-query-api)**: two
  new public items per crate (`MAX_WINDOW_SECONDS`,
  `MAX_RESULT_ROWS`), both `pub const`. The `router()` and all store
  trait signatures (`pulse::MetricStore`, `lumen::LogStore`,
  `ray::TraceStore`) are byte-identical to the prior tag.
- **Graduation tag**: none (no new crate; the three read-API crates
  are not in Gate 2 / Gate 3's locked set).
- **Docker**: out of scope for slice 01; the pre-existing
  kaleidoscope-cli Dockerfile is untouched.

## Artefacts produced by this wave

| Artefact | Path |
|----------|------|
| Environment inventory (clean target environment, in-process tower oneshot, no external services) | `docs/feature/honest-read-caps-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/honest-read-caps-v0/devops/wave-decisions.md` |

## Artefacts judged N/A (with reason)

| Skipped artefact | Reason |
|------------------|--------|
| `kpi-instrumentation.md` | Every KPI maps to Gate 1 on the three new acceptance suites (plus Gate 5 for suite strength on the modified lib.rs of each crate); no instrumentation to design. A separate file would only restate the KPI to gate mapping table above. |
| `ci-cd-pipeline.md` | This feature adds no job and edits no workflow; the three existing `gate-5-mutants-*-query-api` jobs cover the modified lib.rs of each crate as-is. The "Verification against ci.yml" section above is the entire pipeline content for this feature; a separate addendum would be empty. |
| `platform-architecture.md` | No platform infrastructure to architect (no cloud, no orchestration, no service mesh). Morgan's `../design/application-architecture.md` is sufficient. |
| `observability-design.md` / `monitoring-alerting.md` | No runtime monitoring beyond the reused refusal envelope (A3, A6); CI gates are the alerting surface. |
| `infrastructure-integration.md` | No external integrations at runtime (DESIGN: external integrations = none). |
| `branching-strategy.md` | Pure trunk-based is the project default; no per-feature deviation (P5). |
| `deployment-strategy.md` / `rollback.md` | No new deployment artefact; recovery is git revert with no data-format consequence (A7). |
| `docker.md` / `containers.md` | Out of scope for slice 01 (environments.yaml); the pre-existing kaleidoscope-cli Dockerfile is untouched. |

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Write the three new acceptance suites at `crates/query-api/tests/slice_05_honest_caps.rs`, `crates/log-query-api/tests/slice_02_honest_caps.rs`, `crates/trace-query-api/tests/slice_02_honest_caps.rs`; each exercises within-cap, over-window (with LyingStore asserting query() never called), over-result (with real FileBack...Store seeded with MAX_RESULT_ROWS + 1 records), boundary inclusive and exclusive on both caps, redaction per crate's existing posture, and (trace-query-api only) handler order with missing-service 400 first | The `clean` environment with the in-process tower oneshot pattern is the only environment to parametrise over (environments.yaml); the caps are the contract |
| At DISTILL | DO NOT edit `.github/workflows/ci.yml` | No new gate; Gate 1 auto-discovers the three new test files, and the three existing `gate-5-mutants-*-query-api` jobs already cover mutation on each crate's modified `lib.rs` via `--in-diff` (A1, A2) |
| At DISTILL | DO NOT add `query-api`, `log-query-api`, or `trace-query-api` to Gate 2 or Gate 3 | They are not graduated crates; the locked set scopes to harness / spark / sieve / codex (A1) |
| At DISTILL | DO NOT propose a new combined `gate-5-mutants-honest-read-caps` job | The three existing per-crate jobs already cover the modified `lib.rs` of each crate via `--in-diff`; collapsing them would conflate three independent mutation budgets for no benefit (A2) |
| At DELIVER | Declare `MAX_WINDOW_SECONDS: u64 = 86_400` and `MAX_RESULT_ROWS: usize = 100_000` as `pub const` in each of the three crates' `lib.rs`, alongside the existing route constants | DESIGN application-architecture.md "Changes Per File" section; the constants are intended public so the acceptance suite can address them by name (boundary tests stay stable across a future cap-value re-tune) |
| At DELIVER | Place the window-cap check between `parse_time_range` success and `state.store.query(...)`; place the result-cap check between the store result (and any in-handler filtering) and `success_response` | DESIGN ADR-0050 Decision 4; ensures cap-fires-before-store on the window path (killing the swap mutant) and cap-fires-before-serialisation on the result path (saving JSON encoding cost) |
| At DELIVER | Reuse the existing `error_response` helper with the named-class reason strings; do not echo the numeric cap value, the raw window bounds, the raw query / regex, the raw service, "SECRET", or "Bearer" in the cap-400 body | DESIGN ADR-0050 Decision 7 (redaction posture); `trace-query-api` stays stricter |
| At DELIVER | Turn the three modified `lib.rs` files' mutants 100% killed before close (the two `>` boundaries per crate, the order of checks, the two reason strings per crate) | CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5 (A2) |
| At DELIVER | Do not invent a streaming JSON encoder, a paginated endpoint, an `X-Truncated` header, or a partial-200 truncation arm | ADR-0050 Decision 3: REFUSE, never TRUNCATE; the three-way 200 / 200-empty / 4xx distinction is preserved |
| At DELIVER | Do not invent a new event name, a new metric, a new dashboard, or a new alert threshold | The refusal envelope IS the signal (A3); Prism's `isPromError` already handles the existing envelope class |
| At DELIVER | Do not push a `limit` argument into the three store traits | ADR-0050 Decision 5: NO store-trait change; the duplication across the three handlers is deliberate; the cross-cutting `query-http-common` extraction is deferred (ADR-0048 Decision 5, M-5 in the residuality follow-up roadmap) |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**What DISTILL receives**: the mandatory `environments.yaml` for
Mandate 4 (the `clean` environment, in-process via the tower oneshot
pattern, no external services); the confirmation that no CI edit is
needed (A1, A2); the confirmation that `deny.toml` is unchanged (A4);
the per-crate mutation coverage map (each `gate-5-mutants-*-query-api`
job covers its own crate's modified `lib.rs` via `--in-diff`); the
constraint that `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS` are `pub`
informational items in three lib.rs files, alongside the unchanged
`router()` signatures and the unchanged store trait signatures
(`pulse::MetricStore`, `lumen::LogStore`, `ray::TraceStore`); and the
KPI to gate mapping above.

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt
of this wave's outputs.
