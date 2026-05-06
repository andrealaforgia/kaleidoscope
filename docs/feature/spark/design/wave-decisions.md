# Wave Decisions — `spark` v0 (DESIGN)

> **Wave**: DESIGN (`nw-solution-architect` / Morgan).
> **Date**: 2026-05-06.
> **Author**: Morgan, single-pass on Bea's overnight delegation.
> **Companion documents**: `c4-context.md`, `c4-container.md`,
> `c4-component.md`, `technology-choices.md`, `slice-mapping.md`,
> `back-propagation.md`,
> `../../product/architecture/adr-0011-spark-public-api-and-crate-layout.md`,
> `../../product/architecture/adr-0012-spark-error-type-design.md`,
> `../../product/architecture/adr-0013-spark-dependency-pinning.md`,
> `../../product/architecture/adr-0014-spark-flush-timeout-mechanism.md`,
> `../../product/architecture/adr-0015-spark-single-init-invariant.md`,
> `../../product/architecture/adr-0016-spark-guard-posture.md`,
> `../discuss/wave-decisions.md`,
> `../discuss/user-stories.md`,
> `../discuss/journey-spark.yaml`.

This file is the load-bearing artefact for the DISTILL wave (Atlas,
`nw-acceptance-designer`). DISTILL reads this to know which decisions
DESIGN locked, which technologies are in the dependency tree, and
which CI gates the test scaffolding must satisfy.

---

## What DESIGN locked

DESIGN's job per the agent's principle 2 is "WHAT (component
boundaries, technology stack, AC); never HOW (algorithm
implementation, method signatures beyond interface contracts)". The
DISCUSS wave locked the contract; DESIGN locks the technology and
internal structure that satisfies it.

Six ADRs (ADR-0011 through ADR-0016) numbering continued from the
last Aperture ADR (0010). Three C4 diagrams (Mermaid). One
technology-choices table. One slice-mapping. One back-propagation
note for Bea. This wave-decisions summary.

---

## ADR table — what each one locks

| ADR | Subject | Lock |
|---|---|---|
| ADR-0011 | Public API surface and crate layout | Free `pub fn init` in `lib.rs`; `SparkConfig` + `SparkError` + `SparkGuard` re-exported from per-concept modules; `[dev-dependencies]` posture for `aperture`; `Cargo.toml` skeleton including 8 `[[test]]` declarations; five-gate CI contract mirroring harness ADR-0005. |
| ADR-0012 | `SparkError` design | Four DISCUSS-locked variants (`MissingRequiredAttribute`, `InvalidEndpoint`, `ExporterInitFailed`, `GlobalAlreadyInitialised`); `#[non_exhaustive]`; explicit `Display`/`Error` impls; `source()` chain via `Box<dyn Error + Send + Sync>` for `ExporterInitFailed`; minimum trait derives (Debug only); `thiserror` reserved for future variants. |
| ADR-0013 | Dependency pinning | Exact-minor pin `=0.27` for the OTel family (`opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-semantic-conventions`); MSRV inherited from workspace (1.88); `aperture` as `[dev-dependencies]` only; semconv version verification (Spark's `feature_flag.*` does not collide with OTel semconv 0.27); migration path to 0.28 / v1 documented. |
| ADR-0014 | Flush-timeout mechanism | Sequential per-provider flush with shared remaining-time budget; `drained=unknown` / `dropped=unknown` at v0 because the OTel SDK does not expose counts; panic-safety in Drop (no `catch_unwind`, no `unwrap` / `expect` on fallible calls); idempotent second drop via `Option::take`. |
| ADR-0015 | Single-init invariant | Spark-internal `AtomicBool` flag (catches Spark-called-twice) + delegation to `opentelemetry::global::set_*_provider` Err path (catches set-by-other-code); roll-back-on-failure; per-binary test isolation via `[[test]]` declarations; `tests/invariant_single_init.rs` is its own single-test binary. |
| ADR-0016 | `SparkGuard` posture | Opaque struct with private fields; `#[must_use]` with directive message explaining the silent-discard hazard; Drop-only contract (no `shutdown()`, no `flush_now()`, no field accessors); minimum Debug (no resolved-config leak); compiler-inferred Send + Sync; no Clone / Copy / PartialEq. |

---

## Technology stack — locked

Runtime tree, all Apache-2.0 / MIT / BSD:

- `opentelemetry = "=0.27"`
- `opentelemetry_sdk = { version = "=0.27", features = ["trace", "logs", "metrics"] }`
- `opentelemetry-otlp = { version = "=0.27", default-features = false, features = ["grpc-tonic", "trace", "logs", "metrics"] }`
- `opentelemetry-semantic-conventions = "=0.27"`
- `thiserror = "2"`
- `tracing = "0.1"`
- `url = "2"`

Dev tree (allowed AGPL via `aperture`):

- `aperture = { path = "../aperture", version = "0.1.0" }` (AGPL-3.0-or-later; dev-only)
- `tokio = { version = "1.40", features = ["full"] }`
- `tracing-subscriber = { version = "0.3", default-features = false, features = ["fmt", "json", "env-filter", "registry"] }`
- `serde_json = "1"`
- `serial_test = "3"`

Workspace pins co-resolve. `Cargo.lock` already carries
`opentelemetry 0.27.1` and `tonic 0.12.3` transitively via Aperture
and the harness; Spark's pins do not churn the lockfile. Full
analysis in `technology-choices.md`.

---

## C4 diagrams — locked

Three diagrams in Mermaid, embedded in markdown files under
`docs/feature/spark/design/`:

- **L1 (System Context)** — `c4-context.md` — personas (developer,
  operator), application-process boundary, external systems
  (Aperture, OTel SDK, application's tracing subscriber).
- **L2 (Container)** — `c4-container.md` — seven library containers
  inside the application process (spark, opentelemetry,
  opentelemetry_sdk, opentelemetry-otlp, tonic, tracing, env channel)
  + sequence diagram for init and Drop flows.
- **L3 (Component)** — `c4-component.md` — six internal modules in
  `crates/spark/src/`: `lib.rs`, `config.rs`, `error.rs`, `guard.rs`,
  `init.rs`, `observability.rs`. Cross-module flow during init and
  drop.

L3 is justified for Spark per the agent's principle 9: 5+ modules
with load-bearing cross-module relationships during the
init-and-drop flow.

---

## Constraints established for downstream waves

These are constraints DESIGN's choices establish that DISTILL and
DELIVER inherit:

### For DISTILL (Atlas, `nw-acceptance-designer`)

1. **Real Aperture, real wire** — every integration test under
   `crates/spark/tests/` spawns a real Aperture instance via
   `aperture::spawn(Config::for_test())` and uses
   `aperture::testing::RecordingSink`. No InMemoryExporter or
   InMemorySpanExporter at v0 (per DISCUSS Slice 01 "Strategy C
   real local").
2. **Per-binary test isolation** — eight `[[test]]` declarations in
   `Cargo.toml` (six slice tests + two cross-cutting invariant
   tests). Each is a separate process with pristine OTel global
   state. `tests/invariant_single_init.rs` is a single-`#[test]`
   binary.
3. **Tracing event capture** — the integration tests subscribe to
   the application's `tracing` facade via
   `tracing-subscriber` (dev-dep) plus a custom layer that captures
   target=`spark` events into a `Vec<CapturedEvent>`. The mechanism
   is the one Aperture uses (`crates/aperture/src/observability.rs`
   `CapturedEvent` + `crates/aperture/src/testing.rs::stderr_capture`).
4. **`serial_test` for env-mutating tests** — Slice 04's tests carry
   `#[serial]` attributes so the process-global env-var mutations do
   not race across test cases.
5. **Substring assertions on `Display`** — Slice 02's error-path
   tests assert `error.to_string().contains("...")` rather than
   matching the entire `Display` line. The exact `Display` strings
   are locked in ADR-0012; renames are version-bump.
6. **`drained=unknown` / `dropped=unknown` at v0** (per ADR-0014 §2)
   — the SDK does not expose drained/dropped counts at 0.27. Slice
   06's tests must assert "`drained=unknown`" for the clean path and
   "`dropped=unknown`" for the deadline path, or Bea forwards Path A
   from `back-propagation.md` to Luna for a DISCUSS contract update.
7. **Three signal types in Slice 05** — Slice 05's test increments a
   counter, drops the guard, THEN asserts the
   `ExportMetricsServiceRequest` reached the sink (counter
   accumulation is asynchronous in the OTel SDK).

### For DEVOPS (Forge, `nw-platform-architect`)

1. **Five-gate CI contract** mirroring harness ADR-0005:
   - Gate 1 — `cargo test --workspace --all-targets --locked`
   - Gate 2 — `cargo public-api --diff-git-checkouts main HEAD -p spark`
   - Gate 3 — `cargo semver-checks check-release -p spark --baseline-rev main`
   - Gate 4 — `cargo deny check`
   - Gate 5 — `cargo mutants --package spark --in-diff` (workflow
     `gate-5-mutants-spark.yml` mirroring `gate-5-mutants-aperture.yml`)
2. **`cargo deny check` enforces the licence containment** — the
   workspace's `deny.toml` (already authored for the harness, extended
   for Aperture) refuses `AGPL-3.0-or-later` in the runtime closure.
   This is the structural enforcement that keeps `aperture` a dev-dep.
3. **Per-binary test isolation** — Spark's `[[test]]` declarations
   give DEVOPS clean per-slice CI step boundaries (one row per slice
   in the runner's UI). Spark's mutation testing does NOT require
   `--test-threads=1`; per-binary process isolation is sufficient.
4. **Workspace MSRV** — Spark inherits `rust-version.workspace = true`
   (1.88 currently). If a future Spark dep raises Spark's MSRV, the
   policy (per project memory note) is to bump the workspace floor,
   not pin around it.
5. **No production-deployment KPIs at v0** — Spark is a library; KPI
   tracking at v0 is exclusively CI-based. The
   `outcome-kpis.md > DEVOPS handoff` already says this; DESIGN has
   not changed it.

### For DELIVER (Crafty, `nw-software-crafter`)

1. **Module split** — implement six modules per ADR-0011 §"Internal
   layout": `lib.rs`, `config.rs`, `error.rs`, `guard.rs`, `init.rs`,
   `observability.rs`. Crate-root `#![forbid(unsafe_code)]`.
2. **`init.rs` is the orchestrator** — full init flow lives in one
   module (lint -> AtomicBool CAS -> Resource composition -> exporter
   construction -> provider construction -> global-set -> guard
   return); roll-back on failure; AtomicBool semantics as in ADR-0015.
3. **`guard.rs` Drop semantics** — per ADR-0014: sequential per-provider
   flush with shared remaining-time budget; idempotent via
   `Option::take`; emits exactly one observability event after the
   `shutdown initiated` event; never panics, never calls
   `process::exit`, never uses `catch_unwind`.
4. **`config.rs` resolve_endpoint helper** — the precedence chain
   (`SparkConfig::with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` >
   default `http://localhost:4317`) lives here so tests can drive it
   independently.
5. **`observability.rs` is the vocabulary boundary** — all
   `tracing::info!` and `tracing::warn!` calls at `target="spark"`
   flow through pub(crate) helpers in this file. Centralisation gives
   the mutation-test surface a single integration site.
6. **`feature_flag.` prefix is centralised** — single source of truth
   in `init.rs` (or `observability.rs`); the literal `"feature_flag."`
   does not appear in more than one place.
7. **Examples under `examples/`** — three example files for the
   walking-skeleton, the four-attr Resource, and the error-paths
   demos. `cargo build --examples` runs them as part of Gate 1.

---

## Open questions DESIGN deliberately did not answer

Per the agent's principle 2 ("Architecture owns WHAT, crafter owns
HOW"), these are DELIVER-wave decisions:

1. **Whether `with_feature_flags` accepts a HashMap, BTreeMap, slice,
   or array literal at the call site** — ADR-0011 locks the generic
   signature `IntoIterator<Item = (impl Into<String>, impl
   Into<String>)>`; the call-site shape is the application's choice.
2. **Whether `Resource` is shared via `Arc<Resource>` or cloned across
   the three providers** — ADR-0011 / Slice 05 say "all three share";
   the implementation mechanism is the crafter's call.
3. **The exact `Display` punctuation for each `SparkError` variant** —
   ADR-0012 locks the substring assertions (the part the tests
   match); the leading/trailing punctuation is style.
4. **Whether the `tracing` event uses `info!()` macro vs
   `event!(Level::INFO, ...)`** — both produce the same observable
   event; Spark's tests assert on the captured field set, not the
   macro form.
5. **Whether `init.rs::lint` is one function or a `validate_config`
   sub-module** — ADR-0011 keeps the lint inside `init.rs`; the
   crafter may extract it to `init/lint.rs` if it grows beyond ~30
   lines.

These are non-architectural choices. The crafter handles them in
DELIVER's GREEN+REFACTOR phases.

---

## Sentinel's five suggestions — answered

DESIGN was given five suggestions in `peer-review.md > Suggestions
for Morgan`. Each is answered:

| # | Suggestion | Answer |
|---|---|---|
| 1 | OTel semconv version verification | ADR-0013 §2: `service.name` uses the `opentelemetry-semantic-conventions::resource::SERVICE_NAME` constant. `tenant.id`, `feature_flag.*`, `experiment.id` are Kaleidoscope-house attributes (not OTel-semconv at 0.27); migration path to a future semconv-compatibility mode is documented. |
| 2 | OTel SDK version pin | ADR-0013 §1: exact-minor pin `=0.27` for the four OTel crates (mirrors harness ADR-0003 in style); migration path to `=0.28` and v1 documented. |
| 3 | Flush-timeout mechanism | ADR-0014: sequential flush with shared remaining-time budget; `drained=unknown` / `dropped=unknown` at v0 (back-propagation note Path A recommends DISCUSS update to acknowledge this). |
| 4 | `GlobalAlreadyInitialised` test mechanism | ADR-0015 §2: per-binary test isolation; `tests/invariant_single_init.rs` is its own single-test binary. |
| 5 | `SparkGuard` posture | ADR-0016: opaque + `#[must_use]` (with directive message) + Drop-only + no public methods + minimum trait derives. |

---

## Back-propagation to DISCUSS

One issue surfaced (`back-propagation.md`):

- **Drained/dropped counts on shutdown / flush-deadline events** —
  the OTel SDK at 0.27 does not expose counts publicly. DESIGN's
  ADR-0014 §2 locks `drained=unknown` / `dropped=unknown` at v0;
  DISCUSS's user stories US-SP-06 and journey-spark.yaml step 5
  tui_mockup currently imply integer counts (`drained=7`,
  `dropped=3`).

  **Recommended path** (Path A): Bea forwards to Luna a small DISCUSS
  contract update that says "drained=N where N is the SDK-exposed
  count if available; v0 reports `drained=unknown`". The semantic
  intent (event emitted, deadline bounded, outcome observable) is
  preserved; only the literal value changes.

DESIGN has NOT modified DISCUSS artefacts. Bea's call.

---

## Quality gate self-check (per the agent's "Quality Gates" rubric)

| Gate | Status | Evidence |
|---|---|---|
| Requirements traced to components | PASS | `slice-mapping.md` per-slice table maps each US-SP story to ADRs and modules |
| Component boundaries with clear responsibilities | PASS | `c4-component.md` L3 diagram + module-responsibility table |
| Technology choices in ADRs with alternatives | PASS | ADR-0011 (4 options), ADR-0012 (4 options), ADR-0013 (4 options), ADR-0014 (4 options), ADR-0015 (5 options), ADR-0016 (4 options) |
| Quality attributes addressed | PASS | `technology-choices.md > "Why these choices satisfy the quality attributes"` table maps each ISO 25010 attribute |
| Dependency-inversion compliance | PASS | Spark's `init.rs` orchestrates external SDK crates inward; `guard.rs` calls SDK provider trait methods; no Spark internal module is called by the SDK |
| C4 diagrams (L1+L2 minimum, Mermaid) | PASS | All three levels present (L3 justified by 5+ modules) |
| Integration patterns specified | PASS | OTLP/gRPC over `tonic` to Aperture; `tracing` events to application's subscriber; `OTEL_*` env-var contract |
| OSS preference validated | PASS | All runtime deps Apache-2.0 / MIT / BSD; AGPL only as dev-dep with `cargo deny` enforcement |
| AC behavioural, not implementation-coupled | PASS | DISCUSS-locked AC (US-SP-01..06) describe observable behaviour; DESIGN ADRs reference AC by substring/event-shape, not by method signatures |
| External integrations annotated for contract testing | PASS | `slice-mapping.md > "External-integration contract test annotation"` — Spark's integration tests against real Aperture IS the consumer-driven contract test; harness validates the wire bytes |
| Architectural enforcement tooling recommended | PASS | All five gates of ADR-0011 named with specific Rust tools (`cargo public-api`, `cargo semver-checks`, `cargo deny`, `cargo mutants`); none are conventions, all are CI-enforced |
| Peer review completed and approved | PENDING | Bea will dispatch Atlas (`@nw-solution-architect-reviewer`) separately per the brief's instruction. |

---

## Earned-Trust self-application (principle 12)

For each load-bearing decision DESIGN locked, three orthogonal
enforcement layers exist (subtype / structural / behavioural) per the
ArchUnit-style three-layer pattern. The probe contracts:

| Decision | Subtype check | Structural check | Behavioural check |
|---|---|---|---|
| Public-API surface (ADR-0011) | `cargo public-api` (Gate 2) | `cargo semver-checks` (Gate 3) | Slice 01 integration test exercises every public item |
| `SparkError` variants (ADR-0012) | Rust `match` exhaustiveness with `#[non_exhaustive]` | `cargo semver-checks` rejects variant removals | Slice 02 tests assert each variant by name AND its named fields |
| OTel family pin (ADR-0013) | Cargo resolver refuses non-`=0.27` | `cargo deny` rejects wildcards | Slice 01 integration test runs through real wire to Aperture |
| AGPL containment (ADR-0013 §3) | Rust compilation: dev-dep unavailable in `cargo build` | `cargo deny` rejects AGPL in runtime closure | (none needed; the structural rule is sufficient) |
| Bounded flush (ADR-0014) | `Duration` / `Instant::saturating_duration_since` types | Slice 06 wall-clock measurement | Slice 06 Cases A/B/C exercise clean / deadline / down-downstream |
| Single-init (ADR-0015) | `static AtomicBool` lifecycle | dedicated `[[test]]` binary | `invariant_single_init.rs` exercises double-init |
| `SparkGuard` posture (ADR-0016) | Private fields | `cargo public-api` rejects field promotion | Slice 06 asserts observable-event-only contract |
| Single-init test mechanism (ADR-0015 §2) | per-binary process isolation | Cargo test runner native behaviour | each `[[test]]`-declared binary has pristine OTel state |

A change that bypasses one layer is caught by at least one of the
others. The principle 12 self-application is documented inside each
ADR's `## Self-Application of Earned Trust` section.

---

## Handoff to DISTILL

Recipient: `nw-acceptance-designer` (Atlas).

Required reading order:

1. `docs/feature/spark/design/wave-decisions.md` (this file).
2. `docs/feature/spark/design/c4-context.md`,
   `c4-container.md`, `c4-component.md`.
3. `docs/feature/spark/design/technology-choices.md`.
4. `docs/feature/spark/design/slice-mapping.md`.
5. `docs/feature/spark/design/back-propagation.md` (the one DESIGN
   finding that may warrant DISCUSS revision).
6. `docs/product/architecture/adr-0011..0016` (the six locked DESIGN
   decisions).
7. The DISCUSS artefacts already in DISTILL's brief (in the order
   Sentinel's review specified).

Atlas's job is to turn the DISCUSS BDD scenarios + the DESIGN ADRs
into executable Cargo tests against the public surface ADR-0011 locks.
The integration posture is **Strategy C "real local"**: real Aperture
instances at ephemeral loopback ports, `RecordingSink` to capture
export traffic, no InMemory transports. Per-binary test isolation per
ADR-0015 §2.

---

## Handoff to DEVOPS

Recipient: `nw-platform-architect` (Forge).

Receives:

- `docs/feature/spark/design/wave-decisions.md` (this file).
- `docs/feature/spark/design/technology-choices.md` (full dep table
  + licence audit).
- ADR-0011 §"CI gates" (the five-gate contract).
- ADR-0013 §6 (`cargo deny check` config; the workspace `deny.toml`
  already covers Spark's runtime closure verbatim).
- ADR-0015 §5 (the per-binary test isolation contract).
- The mutation-testing pattern Aperture inherited
  (`gate-5-mutants-aperture` in `.github/workflows/ci.yml`); Forge
  produces `gate-5-mutants-spark` mirroring the pattern.

Forge chooses the workflow runner specifics; the contract gates are
runner-agnostic and must all pass on every commit affecting
`crates/spark/**`.

---

## Handoff to DELIVER

Recipient: `nw-software-crafter` (Crafty).

Receives the full DESIGN package (this file + ADRs + C4 + technology-
choices + slice-mapping). The slice-by-slice "Implementation pointers"
sections in `slice-mapping.md` are the entry points for each
RED -> GREEN -> REFACTOR cycle.

Per the project's CLAUDE.md, the crafter is **the only agent that
writes production source under `crates/spark/src/`**. DESIGN does not
write code. The ADRs and C4 + technology choices are the contract.

---

## Iteration budget and gates

Per the agent's "Peer Review Protocol", Atlas (the reviewer) gets two
iterations max. Bea will dispatch Atlas per the brief; DESIGN does
not self-dispatch.

Quality-gate status at handoff: 11 of 12 PASS, 1 PENDING (peer
review). The 11 PASS gates are listed in the table above.

Vai.
