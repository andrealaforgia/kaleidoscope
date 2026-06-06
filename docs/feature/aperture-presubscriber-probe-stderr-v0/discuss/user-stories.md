<!-- markdownlint-disable MD024 -->
# User Stories — aperture-presubscriber-probe-stderr-v0

## System Constraints (cross-cutting)

- **Fail-closed is UNCHANGED.** A probe refusal still exits non-zero and
  binds nothing. This story fixes ONLY the silence, not the decision.
- **Probe semantics UNCHANGED.** The Earned-Trust probe (ADR-0007) and
  the refusal it produces are not touched — only surfaced.
- **No regression on the post-init path.** A post-subscriber failure
  (drain deadline / serve-loop death, ADR-0066) still reports through
  `tracing` exactly as today.
- **No regression on the config-error pre-init line** (`main.rs:63-82`,
  `event=config_validation_failed`).
- **Solution-neutral.** DESIGN owns the mechanism (subscriber-earlier vs.
  direct-stderr). This story asserts the OBSERVABLE outcome, not how.
- Inherits ADR-0005's five gates; per-feature mutation 100% on modified
  files (`gate-5-mutants-aperture`). Rust idiomatic. Never 1.0.0.

---

## US-01: An operator sees why the gateway refused to start

### Elevator Pitch

- **Before**: Priya starts `aperture --config /etc/aperture/aperture.toml`
  against a collector that is not yet accepting OTLP. Aperture exits 1
  with NO stderr line — a silent failure. She cannot tell whether the
  config is wrong, the binary crashed, or the downstream is down.
- **After**: The same start prints a structured-shape stderr line naming
  the startup refusal, the sink, and the underlying probe error
  (`event=health.startup.refused`), then exits non-zero. The downstream
  identity and error are right there in the terminal / supervisor log.
- **Decision enabled**: Priya decides to fix the DOWNSTREAM (bring the
  collector up / correct its endpoint) instead of guessing — she stops
  wasting the first ten minutes of an incident bisecting her own config.

### Problem

Priya Nair is an SRE who runs aperture as the OTLP forwarding gateway in
front of her collector fleet. When she rolls out aperture before the
downstream collector is accepting telemetry (a routine ordering during
deploys and DR drills), aperture's Earned-Trust probe correctly refuses
to start — but it does so SILENTLY: the refusal is emitted through
`tracing` at a point in `run()` where the subscriber is not yet installed,
so the event is dropped and the process just exits 1. She finds it
maddening to triage a gateway that "won't come up" with zero output: she
has to attach a debugger or re-run under strace to discover the cause was
simply a downstream that was not ready. The config-error case already
prints a helpful pre-init line; the probe-refusal case does not.

### Who

- **Priya Nair**, SRE | runs aperture as a systemd/k8s-supervised OTLP
  gateway | wants to read the WHY of a startup failure from stderr /
  `journalctl` without attaching tooling.
- Secondary: **a supervisor / black-box harness** (Bea's A19/A20 compose
  harness) | consumes the stderr line to assert the refusal carries an
  operator-visible reason.

### Solution

When aperture refuses to start because the sink's Earned-Trust probe
failed (the configured downstream is not accepting telemetry), it emits an
operator-visible, structured-shape stderr line naming the startup refusal
(`event=health.startup.refused`), the sink, and the underlying error —
mirroring the existing pre-init config-error precedent at `main.rs:63-82`
— and still exits non-zero, binding nothing. The mechanism (install the
subscriber earlier, or write directly to stderr for the pre-subscriber
window) is DESIGN's choice; the observable line and the unchanged
fail-closed behaviour are the requirement.

### Domain Examples

#### 1: Happy-refusal (the verifier's exact scenario) — collector not up yet

Priya runs `aperture --config /etc/aperture/aperture.toml` (sink_kind =
forwarding, endpoint `http://otelcol-sink:4318`) during a deploy where the
collector pod has not become ready. Today: aperture exits 1, stderr is
empty. After: stderr carries one line —
`aperture: ... event=health.startup.refused reason: sink probe failed:
<probe error against http://otelcol-sink:4318>` — and aperture exits
non-zero. Priya reads it, sees the downstream is the problem, and waits
for / fixes the collector.

#### 2: Boundary — downstream returns the catalogued v0 substrate lie

The collector is up but lies: 200 on OPTIONS preflight, 503 on POST (the
documented probe_gold scenario). The probe returns `Refused`. After:
aperture prints the `event=health.startup.refused` line naming that
underlying error and exits non-zero — Priya learns the downstream is
reachable but rejecting telemetry, a different fix than "bring it up".

#### 3: Negative control — healthy downstream and config error are unchanged

(a) Collector healthy: `aperture --config …` starts normally, binds, emits
its usual `event=startup` info line, and prints NO refusal line. (b) Bad
config (`tls.enabled=true`, ADR-0061): aperture still prints its existing
`aperture: config error: event=config_validation_failed reason: …` line
and exits 2 — UNCHANGED by this story.

### UAT Scenarios (BDD)

```gherkin
Scenario: A probe refusal emits a structured stderr line
  Given Priya starts aperture configured to forward to a downstream that is not accepting telemetry
  When aperture's Earned-Trust probe refuses the start
  Then aperture prints a structured-shape line to stderr carrying event "health.startup.refused"
  And the process exits non-zero
```

```gherkin
Scenario: The refusal line names the sink and the underlying error
  Given Priya's configured downstream "http://otelcol-sink:4318" is not accepting telemetry
  When aperture refuses to start because the sink probe failed
  Then the stderr line identifies the sink that was probed
  And the stderr line carries the underlying probe error text
```

```gherkin
Scenario: Fail-closed exit is unchanged
  Given aperture refuses to start because the sink probe failed
  When the process terminates
  Then aperture has bound no listener
  And aperture exits with a non-zero status (the refusal still fails closed)
```

```gherkin
Scenario: Healthy-downstream and config-error paths are unchanged
  Given a healthy downstream that accepts telemetry
  When Priya starts aperture
  Then aperture starts normally and prints no startup-refusal line
  And given instead a config that sets a forward-compat security knob
  When Priya starts aperture
  Then aperture still prints its existing "event=config_validation_failed" pre-init line and exits 2
```

> Scenario titles describe WHAT Priya observes, not HOW the subscriber is
> wired. The mechanism (subscriber-earlier vs. direct-stderr) is DESIGN's.

### Acceptance Criteria

- [ ] **a-probe-refusal-emits-a-structured-stderr-line** — when the sink's
  Earned-Trust probe refuses the start, aperture prints a structured-shape
  stderr line carrying `event=health.startup.refused` (no longer silent).
- [ ] **the-line-names-the-sink-and-the-error** — the line identifies the
  probed sink (downstream identity) and carries the underlying probe error
  text (the `{e}` from `sink probe failed: {e}`).
- [ ] **fail-closed-exit-is-unchanged** — aperture still binds no listener
  and exits non-zero on probe refusal; the decision is untouched.
- [ ] **healthy-downstream-and-config-error-paths-unchanged** — a healthy
  downstream starts normally with no refusal line; a config error still
  emits the existing `event=config_validation_failed` pre-init line and
  exit 2.

### Outcome KPIs

- **Who**: SREs / operators triaging an aperture that will not start.
- **Does what**: identify the cause of a startup refusal from stderr alone
  (downstream-not-accepting) instead of attaching a debugger / strace.
- **By how much**: probe-refusal starts emit an operator-visible reason
  line in 100% of cases (from 0% today); zero silent exit-1 startups.
- **Measured by**: black-box assertion on the binary's stderr for a
  down-downstream start (Bea's A19/A20 evidence widening); the swallowed-
  errors family audit shows aperture startup has no remaining silent exits.
- **Baseline**: today a probe-refusal start produces 0 stderr lines.

### Technical Notes

- DESIGN decides the mechanism: (a) install the tracing subscriber before
  `wire_sink` so the existing `health.startup.refused` event flows through
  the normal path, or (b) write a structured-shape line directly to stderr
  for the pre-subscriber window (mirroring `main.rs:63-82`). See
  wave-decisions.md "Decisions FLAGGED for DESIGN".
- Verified loci: `run()` ordering `lib.rs:222-224`; `probe_or_refuse`
  `compose.rs:96-104`; subscriber install `compose.rs:134`; main Err
  handling `main.rs:54-60`; pre-init precedent `main.rs:63-82`; event
  vocabulary `observability.rs:49-50`.
- Note the double-probe nuance: `Forwarding` is probed in both `wire_sink`
  (pre-subscriber) and `spawn_with_readiness` (post-subscriber). DESIGN
  may rationalise this; not required by this story's AC.
- Constraints: fail-closed unchanged; post-init tracing path no regression;
  config-error line no regression; secrets out of scope; ADR-0005 five
  gates; per-feature mutation 100% on modified files; never 1.0.0.
- Confirm `tests/probe_gold_runner.rs` and the compose probe tests stay
  green; add a binary-start assertion for the new stderr line.

### Dependencies

- None blocking. Builds on the existing probe (ADR-0007), the existing
  config-error pre-init precedent (main.rs:63-82), and `gate-5-mutants-aperture`.
- Downstream: Bea Verifier widens A19/A20 evidence once this lands.
