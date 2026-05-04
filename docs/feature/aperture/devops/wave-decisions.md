# Wave Decisions — `aperture` v0 (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-04.
> **Author**: Apex.
> **Mode**: autonomous (orchestrator-decided; Andrea is at dinner).
> **Companion documents**: `platform-architecture.md`, `ci-cd-pipeline.md`,
> `branching-strategy.md`, `kpi-instrumentation.md`,
> `observability-design.md`, `monitoring-alerting.md`,
> `environments.yaml`.
> **Workflow file**: `.github/workflows/ci.yml` (extended, not forked).
> **Toolchain pin**: `rust-toolchain.toml` (unchanged from harness DEVOPS).

This file records the choices Apex made when extending the
project's CI/CD and operational posture to cover Aperture v0. The
harness's DEVOPS wave (`docs/feature/otlp-conformance-harness-v0/devops/`)
established the precedent; this wave EXTENDS that precedent rather
than re-deriving it. Where a decision is inherited verbatim from the
harness, this document names the inheritance and stops; where Aperture
introduces new concerns (it IS a service, the harness IS a library),
this document records the new decision with rationale.

---

## Mode

This was an **execute-and-resolve** wave. The orchestrator pre-resolved
nine configuration questions that the skill template normally walks
the architect through (D1–D9, see "Inherited and orchestrator-resolved
decisions" below). Apex's job was to execute on those resolutions and
to surface the genuinely new decisions Aperture's service-shape
introduces.

The wave produced no new architectural ground; every load-bearing
choice traces to either the orchestrator's pre-resolution, the
harness's DEVOPS precedent, ADR-0005, ADR-0006 through ADR-0010, the
DISCUSS-locked scope, the DESIGN-locked architectural-rule
enforcement, or DISTILL's test inventory. This is appropriate for a
DEVOPS wave that adds a second crate to an existing CI surface and
records the operational posture of a service whose runtime is
deliberately operator-owned.

---

## Inherited and orchestrator-resolved decisions

| # | Topic | Resolution | Source |
|---|---|---|---|
| D1 | Deployment target | **Hybrid.** Aperture is consumed as a binary; the v0 deployment model is "ship the binary; operators run it as a long-lived process under their orchestrator of choice". No Kaleidoscope-side production cluster, no Kaleidoscope-side staging cluster. | Orchestrator. |
| D2 | Container orchestration | **None at v0.** Container packaging is a future iteration tracked under the roadmap's Spark/Aegis phases. v0 is "build, test, release the binary". | Orchestrator. |
| D3 | CI/CD platform | **GitHub Actions.** Already in place for the harness; extend, do not replace. | Orchestrator (consistent with harness DEVOPS D3). |
| D4 | Existing infrastructure | **Yes.** `.github/workflows/ci.yml` exists with five gates for the harness. Extend the same workflow to cover Aperture; do not fork into a separate workflow file. | Orchestrator. |
| D5 | Observability and logging | **Structured JSON to stderr** (per DISCUSS Q6 + ADR-0009). No metrics in v0. No telemetry-on-telemetry. Operators capture stderr via their log aggregator of choice; Aperture has no Aperture-specific opinion. | DISCUSS Q6, ADR-0009, orchestrator. |
| D6 | Deployment strategy | **Rolling** (operator-choice). Aperture's drain-respecting shutdown (US-AP-09, DISCUSS D8, ADR-0009) supports rolling deployments naturally; no Kaleidoscope-side deployment automation at v0. | Orchestrator + ADR design surface. |
| D7 | Continuous learning | **No.** No A/B, no flags, no canary at v0. The methodology is "ship a slice, observe, iterate" via nWave waves. | Orchestrator. |
| D8 | Git branching strategy | **Trunk-Based Development.** Codified in `branching-strategy.md` for the project; Aperture inherits unchanged. | Orchestrator + harness DEVOPS D8. |
| D9 | Mutation testing strategy | **Per-feature, 100% kill rate.** Already in root `CLAUDE.md`. Aperture's slices each ship with green tests + 100% kill rate per ADR-0005 Gate 5. | Orchestrator + root `CLAUDE.md` + ADR-0005. |

---

## What Aperture changes about the project's CI surface

The harness is a pure-function library. Aperture is a service: a
binary with two listener ports, a real Tokio runtime, an
operator-supplied downstream backend, a drain-respecting shutdown
sequence. The harness's DEVOPS wave correctly framed itself as
"library, not service, no deployment target". That framing only
partially transfers to Aperture: Aperture HAS a deployment target,
even if v0 leaves the choice to operators; Aperture HAS a runtime,
even if Kaleidoscope does not host it; Aperture HAS observability
needs, even if v0's only emission is structured stderr.

The five-gate CI contract (ADR-0005) still applies — Aperture is a
Cargo crate in the same workspace, so Gates 1, 2, 3, 4, 5 mechanically
extend to it. What is new are the **three Aperture-specific CI gates**
DESIGN named (`single_validator_per_signal`,
`no_telemetry_on_telemetry`, `probe_gold_runner`) and the **operator-
facing observability surface** v0 ships (structured stderr,
`/healthz`, `/readyz`).

Detail: this wave SCAFFOLDS those new gates as future work and ADDS
no Aperture-specific YAML to `ci.yml` until DELIVER lands the
underlying tests and the xtask binary. Adding a red-by-design CI gate
during the DELIVER cycle would block every push for the entire DELIVER
wave. The contract is named here; the wiring waits.

---

## Load-bearing decisions made in DEVOPS

### A1. Single workflow file, additive job extensions

**Decision**: Extend `.github/workflows/ci.yml` to cover Aperture.
Do not create a separate workflow file (e.g. `aperture-ci.yml`).

**Rationale**: a single workflow keeps the project's CI surface small
and discoverable; the harness's DEVOPS rationale for ADR-0005's five
gates is workspace-level, not crate-level (the gates own
cross-workspace concerns: licence policy, advisory database,
public-API stability, mutation testing, test suite). Forking the
workflow per crate would duplicate the toolchain bootstrap and the
cache configuration without buying isolation that the per-`-p`-flag
scoping does not already provide.

**Alternatives considered**:
- (A) Single workflow, additive jobs (recommended, accepted).
- (B) Separate `aperture-ci.yml`. Rejected: duplicates toolchain
  bootstrap; splits CI signal across files; complicates branch
  protection if it ever returns.
- (C) Reusable workflow + per-crate caller workflows. Rejected as
  over-engineering for two crates; revisit when the project has
  3+ crates with genuinely distinct CI shapes.

### A2. Gate 1 stays scoped to harness during DELIVER; flips to workspace at DELIVER close

**Decision**: At this DEVOPS wave's close, Gate 1 in `ci.yml`
continues to invoke `cargo test -p otlp-conformance-harness
--all-targets --locked`. **No Aperture change** to Gate 1 in CI yet.
A comment in `ci.yml` documents the graduation path:

> When DELIVER closes (all 8 Aperture slices green plus the two
> invariant tests), DELIVER's final commit replaces this `-p
> otlp-conformance-harness` invocation with a workspace-wide
> `--workspace --all-targets --locked` invocation.

**Rationale**: Aperture's tests are RED at DISTILL completion (84
active tests + 1 ignored, every one panics on a `unimplemented!()`
production-surface symbol). Adding Aperture to Gate 1 today would
turn `main` red for every commit until DELIVER's last slice lands.
That defeats trunk-based development's "main is socially always
green" property.

**Considered options**:
- (a) Switch to `cargo test --workspace --all-targets --locked` once
  all aperture tests are green. **Recommended for the long term.**
  Clean; matches the harness's own model; no per-crate maintenance.
- (b) Per-package list with explicit `-p` flags that grows
  commit-by-commit. Rejected: invites drift; every DELIVER commit
  becomes a YAML edit; review burden grows with no benefit over (a).
- (c) Marker convention (e.g. `#[ignore]` on RED tests until DELIVER
  lands them, then unmark). Rejected: contradicts DISTILL's
  scaffold-based RED-on-day-one strategy (D7 in DISTILL
  wave-decisions); unmarking is a per-test edit DISTILL deliberately
  avoided by panicking on unimplemented symbols instead.

The brief named option (a) as the recommendation; this wave honours
that. The graduation is a single one-line edit DELIVER makes when its
last slice goes green; no DEVOPS re-engagement required.

**Local pre-commit hook contrast**: the local pre-commit hook
currently runs `cargo test --workspace --exclude aperture`. The
exclusion is provisional (added in the today's hooks commit when
aperture's RED scaffold landed). It is removed by DELIVER's final
commit in lockstep with the CI Gate 1 graduation. Documented in
`ci-cd-pipeline.md > Local pre-commit hook graduation`.

### A3. Gate 4 (cargo deny) covers the workspace; one stub-Cargo.toml fix lands now

**Decision**: Gate 4 (`cargo deny --all-features check`) continues to
walk the workspace dependency graph; it implicitly covers Aperture
the moment Aperture is in the workspace (which DISTILL already
landed). One small `Cargo.toml` repair lands in this wave to make
Gate 4 green:

In `crates/aperture/Cargo.toml`, the line

```toml
otlp-conformance-harness = { path = "../otlp-conformance-harness" }
```

becomes

```toml
otlp-conformance-harness = { path = "../otlp-conformance-harness", version = "0.1.0" }
```

**Rationale**: cargo-deny's `bans.wildcards = "deny"` policy
(established in the harness DEVOPS wave) flags any path-only
dependency declaration as a wildcard, because the manifest version
is implicitly `*`. Adding the explicit version makes the dependency
non-wildcard while preserving path-based resolution inside the
workspace (cargo prefers the path over the registry when both are
specified). This is the canonical Rust-ecosystem idiom for sibling
crates that may eventually be published.

The fix is a one-line edit. DISTILL's scaffold did not include the
version field because no Aperture-specific deny rule had been run
against it; the gap surfaces the moment Gate 4 walks the new crate.
This is the harness's "fix-forward" pattern (cf. `wave-decisions.md
> Post-merge correction — Gate 4 vs wit-bindgen-core`): the failing
gate is the test that demands the fix.

**Verification**: running `cargo deny --all-features check` after the
edit returns `advisories ok, bans ok, licenses ok, sources ok` (the
four `license-not-encountered` warnings — `ISC`, `MPL-2.0`,
`Unicode-DFS-2016`, `Zlib` — are pre-existing and unrelated:
no transitive dep in the resolved tree carries those licences).

**No deny.toml changes required**. Aperture's new transitive
dependencies (tonic, axum, hyper, tower, tower-http, tracing,
tracing-subscriber, figment, async-trait, thiserror, reqwest with
rustls-tls, plus the existing prost / opentelemetry-proto / serde)
are all MIT or Apache-2.0 (or both). The existing allowlist already
includes both. The existing `multiple-versions = "allow"` relaxation
(established for the harness's `opentelemetry-proto` feature-graph
duplication) covers Aperture's case as well.

### A4. Gate 5 (cargo mutants) stays scoped to harness during DELIVER; flips to multi-package at DELIVER close

**Decision**: At this DEVOPS wave's close, Gate 5 in `ci.yml`
continues to invoke `cargo mutants --package otlp-conformance-harness
--no-shuffle --jobs 2`. A comment in `ci.yml` documents the
graduation path:

> When DELIVER closes (all 8 Aperture slices green), DELIVER's
> final commit changes this invocation to `cargo mutants --package
> otlp-conformance-harness --package aperture --no-shuffle --jobs 2`
> per the per-feature, 100 %-kill-rate gate in root `CLAUDE.md`.

**Rationale**: same as A2. Running `cargo mutants` against an
unimplemented production tree produces undefined-behaviour signal
(the harness defines mutants as "source-code edits the test suite
should detect"; an `unimplemented!()` body has no detectable
behaviour to mutate). Aperture must have a green test suite before
mutation testing has anything to measure. The brief explicitly
recommended this graduation; this wave honours it.

`cargo-mutants` supports multiple `--package` invocations natively
(verified at the binary's `--help`); no orchestration plumbing
needed.

**Mutation-test budget rule** (carried forward verbatim from the
harness's DEVOPS wave): start with the full per-push run; switch to
`--in-diff origin/main` if any single feature's full mutation run
exceeds 60 seconds wall-clock for two consecutive merges to `main`.
Aperture's source size at v0 close is comparable to the harness's
(both are small Rust crates with focused test suites); the threshold
is unlikely to fire in v0. The existing `timeout-minutes: 30` upper
bound on `gate-5-mutants` is the runaway-safety net.

### A5. Three Aperture-specific CI gates: documented as future work, not wired today

**Decision**: The three new CI gates Aperture introduces
(`single_validator_per_signal`, `no_telemetry_on_telemetry`,
`probe_gold_runner`) are documented in `ci-cd-pipeline.md` as future
DELIVER work. **They are not wired into `ci.yml` at this DEVOPS
wave's close.** The wiring is a per-gate, per-DELIVER-slice activity:

| Gate | DELIVER slice that delivers the test/code | When it goes into `ci.yml` |
|---|---|---|
| `gate-6-aperture-architectural-rules` (xtask AST walk for `single_validator_per_signal` + dependency direction + no `prost::Message::decode` in aperture/src + no `eprintln!` already enforced by clippy) | Slice 03 (traces) is when `validate_traces` becomes the third call site that needs counting; the xtask binary lands at Slice 03 and is wired into CI then | Slice 03 commit |
| `gate-7-aperture-no-telemetry` (network-namespace integration test at `tests/no_telemetry_on_telemetry.rs`) | Slice 06 (forwarding sink) is when ForwardingSink lands; the net-ns fixture cannot be exercised meaningfully before that | Slice 06 commit; `#[cfg(target_os = "linux")]` with a clear skip-message on macOS runners |
| `gate-8-aperture-probe-gold` (behavioural-layer probe gold-test at `tests/probe_gold_runner.rs`) | Slice 06 is when `Probe` and the wire-then-probe-then-use composition root land | Slice 06 commit |

**Rationale**: every red-by-design CI gate that exists during the
DELIVER cycle blocks every push for the entire DELIVER cycle. Under
trunk-based development, `main` must remain green; the discipline is
"land the test and the implementation in the same commit, then wire
the gate". DISTILL's RED scaffold honours this for `cargo test`
(every Aperture test panics; CI's Gate 1 stays scoped to harness;
graduation lands at DELIVER close). The three Aperture-specific
gates honour the same pattern: name them in `ci-cd-pipeline.md` so
no later wave re-derives the contract; wire them when the underlying
artefact is GREEN.

The brief explicitly recommended this. This wave honours the
recommendation.

**Alternatives considered**:
- (a) Wire all three gates today as `if: false` placeholders. Rejected:
  workflow YAML readability suffers; `if: false` jobs still consume
  GitHub's job-graph quota; the graduation is a clearer one-line
  removal-of-`if: false` than a fresh job add, but the visual noise
  during DELIVER outweighs the saved typing.
- (b) Wire all three gates today as `continue-on-error: true`. Rejected:
  trains contributors to ignore CI failures; the gates carry no signal
  until they are blocking.
- (c) Wire each gate at the DELIVER slice that delivers its underlying
  artefact. **Recommended and accepted.** Each slice's commit IS the
  natural moment to wire its gate.

### A6. Local pre-commit hook keeps `--exclude aperture` for now; graduation in lockstep with CI Gate 1

**Decision**: `scripts/hooks/pre-commit` continues to invoke
`cargo test --workspace --exclude aperture --all-targets --locked`.
The graduation path is documented in `ci-cd-pipeline.md > Local
pre-commit hook graduation`:

> When DELIVER closes (all 8 Aperture slices green plus the two
> invariant tests), the same DELIVER commit that flips CI Gate 1 from
> `-p otlp-conformance-harness` to `--workspace` ALSO removes
> `--exclude aperture` from `scripts/hooks/pre-commit`. Local and CI
> stay symmetric.

**Rationale**: the harness DEVOPS wave's "mirror, not duplicate"
principle requires local hooks to run the same gates as CI's commit
stage. During DELIVER, both local and CI are scoped to the harness;
both graduate together at DELIVER close. The "mirror" property holds
throughout.

**No hook file changes in this wave**. The graduation is a one-line
edit DELIVER makes when its last slice goes green; no DEVOPS re-
engagement.

### A7. Environments inventory mirrors the harness's structure; adds free-port concern

**Decision**: `environments.yaml` mirrors the harness's
4-environment matrix (contributor-linux, contributor-macos,
contributor-wsl [UNTESTED], ci-github-actions) verbatim, with **two
Aperture-specific notes**:

1. **Free-port concern**: every test in `crates/aperture/tests/`
   binds to `127.0.0.1:0` (DISTILL D4) and discovers the actual port
   via the `Handle` trait. The default ports 4317/4318 are documented
   but never asserted in tests. CI runners and contributor
   workstations therefore have NO port-availability requirement; no
   firewall configuration is necessary. The `RUST_TEST_THREADS=1`
   workspace setting (inherited from harness) prevents within-binary
   port contention.

2. **No new tools required**: Aperture's tests rely on the same
   substrate as the harness's (`cargo`, `rustup`, `git`). The
   `wiremock` test double for Slice 06 is a `[dev-dependencies]`
   crate, not a system-installed tool; no contributor setup beyond
   `cargo test`.

**Rationale**: the harness's `environments.yaml` correctly captured
that the project's only "environment" surface is "the conditions
under which the build and the tests must succeed". Aperture inherits
that framing; the only delta is the free-port concern (which is
absent because of DISTILL's ephemeral-port discipline) and the lack
of new tools (which is absent because everything Aperture needs is a
crate, not a system package).

### A8. KPI instrumentation: stderr-vocabulary-driven, build-time at v0

**Decision**: Aperture's eight outcome KPIs are tracked through the
mechanisms summarised below. Full per-KPI specification in
`kpi-instrumentation.md`.

| KPI | Build-time vs runtime | Data source at v0 | CI artefact? | Operator dashboard? |
|---|---|---|---|---|
| 1 — Walking-skeleton round-trip | Build-time | Slice 01 integration test pass/fail | No (run history) | No (binary milestone) |
| 2 — Transport-coverage acknowledgement ratio | Mostly runtime; build-time corroboration | Operator stderr aggregation; `slice_01..04` test outcomes corroborate | No | Yes (operator-side) |
| 3 — Readiness three-state machine | Build-time + survey | `slice_02_*` + `slice_08_*` tests; pilot-operator survey 30 days post-launch | No | Optional |
| 4 — Per-signal acknowledgement ratio | Mostly runtime; build-time corroboration | Operator stderr aggregation; `slice_03_*` + `slice_04_*` tests corroborate | No | Yes (operator-side) |
| 5 — Concurrency saturation events | Future load-test artefact (deferred) | `slice_05_*` cap-hit tests at v0; load test deferred to release-cadence work | Yes (when load test lands) | Yes (operator-side) |
| 6 — Refusal-not-drop invariant | Build-time | `slice_05_*::every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance` (`@property`) | No | n/a |
| 7 — Downstream-acceptance ratio | Mostly runtime; build-time corroboration | `slice_06_*` happy + failure scenarios corroborate; runtime is operator-side | No | Yes (operator-side) |
| 8 — Graceful-restart drop ratio | Future load-test artefact (deferred) | `slice_08_*` clean-drain + deadline-exceeded tests at v0; 1000-restart load test deferred | Yes (when load test lands) | Optional (operator-side) |

**Rationale**: at v0 Aperture has no Kaleidoscope-side runtime; all
eight KPIs are either (a) corroborated by acceptance tests in CI, (b)
emitted as structured stderr events the operator's existing log
aggregator scrapes, or (c) deferred to a release-cadence load-test
job that does not yet exist (KPIs 5 and 8). The existing CI artefact
pattern (`verdict-counts.json` for the harness's KPI 4) does not
transfer cleanly: Aperture's KPIs are runtime ratios, not
corpus counts. A future release-cadence workflow will produce
load-test reports for KPIs 5 and 8; that workflow is out of v0's
scope.

DISCUSS's `outcome-kpis.md > DEVOPS handoff` listed four
infrastructure asks; this wave addresses each:

1. **Data collection requirements**: stderr is the data feed; the
   `log_event_vocabulary` is the schema. Documented in
   `observability-design.md`.
2. **Dashboard / monitoring needs**: operator-side, not Kaleidoscope-
   side. Documented as "operator-owned" in
   `monitoring-alerting.md`; sample dashboard queries provided as
   prose for operators to translate to their preferred system.
3. **Alerting thresholds (guardrails)**: documented in
   `monitoring-alerting.md` as the four operator-side alerting rules
   the DISCUSS handoff named. Aperture itself emits no alerts.
4. **Baseline measurement before release**: greenfield acknowledged
   in `outcome-kpis.md`; no baseline collection needed before launch.

### A9. Branching strategy: trunk-based, inherited unchanged

**Decision**: `branching-strategy.md` for this wave confirms Aperture
inherits the project-wide trunk-based posture established in the
harness's DEVOPS wave (`docs/feature/otlp-conformance-harness-v0/devops/branching-strategy.md`).
**No per-feature branching strategy is needed.**

**Rationale**: the harness's wave already codified trunk-based for
the project. Repeating the codification per crate would be ceremony.
The `branching-strategy.md` in this directory is therefore a thin
pointer to the harness's authoritative document plus a one-line
confirmation that Aperture honours it.

### A10. Monitoring and alerting: minimal at v0; honest about it

**Decision**: `monitoring-alerting.md` documents the v0 state
explicitly: **Aperture itself emits no metrics, runs no alerting
agent, and exposes no Aperture-specific monitoring opinion.**
Operators rely on their existing log aggregator parsing structured
JSON stderr; the four DISCUSS-handoff alerting rules are documented
as queries operators can translate to their preferred alerting
system.

**Rationale**: Aperture's no-telemetry-on-telemetry commitment is
load-bearing (DISCUSS Q6, ADR-0009, CI invariant
`no_telemetry_on_telemetry`). The temptation to bolt on a metrics
endpoint or a Grafana template "for completeness" violates that
commitment. The honest position — "Aperture publishes a structured
event vocabulary; operators bring their own dashboards" — IS the
v0 monitoring posture, and stating it bluntly is a deliverable.
Pulse (Phase 4) is when telemetry-on-telemetry becomes a Kaleidoscope
concern.

---

## Resolution of new questions surfaced by DESIGN's handoff

DESIGN named five infrastructure asks in `design/wave-decisions.md >
Handoff to DEVOPS`. Each is resolved below.

### Q1 — `single_validator_per_signal` enforcement mechanism

**Resolution**: implement as an `xtask` workspace member at
`xtask/` (path TBD by DELIVER) that walks `crates/aperture/src/`
with the `syn` crate, counts call sites of
`otlp_conformance_harness::validate_logs/traces/metrics`, and
asserts each appears at most once. **Wired into CI at Slice 03**
(when the third validator becomes a thing worth counting). The
Slice 03 commit also adds the `gate-6-aperture-architectural-rules`
job to `ci.yml`. Behavioural corroboration ships in DISTILL's
`tests/invariant_single_validator.rs`; the load-bearing defence is
the xtask.

### Q2 — `no_telemetry_on_telemetry` enforcement mechanism

**Resolution**: implement as a Linux-only integration test
(`crates/aperture/tests/no_telemetry_on_telemetry.rs`) that runs
Aperture in a network namespace and inspects `/proc/self/net/tcp`
after each accept-path operation. Uses `unshare(CLONE_NEWNET)` via
the `nix` crate (Linux-only, `#[cfg(target_os = "linux")]`); macOS
runners get a clear skip message. **Wired into CI at Slice 06**
(when `ForwardingSink` lands and the test has a real ForwardingSink-
to-downstream path to assert as the only allowed outbound traffic).
The Slice 06 commit adds the `gate-7-aperture-no-telemetry` job to
`ci.yml`. Behavioural corroboration ships in DISTILL's
`tests/invariant_no_telemetry_on_telemetry.rs`.

**OS coverage gap**: macOS does not have Linux network namespaces.
The integration test is `#[cfg(target_os = "linux")]`; macOS CI (not
in v0's CI matrix anyway) and macOS contributor workstations skip it
with a clear `eprintln!` message. The test's coverage on Linux CI is
sufficient at v0; if Aperture grows to be deployed on non-Linux
platforms, a per-OS network-trace fixture is a future iteration's
concern.

### Q3 — `probe_gold_runner` enforcement mechanism

**Resolution**: implement as a behavioural integration test
(`crates/aperture/tests/probe_gold_runner.rs`) that uses the
`wiremock` test double (already in Aperture's dev-dependencies per
DISTILL) to stand up a fixture downstream that lies (200 OK to
OPTIONS, 503 to POST), then asserts Aperture refuses to start with
`event=health.startup.refused`. **Wired into CI at Slice 06** (when
`Probe` and `wire_then_probe_then_use` land). The Slice 06 commit
adds the `gate-8-aperture-probe-gold` job to `ci.yml`.

This is the third of the three Earned-Trust enforcement layers
DESIGN named (subtype + structural + behavioural). The structural
layer is the xtask check from Q1's mechanism family; the behavioural
layer is this gold-test; the subtype layer is Rust's type system
checking at compile time.

### Q4 — Pact-style contract test for the operator-supplied downstream OTLP backend

**Resolution**: **deferred to Phase 1 / pilot-operator engagement.**
Pact is a polyglot contract-test tool requiring a Pact broker
(self-hosted or hosted) to share contracts between consumer and
provider sides. The harness's DEVOPS wave correctly avoided
introducing infrastructure for hypothetical needs; this wave does
the same. At v0, the contract is defended by:

1. **Probe contract** at startup (`ForwardingSink::probe()`): catches
   "the downstream named in operator config does not honour the
   contract right now". Behavioural-layer test in
   `tests/probe_gold_runner.rs`.

2. **DISTILL Slice 06 happy + failure tests** against an in-process
   `wiremock` server: cover Content-Type strictness, OPTIONS
   support, 5xx, refused, timeout, sink_failed.

3. **The OTLP/HTTP/protobuf wire spec at OTel spec version 1.5.0**
   (the same version the harness pins), as the cross-implementation
   reference.

When pilot operators engage in Phase 1 and a real downstream-version
drift is reported, a Pact-style contract test is added then.
Expected cost: a single new dev-dependency (`pact_consumer`),
a fixture set under `tests/contracts/`, and a CI gate that runs the
Pact consumer side. Until then, the probe + wiremock + spec posture
is sufficient.

**Why this is the conservative default and not a hedge**: the OTLP
wire spec is stable; the most frequent drift mode at v0 will be
operator-side configuration drift (wrong endpoint, wrong port, TLS
misalignment), which the probe catches at startup. Drift in the
downstream's wire-protocol implementation is a Phase-1+ concern that
materialises only with multiple downstream variants in pilot
deployments.

### Q5 — Load-test infrastructure for KPI 5 (1-hour at 2x cap) and KPI 8 (1000-restart)

**Resolution**: **deferred to a release-cadence workflow.** v0 has
no release workflow yet (the crate is `publish = false`; no tag
trigger; no separate release pipeline). KPI 5's 1-hour load test
and KPI 8's 1000-restart scenario both fit in roughly 5–15 minutes
of runner wall-clock (per `outcome-kpis.md`'s budget note for
KPI 8); they are too expensive for per-commit CI but cheap enough
for per-release. When the release workflow is built (likely Phase 1
when v0.1 is cut for crates.io publication), these two load tests
become release-cadence gates.

The DISTILL slice tests (`slice_05_backpressure.rs` and
`slice_08_graceful_shutdown.rs`) corroborate the property-shaped
invariants at the unit level; the load tests are the
runtime-volume-shaped corroboration that operators care about.

Behavioural sketch (for the future release wave):

- **KPI 5 load test**: standalone integration-test binary
  `crates/aperture/tests/load_kpi5_overload.rs` that starts an
  Aperture instance with a small cap (e.g. cap=8), pumps 2x cap
  concurrent gRPC requests for 1 hour (compressed to several
  minutes for CI by adjusting the cadence), and asserts
  `count(request_received) − count(sink_accepted) − count(reject_*) − count(concurrency_cap_hit) == 0`.
  Tagged `#[ignore]` so per-commit CI skips it; release CI
  invokes it with `--ignored`.

- **KPI 8 1000-restart**: standalone integration-test binary
  `crates/aperture/tests/load_kpi8_restart.rs` that loops
  spawn-handle-shutdown 1000 times under continuous offered load
  and asserts the reconciliation invariant. Same `#[ignore]`
  posture.

Both shapes are fixture work for a future wave; this wave names
them so the release wave can reach for the contract without
re-deriving it.

---

## Apex-side decisions (not pre-resolved by orchestrator or DESIGN)

### B1 — Workflow trigger surface unchanged

**Decision**: `on: push: branches: [main]` and `on: pull_request:
branches: [main]`. No path filters. Inherited verbatim from the
harness's DEVOPS A1.

**Rationale**: same as harness A1 — path filters would let a
`docs/`-only commit skip CI, and the cost of running gates on a
docs change is negligible compared with the cost of accidentally
landing a code change masked as a docs change. Trunk-based
discipline favours simplicity over micro-optimisation.

### B2 — `concurrency: cancel-in-progress: true` unchanged

**Decision**: existing `concurrency.group: ${{ github.workflow }}-${{
github.ref }}` with `cancel-in-progress: true` covers Aperture
naturally. No change.

### B3 — `permissions: read` unchanged

**Decision**: workflow-level `permissions: contents: read`. No
Aperture-specific job needs writes (no artefact upload to release
storage, no comment-on-PR, no tag creation).

### B4 — Action version pinning posture unchanged

**Decision**: continue with the harness DEVOPS's pinning posture.
Any new third-party action introduced for Aperture-specific gates is
SHA-pinned. None are introduced in this wave (the three Aperture-
specific gates use plain `cargo` and `cargo run -p xtask`; no new
actions).

### B5 — Single OS in CI for v0 unchanged (`ubuntu-latest`)

**Decision**: `ubuntu-latest` only. No matrix.

**Rationale**: same as harness A5. Aperture's only OS-specific code
is `tokio::signal::unix` for SIGTERM/SIGINT; macOS and Windows-WSL
honour the same shape per Tokio's documented portability. The
`gate-7-aperture-no-telemetry` net-ns test is `#[cfg(target_os =
"linux")]` and skips with a clear message on non-Linux; the matrix
question is therefore moot for that gate.

If a Windows-native deployment scenario emerges (e.g. an operator
running Aperture as a Windows service), a `windows-latest` matrix
arm becomes worth adding. None has emerged.

### B6 — Local quality gates: keep the harness DEVOPS's hooks; document graduation

**Decision**: continue with `scripts/hooks/{pre-commit,pre-push}` as
the local quality gates. The `pre-commit` hook's `--exclude aperture`
is the only Aperture-specific change today (added in the today's
hook commit, see harness DEVOPS post-merge correction "local
pre-commit and pre-push hooks"). DELIVER's last commit removes the
exclusion.

The hooks remain the "mirror, not duplicate" implementation of the
CI commit-stage gates per the `cicd-and-deployment` skill.

---

## Risk register

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| The `--exclude aperture` graduation is forgotten at DELIVER close, leaving aperture's tests un-CI'd indefinitely | Low | Medium | Documented in three places (this file A2, `ci-cd-pipeline.md`, the comment in `ci.yml` itself); peer review at DELIVER close enforces. |
| The three Aperture-specific gates (architectural-rules, no-telemetry, probe-gold) are forgotten at the slices that should wire them | Medium | High | Documented in `ci-cd-pipeline.md`'s wiring schedule (table A5 above); each slice's commit message convention should reference its corresponding gate; the DELIVER agent's per-slice plan will reach for this document. |
| The Linux net-ns test in `gate-7-aperture-no-telemetry` is flaky on `ubuntu-latest` (kernel version drift, namespace permission edges) | Medium | Low | The test is named in DESIGN; DELIVER lands the fixture with `unshare(CLONE_NEWNET)`. If runner-edge issues emerge, fall-back is a process-level netfilter rule plus `lsof` inspection (alternative implementation; same contract). |
| Aperture's transitive deps (tonic, axum, hyper, tower, reqwest) introduce a duplicate-version cluster larger than the harness's existing cluster, eventually demanding a cargo-deny `[bans.skip-tree]` entry | Medium | Low | The `multiple-versions = "allow"` relaxation already covers this. If duplication grows enough to make `cargo update` painful, a per-package `skip-tree` entry is one-line; documented as a possible future deny.toml change. |
| KPI 5 / KPI 8 load tests produce flaky results on shared CI runners (timing-sensitive) | Medium | Medium | Deferred to release-cadence (per A8/Q5). Per-commit CI never runs the load tests; the release wave designs the runner posture (dedicated runner, larger machine, etc.) when the tests land. |
| Aperture's structured-stderr event vocabulary drifts from operator-dashboard queries silently | Low | High | Documented in `kpi-instrumentation.md` and `observability-design.md`; vocabulary is closed-set per DISCUSS D1; renames are version-bump-able. Future CI gate (when needed): a static check against `events.rs` cross-referencing operator-facing dashboard queries. Not yet a concern at v0. |
| Pilot operators report a pattern (e.g. wanting `/metrics` for Prometheus integration) that contradicts the no-telemetry-on-telemetry commitment | Medium | Low | DISCUSS Q6 explicit; Pulse (Phase 4) is the resolution; v0 documents the deliberate scope choice in `monitoring-alerting.md`. |

---

## Quality gate self-check

Per the `production-readiness` skill's "Quality gates for production
readiness" checklist, adapted for a service shipping at DEVOPS-wave
close (DELIVER not yet started, so most "production" items are
preconditions to DELIVER, not checks at DEVOPS close):

- [ ] All acceptance tests passing — **at DEVOPS close, every test is RED by DISTILL design**. The acceptance criterion this wave defends is: the test scaffold compiles cleanly and the deny gate is green. Both verified.
- [ ] Unit coverage meets project standard — **DELIVER concern**.
- [ ] Integration tests validated — **DELIVER concern, slice by slice**.
- [ ] Performance validated under realistic load — **deferred to release wave (KPIs 5, 8)**.
- [x] Security scan completed — `cargo deny check` green after the version-pin fix in A3.
- [x] Monitoring and alerting configured — to the extent v0 calls for it: documented as operator-side in `monitoring-alerting.md`. Aperture itself emits no alerts (DISCUSS Q6).
- [x] Logging structured and searchable — designed in ADR-0009; `observability-design.md` documents the operator pattern.
- [ ] Rollback procedure documented and tested — **N/A at v0 in the conventional sense**: Aperture is shipped as a binary; rollback is operator-side ("re-deploy the previous binary version under your orchestrator"); rolling deployments work naturally per DISCUSS D8 + ADR-0009. Documented in `platform-architecture.md > Rollback posture`.
- [x] Runbook for operational procedures — `observability-design.md` is the v0 runbook (start, stop, drain, what stderr means, how to interpret `/healthz` vs `/readyz`).
- [x] On-call team trained on new feature — Andrea is the sole maintainer; this wave's documents are the training material.

The four "deferred" or "N/A at v0" items each map to a runtime
concern Aperture v0 deliberately scopes out. None is unaddressed; each
has a documented home.

---

## Hand-off

This wave hands off to:

1. **`nw-platform-architect-reviewer` (Forge)** for peer review.
   Maximum 2 iterations. Expected scope: pipeline correctness,
   infrastructure soundness, deployment-readiness honesty,
   observability completeness, handoff completeness for DELIVER.

2. **`nw-software-crafter` (Crafty)** for DELIVER, after peer-review
   approval. DELIVER's per-slice schedule references this document's
   A2, A4, A5, A6 graduation points; DELIVER's last commit performs
   the four lockstep edits (Gate 1 → workspace; Gate 5 → multi-pkg;
   pre-commit hook un-excludes aperture; release-cadence load-test
   stubs left for future wave).

No new ADR is created. Every load-bearing decision in this wave is a
restatement of an existing ADR (0005, 0006, 0009, 0010), an
inheritance from the harness's DEVOPS wave, or an orchestrator-pre-
resolved configuration. The peer review reads this `wave-decisions.md`
as the authoritative record.

---

## DEVOPS wave summary

- 1 workflow file extended (`.github/workflows/ci.yml`) with comments
  documenting the three graduation points (Gate 1 scope, Gate 5
  scope, three Aperture-specific gates' wiring schedule).
- 0 new workflow files (single workflow per A1).
- 1 `crates/aperture/Cargo.toml` repair (add `version = "0.1.0"`
  to the harness path-dep entry per A3).
- 0 changes to `deny.toml` (Aperture's deps fit the existing policy).
- 0 changes to `scripts/hooks/{pre-commit,pre-push}` (graduation is
  DELIVER's last commit per A6).
- 0 changes to `rust-toolchain.toml` or `.cargo/config.toml`.
- 0 changes to root `CLAUDE.md` (per-feature mutation strategy
  unchanged; Aperture inherits).
- 8 nWave devops artefacts in `docs/feature/aperture/devops/`:
  this `wave-decisions.md`, `platform-architecture.md`,
  `ci-cd-pipeline.md`, `branching-strategy.md`,
  `kpi-instrumentation.md`, `observability-design.md`,
  `monitoring-alerting.md`, `environments.yaml`.
- 10 Apex-side decisions recorded (A1–A10).
- 5 DESIGN-handoff questions resolved (Q1–Q5).
- 6 Apex-side trivial-inheritance decisions (B1–B6).
- 0 new architectural ground; every load-bearing choice traces to an
  upstream artefact.

The project's CI surface now covers Aperture's static dependency
hygiene (Gate 4) and is wired to cover Aperture's full test surface
the moment DELIVER's last slice goes green. The three Aperture-
specific CI gates are documented for per-slice DELIVER wiring. The
operator-facing observability surface is fully named without
introducing Kaleidoscope-side telemetry infrastructure.

Vai.
</content>
</invoke>