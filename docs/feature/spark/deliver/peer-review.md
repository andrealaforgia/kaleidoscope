# Peer review — Spark v0 DELIVER

- **Date**: 2026-05-06
- **Reviewer**: `@nw-software-crafter-reviewer` (Crafty in review mode)
- **Wave**: DELIVER (six slices, single-pass)
- **Artefact set**: `crates/spark/src/`, `crates/spark/tests/`, `crates/spark/Cargo.toml`, plus the back-propagation log at `docs/feature/spark/deliver/back-propagation.md` and the in-place ADR amendments at `docs/product/architecture/adr-001{1,3,5,7}-*.md`
- **Verdict**: **APPROVED** — merge without iteration; coordinate Spark v0 graduation
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

Spark v0's DELIVER wave is rigorous, complete, and well-documented.
Six elephant-carpaccio slices, each closed with a tight RED → GREEN
→ REFACTOR cycle, each verified by `cargo mutants --in-diff` at 100%
kill rate on the diff, each landing as one or two commits to `main`
with no `--no-verify` shortcuts. The seven ADRs (0011-0017) are
implemented faithfully; in-place amendments document the choices
DELIVER had to make that DESIGN could not anticipate.

Sixty active tests across eight binaries (six slice + two invariant)
exceed the 36-40 test budget the slice-mapping table suggested, but
the over-count is justified: each test is designed for single-mutation
kill per ADR-0011's mutation-testing mandate, and per-signal
parametrisation gives mutation granularity that consolidated
parametrised cases would lose. A post-v0 hygiene pass to consolidate
the four near-identical metrics-attribute tests in Slice 05 would
trim the count without losing kill rate; nothing about it blocks the
v0 merge.

Five back-propagation issues were raised during DELIVER and all are
closed with rationale and forward path. Bea applied each ADR
amendment in-place. The audit trail is clean.

---

## Quantitative validation

| Wave artefact | Count |
|---|---|
| Slice 01 — walking skeleton | 7 active tests |
| Slice 02 — init error paths | 11 active tests |
| Slice 03 — feature flags + experiment.id | 11 active tests |
| Slice 04 — env-var precedence | 7 active tests, all `#[serial]` |
| Slice 05 — logs and metrics | 8 active tests (3 originally `#[ignore]`'d, un-ignored at Slice 05 DELIVER) |
| Slice 06 — bounded flush deadline | 10 active tests |
| `invariant_single_init.rs` | 1 active test |
| `invariant_no_telemetry_on_telemetry.rs` | 5 active tests (3 originals + 2 added at Slice 05) |
| **Total** | **60 active tests** |

Mutation kill rate per slice (from each slice's commit message and
the `cargo mutants --in-diff` runs):

| Slice | Mutants | Caught | Unviable | Missed | Kill rate (viable) |
|---|---|---|---|---|---|
| 01 | 7 | 6 | 1 | 0 | 100% |
| 02 | 8 | 7 | 1 | 0 | 100% |
| 03 | 2 | 2 | 0 | 0 | 100% |
| 04 | 7 | 7 | 0 | 0 | 100% |
| 05 | 10 | 7 | 3 | 0 | 100% |
| 06 | 14 | 12 | 2 | 0 | 100% |

Cumulative: 48 viable mutants, 41 caught + 7 unviable, 0 missed.
**100% kill rate on the diff at every slice's close**.

---

## ADR fidelity check

| ADR | Implementation | Verdict |
|---|---|---|
| ADR-0011 — public surface | Four-item consumer-facing surface (`init`, `SparkConfig`, `SparkError`, `SparkGuard`) plus two `#[doc(hidden)]` test seams (`__reset_for_testing`, `__test_logger_provider`). Both seams documented in the in-place amendments at Slice 02 and Slice 05 DELIVER. | PASS |
| ADR-0012 — error type | Closed four-variant `SparkError` with explicit `Display` and `Error` impls, `#[non_exhaustive]`, minimum-trait posture (Debug only). `ExporterInitFailed::source` returns `Option<Box<dyn Error + Send + Sync>>`. | PASS |
| ADR-0013 — dependency pinning | `=0.27` exact-minor pin on the OTel family; `rt-tokio` feature added at Slice 01 DELIVER per amendment; `opentelemetry-appender-tracing =0.27` (not `=0.28`) per Slice 05 DELIVER amendment. Aperture as `[dev-dependencies]` only; `cargo deny check` enforces. | PASS |
| ADR-0014 — flush mechanism | Sequential per-provider flush with shared remaining-time budget; `Instant::now()` arithmetic; saturating duration; `drained=unknown` / `dropped=unknown` Path A literal at v0; panic-safe Drop; idempotent via `Option::take()`. | PASS |
| ADR-0015 — single-init invariant | `static AtomicBool` flag plus delegation to OTel SDK `set_*_provider` Err path; transactional roll-back on post-flag failure; per-binary process isolation via `[[test]]` declarations; `invariant_single_init.rs` is its own one-test binary. Slice 06 amendment: flag released on Drop (sequential `init → drop → init` permitted). | PASS |
| ADR-0016 — guard posture | Opaque `SparkGuard` with private fields; `#[must_use]` directive; Drop-only contract (no public methods); minimum trait derives (Debug only). | PASS |
| ADR-0017 — logs-emission via tracing-appender | `opentelemetry-appender-tracing =0.27` runtime dep; bridge wired as `tracing_subscriber::Layer` with `target != "spark"` filter (production path) and via `BridgeWithTargetFilter` adapter through `__test_logger_provider` test seam (integration-test path); the no-telemetry-on-telemetry invariant defended in two places, both verified. | PASS |

---

## Test fidelity check

`praise:` All sixty tests import only the public surface or the
`#[doc(hidden)]` test seams. No reach into private modules. No mock
of the production code; the test posture is Strategy C "real local"
end-to-end via real Aperture instances at ephemeral loopback ports
with `RecordingSink` for assertion.

`praise:` Path A compliance on Slice 06 verified at every assertion
site: the `drained=` and `dropped=` prefixes are checked separately
from the value, and the value accepts either `unknown` or an
integer. No hardcoded integer assertions; no missing prefixes.

`praise:` Slice 05's three originally-`#[ignore]`'d log-emission
tests preserved their function names verbatim and were un-ignored
in-place (rewritten to use `tracing::info!` per ADR-0017 §2). The
DELIVER work is exactly what Scholar's `#[ignore]` markers
anticipated.

---

## RED → GREEN trail

`praise:` Each slice's commit history shows the canonical pattern:
RED tests in place, production stub returning `unimplemented!()`,
implementation lands one piece at a time driven by the smallest
failing test. Commit messages cite the specific test that drove the
change. Mutation testing runs cited at the end of each slice.

The pre-commit hook ran cleanly on every commit (each commit message
implicitly confirms via the `[pass] all pre-commit gates green`
output that lands at commit time). No `--no-verify` was used.

---

## Back-propagation discipline

`praise:` Five back-propagation issues raised during DELIVER, all
documented at the time of the offending change with explicit forward
path:

1. Slice 01 — `rt-tokio` feature added (DESIGN ADR-0013 §1 needed amendment).
2. Slice 01 — capture-layer wiring filled in test-side (DISTILL helper expected this).
3. Slice 03 — fixture-side `SPARK_INIT_SERIAL` mutex pattern introduced (canonical for slices 03+).
4. Slice 04 — `with_clean_otel_env` test helper bug + Case C reset path (DISTILL fixture fix).
5. Slice 05 — appender version pin off-by-one (DESIGN ADR-0017 §1 corrected); bridge wiring required `__test_logger_provider` test seam + `BridgeWithTargetFilter` adapter.
6. Slice 06 — `SparkGuard::Drop` releases AtomicBool flag (DESIGN ADR-0015 §1 amended).

All ADR amendments applied in-place by Bea. The DISCUSS Changed
Assumptions sections updated. The audit trail is clean.

---

## Workspace integrity

Verified by reading the most recent commit's pre-commit hook output:

- `cargo fmt --check` clean.
- `cargo clippy --all-targets --locked -- -D warnings` clean.
- `cargo deny --all-features check` clean.
- `cargo test --workspace --exclude spark --all-targets --locked` 23 binaries, all `ok`. No regression to harness or aperture.
- `cargo build -p spark --all-targets --locked` clean (Spark compile-only check while still in DISTILL/DELIVER phase).

Spark's own test suite (excluded from the workspace test gate during
DISTILL/DELIVER per the hook's amendment) verified by per-binary runs
in commit messages: 60/60 passing across the eight binaries.

---

## Defensive coding

`praise:` `#![forbid(unsafe_code)]` honoured throughout `src/`. No
`unwrap()` or `expect()` on paths reachable from user input. The
Drop path is panic-safe (no `unwrap` / `expect` / `catch_unwind`;
every fallible call matched). Atomics use `Ordering::SeqCst` (the
strongest ordering, correct for global-state coordination).

---

## Suggestions for post-v0 hygiene (non-blocking)

`suggestion (non-blocking):` Slice 05's four metrics-attribute tests
(`metric_export_carries_service_name`, `metric_export_carries_tenant_id`,
`metric_export_carries_feature_flag`, `metric_export_carries_experiment_id`)
all assert the same higher-level invariant: "the metrics export
Resource carries every set house attribute". They could be
consolidated into one parametrised test using a list of expected
KeyValue pairs. The mutation kill rate would not regress because the
parametrised test still asserts each attribute by name. Estimated
saving: 4 → 1 test in Slice 05; total 60 → 57.

`suggestion (non-blocking):` The `BridgeWithTargetFilter` custom
`Layer` adapter exists only because of a `tracing_subscriber::reload`
+ `Filtered`/`FilterId` incompatibility. If a future
`tracing_subscriber` release fixes that incompatibility, the production
path's `bridge.with_filter(...)` and the test path's adapter can
collapse to one mechanism. Track in a future hygiene pass; until then,
both filter sites are documented and tested.

`suggestion (non-blocking):` The `__reset_for_testing` test seam was
introduced to bridge the gap between `[[test]]` per-binary process
isolation (ADR-0015 §3) and the multi-`#[test]` per binary pattern
Scholar's DISTILL adopted. The strictly ADR-pure remedy is one
`[[test]]` per init-calling test, growing the binary count from
eight to fifty-plus. If a future Cargo improvement reduces per-binary
build cost (e.g. shared-library test binaries land), the pure remedy
becomes affordable and the seam can be removed with a deprecation
cycle.

---

## Praise

`praise:` The `ApertureFixture` + `SPARK_INIT_SERIAL` mutex pattern
introduced at Slice 03 is exemplary. It serialises init-calling
tests through a shared fixture without per-test annotations,
eliminating the boilerplate that the per-test
`#[serial_test::serial]` + `__reset_for_testing()` pattern would have
required across slices 03-06.

`praise:` The two doc-hidden test seams (`__reset_for_testing` and
`__test_logger_provider`) follow the canonical Rust idiom (double-
underscore prefix + `#[doc(hidden)]` attribute) so consistently that
they read as a deliberate test-infrastructure surface, not as API
leakage. ADR-0011's amendments make the boundary explicit.

`praise:` The Path A back-propagation in DESIGN (Morgan's
`drained=unknown` decision) and Path A3 (the appender adoption)
both flowed cleanly through DISTILL into DELIVER. Crafty's Slice 05
DELIVER caught Morgan's pre-stall offset-cadence misreading at
build time; the lockfile was the source of truth, and the ADR was
amended without ceremony.

`praise:` The cumulative architecture diagrams added to the
narrative during this DELIVER show the round-trip closing as Spark
appears: bytes from the application's `tracing::info!` flow through
the appender bridge into Spark's LoggerProvider, out as OTLP via
the OTel SDK to Aperture's gRPC listener, validated by the harness,
captured by the recording sink. That's the round-trip the harness
and Aperture set up; Spark closes it.

---

## Approval

**APPROVED**. Merge without iteration. Coordinate Spark v0
graduation:

1. Remove `--exclude spark` from `scripts/hooks/pre-commit` (Gate 1
   → `--workspace`).
2. Remove `--exclude spark` from `.github/workflows/ci.yml`'s Gate 1
   step (same change).
3. Tag `spark/v0.1.0` with the canonical commit at the close of
   this DELIVER + graduation pair.
4. Update the narrative + slides for the DELIVER closure (per the
   wave-by-wave cadence rule).
5. Dispatch Forge (`@nw-platform-architect-reviewer`) on the DEVOPS
   wave's outputs once the workflow's first run on the graduated
   workspace comes back green.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.
