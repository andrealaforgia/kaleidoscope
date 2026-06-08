# Wave Decisions — aperture-body-size-cap-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-07
- **Mode**: Autonomous overnight run. **SLIM** wave — an INTERNAL,
  single-crate change to the EXISTING live `aperture` crate; NO new crate, NO
  new dependency, NO new event constant, NO deploy surface, NO new
  infrastructure. There IS one PUBLIC-ADDITIVE surface delta (a new
  ConfigBuilder setter) — recorded honestly below; it is additive only,
  semver-MINOR pre-1.0, NEVER 1.0.0.
- **Inputs read**: `design/wave-decisions.md` (DD1-DD5 resolved; the honest
  protection-strength envelope DD1a; the Reuse Analysis = REUSE/MIRROR with
  four justified CREATE-NEW internal items),
  `docs/product/architecture/adr-0073-aperture-body-size-cap.md`,
  `design/upstream-changes.md` (the honest-strength AC refinements),
  `discuss/outcome-kpis.md` (KPI-1..6 + the DEVOPS instrumentation handoff),
  ADR-0005 (the five workspace gates), ADR-0072 (the local hook now runs the
  fast unit subset; the deep Gate 1 + Gate 5 gate in CI),
  `.github/workflows/ci.yml`, `CLAUDE.md` (per-feature 100% mutation; CI
  watch), and the prior slim-DEVOPS shape
  (`aperture-serve-loop-error-surfacing-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | - (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | DD1 transport-boundary placement (the strong guard) + app.rs secondary; DD1a the honest protection-strength table (per-arm strength + disclosed residual); DD2 single collapsed `max_recv_msg_size` cap mirroring the concurrency precedent; DD3 honest `size` field; DD4 metrics IN-scope; DD5 413 / RESOURCE_EXHAUSTED; the Reuse table (REUSE BODY_TOO_LARGE constant + concurrency-cap shape; four justified internal CREATE-NEW items) — all consumed | - DESIGN's "Net new surface (all INTERNAL)" line under-states ONE point: the mirrored builder setter is `pub fn` (config/mod.rs:315 precedent), so this feature DOES add a public-additive method. Recorded honestly below (does not change the INTERNAL-behaviour verdict; Gate 2/3 still do not fire). |
| ADR-0073 | the D2 transport-boundary placement + 5 alternatives (A accepted, B/C/D/E rejected); the honest protection-strength envelope; the disclosed gRPC `size`-fidelity consequence; the disclosed rejection-counter deferral (DD4) | - none; ADR-0073 already discloses the counter deferral and the size-fidelity trade as Consequences, not silent gaps |
| `design/upstream-changes.md` | the AC-wording refinement to the honest strength ("before the full body is buffered/decoded", with the stronger "before any byte" reserved for HTTP Content-Length-present); the `size` = observed-value refinement (DD3) | - none; these are wording refinements for DISTILL, fully met by the placement |
| `discuss/outcome-kpis.md` | KPI-1 (enforced sites 0 -> configured paths), KPI-2 (one body_too_large event/rejection naming limit+size), KPI-3 (exact inclusive boundary), KPI-4 (zero unset behaviour change, guardrail), KPI-5 (both signals + metrics, D4), KPI-6 (100% mutation kill, guardrail); the explicit DEVOPS instrumentation handoff (event stream is the v0 surface; rate-per-tenant dashboard + alert + rejection counter are FUTURE) | - none; the "wire into fleet observability" + "rejection counter" notes are explicitly future, out-of-this-wave concerns |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | - Gate 2/Gate 3 enrolled for only 4 graduated packages; aperture is not among them (finding below) |
| ADR-0072 (local hook scope) | the local pre-commit hook now runs the FAST unit subset (`cargo test --workspace --lib --locked`) + Gate 4; the DEEP Gate 1 (`--all-targets`) and Gate 5 (mutants) gate in CI, watched via scripts/ci-watch.sh | - none; the cap tests must be deterministic so neither the fast hook nor CI flakes |
| `.github/workflows/ci.yml` | `gate-5-mutants-aperture` (:562-661) EXISTS, `--in-diff` path-filtered on `crates/aperture/**`, baseline cascade origin/main -> HEAD~1 -> full, 30-min timeout, --jobs 2; Gate 1 (:136-182); Gate 4 (:83); the COMMENTED service-gates 6/7/8 sketch | - Gate 2 (:354-407) and Gate 3 (:413) list ONLY otlp-conformance-harness, spark, sieve, codex; aperture explicitly not graduated (ci.yml:361-367); gates 6/7/8 are not-yet-wired sketches (both noted, neither a blocker) |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and NO CI-config change is made by
this wave.** The feature modifies existing source inside the single live crate
`aperture` (`src/config/mod.rs` — wire `max_recv_msg_size` through to `Config`
+ a public builder setter; `src/transport.rs` — the HTTP length-checked
body-read seam + the gRPC `max_decoding_message_size` codec-error event
surface; `src/app.rs` — the disclosed secondary early-return; `src/observability.rs`
— the `body_too_large` EMITTER for the already-existing constant). aperture
already owns a path-filtered `gate-5-mutants-aperture --in-diff` job that
mutates exactly its changed lines automatically.

**Confirmed against the live source**: `max_recv_msg_size` is parsed and
ignored today (`config/mod.rs:474-485`, `#[allow(dead_code)]`); the
`BODY_TOO_LARGE` constant exists with no emitter (`observability.rs:46`); the
`max_concurrent_requests` precedent has a `pub fn` builder setter
(`config/mod.rs:315`) on the public `ConfigBuilder` (`:251`) — so DD2's
mirrored `max_recv_msg_size` setter is a PUBLIC-ADDITIVE method. aperture's
`Cargo.toml` is `version = "0.1.0"`.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production code, NO tests, and NO CI-config
changes exist yet for this feature. That absence is the EXPECTED and CORRECT
state — it is not a finding. This wave's job is to CONFIRM the existing
ADR-0005 CI contract covers the feature and to produce `environments.yaml` +
this file; review THAT, not the non-existence of code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers the change.

## Decision summary (D1-D9, all existing / inherited — brownfield, NOT a deploy)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | Internal change to a live library + binary. aperture is the operator-run / orchestrator-run OTLP gateway; Kaleidoscope deploys nothing. No deploy step added or required. |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface added. (The cap makes aperture a BETTER resource citizen — it refuses an OOM-class payload before allocation — but adds no orchestration artefact.) |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the modified aperture files automatically (Gate 5 via the existing `gate-5-mutants-aperture --in-diff` job). Gate 2/Gate 3 do NOT cover aperture (finding below; the public-additive setter is therefore not machine-locked, but is additive-only). No new gate. |
| D5 | Observability | **Existing convention — one warn-level structured STDERR `body_too_large` event** | The feature ADDS the EMITTER for the already-existing `BODY_TOO_LARGE` constant (observability.rs:46; NO new constant, C5), in the concurrency_cap_hit JSON shape (backpressure.rs:116-122; ADR-0009 closed vocabulary). Fields: transport / signal / limit (exact configured cap) / size (the value the rejection surface truthfully observed, DD3). The cap covers logs, traces, AND metrics (DD4). No new metric, no new dashboard, no new stack. The rejection COUNTER and the fleet rate-per-tenant dashboard + alert (outcome-kpis.md DEVOPS handoff) are DISCLOSED-DEFERRED to a future `aperture-body-too-large-metric-v0`, NOT this wave. |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; aperture is stateless (no WAL/snapshot/on-disk format), wire/probe contracts unchanged, the deltas (413 / RESOURCE_EXHAUSTED on an over-limit body, the body_too_large event, the new config knob) are additive, and an unset gateway sees zero change (KPI-4) — so a revert is clean with no data or wire-format consideration. |
| D7 | Continuous learning | **N/A** | No live telemetry loop; the KPIs are in-suite falsifiability + 100% mutation-kill (the K6 raw-observation idiom). |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5 / CLAUDE.md)** | Already pinned in CLAUDE.md. Mutation scope = the modified aperture files (`config/mod.rs`, `transport.rs`, `app.rs`, `observability.rs`). The two load-bearing mutants are the boundary comparison (`>`/`>=`, KPI-3) and the unset-no-cap `None` early-return (the mutant that imposes a cap when unset, KPI-4). Covered by the existing `gate-5-mutants-aperture --in-diff` job. **No CLAUDE.md change needed.** |

## CI Contract — confirmation and findings

### Gate 5 (mutants, 100% kill) — CONFIRMED EXISTS, no new job, ADDED NOTHING

**`gate-5-mutants-aperture` ALREADY EXISTS (ci.yml:562-661).** It was NOT
missing; this wave adds NO workflow change and makes NO commit. The job runs:

```
cargo mutants --package aperture --in-diff "$DIFF_FILE" --no-shuffle --jobs 2
```

against `git diff "$BASELINE" HEAD -- 'crates/aperture/**'` with the baseline
cascade origin/main -> HEAD~1 -> full (ci.yml:626-653), a 30-minute
timeout-minutes safety net, and the mutants.out artefact upload.

| Touched path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|--------------|------------------------|---------------------|-------------|----------|
| `crates/aperture/src/config/mod.rs` | `Config.max_recv_msg_size: Option<u32>` field + `pub(crate)` accessor + `pub fn` builder setter + `into_config` honour-gRPC arm + the `0`-means-no-cap normalisation | `gate-5-mutants-aperture` | 562-661 | OK `--in-diff` on `crates/aperture/**` |
| `crates/aperture/src/transport.rs` | the HTTP length-checked body-read seam (Content-Length-first + streamed backstop); the gRPC `max_decoding_message_size(cap)` per service + the codec-error event surface; the boundary comparison | `gate-5-mutants-aperture` | 562-661 | OK same job |
| `crates/aperture/src/app.rs` | the disclosed `&[u8]` secondary early-return at the top of each `ingest_*`, BEFORE `validate_*` (C12 single-validator invariant) | `gate-5-mutants-aperture` | 562-661 | OK same job |
| `crates/aperture/src/observability.rs` | the `body_too_large` EMITTER + the shared event-constructor (NO new constant — BODY_TOO_LARGE exists at :46) | `gate-5-mutants-aperture` | 562-661 | OK same job |

The `--in-diff` filter means the job mutates ONLY the lines this feature
changes — a mutant flipping `>` to `>=` on the boundary (must be killed by the
at-limit-accept + at-limit-plus-one-reject tests, KPI-3), deleting the `None`
no-cap early-return (must be killed by the unset-no-cap negative control,
KPI-4), weakening the size check, or dropping the `body_too_large` emit (must
be killed by the one-event-per-rejection assertion, KPI-2) — must be killed at
100% (KPI-6). **No per-feature wiring, no new gate-5 job.** aperture was
already enrolled in the per-crate `--in-diff` model (the close of
`gate-5-mutants-batch-v0`), so this feature inherits gating for free.

### Gate 1 (test) + Gate 4 (deny) — CONFIRMED unchanged, COVER this feature

- **Gate 1 (`cargo test --workspace --all-targets --locked`, ci.yml:136-182)**
  runs the body-size-cap acceptance tests (DISTILL authors US-01/02/03 +
  the unset/at-limit negative controls; DELIVER turns them green) plus the
  existing slice-01..05 aperture acceptance guardrail suites (KPI-4),
  identically in the local pre-commit hook's deep-suite-in-CI split (ADR-0072)
  and in CI. No change.
- **Gate 4 (`cargo deny`, ci.yml:83)** — no new dependency is introduced
  (DD-Reuse: the feature REUSES the axum `DefaultBodyLimit` primitive wrapped
  in the custom seam + tonic `max_decoding_message_size` + `tracing`; the pins
  are untouched), so Gate 4 is a no-op confirmation.

### Gate 2 (public-api) + Gate 3 (semver) — CONFIRMED: do NOT fire; the public-additive setter is NOT machine-locked; additive-only, NO break, NO bump

This is the honest CI-coverage point that DIFFERS from the serve-loop sibling.

1. **This feature DOES add a public method.** DD2 mirrors `max_concurrent_requests`,
   whose builder setter is `pub fn max_concurrent_requests(mut self, cap: u32)`
   (config/mod.rs:315) on the PUBLIC `ConfigBuilder` (config/mod.rs:251), itself
   reachable via the public `Config::builder()` (`:120`) in the `pub mod config`
   (lib.rs:35). So DD2's `pub fn max_recv_msg_size(u32)` is a PUBLIC-ADDITIVE
   surface change. (The `Config` field and accessor are `pub(crate)` — only the
   builder setter is public.) DESIGN's "all INTERNAL" line is true of the
   BEHAVIOUR and of three of the four CREATE-NEW items, but the setter is
   public; this wave records that honestly rather than echoing "all internal".
2. **aperture is NOT enrolled in Gate 2/Gate 3.** Gate 2 (`cargo public-api`,
   ci.yml:354-407) and Gate 3 (`cargo semver-checks`, ci.yml:413) are enrolled
   for ONLY the four graduated packages — otlp-conformance-harness, spark,
   sieve, codex (ci.yml:361-367 documents the deliberate omission: aperture's
   only library surface is the dev-only `aperture::testing` seam, ADR-0007;
   locking it would invite churn without consumer value). The pre-push hook
   mirrors exactly that set. **So the new public setter is NOT machine-locked by
   any gate.**
3. **It is additive-only, so there is no break to flag anyway.** Adding a
   `pub fn` is, by SemVer, a MINOR addition, never a break. Even WERE aperture
   enrolled, Gate 2 would report an ADDED item and Gate 3 a compatible MINOR
   change, never a removal/change/break. **Therefore aperture stays `0.1.0`;
   DELIVER must NOT bump `crates/aperture/Cargo.toml`** (additive pre-1.0 needs
   no bump under this project's convention, and 1.0.0 is NEVER autonomous —
   Andrea's call). CONTRAST with cinder-wal-error-surfacing-v0 (a genuine trait
   break that DID need a manual 0.1.0 -> 0.2.0 bump); this feature does NOT.
4. **Decision: do NOT enrol aperture into Gate 2/Gate 3 in this wave.**
   Graduating a crate into the public-surface lock is a separate, deliberate
   decision (as it was for spark/sieve/codex). aperture's public config surface
   growing one additive setter does not, by itself, warrant graduation. Flagged
   honestly, not actioned. (If Andrea later wants the config surface
   machine-locked, that is its own DESIGN/DEVOPS decision.)

### Gates 6/7/8 (aperture service-specific) — NOTED, not perturbed, not wired by this wave

aperture has three SERVICE-specific gates SKETCHED (commented) in ci.yml:
`gate-6-aperture-architectural-rules` (xtask AST walks: single-validator-per-signal,
hexagonal layer direction, no `prost::Message::decode` in `crates/aperture/src/`),
`gate-7-aperture-no-telemetry`, and `gate-8-aperture-probe-gold`. They are
COMMENTED sketches to be wired by aperture DELIVER Slices 03/06, NOT live jobs
today. This feature adds NO new validator (the app.rs secondary is an early
return BEFORE `validate_*`, honouring the single-validator-per-signal invariant,
C12), NO outbound traffic, NO probe behaviour change, NO hexagonal
layer-direction change, and crucially NO `prost::Message::decode` in src (the
strong guard refuses BEFORE decode — the OOM-prevention is precisely that the
oversized frame is never decoded). So even once those gates are wired they are
NOT perturbed. **No action; not a delta this wave introduces.**

## KPI Instrumentation Confirmation (the outcome-kpis.md DEVOPS handoff)

| KPI | What it measures | Carried by the EXISTING surface? | DELIVER-owed wiring |
|---|---|---|---|
| KPI-1 enforced sites 0 -> configured | over-limit body rejected before harness decode | the rejection itself is new behaviour DELIVER writes; the test surface (in-process HTTP/gRPC drive + recording sink) EXISTS | DELIVER: the transport-boundary size check (HTTP seam + gRPC max_decoding_message_size) |
| KPI-2 one body_too_large event/rejection, naming limit+size | the operator-facing signal | the CHANNEL (warn-level structured stderr, ADR-0009 closed vocab), the CONSTANT (BODY_TOO_LARGE, observability.rs:46), and the SHAPE precedent (concurrency_cap_hit, backpressure.rs:116-122) all EXIST | DELIVER: the EMITTER + the shared event-constructor building the transport/signal/limit/size fields once (DD3 honest size) |
| KPI-3 exact inclusive boundary | at-limit accepted, at-limit-plus-one rejected | the test surface EXISTS; nothing to instrument beyond the check | DELIVER: the inclusive comparison; Gate 5 kills the `>`/`>=` mutant |
| KPI-4 zero unset behaviour change (guardrail) | unset gateway unchanged | FULLY carried — it is the EXISTING slice-01..05 acceptance suites staying green + the new unset negative control | DELIVER: the `None` no-cap early-return; Gate 5 kills the mutant that imposes a cap when unset |
| KPI-5 both signals + metrics (D4) | logs + traces + metrics all guarded | the per-signal `signal` field shape EXISTS (CapTransport / signal vocabulary) | DELIVER: wire the cap on all three routes + all three gRPC services |
| KPI-6 100% mutation kill (guardrail) | correctness pin | FULLY carried by the EXISTING gate-5-mutants-aperture --in-diff job (ci.yml:562-661) | DELIVER: write the tests that kill the boundary + unset + emit mutants |

**Summary of the instrumentation confirmation:** the structured-logging
CHANNEL, the JSON event SHAPE, the BODY_TOO_LARGE CONSTANT, the per-signal
field vocabulary, the in-process test-drive surface, and the Gate 5 mutation
job ALL ALREADY EXIST. The v0 observability surface is the `body_too_large`
event stream on aperture stderr; the existing fleet scrape already ingests that
JSON shape, so NO new collection, dashboard, or alert is built in this wave.
What DELIVER OWES is purely the EMITTER (firing the existing constant with the
DD3-honest size) plus the rejection behaviour the event reports on. The
rejection COUNTER (per tenant/signal) and the fleet rate-per-tenant dashboard +
alert named in the outcome-kpis.md DEVOPS handoff are DISCLOSED-DEFERRED to a
future `aperture-body-too-large-metric-v0` (DD4) — disclosed here and in
ADR-0073 Consequences, NOT a silent gap.

## Infrastructure Summary

- **New infrastructure**: none. No crate, no container, no service, no cloud
  resource, no IaC, no orchestration, no new dependency.
- **CI changes**: NONE. The five ADR-0005 gates are inherited unchanged; the
  single relevant Gate 5 job (`gate-5-mutants-aperture`, ci.yml:562-661)
  already path-filters `--in-diff` onto all modified aperture files. No new
  job, no edit to any existing job, NO commit made by this wave.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for an
  internal single-crate change, NOT deploy targets. See `environments.yaml`.
- **Cap test environment**: in-process driven HTTP/gRPC requests above and
  below a cap set via the new `Config::builder().max_recv_msg_size(n)` setter,
  asserting reject/accept + status code + sink touched/untouched + the
  exactly-one event with honest limit/size/signal/transport — a TEST concern,
  no infra, no real OOM, no real network peer. Recorded in `environments.yaml >
  cap_test_environment`.
- **Observability**: one additive warn-level `body_too_large` stderr event
  (existing channel, existing constant, mirrored shape); no new metric (the
  counter is disclosed-deferred), no new dashboard, no new stack.
- **Rollback**: `git revert` (trunk-based); aperture is stateless, wire/probe
  contracts unchanged (deltas additive), unset gateways unchanged (KPI-4), so a
  revert is clean.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** `gate-5-mutants-aperture`
  ALREADY EXISTS (ci.yml:562-661) and covers all modified files via `--in-diff`.
  DELIVER must NOT add a per-feature gate-5 job.
- **C-DEVOPS-2 — NO public-API BREAK, NO semver bump; but ACKNOWLEDGE the
  public-additive setter.** This feature adds ONE public method
  (`ConfigBuilder::max_recv_msg_size`); the field + accessor are `pub(crate)`.
  It is ADDITIVE-only (semver-MINOR, pre-1.0), NOT a break. aperture is NOT
  enrolled in Gate 2/Gate 3, so the addition is NOT machine-locked — that is
  acceptable for an additive change but must be stated, not hidden. DELIVER must
  NOT bump `crates/aperture/Cargo.toml` (stays 0.1.0) and must NOT enrol
  aperture into Gate 2/Gate 3 speculatively. NEVER 1.0.0 — Andrea's call.
- **C-DEVOPS-3 — Cap tests must be deterministic and run in BOTH the local hook
  AND CI Gate 1.** In-process driven HTTP/gRPC requests + stderr
  presence/absence + status-code + size/limit-field assertions, NO wall-clock
  threshold — so neither the fast pre-commit subset (ADR-0072) nor CI flakes
  (the p95-flake class does NOT apply; these are boolean / status-code /
  structured-field assertions, not p95 latency).
- **C-DEVOPS-4 — Falsifiability is mandatory.** Each reject/boundary/event AC
  MUST fail on today's parsed-but-ignored `max_recv_msg_size` (over-limit body
  ACCEPTED, no event) and pass ONLY on the rejected-and-emitted fix. Do NOT
  inherit a too-large test that passes on the unwired knob. The unset control
  MUST assert no event + no reject on a body that WOULD be rejected under a set
  cap (KPI-4). Word the event-survival AC at the honest strength (DD1a /
  upstream-changes.md): "before the full body is buffered/decoded", with the
  stronger "before any body byte is read" reserved for the HTTP
  Content-Length-present case; `size` is the value the rejection surface
  observed (DD3), not a fabricated exact byte count.
- **C-DEVOPS-5 — Guardrails must stay green.** The existing slice-01..05
  aperture acceptance suites (the unset-no-cap guardrail, KPI-4) and every
  under-limit / at-limit accept must not regress; Gate 5 must reach 100% kill on
  the modified files, including the `>`/`>=` boundary mutant (KPI-3) and the
  unset `None` early-return mutant (KPI-4) — KPI-6.
- **C-DEVOPS-6 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy
  is already pinned (D9).
- **C-DEVOPS-7 — Gates 6/7/8 are not perturbed.** This feature adds no
  validator (the secondary is an early return BEFORE `validate_*`), no outbound
  traffic, no probe behaviour, no layer-direction change, and NO prost decode in
  src (it refuses before decode); DELIVER need do nothing for the sketched
  aperture service-gates.
- **C-DEVOPS-8 — Metrics counter is disclosed-deferred.** The v0 observability
  surface is the `body_too_large` event stream only. DELIVER must NOT add a
  rejection counter / metric / dashboard (DD4); that is a future
  `aperture-body-too-large-metric-v0`.

## Upstream Changes

**None from this wave.** DESIGN resolved DD1-DD5 and back-propagated the
honest-strength AC wording to DISTILL via `design/upstream-changes.md`; this
DEVOPS wave confirms the existing ADR-0005 CI contract covers the feature. The
ONE honest correction this wave makes to a prior-wave statement is internal to
DEVOPS's own confirmation, not a re-scope: DESIGN's "Net new surface (all
INTERNAL)" line is qualified here — the mirrored builder setter is `pub fn`, so
the feature adds a public-additive method (recorded in CI Contract > Gate 2/3
and C-DEVOPS-2). This does not weaken the feature, change any AC, or alter the
no-break / no-bump posture; it is an accuracy note so DISTILL/DELIVER do not
believe the public surface is untouched. No story re-scoping; no DISCUSS/DESIGN
artifact needs rewriting.

## Production Readiness (scoped to an internal, stateless-service change)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance criteria defined for the reject path (HTTP logs + gRPC traces),
      the inclusive boundary, the unset negative control, and all three signals
      (logs+traces+metrics) via the in-process drive seam + the new builder
      setter; DISTILL authors them, DELIVER turns them green (KPI-1/2/3/5).
- [x] Mutation gate (Gate 5, 100% kill) auto-covers all modified aperture files
      via the EXISTING `gate-5-mutants-aperture --in-diff` job, including the
      `>`/`>=` boundary mutant and the unset `None` mutant (KPI-6).
- [x] Operator signal surfaced on the existing channel (D5): exactly one
      warn-level `body_too_large` stderr event per rejection, naming
      transport/signal/limit/size (DD3 honest size).
- [x] No new event family / metric / dashboard / observability stack; the
      rejection counter + fleet dashboard are disclosed-deferred (DD4, KPI handoff).
- [x] Rollback posture: `git revert`; aperture is stateless, wire/probe
      contracts unchanged (deltas additive), unset gateways unchanged.
- [x] Public-API posture confirmed AND HONESTLY QUALIFIED: one public-additive
      setter, additive-only, NOT machine-locked (aperture not enrolled in Gate
      2/3), NO break, NO bump, aperture stays 0.1.0.
- [n/a] Canary / blue-green / rolling — no deployment surface.
- [n/a] On-call / runbook — operators run / orchestrate the binary; the
      `body_too_large` event + the 413 / RESOURCE_EXHAUSTED reject ARE the
      operator-facing signals (the very thing the feature adds). A fleet alert on
      the event rate is a future separate observability feature.

## Peer Review (self-review; reviewer not nested-invocable)

The `nw-platform-architect-reviewer` Agent could not be invoked as a nested
subagent from within this subagent context (the identical constraint was
recorded for the prior slim-DEVOPS features, e.g.
`aperture-serve-loop-error-surfacing-v0`). Per the established slim-DEVOPS
precedent on this project, a structured self-review was conducted against the
reviewer's critique dimensions.

| Dimension | Check | Verdict |
|---|---|---|
| CI coverage complete | All modified files (config/mod.rs, transport.rs, app.rs, observability.rs) mapped to the existing gates; Gate 5 `--in-diff` confirmed to cover them; the two load-bearing mutants (boundary, unset) named | PASS |
| gate-5-mutants-aperture present | Confirmed EXISTS at ci.yml:562-661 with `--package aperture --in-diff` on `crates/aperture/**`; NOT missing; nothing added; no commit | PASS |
| Environment inventory present | `environments.yaml` scoped to clean + with-pre-commit + ci (the standard internal-change matrix, slim precedent), plus the in-process cap_test_environment; deploy_surface=none justified | PASS |
| Observability aligned to KPIs | Every KPI (1-6) traced to either the existing surface (channel/constant/shape/test-drive/Gate 5) or the DELIVER-owed emitter; the counter deferral disclosed, not vacuous; size-field honesty (DD3) carried | PASS |
| No overstated readiness | The public-additive setter is recorded HONESTLY (not echoed as "all internal"); Gate 2/3 non-coverage of it is stated as a real fact, not hidden; the honest protection-strength wording (DD1a) carried into C-DEVOPS-4; the counter is deferred not claimed | PASS |
| DORA / trunk-based fit | Trunk-based, CI-as-feedback (not a gate) confirmed; deep Gate 1 + Gate 5 gate in CI per ADR-0072 with ci-watch.sh as the safety net; no DORA regression (no new gate slows lead time) | PASS |
| Handoff completeness | Eight constraints (C-DEVOPS-1..8) handed to DISTILL/DELIVER; the KPI instrumentation confirmation table is the DELIVER-owed-vs-existing ledger; no deploy/runbook overreach | PASS |
| Rollback designed | `git revert`, stateless, additive deltas, unset unchanged — clean; documented before any rollout (there is none) | PASS |

**Verdict: APPROVED (self-review), 0 blocking issues.** One honest accuracy
note carried (the public-additive ConfigBuilder setter, qualifying DESIGN's
"all INTERNAL" line) — disclosed in CI Contract > Gate 2/3 and C-DEVOPS-2, not
a silent gap. An independent top-level `nw-platform-architect-reviewer` run is
recommended before DISTILL.

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job; `gate-5-mutants-aperture`
  already exists and is untouched. NO commit is made by this wave (the
  orchestrator commits the docs between waves).
- Does not enrol aperture into Gate 2/Gate 3 (a separate graduation decision;
  flagged honestly given the new public-additive setter, not actioned).
- Does not wire the sketched aperture service-gates 6/7/8.
- Does not write production code or the cap / negative-control tests (crafter
  owns DELIVER; acceptance-designer owns the test specs in DISTILL).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not bump any `Cargo.toml` version (aperture stays 0.1.0 — additive only,
  NO break, NO bump, NEVER 1.0.0).
- Does not add the rejection counter / metric / dashboard (disclosed-deferred,
  DD4).
- Does not proceed into DISTILL.
