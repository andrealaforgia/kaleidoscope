# Wave Decisions - log-query-severity-filter-v0 / DEVOPS

British English. No em dashes.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-27
- **Mode**: slim DEVOPS. This wave confirms that the existing CI
  contract already covers the modified `crates/log-query-api/src/lib.rs`
  via the existing `gate-5-mutants-log-query-api` job, and that no new
  infrastructure is warranted by a parse + wire growth of one optional
  query-string parameter. The decision to run slim, and its shape, are
  Apex's own judgement from the DESIGN handoff, not pre-taken.

## Why this wave is slim

The feature grows the lumen log-query-api by ONE optional query-string
parameter on `GET /api/v1/logs`: `min_severity`, mapped case-insensitively
to one of the six OTel severity names and used as a `>=` floor against
the existing `lumen::Predicate::min_severity` builder through the
existing `lumen::LogStore::query_with` seam. The deliverable in source
is a thin parse + wire growth inside a single file:

- ONE additive field on the private `LogsParams` struct
  (`min_severity: Option<String>`).
- ONE new free function `parse_min_severity(&str) ->
  Result<SeverityNumber, String>` next to the existing
  `parse_time_range_seconds` and `parse_epoch_seconds`.
- ONE new parse step in `handle_logs` after the window-cap check and
  BEFORE the existing store call.
- ONE branched dispatch on `Option<SeverityNumber>`: `Some(floor)`
  routes to `state.store.query_with(&tenant, range,
  &Predicate::new().min_severity(floor))`; `None` falls through to the
  existing `state.store.query(&tenant, range)`.
- ONE new 400 arm reusing `error_response` with the named-class
  reason `"unknown severity"` (raw parameter value NEVER echoed).
- Inline unit tests next to the existing parse-helper tests.

There is NO new workspace crate, NO new external dependency (the parse
helper uses `str::eq_ignore_ascii_case` from `core` and maps to the
existing `SeverityNumber` constants in `crates/lumen/src/record.rs`),
NO new public event name (the existing `{status:"error",
error:"<reason>"}` envelope is reused with one new named reason),
NO new CI gate, NO new graduation tag (no new crate to tag; the
`LogsParams` field addition is private and does NOT appear in any
public-api diff; the `log-query-api` crate is not in Gate 2 / Gate 3's
locked set), and NO new external integration. The DESIGN "Handoff to
DEVOPS (Apex)" section
(`../design/wave-decisions.md` lines 328-341 and
`../design/application-architecture.md` lines 176-187) anticipated every
DEVOPS conclusion; the job of this wave is to VERIFY those conclusions
against the live CI workflow and `deny.toml`, record the verification,
and hand off, not to re-litigate the design.

This follows the `honest-read-caps-v0` slim-DEVOPS precedent. That
precedent produced two files (`environments.yaml`, `wave-decisions.md`);
this wave produces the same two, for the same reasons recorded there
under "Artefacts judged N/A".

## Inputs read (in dependency order)

1. `CLAUDE.md` - paradigm (Rust idiomatic) and the per-feature mutation
   testing strategy at 100% kill rate (declared; not modified here).
2. `../discuss/wave-decisions.md` and `../discuss/user-stories.md` -
   the four DISCUSS flags Morgan resolved at DESIGN, and the US-01
   through US-05 acceptance scenarios with six Gherkin scenes.
3. `../design/wave-decisions.md` - DESIGN flags pinned (`min_severity`,
   case-insensitive, filter BEFORE cap, ADR-0052) and the parse + wire
   micro-decisions D5-D9; explicit DEVOPS Handoff Annotation (no new
   crate, no new dependency, the existing
   `gate-5-mutants-log-query-api` covers the modified file via
   `--in-diff` at the 100% kill-rate gate).
4. `../design/application-architecture.md` - C4 L2 request flow, the
   "Changes Per File" table (the exact loci inside
   `crates/log-query-api/src/lib.rs`), and the ISO 25010 quality
   attribute coverage table.
5. `docs/product/architecture/adr-0052-log-query-severity-filter.md` -
   the contract growth ADR (Accepted, 2026-05-27); ten decisions
   covering parameter name, accepted values, `>=` semantics, filter
   BEFORE cap, handler order, envelope reuse, parse-helper location,
   wiring, lumen trait unchanged, no new event / metric / dashboard.
6. `docs/feature/honest-read-caps-v0/devops/{environments.yaml,wave-decisions.md}`
   - the immediate slim-DEVOPS sibling shape this wave mirrors.
7. `.github/workflows/ci.yml` - the existing five-gate workflow, read
   to CONFIRM (not modify) the `gate-5-mutants-log-query-api` job's
   existence and `--in-diff` scope (see "Verification against ci.yml"
   below).
8. `deny.toml` - read to CONFIRM no licence / ban / advisory policy
   change is needed (no new dependency).

## Pre-wave decisions (carried in from project convention, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| P1 | `deployment_target` | None new (log-query-api stays library + thin main.rs; no new binary, no new container) | DESIGN handoff + ADR-0052 |
| P2 | `container_orchestration` | N/A (slice 01 produces no container image; the pre-existing kaleidoscope-cli Dockerfile is untouched) | environments.yaml |
| P3 | `cicd_platform` | GitHub Actions (existing, unchanged) | ADR-0005 |
| P4 | `existing_infrastructure` | Yes (workspace + five-gate CI; `gate-5-mutants-log-query-api` already present at ci.yml line 1123) | ci.yml |
| P5 | `git_branching_strategy` | Trunk-based, pure (main has no required-status-checks and no enforce_admins; CI is feedback, not a gate) | memory `project_kaleidoscope_pure_trunk_based` |
| P6 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md, ADR-0005 Gate 5 |

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] No new CI gate; ADR-0005's five gates inherited unchanged

The change touches ONE file in ONE crate
(`crates/log-query-api/src/lib.rs`), adds zero new modules under
`crates/log-query-api/src/`, and adds ONE new test file under
`crates/log-query-api/tests/`. Each gate is satisfied by existing
machinery:

- **Gate 1 (`cargo test --workspace`)**: runs the new acceptance
  suite `crates/log-query-api/tests/slice_01_severity_filter.rs` with
  zero workflow edit. This is the KPI collection surface for every
  behavioural KPI (walking skeleton, default-unchanged backward
  compatibility, boundary inclusive / exclusive on the `>=` semantics,
  case-insensitivity per-name, unknown-severity 400 with redaction,
  no-store-call on the unknown path, filter-BEFORE-cap interaction).
- **Gate 2 (`cargo public-api`)** and **Gate 3 (`cargo semver-checks`)**:
  scope to `otlp-conformance-harness` / `spark` / `sieve` / `codex`
  only; `log-query-api` is not in the locked set (verified below).
  No diff applies. The `lumen::LogStore` trait signatures are
  byte-identical to the prior tag regardless (ADR-0052 Decision 9:
  NO trait change); the `LogsParams` field addition is `pub(crate)`
  and does NOT appear in any public-api diff.
- **Gate 4 (`cargo deny`)**: no new external dependency, so no scan
  change. VERIFIED in `deny.toml`: no edit required (A4 below).
- **Gate 5 (`cargo mutants`)**: covered by the existing
  `gate-5-mutants-log-query-api` job for the modified `lib.rs`; no
  new job (A2).

No new or amended gate is warranted. No new CI workflow file is
created; no existing gate is added to, removed, or modified by this
feature.

### [A2] Mutation testing: the existing `gate-5-mutants-log-query-api` job covers the modified `lib.rs`; no workflow edit

**Options considered**:

1. **Rely on the existing `gate-5-mutants-log-query-api` job**, which
   runs `cargo mutants --package log-query-api --in-diff "$DIFF_FILE"`
   against the diff filtered to `crates/log-query-api/**`.
2. Add a new file-scoped job pinned to
   `crates/log-query-api/src/lib.rs`.
3. Add a new combined job covering `parse_min_severity` and the new
   dispatch branch as a named target.

**Decision**: Option 1.

**Rationale**: The job already exists in `ci.yml` at line 1123
(`gate-5-mutants-log-query-api: name: Gate 5 - cargo mutants
(log-query-api)`), with the `origin/main -> HEAD~1 -> full` baseline
cascade (lines 1173-1185), the empty-diff short-circuit (lines
1182-1185), the diff filter `git diff "$BASELINE" HEAD --
'crates/log-query-api/**'` (line 1181), and the invocation
`cargo mutants --package log-query-api --in-diff "$DIFF_FILE"
--no-shuffle --jobs 2` (lines 1189-1193). Because this feature
modifies `crates/log-query-api/src/lib.rs` exclusively (the new test
file is under `crates/log-query-api/tests/`, also covered by the
`crates/log-query-api/**` filter), the diff filter naturally picks
up the modified `lib.rs` of the crate. The job runs at the 100%
kill-rate gate (CLAUDE.md, ADR-0005 Gate 5). Option 2 would duplicate
the existing job's behaviour for no benefit. Option 3 would conflate
the existing per-crate mutation budget into a feature-named job for no
benefit, REJECTED on proportionality grounds: the per-crate scope is
already correct.

**Mutation scope (per DESIGN ADR-0052 Verification and
`../design/wave-decisions.md` "Primary mutation targets" table)**: the
modified `lib.rs`. Primary mutation targets:

- The `>=` boundary on `Predicate::min_severity` as inherited at the
  HTTP boundary (`>=` -> `>` killed by the boundary-inclusive scenario
  at exactly the floor; `>=` -> `<` killed by the walking-skeleton
  WARN-includes-ERROR scenario).
- The six-name mapping table in `parse_min_severity` (drop or rename
  any of TRACE, DEBUG, INFO, WARN, ERROR, FATAL killed by the per-name
  acceptance assertions; the suite references each by literal).
- The case-insensitivity (`eq_ignore_ascii_case` -> `eq` killed by the
  WARN / warn / Warn / wArN per-case-form assertions).
- The redaction on the unknown-severity 400 (echo-raw-value mutant
  killed by the substring assertion that the body does NOT contain
  the literal "WARNING").
- The order of checks (parse-after-store mutant killed by the
  no-store-call assertion via a counting test double whose `query`
  and `query_with` counters are both zero on the unknown-severity
  path).
- The dispatch branch (`Some` -> `query` collapse killed by the
  walking-skeleton scenario; `None` -> `query_with` collapse killed
  by the default-unchanged scenario, since the parameter-less path
  must behave byte-identically to the slice-prior shape).
- The filter-BEFORE-cap ordering (cap-then-filter mutant killed by
  the 150_000 INFO + 50_000 ERROR scenario reusing the `BulkLogStore`
  pattern from `crates/log-query-api/tests/slice_02_caps.rs:86`).

### [A3] No new public event name; no new dashboard

Refusal rides on the existing `{status:"error", error:"<reason>"}`
envelope (ADR-0047 Decision 1, ADR-0050 Decision 7, ADR-0052
Decision 6). The new reason string `"unknown severity"` is inside the
existing envelope shape; Prism's `isPromError` already handles it as
the same error class as the matcher-400, bounds-400, window-cap-400,
and result-cap-400 errors. There is no new event vocabulary, no metric
counter, no dashboard, no alert threshold. At v0/v1 the platform has
no live observability stack of its own; the 400 IS the signal
(ADR-0052 Decision 10). Recorded so DELIVER does not invent an alert
routing story.

### [A4] No new external dependency; Gate 4 unaffected (`deny.toml` unchanged)

The parse helper uses `str::eq_ignore_ascii_case` from `core` and
maps to the six existing `SeverityNumber` constants in
`crates/lumen/src/record.rs:32-39`. The dispatch branch uses the
existing `lumen::LogStore::query_with` trait method at
`crates/lumen/src/store.rs:89` and the existing
`lumen::Predicate::new().min_severity(floor)` builder at
`crates/lumen/src/predicate.rs:33,46`. No `serde` feature flag is
added (the existing `#[derive(Deserialize)]` on `LogsParams` covers
the new `Option<String>` field). No new third-party crate. VERIFIED by
reading `deny.toml`:

- The `[graph].targets` (x86_64-linux-gnu, aarch64-linux-gnu,
  x86_64-darwin, aarch64-darwin) already cover the supported
  platforms.
- The `[licenses].allow` list is unaffected (no new transitive crate
  is added).
- The `[bans]` list is unaffected (no new dependency, so no new
  duplicate-version concern; the `multiple-versions = "allow"`
  relaxation is unchanged; the `deny = [{ name = "openssl", ... }]`
  clause is unaffected).
- The `[advisories]` and `[sources]` policies are unaffected
  (`yanked = "deny"`, `unknown-registry = "deny"`,
  `unknown-git = "deny"` all hold).

**Verdict**: zero change to `deny.toml`. The Gate 4 (`cargo deny`)
run on the slice's commit will pass with the same policy that passed
at `honest-read-caps-v0` close.

### [A5] No new graduation tag; no per-crate release

There is no new crate, so there is no new per-crate tag at
graduation. The change lands as a `git commit` on `main` (pure
trunk-based per P5) under the existing `log-query-api` crate
manifest. No `log-query-api-vX.Y.Z` tag is created by this slice
(`log-query-api` is not in Gate 2 / Gate 3's locked set; it is not a
graduated crate). The `LogsParams` field addition is private (the
struct is `pub(crate)` at `crates/log-query-api/src/lib.rs:107`) and
does NOT appear in any public-api diff; the `router()` signature is
byte-identical to the prior tag. Recorded so DELIVER does not invent
a release story.

### [A6] No observability / monitoring / alerting instrumentation beyond the refusal envelope

The 400 with the existing `{status:"error", error:"<reason>"}`
envelope is the entire observability surface for slice 01. There is
no new payload-reduction histogram (KPI-1's 5x target is a DELIVER
calibration measurement, NOT a runtime metric), no narrowed-read
adoption counter, no post-filter record-count gauge. For a
no-new-deployment parse + wire growth, the CI gates ARE the alerting
surface: a regression fails Gate 1 (test) on the new acceptance
suite, or Gate 5 (`gate-5-mutants-log-query-api`) on the modified
`lib.rs` at the next push. ADR-0052 Decision 10 records the same
posture.

### [A7] No deployment / rollback procedure beyond git revert

There is no new deployment artefact, so there is nothing to roll back
at the deployment layer. The `log-query-api` crate remains library +
thin `main.rs`; the `router()` signature is byte-identical to the
prior tag (ADR-0052 Decision 9); consumers
(`kaleidoscope-gateway` and any future composition root) compile
without diff. The project is pure trunk-based with no merge gate
(memory `project_kaleidoscope_pure_trunk_based`); the recovery is
fix-forward on `main`. This satisfies the rollback-first principle
vacuously for the deployment layer: the only "rollback" available
and needed is a git revert of the slice commit, and because the
parameter is OPTIONAL and the parameter-less path is byte-equal to
the slice-prior response shape (ADR-0052 Verification, KPI-2), a
revert has no data consequence (any in-flight client that received
the new `"unknown severity"` 400 on the reverted branch sees only
a "named class disappeared" on the next request; the within-cap and
existing-error-class paths are untouched by the slice and untouched
by the revert).

## Verification against ci.yml (CONFIRM, not modify)

Read of `.github/workflows/ci.yml` in this wave confirmed:

| Claim | Verified location | Result |
|-------|-------------------|--------|
| Gate 2 (`cargo public-api`) scopes to harness / spark / sieve / codex; log-query-api excluded | lines 326-347 (`-p otlp-conformance-harness`, `-p spark`, `-p sieve`, `-p codex`) | CONFIRMED, `log-query-api` is not present |
| Gate 3 (`cargo semver-checks`) scopes to the same four; log-query-api excluded | lines 420-433 (`--package` for the same four) | CONFIRMED, `log-query-api` is not present |
| `gate-5-mutants-log-query-api` job exists | line 1123: `gate-5-mutants-log-query-api: name: Gate 5 - cargo mutants (log-query-api)` | CONFIRMED present |
| The job runs on `ubuntu-latest` with `needs: [gate-2-public-api, gate-3-semver]` and a 30-minute timeout | lines 1125-1129 | CONFIRMED |
| The job uses the `origin/main -> HEAD~1 -> full` baseline cascade with empty-diff short-circuit | lines 1173-1185: `if git rev-parse --verify origin/main ...; elif git rev-parse --verify HEAD~1 ...; ... if [ ! -s "$DIFF_FILE" ]; then echo "No log-query-api-touching changes vs $BASELINE; skipping mutation testing."; exit 0; fi` | CONFIRMED |
| The diff filter is `crates/log-query-api/**` | line 1181: `git diff "$BASELINE" HEAD -- 'crates/log-query-api/**' > "$DIFF_FILE"` | CONFIRMED |
| The invocation is `cargo mutants --package log-query-api --in-diff "$DIFF_FILE" --no-shuffle --jobs 2` | lines 1189-1193 | CONFIRMED |

The crate path `crates/log-query-api/**` includes
`crates/log-query-api/src/lib.rs` (the only `src/` file modified by
this slice) and `crates/log-query-api/tests/slice_01_severity_filter.rs`
(the new acceptance file). The `--in-diff` filter naturally picks up
the modified `lib.rs`. No new mutation job is needed; the existing job
covers the modified file via `--in-diff` at the 100% kill rate.

No workflow file was modified by this wave. No gate was added,
removed, or amended.

## Verification against deny.toml (CONFIRM, not modify)

Read of `deny.toml` in this wave confirmed that no policy change is
needed:

- `[graph].targets`: the four supported triples (linux x86_64,
  linux aarch64, darwin x86_64, darwin aarch64) are unchanged.
- `[licenses].allow`: zero new transitive licence (no new dependency
  introduced; `core` is unlicenced for purposes of `cargo deny`).
- `[licenses.private] ignore = true`: unchanged.
- `[bans]`: zero new dependency, so the
  `multiple-versions = "allow"` relaxation and the
  `deny = [{ name = "openssl", reason = "use rustls" }]` clause are
  unaffected; `wildcards = "deny"` is unaffected.
- `[advisories]`: `db-urls`, `yanked = "deny"` policy is unaffected.
- `[sources]`: `unknown-registry = "deny"`,
  `unknown-git = "deny"`, `allow-registry` policies are unaffected.

**Verdict**: zero change to `deny.toml`. The Gate 4 (`cargo deny`)
run on the slice's commit will pass with the same policy that passed
at `honest-read-caps-v0` close.

## KPI to gate mapping

All outcome KPIs (`../discuss/outcome-kpis.md`) are correctness and
payload-shape indicators collected by green acceptance tests under
**Gate 1** (`cargo test --workspace`) running the new acceptance
suite, with Gate 5 (`gate-5-mutants-log-query-api`) guarding the
test-suite strength behind the parse helper, the dispatch branch,
the `>=` boundary inheritance, and the redaction substring assertions
on the modified `lib.rs`. The trait-signature KPI is additionally
collected by the compile of existing consumers under Gate 1.

| KPI (from outcome-kpis.md) | Target | Gate | Collection |
|----------------------------|--------|------|------------|
| North star: WARN-or-worse job served at the HTTP boundary | walking-skeleton scenario passes (mixed fixture, only WARN+ERROR returned) | Gate 1 | walking-skeleton in `slice_01_severity_filter.rs` |
| KPI-1: payload reduction (>= 5x on the INFO-heavy fixture) | 5x on a representative fixture | Gate 1 | the walking-skeleton fixture is sized for the multiplier; the precise multiplier is a DELIVER calibration, not a contract pin |
| KPI-2: backward compatibility (parameter-less byte-equal) | 0 byte-level diffs on the parameter-less response shape for the same inputs | Gate 1 | default-unchanged scenario in the new suite; plus the existing `tests/slice_01_logs_read.rs` and `tests/slice_02_caps.rs` stay green unchanged (DISCUSS Decision 8) |
| KPI-3: redacted 400 on unknown severity | 400 with reason `"unknown severity"`; body does NOT contain the raw parameter value | Gate 1 | unknown-severity scenario with substring assertion |
| `>=` boundary inclusive at the floor | record at exactly the floor INCLUDED | Gate 1 | boundary-inclusive scenario |
| `<` boundary exclusive just below the floor | record one notch below the floor EXCLUDED | Gate 1 | boundary-exclusive scenario |
| Case-insensitivity (WARN / warn map to same `SeverityNumber`) | per-case-form assertions pass | Gate 1 | case-form per-name assertions plus inline unit tests next to `parse_time_range_seconds` |
| Per-name mapping (TRACE / DEBUG / INFO / WARN / ERROR / FATAL accepted) | each accepted in at least one canonical case | Gate 1 | per-name acceptance assertions |
| Order of checks (severity parse BEFORE store; unknown-severity 400 NEVER touches the store) | counting test double's `query` and `query_with` counters both zero on the unknown path | Gate 1 | no-store-call assertion |
| Filter BEFORE cap (cap measures post-filter `Vec::len()`) | 150_000 INFO + 50_000 ERROR with `min_severity=ERROR` returns 200 with 50_000 ERROR records (NOT a cap-400) | Gate 1 | filter-BEFORE-cap scenario reusing the `BulkLogStore` pattern from `slice_02_caps.rs:86` |
| `lumen::LogStore` trait signatures unchanged | 0 signature changes | Gate 1 | compile of `kaleidoscope-gateway` / `aperture-storage-sink` / `self-observe` under Gate 1 |
| `>=` boundary on `Predicate::min_severity` not deletable | 0 surviving mutants on the boundary | Gate 5 | `gate-5-mutants-log-query-api` --in-diff over the modified `lib.rs` |
| Six-name mapping table not deletable | 0 surviving mutants on the six names | Gate 5 | same job; killed by the per-name acceptance assertions |
| Case-insensitivity not weakenable | 0 surviving mutants on `eq_ignore_ascii_case` | Gate 5 | same job; killed by the case-form assertions |
| Redaction on `"unknown severity"` reason not echoable | 0 surviving mutants on the reason text | Gate 5 | same job; killed by the substring assertion that the body does NOT contain `"WARNING"` |
| Order of checks (severity parse BEFORE store) not swappable | 0 surviving mutants on the order | Gate 5 | same job; killed by the no-store-call assertion |
| Dispatch branch (`Some` -> `query_with`, `None` -> `query`) not collapsible | 0 surviving mutants on the branch | Gate 5 | same job; killed by the walking-skeleton and default-unchanged scenarios |

## Infrastructure summary

- **Deployment**: none new (`log-query-api` stays library + thin
  `main.rs`; no new binary, no new container).
- **CI/CD**: GitHub Actions, ADR-0005 five gates, inherited
  unchanged. `gate-5-mutants-log-query-api` already present at
  ci.yml line 1123; covers the modified `lib.rs` via `--in-diff`
  over `crates/log-query-api/**`. No new or amended job.
- **Branching**: pure trunk-based (project default, unchanged); main
  has no required-status-checks and no enforce_admins; CI is
  feedback, not a gate.
- **Mutation testing**: per-feature, 100% kill rate, scoped by
  `--in-diff` to `crates/log-query-api/src/lib.rs` (one file).
- **External integrations**: none. No contract tests apply.
- **External dependencies**: none new. `deny.toml` unchanged.
- **Observability**: no new instrumentation beyond the reused
  `{status:"error", error:"<reason>"}` envelope with one new named
  reason `"unknown severity"`; CI gates are the alerting surface.
- **Public surface (`log-query-api`)**: zero new public items. The
  `LogsParams` field addition is `pub(crate)` and does NOT appear in
  any public-api diff; the `router()` signature and all store trait
  signatures (`lumen::LogStore`) are byte-identical to the prior
  tag.
- **Graduation tag**: none (no new crate; `log-query-api` is not in
  Gate 2 / Gate 3's locked set).
- **Docker**: out of scope for slice 01; the pre-existing
  `kaleidoscope-cli` Dockerfile is untouched.

## Artefacts produced by this wave

| Artefact | Path |
|----------|------|
| Environment inventory (clean target environment, in-process tower oneshot, no external services) | `docs/feature/log-query-severity-filter-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/log-query-severity-filter-v0/devops/wave-decisions.md` |

## Artefacts judged N/A (with reason)

| Skipped artefact | Reason |
|------------------|--------|
| `kpi-instrumentation.md` | Every KPI maps to Gate 1 on the new acceptance suite (plus Gate 5 for suite strength on the modified `lib.rs`); no instrumentation to design. KPI-1's 5x multiplier is a DELIVER calibration measurement, NOT a runtime metric (A6). A separate file would only restate the KPI to gate mapping table above. |
| `ci-cd-pipeline.md` | This feature adds no job and edits no workflow; the existing `gate-5-mutants-log-query-api` job covers the modified `lib.rs` as-is. The "Verification against ci.yml" section above is the entire pipeline content for this feature; a separate addendum would be empty. |
| `platform-architecture.md` | No platform infrastructure to architect (no cloud, no orchestration, no service mesh). Morgan's `../design/application-architecture.md` is sufficient. |
| `observability-design.md` / `monitoring-alerting.md` | No runtime monitoring beyond the reused refusal envelope (A3, A6); CI gates are the alerting surface. |
| `infrastructure-integration.md` | No external integrations at runtime (DESIGN: external integrations = none; the parse helper is in-process string matching; the store call uses an in-process trait method against the durable `FileBackedLogStore`, a first-party library). |
| `branching-strategy.md` | Pure trunk-based is the project default; no per-feature deviation (P5). |
| `deployment-strategy.md` / `rollback.md` | No new deployment artefact; recovery is git revert with no data-format consequence (A7). |
| `docker.md` / `containers.md` | Out of scope for slice 01 (environments.yaml); the pre-existing `kaleidoscope-cli` Dockerfile is untouched. |

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Write the new acceptance suite at `crates/log-query-api/tests/slice_01_severity_filter.rs`; it must exercise the walking skeleton (mixed INFO/WARN/ERROR fixture, `min_severity=WARN` returns only WARN+ERROR), default-unchanged (parameter-less byte-equal), boundary-inclusive at the floor, boundary-exclusive just below, case-insensitivity (WARN / warn / Warn / wArN), per-name (TRACE / DEBUG / INFO / WARN / ERROR / FATAL each in at least one case), unknown-severity 400 with redaction, no-store-call assertion via a counting test double, and filter-BEFORE-cap via the `BulkLogStore` pattern reused (NOT edited) from `crates/log-query-api/tests/slice_02_caps.rs:86` | The `clean` environment with the in-process tower oneshot pattern is the only environment to parametrise over (environments.yaml); the parameter parsing, the dispatch branch, and the cap interaction are the contract |
| At DISTILL | DO NOT edit `.github/workflows/ci.yml` | No new gate; Gate 1 auto-discovers the new test file, and the existing `gate-5-mutants-log-query-api` job already covers mutation on the modified `lib.rs` via `--in-diff` (A1, A2) |
| At DISTILL | DO NOT add `log-query-api` to Gate 2 or Gate 3 | It is not a graduated crate; the locked set scopes to harness / spark / sieve / codex (A1) |
| At DISTILL | DO NOT propose a new combined `gate-5-mutants-severity-filter` job | The existing `gate-5-mutants-log-query-api` job covers the modified `lib.rs` via `--in-diff`; a feature-named job would duplicate the existing per-crate budget for no benefit (A2) |
| At DISTILL | DO NOT edit `crates/log-query-api/tests/slice_01_logs_read.rs` or `crates/log-query-api/tests/slice_02_caps.rs` | DISCUSS Decision 8: the existing acceptance suites stay green unchanged; the `BulkLogStore` pattern at `slice_02_caps.rs:86` is REUSED (not edited) by referencing it from the new file |
| At DELIVER | Add the `min_severity: Option<String>` field to the existing `LogsParams` struct at `crates/log-query-api/src/lib.rs:107`; the field is private (the struct is `pub(crate)`) | DESIGN `application-architecture.md` "Changes Per File"; the field is private so it does NOT appear in any public-api diff (A1, A5) |
| At DELIVER | Add the free function `fn parse_min_severity(raw: &str) -> Result<SeverityNumber, String>` next to `parse_time_range_seconds` and `parse_epoch_seconds`; not `pub`; uses `str::eq_ignore_ascii_case`; maps to the six existing `SeverityNumber` constants; returns `Err("unknown severity".to_string())` for any other value (including empty string and `"UNSPECIFIED"`) | DESIGN `wave-decisions.md` D6 and D7; ADR-0052 Decision 7 |
| At DELIVER | Place the new parse step in `handle_logs` AFTER the window-cap check and BEFORE the existing `state.store.query(...)` call; the unknown-severity 400 returns via `error_response(StatusCode::BAD_REQUEST, "unknown severity")`; the store is NEVER called on this path | DESIGN `wave-decisions.md` D5 and D8; ADR-0052 Decision 5; killed-by-no-store-call mutant target |
| At DELIVER | Branch the store dispatch on the parsed `Option<SeverityNumber>`: `Some(floor)` -> `state.store.query_with(&tenant, range, &Predicate::new().min_severity(floor))`; `None` -> existing `state.store.query(&tenant, range)` | DESIGN `wave-decisions.md` D5; ADR-0052 Decision 8 |
| At DELIVER | Leave the result-cap check at `crates/log-query-api/src/lib.rs:153` EXACTLY where it is; only the source of the `Vec<LogRecord>` it measures changes when the parameter is present (post-filter via `query_with`, unchanged via `query`) | DESIGN `wave-decisions.md` D5; ADR-0052 Decision 4; ADR-0050 Decision 4 preserved |
| At DELIVER | Reuse the existing `error_response` helper with the named-class reason string `"unknown severity"`; do not echo the raw `min_severity` parameter value in the cap-400 body | DESIGN `wave-decisions.md` D7; ADR-0052 Decision 6 (redaction posture); symmetric with the existing `the_bounds_error_never_echoes_the_raw_value` precedent |
| At DELIVER | Turn the modified `lib.rs` mutants 100% killed before close (the `>=` boundary, the six-name mapping, the case-insensitivity, the redaction substring, the order of checks, the dispatch branch, the filter-BEFORE-cap ordering) | CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5 (A2) |
| At DELIVER | Do not invent aliases (e.g. `WARNING` -> `WARN`, `err` -> `ERROR`, `critical` -> `FATAL`) | ADR-0052 Decision 2: case-insensitive, NO aliases; a typo is a typo, refused with a named 400 |
| At DELIVER | Do not push the parse helper or the six-name mapping into the `lumen` crate | ADR-0052 Decision 7: parse-helper location is `log-query-api`-local; `lumen` stays HTTP-shape-free; `query-http-common` extraction is deferred (ADR-0048 Decision 5, M-5) |
| At DELIVER | Do not invent a new event name, a new metric, a new dashboard, or a new alert threshold | The refusal envelope IS the signal (A3, A6); ADR-0052 Decision 10; Prism's `isPromError` already handles the existing envelope class |
| At DELIVER | Do not add `min_severity` to the `lumen::LogStore` trait | ADR-0052 Decision 9: the lumen trait signatures stay byte-identical; the predicate-carrying seam (`query_with`) already exists at `crates/lumen/src/store.rs:89` |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**What DISTILL receives**: the mandatory `environments.yaml` for
Mandate 4 (the `clean` environment, in-process via the tower
oneshot pattern, no external services); the confirmation that no CI
edit is needed (A1, A2); the confirmation that `deny.toml` is
unchanged (A4); the per-crate mutation coverage map (the existing
`gate-5-mutants-log-query-api` job covers the modified `lib.rs` via
`--in-diff` over `crates/log-query-api/**`); the constraint that
`min_severity` is a `pub(crate)` field addition with no public-api
diff and that the `lumen::LogStore` trait signatures and the
`log-query-api` `router()` signature are byte-identical to the prior
tag; the constraint that the existing `tests/slice_01_logs_read.rs`
and `tests/slice_02_caps.rs` MUST stay green unchanged (DISCUSS
Decision 8); and the KPI to gate mapping above.

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt
of this wave's outputs.
