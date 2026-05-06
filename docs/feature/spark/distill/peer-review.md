# Peer review — Spark v0 DISTILL

- **Date**: 2026-05-06
- **Reviewer**: `@nw-acceptance-designer-reviewer` (Sentinel)
- **Wave**: DISTILL (Scholar, single-pass)
- **Artefact set**: `docs/feature/spark/distill/` plus `crates/spark/` skeleton at HEAD
- **Verdict**: **APPROVED** — forward to DELIVER (Crafty)
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

The DISTILL wave establishes a stable, well-contracted acceptance test
surface with zero mandate failures across all three design mandates
(hexagonal boundary, business language, user journey). 57 test
functions across 8 binaries (54 active + 3 `#[ignore]`'d in Slice 05),
providing complete BDD-to-test traceability. Every test imports only
the four-item public surface (`init`, `SparkConfig`, `SparkError`,
`SparkGuard`). RED-on-day-one posture verified: `cargo build`
succeeds; every active test panics at first call to `spark::init`
(which contains `unimplemented!()`). Strategy C "real local"
implemented end-to-end: real Aperture instances at ephemeral loopback
ports, `RecordingSink` for assertions, no in-memory stubs.

The back-propagation note on the OTel SDK 0.27 logger-provider gap
is honest, well-scoped, and proposes Path A with four concrete
options (A1-A4) for DESIGN to choose from. The 3 `#[ignore]`'d Slice
05 log-emission tests preserve the BDD-scenario function names
verbatim, ready for DELIVER once the contract resolution lands.

---

## Mandate verification

| Mandate | Status | Evidence |
|---|---|---|
| CM-A — Hexagonal Boundary | PASS | All test imports are from the public surface only; no `use spark::config::internal_*`, no `use spark::init::internal_*`, no module-private reaches. |
| CM-B — Business Language | PASS | Test names read as user outcomes; no internal machinery exposed in test names; the wording matches the BDD scenarios. |
| CM-C — User Journey | PASS | Slice 01 walking skeleton + focused boundary tests per slice; per-slice binaries align with the elephant-carpaccio decomposition. |

---

## Dimension scores

| # | Dimension | Score |
|---|---|---|
| 1 | Happy-path bias | 8/10 — 26% error paths (below 40% threshold but correct for Spark's closed error surface; the four `SparkError` variants exhaustively cover the failure space) |
| 2 | Given-When-Then format | 10/10 — perfect GWT structure; every Then asserts observable behaviour |
| 3 | Business language | 10/10 — zero technical leakage in test names; pure user outcomes |
| 4 | Coverage completeness | 10/10 — every user story mapped to ≥1 test; all UAT scenarios traced |
| 5 | Walking-skeleton user-centricity | 9/10 — end-to-end wire exercise; fixture seam clearly documented |
| 6 | Priority validation | 10/10 — 8-binary structure matches DESIGN's slice mapping exactly |
| 7 | Observable behaviour assertions | 10/10 — only externally observable state asserted; zero private-field or mock assertions |
| 8 | Traceability coverage | 10/10 — story coverage perfect; environments check N/A for library |
| 9 | Fixture correctness | 10/10 — real Aperture fixture; ephemeral ports; canonical values centralised |

---

## Per-binary findings

### Slice 01 — walking skeleton (7 tests) — APPROVED

`praise:` Every test imports only the public surface. Every BDD
scenario has a corresponding `#[test]` function with a traceable name.
The walking skeleton scenario (one span round-trips through OTLP/gRPC
to Aperture's RecordingSink with the four house attributes on the
Resource) is the load-bearing assertion and is tested directly.

### Slice 02 — init error paths (11 tests) — APPROVED

`praise:` Each of the four `SparkError` variants has at least one
covering test. `MissingRequiredAttribute`, `InvalidEndpoint`,
`ExporterInitFailed`, and `GlobalAlreadyInitialised` are all
exercised. The closed-set discipline holds.

### Slice 03 — feature flags and experiment (10 tests) — APPROVED

The `IntoIterator<(K, V)>` ergonomics from ADR-0011 are exercised at
the call site (array literals, HashMap, BTreeMap, Vec). The four
house attributes (`service.name`, `tenant.id`, `feature_flag.*`,
`experiment.id`) are asserted on the Resource of every emitted span.

### Slice 04 — env-var precedence (7 tests, all `#[serial]`) — APPROVED

`praise:` Correct use of `#[serial]` for env-var manipulation. The
four-case precedence (builder > OTEL_EXPORTER_OTLP_ENDPOINT >
default) is exhaustively covered.

### Slice 05 — logs and metrics (5 active + 3 `#[ignore]`'d) — APPROVED with documented deferral

`praise:` The deferral is exemplary in transparency. The 3
`#[ignore]`'d log-emission tests preserve the BDD scenario function
names verbatim (`logs_export_carries_same_four_house_attributes_on_resource`
etc.), so when Path A's resolution lands the tests can be un-ignored
without renaming. The 5 active tests assert the traces+metrics
symmetry contract for the two API-supported signal types at OTel SDK
0.27. The back-propagation note documents the gap honestly.

### Slice 06 — flush deadline (10 tests) — APPROVED with Path A compliance verified

`praise:` Path A is correctly operationalised. Assertions check the
`drained=` and `dropped=` prefixes, then accept either `unknown` (v0
SDK 0.27) or an integer value (forward-compat). No hardcoded integer
assertions. The `flush_timeout_ms` field is asserted verbatim per
the contract. The down-downstream test (Aperture forcibly killed)
asserts the drop does not panic.

### invariant_single_init.rs (1 test) — APPROVED

`praise:` Single `#[test]` function in its own binary per ADR-0015
§3. Per-binary process isolation correctly applied. The test exercises
the GlobalAlreadyInitialised path: after a successful init, a second
init returns `Err(GlobalAlreadyInitialised)` with no panic, no global
state corruption.

### invariant_no_telemetry_on_telemetry.rs (3 tests) — APPROVED

Defends D5 (no telemetry-on-telemetry). The test asserts Spark's own
diagnostics (`tracing::info!(target: "spark", ...)`) reach the
application's tracing facade, NOT the OTel pipeline Spark configured.
The cross-check is rigorous: a RecordingSink behind Aperture is
inspected for any record with `service.name = "spark"` or any
Spark-internal identifier; the assertion is that no such record
appears.

---

## Source skeleton verification

| File | Posture | Status |
|---|---|---|
| `src/lib.rs` | Re-exports the four-item public surface; `#![forbid(unsafe_code)]`; no internal types leak | PASS |
| `src/config.rs` | Real `SparkConfig` builder (data + `with_*` methods); intentional, so tests can construct configs | PASS |
| `src/error.rs` | Real four-variant `SparkError` with explicit `Display` and `Error` impls; `#[non_exhaustive]` | PASS |
| `src/guard.rs` | Opaque `SparkGuard` with `#[must_use]` directive message; Drop is a no-op stub at DISTILL | PASS |
| `src/init.rs` | `unimplemented!()` panic on the first instruction; the canonical RED-state stub | PASS |
| `src/observability.rs` | `pub(crate)` tracing-vocabulary helpers panicking on `unimplemented!()` | PASS |

---

## Cargo.toml verification

- `license = "Apache-2.0"` — PASS (matches LICENSING.md SDK class)
- OTel family `=0.27` exact-minor pins — PASS (`opentelemetry`,
  `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-semantic-conventions`)
- `aperture` in `[dev-dependencies]` only, path-resolved with explicit
  version pin — PASS (AGPL containment via `cargo deny check`)
- Eight `[[test]]` declarations matching the eight test files — PASS
- `[lints.rust]` and `[lints.clippy]` reasonable — PASS

---

## Documentation completeness

`praise:` `wave-decisions.md`, `test-mapping.md`, and
`back-propagation.md` are all present, complete, and honest.

`test-mapping.md` provides per-slice mapping (BDD scenario → test
binary → `#[test]` function name → asserted public-API touchpoint),
which gives Crafty an unambiguous specification for DELIVER.

`back-propagation.md` is exemplary in surfacing the OTel SDK 0.27
logger-provider gap, naming four concrete resolution options
(A1-A4), and recommending Path A while leaving the choice to Bea.

---

## Path-A compliance scan (Slice 06)

Every assertion against the shutdown / flush-deadline tracing event
checks the **prefix** (`drained=` or `dropped=`), then matches either
`unknown` or an integer value. No hardcoded integer assertion that
would regress Path A. Verified by inspection of `slice_06_flush_deadline.rs`
lines around the prefix-extraction logic.

---

## Suggestions for Bea

`suggestion (non-blocking):` Route the back-propagation note to
Andrea for the A1/A2/A3/A4 decision (already in flight per the
prompt context). Once chosen:

- If A1: Morgan supersedes ADR-0011's public-surface count and writes
  ADR-0017 locking the v0 logs-emission seam. Luna rephrases US-SP-05's
  BDD scenarios. DISTILL re-runs Slice 05's `#[ignore]`'d tests.
- If A2: Morgan's ADR-0017 names the test seam mechanism; the
  consumer-facing surface stays at four items.
- If A3: Morgan's ADR-0017 adopts `opentelemetry-appender-tracing` as
  the v0 logs-emission path; Cargo.toml gains the dep at runtime;
  Slice 05's `#[ignore]`'d tests use `tracing::info!` macros and
  assert the records reach Aperture.
- If A4: KPI 5 (three-signal symmetry) defers to v0.1; the three
  `#[ignore]`'d tests stay until a future SDK release adds the global
  setter.

---

## Praise

`praise:` Scholar's DISTILL package is rigorous and honest. The
hexagonal boundary is upheld without exception. The Strategy C "real
local" posture is implemented end-to-end. The Path A compliance for
Slice 06 is correctly operationalised at every assertion site. The
back-propagation note for the logs-emission gap is the right
escalation pattern at the right time.

`praise:` The 3 `#[ignore]`'d Slice 05 tests preserving the BDD
scenario function names is exemplary forward-thinking. When Path A
resolves, those tests can be un-ignored without renaming, and the
DELIVER work for Slice 05 stays a small focused change rather than a
sweeping rewrite.

`praise:` The per-binary isolation discipline (eight `[[test]]`
declarations, `invariant_single_init.rs` as its own one-test binary,
`#[serial]` on env-var manipulating tests in Slice 04) shows deep
respect for the OTel global-state hazard. ADR-0015's mandate is
honoured exactly.

---

## Approval status

**APPROVED** — forward to DELIVER (Crafty).

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.

The back-propagation note on the OTel logs-emission gap is Bea's call
to route; it does not block this approval. DELIVER can begin on
Slices 01-04 and 06 immediately; Slice 05's logs portion waits for
Path A's resolution.
