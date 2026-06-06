# Wave Decisions — aperture-presubscriber-probe-stderr-v0

DISCUSS wave (nWave). Owner: Luna (nw-product-owner). Date: 2026-06-07.

## Origin

Bea Verifier flagged it (msg 038, while migrating her compose harness):
aperture's forwarding-sink Earned-Trust probe runs as the FIRST step of
`run()`, BEFORE the tracing subscriber is installed, so when the
downstream is not yet accepting OTLP, aperture exits 1 SILENTLY (the
probe error has no subscriber to log through). She named it "the small
honesty gap" and committed to widening her A19/A20 evidence to confirm
the probe failure now carries an operator line once it lands. It belongs
to the swallowed-errors family (sibling of the serve-loop + cinder/sluice
swallow fixes).

## DIVERGE artifacts

ABSENT. No `docs/feature/aperture-presubscriber-probe-stderr-v0/diverge/`
directory. This is a small, well-bounded honesty fix flagged directly
from a verifier observation against existing code, not a divergence
exploration. JTBD grounded directly from the operator job below.

RISK (noted, accepted): no DIVERGE means the job statement was authored
in DISCUSS rather than validated upstream. Mitigated by the gap being
mechanically verifiable in source (loci below all confirmed) and by a
single, narrow operator job with an unambiguous before/after.

## Verified code loci (read in source, 2026-06-07)

| Locus | File:line | What it shows |
|-------|-----------|---------------|
| `run()` ordering | `crates/aperture/src/lib.rs:222-224` | `wire_sink(&config)` (223) runs BEFORE `spawn_with_readiness(config, sink)` (224). |
| Subscriber install | `crates/aperture/src/compose.rs:134` | `observability::install_subscriber()` is the FIRST line of `spawn_with_readiness` — i.e. AFTER `wire_sink` already ran. |
| Probe refusal | `crates/aperture/src/compose.rs:96-104` | `probe_or_refuse` emits `tracing::error!(event=health.startup.refused, reason=%e)` then returns `Err(ApertureError("sink probe failed: {e}"))`. When called from `wire_sink` this tracing event is DROPPED — no subscriber yet. |
| main Err handling | `crates/aperture/src/main.rs:54-60` | On `run()` Err, `tracing::error!(error=%e, "aperture exited with error")` (57) — ALSO dropped if the failure was pre-subscriber `wire_sink`. Returns `ExitCode::FAILURE` (exit 1). |
| Pre-init precedent | `crates/aperture/src/main.rs:63-82` | `emit_config_error` writes a structured-shape line DIRECTLY to stderr (`eprintln!`) for the pre-init config window: `aperture: config error: event=config_validation_failed reason: {error}`. The "tracing is the only stderr path" rule is documented as POST-init only. The direct-stderr precedent for pre-subscriber failures ALREADY EXISTS for config errors but NOT for the probe refusal. |
| Event vocabulary | `crates/aperture/src/observability.rs:49-50` | `HEALTH_STARTUP_REFUSED = "health.startup.refused"`, `CONFIG_VALIDATION_FAILED = "config_validation_failed"` — the binary cannot reach the crate-private constant and re-states the literal (see main.rs:72 comment). |

### Additional finding for DESIGN (a double-probe nuance)

The `Forwarding` sink is probed TWICE on the production path:

1. `wire_sink` (lib.rs:223) → `probe_or_refuse` — PRE-subscriber (silent).
2. `spawn_with_readiness` (compose.rs:157-167) → `probe_or_refuse` again — POST-subscriber (this one WOULD log).

Because step 1 runs first and fails fast, the post-subscriber probe at
step 2 is never reached when the downstream is down. So the OBSERVABLE
outcome today is the silent step-1 failure. DESIGN should note the
redundant probe and decide whether the fix also rationalises it (e.g.
removing the pre-subscriber probe from `wire_sink` and relying on the
post-subscriber one is one candidate mechanism — see Decision 1 option (a)).

## Decisions taken in DISCUSS (autonomous)

| # | Decision | Value |
|---|----------|-------|
| D1 | Feature Type | Backend (gateway startup observability / honesty) |
| D2 | Walking Skeleton | No — brownfield change to existing startup path |
| D3 | UX research depth | Lightweight (operator reading startup stderr) |
| D4 | JTBD | the "see why the gateway refused to start" job |
| D5 | Slicing | ONE thin slice (the pre-subscriber probe-refusal stderr line) |
| D6 | Fix direction | Surface the silence only; refusal decision and fail-closed exit UNCHANGED |

### Carpaccio taste tests (one slice is right)

- Single user outcome (operator sees WHY the gateway refused). PASS as one slice.
- 0 bounded contexts crossed beyond aperture's own startup path. PASS.
- Walking skeleton not applicable (brownfield); slice is itself end-to-end (binary start → stderr line → non-zero exit). PASS.
- 4 testable AC, well under the 7-scenario ceiling; ~1 day. PASS.
- Splitting further would produce fragments with no independent operator value. Do NOT split.

## Decisions FLAGGED for DESIGN (DESIGN owns the mechanism)

The fix DIRECTION is decided (surface the silence; do not change
semantics). The MECHANISM is DESIGN's call. Four flagged decisions:

1. **Mechanism — which path carries the line.**
   - (a) Install the tracing subscriber EARLIER (before `wire_sink`, e.g.
     move `install_subscriber()` ahead of the probe / into `run()` or
     `main`), so the probe's `health.startup.refused` event and main's
     error line are captured through the normal post-init path. More
     invasive; unifies the path; would also resolve the double-probe
     nuance if `wire_sink`'s redundant probe is then dropped in favour of
     the post-subscriber one.
   - (b) The probe-refusal path / `main.rs` writes a structured-shape line
     DIRECTLY to stderr (mirroring the EXISTING config-error precedent at
     main.rs:63-82) for the pre-subscriber window. Less invasive; reuses a
     pattern that already exists and is already tested; keeps the
     "tracing-is-the-only-stderr-path post-init" rule intact.
   - DESIGN weighs invasiveness vs. path unification and decides. The
     direct-stderr precedent already exists for config errors, which lowers
     the cost of (b); moving the subscriber earlier is cleaner long-term
     but touches the idempotent `install_subscriber` ordering and the
     ADR-0066 serve-failure seam.

2. **Exact event / line shape.** Reuse the `health.startup.refused` event
   vocabulary plus the sink identity (which downstream / sink kind) plus
   the underlying error (`{e}` from `sink probe failed: {e}`). Mirror the
   config-error line shape: `aperture: ... event=health.startup.refused
   reason: {…}`. DESIGN fixes the precise tokens and whether the sink
   endpoint is named.

3. **main.rs run()-Err discrimination.** Decide whether `main.rs` must
   distinguish PRE-subscriber failures (a probe refusal at `wire_sink`)
   from POST-subscriber failures (a drain / serve-loop failure, already
   covered by tracing). A probe failure is pre-subscriber; a serve failure
   is post-subscriber. Under mechanism (a) the discrimination may become
   unnecessary; under (b) main.rs needs to know which window it is in.

4. **Test impact.** Confirm the existing probe-refusal tests still pass
   (`tests/probe_gold_runner.rs` + the compose probe tests) and that the
   new behaviour is what a startup test asserts (a black-box assertion on
   the stderr line for a down-downstream start). The `probe_gold_runner`
   enters at the probe surface directly and should be unaffected; the new
   assertion is at the binary-start surface.

## Constraints to carry into DESIGN

- Fail-closed is UNCHANGED: aperture still exits non-zero and binds
  nothing on probe refusal. Only the SILENCE is fixed.
- Do NOT change the probe semantics or the refusal decision — only surface it.
- The POST-subscriber path (normal tracing-to-stderr) must NOT regress: a
  post-init error (drain / serve-loop) still goes through tracing.
- The config-error pre-init line (main.rs:63-82) must NOT regress.
- Secret / no-token rules are NOT in scope.
- Inherits ADR-0005's five gates; per-feature mutation 100% on modified
  files; `gate-5-mutants-aperture` exists.
- Rust idiomatic (data + free functions + traits; no inheritance, no
  needless `dyn`). NEVER bump to 1.0.0.
- No public-API / semver concern expected: aperture is not in Gate 2/3;
  this is internal startup behaviour.

## SSOT references read

- `docs/product/architecture/brief.md` — Earned-Trust probe posture
  (Principle 12), fail-closed precedent (ADR-0061 / ADR-0042 query-api).
- ADR-0007 (Probe trait, separate from OtlpSink).
- ADR-0066 (serve-loop death post-bind, exit 3) — the post-subscriber
  swallow-fix sibling.
- CLAUDE.md (Rust idiomatic; per-feature mutation 100%).

## Risks

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| No DIVERGE job validation | Low | Low | Gap mechanically verified in source; single narrow job. |
| Mechanism (a) disturbs `install_subscriber` ordering / ADR-0066 seam | Med | Med | DESIGN owns the choice; (b) is the lower-risk fallback with an existing precedent. |
| Double-probe rationalisation creeps scope | Low | Med | Flagged as a DESIGN note, not a DISCUSS requirement; the slice's AC do not require removing the redundant probe. |
| New startup test flakes (network-down timing) | Low | Low | Assert on the stderr line + non-zero exit, not on timing; reuse the down-downstream fixture pattern. |
