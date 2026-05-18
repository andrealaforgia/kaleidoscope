# Wave Decisions — `cinder-to-otlp-json-bridge-v0` / DISTILL

- **Wave**: DISTILL
- **Author**: Quinn (`nw-acceptance-designer`)
- **Date**: 2026-05-18
- **Mode**: Decide-and-record. The slice files and ADR-0039 §3 had
  already locked the test seam, the assertion shape, and the per-event
  contract; DISTILL's job here is to land the Rust integration-test
  file and document the irreducible choices.

## Inputs read

DISCUSS: `wave-decisions.md` (D1-D10), `user-stories.md` (US-01/02/03),
`outcome-kpis.md` (OK1-OK5 — OK5 is new to this feature).
DESIGN: `wave-decisions.md` (DD1-DD5), `application-architecture.md`,
ADR-0039 (§1-§7).
DEVOPS: `wave-decisions.md` (A1-A11), `kpi-instrumentation.md`,
`environments.yaml`.
Slices: 01, 02, 03.
Precedents: `crates/self-observe/tests/lumen_to_otlp_json.rs`,
`crates/self-observe/tests/cinder_to_pulse.rs`,
`crates/self-observe/src/lumen_otlp_json.rs`,
`crates/self-observe/src/cinder_bridge.rs`.

## In-wave decisions

### DWD-01: Test idiom — Rust integration tests (`#[test] fn ...`), not Gherkin

Mirror of `tests/lumen_to_otlp_json.rs`. One `#[test]` per behaviour,
explicit `// Given / // When / // Then` comment blocks. No
`.feature` files, no pytest-bdd, no scaffold markers. RED-gate is
compile-time: the test file imports `self_observe::CinderToOtlpJsonWriter`
which does not yet exist at DISTILL close; Rust's type system makes
"the production code is not written" the same condition as "the test
does not link". DELIVER's first action is to land
`crates/self-observe/src/cinder_otlp_json.rs` with the public surface
from ADR-0039 §1, which flips RED to GREEN one slice at a time.

Decided because: (a) the project is a Rust workspace, not Python; (b)
the BDD intent (one behaviour per scenario, Given/When/Then framing,
business-language assertions through a single seam) is preserved by
Rust integration-test conventions when paired with explicit
Given/When/Then comments; (c) the precedent
`tests/lumen_to_otlp_json.rs` already shipped this idiom and the
operator persona (Priya) has been auditing it for two prior commits
without friction.

### DWD-02: Test seam — `CapturingWriter` is `SharedBuf(Arc<Mutex<Vec<u8>>>)`

Locked by ADR-0039 §3. Copied verbatim from
`tests/lumen_to_otlp_json.rs:54-64` with `Clone` derive so the test
can hold the buffer handle while Cinder holds the recorder. The
production seam is `W: Write + Send + Sync`; `Arc<Mutex<Vec<u8>>>`
implements `std::io::Write` through the wrapper and gives the test
shared post-call read access.

The shared `wire()` helper (mirroring `tests/cinder_to_pulse.rs:64-69`)
encapsulates the standard wiring (buf → writer → cinder) so every
test reads one line shorter and the assertion blocks dominate the
test body.

Rule-of-three deferral: when the third OTLP-JSON writer test file
lands (Sluice / Augur / Ray / Strata), `SharedBuf` + `collect_lines`
+ `point_attrs_contain` graduate to a shared `tests/common.rs`. At
N=2 OTLP-JSON-writer test files, in-file duplication is cheaper than
the abstraction (DISCUSS D7 + ADR-0039 §3).

### DWD-03: Scenario coverage table — slice ↔ test name ↔ OK# coverage

| Slice | Test | OKs covered | Category |
|-------|------|-------------|----------|
| 01 / US-01 | `cinder_place_emits_one_otlp_resource_metrics_line_under_same_tenant` | OK1, OK4 | happy path |
| 01 / US-01 | `cinder_place_serialises_each_tier_as_lowercase_string` | OK1 | three-tier edge |
| 01 / US-01 | `two_tenants_cinder_place_emit_distinct_otlp_resource_attributes` | OK1 | per-tenant isolation |
| 01 / US-01 | `no_cinder_event_means_zero_bytes_in_the_ndjson_sink` | OK1, OK4 | quiescence (negative) |
| 01 / US-01 | `output_is_ndjson_one_line_per_event_with_trailing_newline` | **OK5** | NDJSON-validity guardrail |
| (cross-cutting) | `the_writer_is_send_and_sync` | structural | compile-time Earned-Trust probe |
| 02 / US-02 | `cinder_migrate_emits_line_with_from_and_to_attributes` | OK2 | happy path |
| 02 / US-02 | `failed_cinder_migrate_emits_no_otlp_line` | OK2, OK4 | failure-path quiescence (negative) |
| 02 / US-02 | `two_tenants_cinder_migrate_emit_isolated_otlp_lines` | OK2 | per-tenant isolation |
| 03 / US-03 | `cinder_evaluate_emits_dual_lines_n_migrate_plus_one_evaluate` | OK3, OK4 (+ DISCUSS D8) | dual-emission (highest density) |
| 03 / US-03 | `cinder_evaluate_with_zero_eligible_items_emits_no_evaluate_line` | OK3, OK4 | conditional emission (negative) |
| 03 / US-03 | `two_tenants_cinder_evaluate_emits_per_tenant_evaluate_lines` | OK3, OK4 | per-tenant evaluate split |

12 tests. Negative/edge ratio: 5/12 = 41.7% (zero-bytes,
failed-migrate, zero-eligible-evaluate, and 3 per-tenant isolation
scenarios — exceeds the 40% nWave error-path target). Every story
(US-01/02/03) has at least one happy-path test, one isolation test,
and at least one negative test. OK5 has its own dedicated test;
OK4 is asserted as a side-condition in every test that calls a
fallible Cinder method.

### DWD-04: Out-of-scope confirmation — cross-writer (Lumen+Cinder) concurrency NOT tested here

ADR-0039 §7 documents that the post-v0 CLI feature owns the
cross-writer NDJSON-validity test surface. This DISTILL wave
deliberately does NOT add tests that spin up both
`LumenToOtlpJsonWriter` and `CinderToOtlpJsonWriter` against the same
`File` and race them — that is the CLI follow-up feature's territory
(`kaleidoscope-cli-wires-cinder-otlp-bridge-v0` or merged into the
Pulse CLI wiring feature).

The within-writer NDJSON-validity invariant (OK5,
`output_is_ndjson_one_line_per_event_with_trailing_newline`) is the
necessary preliminary: a single writer's Mutex<W> + write_all-pair +
flush triple must itself be atomic across multiple invocations from
one thread before the cross-writer story can be told. That
preliminary is what this DISTILL wave delivers.

### DWD-05: Driving-port adapter verification — N/A (library only, no CLI/HTTP entry point in v0)

Per DISCUSS D9 + DEVOPS A7. The bridge is library-only at v0. There
is no CLI flag, no HTTP handler, no message consumer. The only "user"
of the writer is in-process Rust code (the post-v0 CLI feature) that
constructs `CinderToOtlpJsonWriter::new(file)` and passes it as
Cinder's `MetricsRecorder`. The acceptance tests stand in for that
in-process user.

Mandate 1 (Hexagonal Boundary Enforcement) is satisfied trivially:
the tests' only "entry point" is the `pub struct
CinderToOtlpJsonWriter` constructor + the `cinder::MetricsRecorder`
trait dispatch through `InMemoryTieringStore`. Both are public,
both are the surface the post-v0 CLI feature will use. No internal
component (e.g. the private `emit` helper, the private OTLP-JSON
serde structs) is imported by the tests.

The DEVOPS `environments.yaml` declares one environment (`clean`)
with no external preconditions; Mandate 4 (Environmental Realism) is
satisfied by the in-memory `SharedBuf` substrate — the real-File
substrate is the post-v0 CLI feature's responsibility per
ADR-0039 §3 and confirmed by the Lumen sibling having already
exercised the identical `Mutex<W>` pattern against real `File`s in
production (commits c6b336c, 3af7e82).

## Mandate compliance evidence

- **CM-A (Hexagonal Boundary)**: test file imports only public items
  (`self_observe::CinderToOtlpJsonWriter`, `cinder::{InMemoryTieringStore,
  ItemId, MigrateError, Tier, TierPolicy, TieringStore}`,
  `aegis::TenantId`, `serde_json::Value`). No `use
  self_observe::cinder_otlp_json::OtlpResourceMetrics` or any other
  internal-component import. Verifiable by grep.
- **CM-B (Business Language)**: test names use domain language
  ("cinder_place_emits_one_otlp_resource_metrics_line_under_same_tenant"
  describes the operator-observable outcome — OTLP-JSON line per
  place — not the implementation; "kaleidoscope.cinder" is a
  Kaleidoscope domain term, not a generic technical noun). Assertions
  speak to the observable wire contract (`line["scopeMetrics"][0]["scope"]["name"]
  == "kaleidoscope.cinder"`) rather than internal state.
- **CM-C (Walking Skeleton)**: per DISCUSS pre-wave decision, this
  feature ships `walking_skeleton: no`. There is no UI backbone to
  span; every story IS a thin end-to-end slice through the writer.
  The closest analogues are the slice-01 happy-path test (which proves
  one Cinder call traverses the writer and lands as a parseable
  OTLP-JSON line — the demo-able operator outcome) and the slice-03
  dual-emission test (which proves the cascade-through-Cinder works
  end-to-end for the most operator-interesting case).
- **CM-D (Pure Function Extraction)**: no fixture parametrisation —
  all tests run against the one declared environment (`clean`). The
  writer's only impure dependencies are `SystemTime::now()`,
  `Mutex<W>::lock`, `serde_json::to_string`, and the runtime-supplied
  `W: Write` — all of which are isolated behind the
  `cinder::MetricsRecorder` port from the test's perspective. No
  business logic lives in the test file; assertions delegate to the
  production type via the same trait that production callers use.

## Handoff to DELIVER (Crafty)

**Test file delivered**: `crates/self-observe/tests/cinder_to_otlp_json.rs`
(12 tests, will not compile until `self_observe::cinder_otlp_json`
module exists — that IS the RED gate per ADR-0039 §5 and DEVOPS A2).

**Crafty's first action**: land
`crates/self-observe/src/cinder_otlp_json.rs` (public surface per
ADR-0039 §1, recommended internal shape per ADR-0039 §5), add the
`mod` + `pub use` lines in `crates/self-observe/src/lib.rs`, add the
`[[test]]` block in `crates/self-observe/Cargo.toml` per ADR-0039 §6.
Slice 01 first (place + envelope shape + NDJSON-validity); then
Slice 02 (migrate); then Slice 03 (evaluate + dual emission).

**100% mutation kill rate** per CLAUDE.md and DEVOPS A3 on every
slice before review. The inherited `gate-5-mutants-self-observe` job
on `--in-diff` matches `cinder_otlp_json.rs` automatically; no CI
workflow edit required.
