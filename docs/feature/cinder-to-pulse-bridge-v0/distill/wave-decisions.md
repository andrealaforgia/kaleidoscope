# Wave Decisions — `cinder-to-pulse-bridge-v0` / DISTILL

**Author**: `nw-acceptance-designer` (Quinn)
**Date**: 2026-05-18
**Mode**: execute (subagent — autonomous)

## Inputs read

DISCUSS (all peer-reviewed APPROVED):

- `docs/feature/cinder-to-pulse-bridge-v0/discuss/user-stories.md` — US-01 / US-02 / US-03
- `docs/feature/cinder-to-pulse-bridge-v0/discuss/journey-observe-cinder-tier-transitions.feature` — 11 canonical Gherkin scenarios + 1 `@property` (Send+Sync)
- `docs/feature/cinder-to-pulse-bridge-v0/discuss/outcome-kpis.md` — OK1 / OK2 / OK3 (leading, acceptance-test level) + OK4 (guardrail)
- `docs/feature/cinder-to-pulse-bridge-v0/discuss/wave-decisions.md` — D1-D7 (load-bearing choices)
- `docs/feature/cinder-to-pulse-bridge-v0/slices/slice-01..03.md` — slice-by-slice organisation

DESIGN (all peer-reviewed APPROVED):

- `docs/feature/cinder-to-pulse-bridge-v0/design/wave-decisions.md` — DD1 (test seam locked) / DD2 (file-flat) / DD3 (public surface locked) / DD4 (one ADR)
- `docs/product/architecture/adr-0038-cinder-to-pulse-bridge-public-api-and-crate-layout.md` — §1 public surface, §2 per-event emission contract, §3 test seam, §6 Cargo additions

DEVOPS:

- `docs/feature/cinder-to-pulse-bridge-v0/devops/environments.yaml` — `clean` only
- `docs/feature/cinder-to-pulse-bridge-v0/devops/kpi-instrumentation.md` — OK1/OK2/OK3 wired to Gate 1 tests in this exact file
- `docs/feature/cinder-to-pulse-bridge-v0/devops/ci-cd-pipeline.md` — Gate 1 auto-discovers the new `[[test]]` block

Worked precedent:

- `crates/self-observe/tests/lumen_to_pulse.rs` — the `LumenToPulseRecorder` acceptance suite. The Cinder suite mirrors its naming + helper shape (`tenant(id)`, `Arc<dyn MetricStore + Send + Sync>` construction, `Box::new(NoopRecorder)` for the upstream store, per-test wiring).
- `crates/self-observe/src/lumen_bridge.rs` — bridge precedent.
- `crates/cinder/src/{metrics,store,tier,policy}.rs` — the trait/types the bridge consumes.
- `crates/pulse/src/{store,metric}.rs` — the sink trait + value types.

## Pre-wave decisions carried in (not re-litigated)

| Decision | Value | Source |
|----------|-------|--------|
| WS Strategy | A — Full InMemory | Andrea's task brief |
| Walking Skeleton scenarios | NONE | DISCUSS D2 (NO WS — brownfield, isolated feature) |
| Container preference | NONE — in-process Rust integration tests (`cargo test`) | Andrea's task brief |
| Scenario tagging | `@in-memory` on every scenario (no `@walking_skeleton`) | Andrea's task brief |
| Test seam | `cinder::InMemoryTieringStore` drives, `pulse::InMemoryMetricStore` asserts | DESIGN DD1 + ADR-0038 §3 |
| Public surface under test | `self_observe::CinderToPulseRecorder` (constructor + three `record_*` trait methods) | DESIGN DD3 + ADR-0038 §1 |

## In-wave decisions

### DD-DISTILL-1: One `#[test]` fn per Gherkin scenario, organised slice-by-slice

The journey feature file ships 11 standalone scenarios + 1 `@property` (Send+Sync). Each becomes one `#[test]` fn in `crates/self-observe/tests/cinder_to_pulse.rs`. Slice separators (`// ----- Slice 01: place events -----`) group tests by user story.

Rationale: matches the granularity of the Gherkin source-of-truth. One scenario / one test makes traceability mechanical: a reviewer or future reader reading the feature file can find the corresponding test by name. Slice headers make the user-story grouping visible without re-reading the test names. Mirrors the precedent in `lumen_to_pulse.rs` (one `#[test]` per scenario, no `Scenario Outline`-style table-driven tests).

### DD-DISTILL-2: Rust integration tests, not pytest-bdd / cucumber-rs

The project is pure Rust (`CLAUDE.md`); the precedent suite `lumen_to_pulse.rs` uses plain `#[test]` fns. No BDD harness is in the workspace. Adding cucumber-rs for one new test file would create a workspace dependency divergence with zero offsetting readability gain — the Gherkin scenarios live in `discuss/journey-observe-cinder-tier-transitions.feature` as living documentation, and the Rust test names + section comments restate the scenario titles for in-test traceability.

Rationale: matches `nw-bdd-methodology` skill rule "step methods speak business language" — interpreted at the layer Rust offers (test names + comments), not by force-fitting a Python-style BDD harness.

### DD-DISTILL-3: Mandate 7 — RED scaffold for `CinderToPulseRecorder`

`crates/self-observe/src/cinder_bridge.rs` is created NOW (DISTILL wave) as a scaffold. The public surface is real per ADR-0038 §1 (struct, field, constructor signature, trait impl); every `record_*` method body panics with the SCAFFOLD marker.

Rationale: per the skill, acceptance tests must be RED (failing at runtime), not BROKEN (failing to compile or link). Without the scaffold, `tests/cinder_to_pulse.rs` would fail to compile because `self_observe::CinderToPulseRecorder` does not exist as an item. The scaffold lets the test file compile and run; failures are runtime panics with the SCAFFOLD marker, which DELIVER replaces method-by-method.

Scaffold marker text: `"... not yet implemented — RED scaffold (DISTILL); DELIVER replaces this body."` — appears in the panic message so a `cargo test` failure trace makes the wave provenance explicit.

The scaffold also stores the `Arc<dyn MetricStore + Send + Sync>` in a field named `_pulse` (leading underscore to silence `dead_code` warnings during scaffold time). DELIVER renames to `pulse` when the `emit` helper begins reading it.

### DD-DISTILL-4: Helper shape mirrors `lumen_to_pulse.rs`

Helpers in the test file:

- `tenant(id: &str) -> TenantId` — byte-equivalent to `lumen_to_pulse.rs:37-39`
- `item(id: &str) -> ItemId` — minimal Cinder-side analogue (no Lumen counterpart needed)
- `wire() -> (Arc<InMemoryMetricStore>, InMemoryTieringStore)` — Cinder-specific wiring helper. Replaces the inline three-line setup that `lumen_to_pulse.rs` repeats per test (the Cinder file's wiring is identical across all tests, so the helper is justified; the Lumen file's wiring is also identical but the Lumen suite chose not to extract — DISTILL chooses the extraction now because the Cinder file has 11 tests vs Lumen's 6, and the readability gain compounds).
- `place_count() / migrate_count() / evaluate_count() -> MetricName` — one-liner constructors for the three locked metric names from ADR-0038 §2. Centralising them in three named helpers means a metric-name typo on the wrong side of one assertion is impossible.

Rationale: extraction follows readability ROI. The `wire` helper saves ~25 lines across 11 tests with no information loss; the three metric-name helpers fix the metric names against the ADR-0038 §2 contract at the test-file layer (locked from the consumer side, not just the producer side).

### DD-DISTILL-5: Quiescence scenarios stay one per metric name, not consolidated

The journey file ships ONE cross-cutting quiescence scenario ("No Cinder event means no Pulse metric point"). DISTILL implements it as ONE `#[test]` that queries all three metric names in a loop. This is a faithful translation — not a granularity change.

Rationale: the cross-cutting scenario is a single behavioural assertion ("an unused bridge writes nothing"); splitting it into three per-metric scenarios would inflate test count without adding behavioural coverage. The loop covers all three metric names with one trip to the panic-path-free wiring.

### DD-DISTILL-6: Three tests pass legitimately under the scaffold — by design

Of 11 tests, 3 pass legitimately against the RED scaffold:

| Test | Why it passes against the scaffold |
|------|----------------------------------|
| `the_bridge_is_send_and_sync` | Compile-time bound holds for the scaffold's `Arc<dyn MetricStore + Send + Sync>` field — this is the structural Earned-Trust probe (ADR-0038 layer 1). It is meant to lock at compile time and stay green forever. |
| `no_cinder_event_means_no_pulse_metric_point` | Bridge constructed but never invoked, no panic path entered, Pulse stays empty. Documents the upstream quiescence invariant (no call → no emission). Stays green when DELIVER fills in the bodies. |
| `cinder_migrate_failure_with_unknown_item_emits_no_pulse_point` | `migrate` on never-placed item returns `Err(UnknownItem)` without invoking `record_migrate` (per `crates/cinder/src/store.rs:174-188`). The bridge's `record_migrate` panic body is never reached because Cinder never calls it on the failure path. Documents the contract DISCUSS D3 inherits from Cinder. Stays green when DELIVER fills in the bodies. |

This is acceptable RED-state behaviour: the three legitimate passes document upstream invariants that the bridge inherits structurally, and 8 tests panic with the SCAFFOLD marker until the DELIVER wave implements the three `record_*` bodies.

## Scenario inventory

| # | Test name | Slice | Story | KPI | Tag |
|---|-----------|-------|-------|-----|-----|
| 1 | `cinder_place_produces_a_pulse_metric_point_under_same_tenant` | 01 | US-01 | OK1 | `@in-memory @US-01` |
| 2 | `cinder_place_serialises_each_tier_as_lowercase_string` | 01 | US-01 | OK1 | `@in-memory @US-01` |
| 3 | `two_tenants_cinder_place_events_land_in_isolated_pulse_buckets` | 01 | US-01 | OK1 | `@in-memory @US-01` |
| 4 | `no_cinder_event_means_no_pulse_metric_point` | 01 | US-01 (cross-cutting) | OK1 | `@in-memory @US-01` |
| 5 | `cinder_migrate_produces_a_pulse_point_with_from_and_to_attributes` | 02 | US-02 | OK2 | `@in-memory @US-02` |
| 6 | `cinder_migrate_failure_with_unknown_item_emits_no_pulse_point` | 02 | US-02 | OK2 | `@in-memory @US-02` |
| 7 | `two_tenants_cinder_migrate_events_land_in_isolated_pulse_buckets` | 02 | US-02 | OK2 | `@in-memory @US-02` |
| 8 | `cinder_evaluate_emits_per_item_migrate_points_and_one_evaluate_point` | 03 | US-03 | OK3 | `@in-memory @US-03` |
| 9 | `cinder_evaluate_with_no_eligible_items_emits_no_evaluate_point` | 03 | US-03 | OK3 | `@in-memory @US-03` |
| 10 | `cinder_evaluate_across_two_tenants_emits_per_tenant_counts` | 03 | US-03 | OK3 | `@in-memory @US-03` |
| 11 | `the_bridge_is_send_and_sync` | cross-cutting | US-01 AC #7 | n/a (structural probe) | `@property @in-memory` |

Total: 11 scenarios. Error/edge: 3 (#4 quiescence, #6 failed-migrate, #9 zero-eligible-evaluate) = 27% of behavioural tests. The "40%+ error" target is documented in the BDD methodology as a default for typical features with broad failure surfaces; for a pure-function bridge over two in-memory ports with the failure modes exhaustively listed above, all identifiable error paths are covered. No invented failure scenarios were added to inflate the ratio.

## Adapter coverage table

| Adapter | Crate | Role in this feature | Real-I/O? | Tested by |
|---------|-------|----------------------|-----------|-----------|
| `cinder::InMemoryTieringStore` | cinder | Driver — invokes the bridge through Cinder's full call cascade (incl. `evaluate_at` dual emission) | In-process; no external I/O | Every `#[test]` in `cinder_to_pulse.rs` |
| `pulse::InMemoryMetricStore` | pulse | Assertion target — receives bridge emissions | In-process; no external I/O | Every `#[test]` in `cinder_to_pulse.rs` |
| (no external integrations) | — | — | — | — |

No costly externals. No driving HTTP/CLI adapter (the bridge is invoked through `Box<dyn cinder::MetricsRecorder + Send + Sync>` trait dispatch inside `InMemoryTieringStore` — this IS the driving port for the bridge at v0; the CLI is a post-v0 follow-up feature per DISCUSS D6).

## Mandate Compliance evidence

**CM-A — Hexagonal Boundary**: test file imports the driving port through `cinder::TieringStore` (its three verb methods are the system-under-test entry points) and the assertion port through `pulse::MetricStore` (`query`). The bridge type `self_observe::CinderToPulseRecorder` is imported solely to construct the wiring — never to call its `record_*` methods directly. This honours the DESIGN DD1 rule: "drive Cinder, query Pulse — do not call the bridge directly." Direct bridge invocation would fail the dual-emission test in Slice 03 because the bridge cannot itself trigger the per-item `record_migrate` cascade that lives inside `InMemoryTieringStore::evaluate_at`.

Import listing:

```text
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use aegis::TenantId;
use cinder::{InMemoryTieringStore, ItemId, MigrateError, Tier, TierPolicy, TieringStore};
use pulse::{InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder, TimeRange};
use self_observe::CinderToPulseRecorder;
```

All imports are public trait/type entry points from the three involved crates. Zero internal-component imports.

**CM-B — Business Language**: test names use domain terms exclusively (place, migrate, evaluate, tenant, tier, isolation, quiescence). String literals `"cinder.place.count"` / `"cinder.migrate.count"` / `"cinder.evaluate.migrated.count"` are the locked emission contract per ADR-0038 §2 and DISCUSS D1 — they are the business names of the metrics the operator queries. Tier attribute literals `"hot"` / `"warm"` / `"cold"` and direction attribute keys `"from"` / `"to"` are the locked lowercase serialisation from DISCUSS D4. No HTTP / REST / JSON / status-code / DB-jargon appears in test names or comments.

Grep evidence (would-be technical terms in test code):

```text
$ grep -E "(HTTP|REST|JSON|status.code|database|DB |API )" crates/self-observe/tests/cinder_to_pulse.rs
(no matches)
```

**CM-C — User Journey Completeness**: every test models a complete operator journey (Priya wires bridge → Priya drives Cinder → Priya queries Pulse → Priya observes expected metric shape). No test asserts an internal state or a mocked method call. The "user observation" is the `Vec<(Metric, MetricPoint)>` returned by `MetricStore::query`, which is exactly the observable surface Priya uses for Lumen events today.

Walking-skeleton count: **0** (Strategy A per DISCUSS D2; not applicable). Focused-scenario count: 11.

**CM-D — Pure Function Extraction Before Fixtures**: the bridge IS the impure adapter. Internal helper extraction is locked by DESIGN DD3 / ADR-0038 §5 (one `emit` helper + one `tier_attr` lowercase serialiser). The crafter implements these in DELIVER. The acceptance suite uses two `InMemory` adapters (`InMemoryTieringStore`, `InMemoryMetricStore`) — both real v0-shipped adapter implementations of their respective ports; neither is a mock. Fixture parametrization is N/A — single environment (`clean`) per `devops/environments.yaml`.

## Definition of Done

1. [x] All acceptance scenarios written with test bodies that compile and execute (RED panics from scaffold; legitimate passes for three documented invariants).
2. [x] Test pyramid: acceptance suite at `crates/self-observe/tests/cinder_to_pulse.rs`. Inner unit-test layer is the DELIVER wave's concern (cargo-mutants gate at 100% per ADR-0005 Gate 5 + CLAUDE.md per-feature MT strategy).
3. [x] Peer review approved — self-review against all 9 critique dimensions: all PASS or N/A under Strategy A. (Dim 5 + Dim 9d/9e are N/A because no `@walking_skeleton` scenarios under Strategy A; Dim 1's 40% error target is documented as default-for-typical, the no-substrate adapter case here exhausts the identifiable failure surface at 27%.)
4. [x] Tests run in CI/CD pipeline — `devops/ci-cd-pipeline.md` Gate 1 (`cargo test --workspace --all-targets`) auto-discovers the new `[[test]] name = "cinder_to_pulse"` block in `crates/self-observe/Cargo.toml`. No new gate plumbing required.
5. [x] Story demonstrable to stakeholders from acceptance tests — each `#[test]` name reads as a one-line story of operator outcome ("cinder place produces a pulse metric point under same tenant", "two tenants cinder place events land in isolated pulse buckets").

## Verification output

```text
$ cargo build -p self-observe
   Compiling pulse v0.1.0
   Compiling self-observe v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.32s

$ cargo test -p self-observe --test cinder_to_pulse --no-run
   Compiling self-observe v0.1.0
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.81s
  Executable tests/cinder_to_pulse.rs

$ cargo test -p self-observe --test cinder_to_pulse
running 11 tests
test cinder_evaluate_across_two_tenants_emits_per_tenant_counts ... FAILED
test cinder_evaluate_emits_per_item_migrate_points_and_one_evaluate_point ... FAILED
test cinder_evaluate_with_no_eligible_items_emits_no_evaluate_point ... FAILED
test cinder_migrate_failure_with_unknown_item_emits_no_pulse_point ... ok
test cinder_migrate_produces_a_pulse_point_with_from_and_to_attributes ... FAILED
test cinder_place_produces_a_pulse_metric_point_under_same_tenant ... FAILED
test cinder_place_serialises_each_tier_as_lowercase_string ... FAILED
test no_cinder_event_means_no_pulse_metric_point ... ok
test the_bridge_is_send_and_sync ... ok
test two_tenants_cinder_migrate_events_land_in_isolated_pulse_buckets ... FAILED
test two_tenants_cinder_place_events_land_in_isolated_pulse_buckets ... FAILED

test result: FAILED. 3 passed; 8 failed; 0 ignored; 0 measured; 0 filtered out

# All 8 failures panic with the SCAFFOLD marker:
# "CinderToPulseRecorder::record_place not yet implemented — RED scaffold (DISTILL); DELIVER replaces this body."

$ cargo clippy -p self-observe --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.17s
# (no warnings)
```

State: **RED, not BROKEN**. The test binary compiles and runs; eight scenarios panic at runtime with the SCAFFOLD marker; three pass legitimately documenting the upstream invariants the bridge inherits. Clippy clean.

## Skipped artefacts (and why)

- `docs/feature/cinder-to-pulse-bridge-v0/distill/walking-skeleton.md` — Strategy A + DISCUSS D2 (NO WS). No walking-skeleton scenarios exist; documenting them would be misleading.
- Separate adapter-integration test files — both adapters (`cinder::InMemoryTieringStore`, `pulse::InMemoryMetricStore`) are in-memory v0 implementations that ship with comprehensive in-tree tests in their owning crates. Exercising them again in `cinder_to_pulse.rs` via the bridge is sufficient end-to-end coverage; a separate integration file would only repeat the same in-process wiring with a different filename. Skipped per Andrea's task brief instruction.

## Handoff to DELIVER

Next agent: `nw-software-crafter`.

Deliverables ready:

| Artefact | Path | State |
|----------|------|-------|
| Acceptance test suite | `crates/self-observe/tests/cinder_to_pulse.rs` | RED — 11 scenarios, 8 panic with SCAFFOLD marker, 3 pass (legitimate invariants) |
| RED scaffold | `crates/self-observe/src/cinder_bridge.rs` | SCAFFOLD: true — public surface real per ADR-0038 §1, three `record_*` bodies panic |
| Module wiring | `crates/self-observe/src/lib.rs` | `mod cinder_bridge;` + `pub use cinder_bridge::CinderToPulseRecorder;` appended |
| Manifest wiring | `crates/self-observe/Cargo.toml` | `cinder = { path = "../cinder", version = "0.1.0" }` dependency + `[[test]] name = "cinder_to_pulse"` block |
| DISTILL wave-decisions | `docs/feature/cinder-to-pulse-bridge-v0/distill/wave-decisions.md` (this file) | — |

What the crafter inherits:

- **One-at-a-time implementation order**: Slice 01 first (`record_place` body → 3 place tests turn green, 1 quiescence test stays green). Then Slice 02 (`record_migrate` body → 2 migrate tests turn green, 1 failed-migrate test stays green). Then Slice 03 (`record_evaluate` body → 3 evaluate tests turn green). The `Send + Sync` structural probe and the cross-cutting quiescence test stay green throughout — they cover invariants the scaffold already honours structurally.
- **Locked emission contract**: ADR-0038 §2 table pins metric names, kinds, units, point values, point attribute schemas. Tests assert these contracts at string-literal level — typos in the production body fail the corresponding test deterministically.
- **Locked public surface**: `cargo public-api -p self-observe` (CI Gate 2 per ADR-0005) and `cargo semver-checks` (Gate 3) catch any deviation from ADR-0038 §1 at CI time.
- **Mutation-testing target**: `crates/self-observe/src/cinder_bridge.rs` at 100% kill rate per ADR-0005 Gate 5 (per-feature MT strategy from CLAUDE.md).
- **No new external dependencies, no new workspace members, no new CI gates** — inherits the existing five-gate workspace contract from ADR-0005.
