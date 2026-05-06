# Peer review — Spark v0 DESIGN

- **Date**: 2026-05-06
- **Reviewer**: `@nw-solution-architect-reviewer` (Atlas)
- **Wave**: DESIGN (Morgan, single-pass)
- **Artefact set**: `docs/feature/spark/design/` plus six ADRs (0011-0016) at HEAD
- **Verdict**: **APPROVED** — ready for immediate DISTILL handoff
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

The Spark v0 DESIGN wave is architecturally sound, comprehensively documented,
and locked with rigorous evidence. Six well-crafted ADRs (0011-0016) specify
the public API surface, error handling, dependency pins, flush-timeout
mechanism, single-init invariant, and guard semantics. The C4 decomposition
(L1-L3) correctly scopes the system. The back-propagation note surfaced one
DISCUSS contract refinement (drained/dropped counts = `unknown` at v0); Path
A has been accepted by Bea and DISCUSS is already updated. Zero blocking
issues, zero architectural bias. All five CI gates are either implemented or
ready for DEVOPS handoff.

**Handoff readiness**: Scholar (DISTILL) can immediately read the six ADRs
plus the C4 diagrams plus slice-mapping and turn the six user stories' BDD
scenarios into executable Cargo tests against the locked public surface.

---

## Per-ADR findings

### ADR-0011 — public API surface and crate layout — APPROVED

`praise:` The four-item public surface (`init`, `SparkConfig`, `SparkError`,
`SparkGuard`) is minimal and correct. Matches idiomatic Rust SDK patterns
(`tracing-subscriber::fmt().init()`, `env_logger::init()`).

The `IntoIterator<(impl Into<String>, impl Into<String>)>` signature for
`with_feature_flags` is maximally ergonomic; covers array literals, HashMap,
BTreeMap, Vec without forcing shape.

The five-module split from day one (lib.rs, config.rs, error.rs, guard.rs,
init.rs, observability.rs) aligns with the journey's five backbone activities.
Harness ADR-0001 precedent justifies the mechanical upfront cost.

Eight `[[test]]` declarations (six slices + two invariants) give clean
per-slice CI output and per-binary process isolation for global-state
hazards.

Five CI gates mirror harness ADR-0005; Gate 4 (`cargo deny check`) is the
structural enforcement of Apache-2.0 licence containment.

### ADR-0012 — error type design — APPROVED

`praise:` The four variants (`MissingRequiredAttribute`, `InvalidEndpoint`,
`ExporterInitFailed`, `GlobalAlreadyInitialised`) implement DISCUSS-locked
contracts exactly.

Explicit `impl Display` plus `impl Error` with `source()` chain is cleaner
than `#[derive(thiserror::Error)]` for the current variant set. `thiserror`
is held in reserve for future variants without re-architecting.

`#[non_exhaustive]` discipline is rigorous. `cargo semver-checks` (Gate 3)
enforces that additions are non-breaking, removals are breaking. Aperture
ADR-0002 sets precedent; Spark mirrors correctly.

Minimum-trait posture (Debug only; no Clone/PartialEq/Eq) avoids coupling
to upstream error trait surfaces.

`ExporterInitFailed::source` as `Option<Box<dyn Error + Send + Sync>>` is
canonical upstream-hygiene pattern.

### ADR-0013 — dependency pinning policy — APPROVED

`praise:` Exact-minor pin `=0.27` for the OTel family is the right choice
for Spark's job (use the SDK, not defend the wire format). Harness's
exact-patch pin `=0.27.0` is right for its job; Spark's exact-minor is right
for Spark's. The distinction is clear and justified.

Co-resolution with harness verified: `Cargo.lock` carries `opentelemetry
0.27.1` and `tonic 0.12.3` transitively. No lockfile churn.

Semantic-conventions verification (§2) is thorough: `service.name` uses
`opentelemetry_semantic_conventions::resource::SERVICE_NAME` constant
(future-proof); `tenant.id`, `feature_flag.*`, `experiment.id` are
Kaleidoscope-house (not OTel semconv 0.27). Migration path is Codex Phase 0+.

AGPL containment via `[dev-dependencies]` plus `cargo deny check` is the
canonical licence-split mechanism. Version pin on path-resolved aperture
satisfies the wildcard ban.

MSRV verification confirms no upstream dep pushes above workspace 1.88.

Migration path table (v0 uses `=0.27`, future v0.x bumps to `=0.28`, v1 moves
to `^1.0` once OTel stabilises) is clear and forward-looking.

### ADR-0014 — flush-timeout mechanism — APPROVED

`praise:` Sequential-with-shared-budget flush (§1) is the right trade-off:
avoids async runtime coupling, avoids thread spawning at exit, total drop
time bounded by deadline. The correlated worst-case latency (slow tracer
starves meter) is accepted because all three providers hit the same OTLP
exporter and backed-up downstream.

Remaining-time budget arithmetic
(`deadline = Instant::now() + flush_timeout`, then
`remaining = deadline.saturating_duration_since(Instant::now())` per
provider) is overflow-safe. Sub-second precision works cleanly.

Drained/dropped count decision (§2) is honest and data-driven: OTel SDK 0.27
does NOT expose counts publicly. Rather than speculative Spark-side counting
(Path B, rejected), DESIGN locks `drained=unknown` / `dropped=unknown` at v0
with a documented path to integer when the SDK exposes it. This is the right
call; the SDK upgrade window is shorter than Spark's maintenance burden.

Panic-safety posture (§3) is explicit: no `unwrap()` / `expect()` /
`catch_unwind` in Drop. Every fallible call is matched. The downstream case
(Aperture killed mid-flush) is tested as proof Drop does not panic.

Idempotent Drop (§4) via `Option::take()` is standard Rust.

Three Earned-Trust layers (subtype via Duration typing, structural via Slice
06 wall-clock assertions, behavioural via mutants) are orthogonal.

**Back-propagation alignment**: ADR-0014 §2 locks
`drained=unknown` / `dropped=unknown` and DISCUSS `user-stories.md` now
documents this via the Changed Assumptions section (Path A, accepted by Bea
and committed at `25e3732`). DESIGN is fully compliant with Path A.

### ADR-0015 — single-init invariant and test mechanism — APPROVED

`praise:` Two-layer detection (Spark-internal `AtomicBool` plus delegation
to OTel SDK's `set_tracer_provider` Err path) is defence-in-depth. Catches
both common failure modes: Spark-called-twice and someone-else-set-it-first.

Roll-back-on-failure semantic (`SPARK_INITIALISED.store(false)` on any
post-flag error) is crucial for testing. A failed init does not leave Spark
in a half-initialised state where a retry would falsely report
`GlobalAlreadyInitialised`.

Per-binary test isolation (ADR-0011's eight `[[test]]` declarations) is
Cargo-native and solves the OTel global-state hazard cleanly. Each binary
gets a pristine process; no thread-sequencing gymnastics.

`invariant_single_init.rs` as a dedicated binary with a single `#[test]`
function is the right choice for the GlobalAlreadyInitialised case. Other
three variants (MissingRequiredAttribute, InvalidEndpoint, ExporterInitFailed)
share `slice_02_init_error_paths.rs` because they do not touch global state.

Three-layer Earned-Trust check (subtype via `static AtomicBool` semantics,
structural via `[[test]]` declarations as load-bearing contract, behavioural
via test asserting double-init path) is watertight.

### ADR-0016 — SparkGuard posture — APPROVED

`praise:` Opaque plus `#[must_use]` plus Drop-only design is exactly right
for an RAII guard. Matches `tracing-subscriber::fmt::WorkerGuard`.

`#[must_use]` directive message explains the consequence ("stops the OTel
pipeline before any telemetry is emitted"), so the compiler warning is
self-explaining. The canonical `let _guard = spark::init(config)?;` pattern
is taught by warning + docstring + examples.

Rejection of `shutdown()` method is correct: D1 (DISCUSS) locks "spark::init
is the only public entry point at v0". Adding shutdown creates two paths
and invites bugs. The application calls `drop(guard)` explicitly if it needs
explicit control (idiomatic Rust).

Rejection of public fields is correct: resolved configuration is observable
via the tracing INFO event at init (Slice 04). No need for a second
observability surface; some fields (endpoint URL) may be sensitive.

`Debug` impl (`finish_non_exhaustive()`) is minimal and non-leaking.

Compiler-inferred Send + Sync is correct: upstream OTel providers are
Send + Sync; Spark's guard inherits.

Three-layer Earned-Trust check (subtype via private fields, structural via
`cargo public-api`, behavioural via Slice 06 tests asserting observable-event
contract) ensures future changes that expose inner shape are caught.

---

## Cross-ADR consistency

**DISCUSS contract fidelity**: All six ADRs correctly implement the
DISCUSS-locked contracts.

| DISCUSS lock | DESIGN evidence |
|---|---|
| D1 — `spark::init` only | ADR-0011 (public surface) + ADR-0016 (no public methods on guard) |
| D2 — closed `SparkError` variant set | ADR-0012 locks four variants verbatim |
| D3 — `SparkConfig` builder | ADR-0011 locks builder methods verbatim |
| D4 — house attributes on Resource | Slice 05 + ADR-0014 both reference; no override |
| D5 — no telemetry-on-telemetry | ADR-0014 §3 + slice-mapping invariant |
| D6 — OTEL_* env-var contract | ADR-0011 + technology-choices.md honour upstream contract |
| D7 — single-init invariant | ADR-0015 locks it |

**CI gate alignment**: Five gates in ADR-0011 match harness ADR-0005 gates
and the existing `.github/workflows/ci.yml`:

- Gates 1-4 are implemented and running.
- Gate 5 needs DEVOPS implementation of `gate-5-mutants-spark.yml` (Forge's
  job; 100% kill rate per ADR-0005).

**Scope discipline**: PASS. No over-specification of v0.2+ concerns.
Migration paths for OTel 0.28/v1 (ADR-0013 §5), semconv alignment
(ADR-0013 §2), Aegis Phase 2 (ADR-0016 Future) are documented but not
load-bearing at v0.

---

## Quality attribute coverage

| Attribute | ADR evidence | Status |
|---|---|---|
| Performance (latency, throughput) | ADR-0014 bounds Drop time; sequential flush prevents network contention | COVERED |
| Reliability (fault tolerance, recovery) | ADR-0014 panic-safety in Drop; ADR-0015 transactional roll-back; ADR-0016 opaque guard | COVERED |
| Security (auth, data protection) | ADR-0013 enforces Apache-2.0 runtime closure (no AGPL viral). No TLS at v0 (Aegis Phase 2) | COVERED |
| Maintainability (modularity, testability) | ADR-0011 five-module split; per-binary test isolation; slice-mapping shows module exercising | COVERED |
| Observability (logging, monitoring) | ADR-0014 tracing events (init-succeeded, shutdown-initiated, shutdown-complete, flush-deadline-exceeded) | COVERED |

---

## Sentinel's five suggestions — coverage

| # | Suggestion | DESIGN answer | Status |
|---|---|---|---|
| 1 | OTel semconv version verification | ADR-0013 §2 (constant from semconv crate; house attributes flagged) | ANSWERED |
| 2 | OTel SDK version pin | ADR-0013 §1 (exact-minor `=0.27` mirroring harness ADR-0003; migration path) | ANSWERED |
| 3 | Flush-timeout mechanism | ADR-0014 (sequential, shared budget; `=unknown` at v0 via Path A) | ANSWERED |
| 4 | GlobalAlreadyInitialised test mechanism | ADR-0015 §2 (per-binary test isolation; `invariant_single_init.rs` is its own binary) | ANSWERED |
| 5 | SparkGuard posture | ADR-0016 (opaque + `#[must_use]` + Drop-only + minimum traits) | ANSWERED |

All five suggestions answered concretely with ADR plus specific evidence.

---

## API surface ergonomics (ADR-0011 deep dive)

`praise:` `SparkConfig` builder.

- `for_service(name: impl Into<String>)` — required, forces attention at
  call site.
- `require_tenant_id()` — opt-in for multi-tenant deployments;
  non-breaking to add.
- `with_tenant_id(impl Into<String>)` — optional setter.
- `with_feature_flags<I, K, V>(flags: I)` — accepts any
  `IntoIterator<(K, V)>` shape. Array literal, HashMap, BTreeMap, Vec all
  work. Excellent ergonomics without forcing shape at the call site.
- `with_experiment_id`, `with_endpoint`, `with_flush_timeout` — follow the
  same builder pattern.

The API is defensible, ergonomic, and meets Rust SDK expectations. Thin
wrapper, minimal surface, zero over-design.

---

## Praise

`praise:` Morgan's entire DESIGN package is architecturally mature. The
work demonstrates deep understanding of OTel SDK constraints (no public
drained/dropped counts at 0.27), Rust idioms (RAII, AtomicBool,
Option::take for idempotent drops), and forward-compatibility design
(feature pins with migration paths, `#[non_exhaustive]`, path-resolved
dependencies). Six ADRs form a coherent narrative: a thin, opinionated
wrapper that puts the OTel SDK in front of developers with house attributes
pre-configured and shutdown guarantees locked. No over-engineering, no
speculation, no resume-driven choices.

`praise:` The Earned-Trust principle (subtype / structural / behavioural)
is applied rigorously to every decision. A change that bypasses one
enforcement layer is caught by at least one of the others. This is rare in
architecture reviews and signals deep confidence in the design.

`praise:` The back-propagation note is exemplary transparency. Rather
than hiding a contract mismatch (DISCUSS's integer counts vs OTel SDK's
unavailable counts), Morgan surfaced it, proposed Path A, argued the
trade-off against Path B, and let Bea decide. This is the right escalation
pattern.

`praise:` The slice-mapping file (tying each user story → ADR → module →
CI invariant → KPI) is exemplary. Scholar (DISTILL) has a clear roadmap
for turning BDD scenarios into executable Cargo tests against the locked
public surface.

---

## Handoff requirements

**For Bea (orchestrator)**:

- Path A is already applied to DISCUSS (`25e3732`); DESIGN is fully
  compliant. No further action required from this review.

**For Scholar (DISTILL, `@nw-acceptance-designer`)**:

- Read in order: `wave-decisions.md` → C4 L1/L2/L3 → `technology-choices.md`
  → `slice-mapping.md` → ADRs 0011-0016 → DISCUSS artefacts (note Changed
  Assumptions in `user-stories.md`).
- Turn the six user stories' BDD scenarios plus the six ADRs into
  executable Cargo tests against the locked public surface.
- Use the real-Aperture plus RecordingSink posture (Strategy C "real local"),
  per DISCUSS `wave-decisions.md`.

**For Forge (DEVOPS, `@nw-platform-architect`)**:

- Implement `gate-5-mutants-spark.yml` workflow mirroring
  `gate-5-mutants-aperture.yml`.
- Contract: 100% kill rate per ADR-0005 Gate 5.

**For Crafty (`@nw-software-crafter`)**:

- Implement six modules (lib.rs, config.rs, error.rs, guard.rs, init.rs,
  observability.rs).
- Implement eight test binaries (six slices plus two invariants) per
  `slice-mapping.md`.
- Implement three examples (send_one_span_grpc, send_one_span_with_house_attrs,
  trigger_init_errors).
- The module split, linting, orchestration, and guard drop semantics are
  specified in the ADRs; implementation detail is the crafter's domain
  (method signatures, function bodies, exact macro usage).

---

## Approval status

**APPROVED** — ready for immediate DISTILL handoff.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.
