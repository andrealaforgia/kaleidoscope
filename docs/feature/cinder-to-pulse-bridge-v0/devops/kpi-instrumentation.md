# KPI Instrumentation — `cinder-to-pulse-bridge-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-18
- **Source-of-truth for KPIs**: `docs/feature/cinder-to-pulse-bridge-v0/discuss/outcome-kpis.md`

## Why this document is short

The DISCUSS-wave outcome-kpis.md is explicit: this is a library-only
feature; the operator persona (Priya) cannot directly exercise the
bridge without the post-v0 CLI wiring feature. Therefore the "behaviour
change" KPIs land at the **library contract level**, measured through
acceptance tests, not through runtime observability dashboards or
production alerting.

There is no Grafana dashboard, no Prometheus alert, no log-stream
aggregation, no error-rate SLO, and no synthetic monitor for this
feature. The CI pipeline IS the measurement instrument. A failing
acceptance test IS the alert. The git-history-of-green is the
historical-measurement record.

This document maps each KPI from outcome-kpis.md to:

1. The collection method (which CI artefact emits the signal).
2. The data path (how the signal reaches a person who can act on it).
3. The alerting rule (what condition triggers human attention).
4. The dashboard surface (what summary view answers "is this KPI
   green?").

For a library-only feature with no deployed runtime, items (3) and (4)
collapse to "CI run status" and "CI run history" respectively.

## Per-KPI design

### OK1 — `cinder.place.count` per place call

> 100% of `place` calls produce exactly one queryable point with the
> correct `tier` attribute; no drops, no duplicates.

| Field | Value |
|-------|-------|
| Type | Leading (library contract) |
| Baseline | 0% (NoopRecorder swallows everything) |
| Target | 100% (every `place` call → one queryable point) |
| Data source | `crates/self-observe/tests/cinder_to_pulse.rs` — Slice 01 tests |
| Collection method | `cargo test --workspace --all-targets --locked` exit code (Gate 1) |
| CI gate enforcing | Gate 1 — `cargo test` (line 182 of `.github/workflows/ci.yml`) |
| Collection frequency | At every commit affecting the workspace (push or PR) |
| Owner | self-observe crate maintainer (Andrea); enforced by CI |
| Data path | Test pass/fail → workflow run status → GitHub commit status check → PR merge gate / push-to-main alert |
| Alerting rule | Workflow run = "failure" on Gate 1 job → GitHub email/web notification to the commit author |
| Dashboard surface | GitHub Actions "All workflows" view, filter by branch `main`. Green/red per-commit history is the time-series. Mutation-testing artefact (`mutants-out-self-observe`) supplements: a surviving mutant on `record_place` would be a flag that OK1's measurement is weaker than claimed. |
| Acceptance-test scenarios (from BDD) | "Place produces one point per call, partitioned per tenant" — Slice 01 acceptance scenarios |
| What instrumentation looks like in the test | `pulse.query(&tenant("acme"), &MetricName::new("cinder.place.count"), TimeRange::all())` returns exactly one `MetricPoint` with `attributes == {"tier": "hot"}` (or "warm"/"cold") after one `cinder.place(...)` call |
| Forward-compatible note for post-v0 CLI feature | The acceptance-test target IS Pulse's `query` API. When the CLI follow-up ships, the same `MetricStore::query` is what the CLI's `--cinder place` flag invokes; the bridge's KPI and the CLI's KPI share one data source. |

### OK2 — `cinder.migrate.count` per successful migrate call

> 100% of successful `migrate` calls produce exactly one point with
> correct `from`/`to` attributes; 0% of failed (`UnknownItem`) calls
> produce any point.

| Field | Value |
|-------|-------|
| Type | Leading (library contract) |
| Baseline | 0% (NoopRecorder) |
| Target | 100% on success path + 0% on failure path (the negative-case probe) |
| Data source | `crates/self-observe/tests/cinder_to_pulse.rs` — Slice 02 tests |
| Collection method | Same as OK1 (Gate 1, `cargo test`) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | self-observe crate maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting rule | Same as OK1 |
| Dashboard surface | Same as OK1 |
| Acceptance-test scenarios | "Successful migrate emits one point with from+to attributes" + "Failed migrate (UnknownItem) emits zero points" — Slice 02 acceptance scenarios |
| What instrumentation looks like in the test | After `cinder.migrate(&tenant, &item, Tier::Hot, Tier::Warm, t_now).unwrap()`, query returns one point with `attributes == {"from": "hot", "to": "warm"}`. After `cinder.migrate(&tenant, &unknown_item, ...)` returning `Err(UnknownItem)`, query returns zero points. The negative case is the harder probe — it tests that the bridge is NOT called by Cinder on the failure path (a contract inherited from `crates/cinder/src/store.rs:174-188`). |

### OK3 — `cinder.evaluate.migrated.count` per evaluate_at with migrations

> 100% of (tenant, evaluate) pairs with N>=1 migrations produce
> exactly one point with `value=N`; 0% of (tenant, evaluate) pairs
> with 0 migrations produce a point.

| Field | Value |
|-------|-------|
| Type | Leading (library contract) — dual-emission probe |
| Baseline | 0% (NoopRecorder) |
| Target | Conditional emission: point IFF N>=1; if emitted, `value = N` |
| Data source | `crates/self-observe/tests/cinder_to_pulse.rs` — Slice 03 tests |
| Collection method | Same as OK1 (Gate 1, `cargo test`) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | self-observe crate maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting rule | Same as OK1 |
| Dashboard surface | Same as OK1 — with the explicit note that Slice 03's dual-emission test is the HIGHEST-INFORMATION-DENSITY probe in the suite (one `evaluate_at` call asserts both `cinder.migrate.count` and `cinder.evaluate.migrated.count` streams). A Slice 03 failure can indicate a regression in EITHER the bridge OR Cinder's `InMemoryTieringStore::evaluate_at` cascade — diagnose from Cinder's own in-tree tests (they will go red first on a Cinder-side regression). |
| Acceptance-test scenarios | "evaluate_at with N>=1 migrations emits one cinder.evaluate.migrated.count point with value=N AND N cinder.migrate.count points" + "evaluate_at with 0 migrations emits zero points" — Slice 03 acceptance scenarios |
| What instrumentation looks like in the test | After `cinder.evaluate_at(&t_now, &policy_promoting_3_items)`, query `cinder.evaluate.migrated.count` returns one point with `value == 3.0` AND query `cinder.migrate.count` returns three points. The conjunction is the dual-emission contract from DISCUSS D3. |

### OK4 — Cinder API behaviour unchanged (guardrail)

> Zero observable change in Cinder's user-facing API behaviour when
> CinderToPulseRecorder is substituted for NoopRecorder.

| Field | Value |
|-------|-------|
| Type | Guardrail |
| Baseline | n/a (baseline = current NoopRecorder behaviour) |
| Target | Zero behavioural change at Cinder's API boundary |
| Data source | The acceptance tests themselves: every test calls Cinder identically to its NoopRecorder usage and asserts on Cinder's return values (`Result<...>` types) as well as Pulse's stored points |
| Collection method | (a) Gate 1 (`cargo test`) — failing assertions on Cinder return values catch behavioural drift; (b) code review at DESIGN handoff + DELIVER review |
| CI gate enforcing | Gate 1 (behavioural assertions on Cinder return values) |
| Collection frequency | Every commit (Gate 1); every wave-close review (code review) |
| Owner | self-observe crate maintainer (Gate 1); Reviewer (code review) |
| Data path | Test pass/fail → workflow status (same as OK1). Code review path: nWave wave-close review surfaces any test that calls Cinder DIFFERENTLY from its NoopRecorder usage, fails the review. |
| Alerting rule | (a) Gate 1 failure on a Cinder-return-value assertion → standard CI alert; (b) reviewer flagging a non-mirror test pattern → review-block. |
| Dashboard surface | Same Gate 1 surface as OK1/OK2/OK3. Code-review record persists in the PR thread / wave-decisions.md "Hand-off" section. |
| Acceptance-test pattern | Every test in `tests/cinder_to_pulse.rs` follows the shape `let cinder = InMemoryTieringStore::new(Box::new(bridge)); cinder.<method>(...)` — i.e., Cinder is constructed exactly as it would be with NoopRecorder, only the recorder swap is different. Assertions on Cinder's return values precede assertions on Pulse's stored points. |
| What instrumentation looks like in the test | `assert_eq!(cinder.migrate(&tenant, &unknown_item, ...), Err(MigrateError::UnknownItem))` followed by `assert_eq!(points.len(), 0)`. The first assertion defends OK4; the second defends OK2 (negative case). The two assertions share one test body to lock the simultaneity of the two contracts. |

## Cross-KPI considerations

### Why mutation testing is part of OK1/OK2/OK3's measurement, not separate

A green Gate 1 with poor test quality is a false positive on OK1/OK2/
OK3. Gate 5 (mutation testing, per-feature, 100% kill rate per
ADR-0005 + CLAUDE.md) is the test-quality probe: a surviving mutant
on `cinder_bridge.rs` means the test suite cannot distinguish the
unmutated bridge from a behaviourally-different bridge. That is a
gap in the KPI measurement itself, not a separate quality concern.

Treatment: surviving mutants in `cinder_bridge.rs` fail Gate 5
(`gate-5-mutants-self-observe`, added in this DEVOPS wave per A3).
The DELIVER wave's responsibility (per CLAUDE.md per-feature MT
strategy) is to turn each slice's mutants 100% killed before review
approval.

### Why no separate "test-coverage threshold" KPI

OK1/OK2/OK3 are 100% binary (every event type produces its
documented point), not statistical. A line-coverage percentage would
be a weaker signal than the per-event contract assertions. The
mutation kill rate (Gate 5) is the supplemental quality signal;
line coverage is not part of this feature's KPI set.

### Why no separate "performance / latency" KPI

The library-side performance is dominated by one BTreeMap allocation
and one Mutex acquisition per event (per `application-
architecture.md > Quality attributes — Performance Efficiency`).
This is operationally negligible relative to the operator-visible
signal-shape time scale. A latency KPI would be a vanity metric at
v0; outcome-kpis.md explicitly defers operator-time-to-answer to
the post-v0 CLI feature (OK1-CLI, OK2-CLI). No v0 instrumentation
needed.

### Why no separate "deployment frequency" / DORA metric KPI

The bridge has no deployment. DORA metrics (per `platform-
engineering-foundations` skill) apply to deployed services. For a
library, the closest analog is "merge frequency to main", which is
trivially measured by `git log` and does not warrant a dashboard.

## Post-v0 instrumentation (deferred)

When `kaleidoscope-cli-wires-cinder-bridge-v0` (or its successor
name) ships, two new KPIs become measurable:

- **OK1-CLI**: time-to-first-answer for "how many Hot→Warm
  migrations did tenant `acme` see in the last hour?" via the
  operator CLI. Target: <30 seconds. Baseline: N/A (today the
  question is unanswerable without source modification).
- **OK2-CLI**: number of operators reporting "I had to add
  `println!` to Cinder to debug tier movements" in the 90 days
  following CLI ship. Target: 0. Baseline: ad hoc.

Both KPIs require operator-facing instrumentation that does not
exist at the library layer. This feature's KPI design does NOT
include them; the future CLI feature's DEVOPS wave will design the
collection method (likely a CLI invocation timer + a user research
mailout, respectively).

## Summary table — KPI to CI gate mapping

| KPI | What it measures | CI gate enforcing | Test file(s) |
|-----|------------------|-------------------|--------------|
| OK1 | `cinder.place.count` library contract | Gate 1 (cargo test) | `tests/cinder_to_pulse.rs` — Slice 01 block |
| OK2 | `cinder.migrate.count` library contract (success + failure paths) | Gate 1 | `tests/cinder_to_pulse.rs` — Slice 02 block |
| OK3 | `cinder.evaluate.migrated.count` + dual-emission contract | Gate 1 | `tests/cinder_to_pulse.rs` — Slice 03 block |
| OK4 | Cinder API behaviour unchanged (guardrail) | Gate 1 (assertions on Cinder return values) + code review at DESIGN/DELIVER handoff | `tests/cinder_to_pulse.rs` — every test body |
| (supplementary) | Test-suite quality / mutation kill rate | Gate 5 (`gate-5-mutants-self-observe`, added this wave per A3) | Mutations of `crates/self-observe/src/cinder_bridge.rs` |

Every KPI from outcome-kpis.md has a CI gate. Every CI gate has a
single named owner (Gate 1: workspace maintainer; Gate 5: same).
Every alert path is the existing GitHub Actions email/web
notification surface. Zero new tooling, zero new dashboards, zero
new on-call rotations.
