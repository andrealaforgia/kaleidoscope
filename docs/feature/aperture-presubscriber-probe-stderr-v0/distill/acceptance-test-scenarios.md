# Acceptance Test Scenarios — aperture-presubscriber-probe-stderr-v0

DISTILL wave. Owner: Quinn (nw-acceptance-designer). US-01 (4 AC).
Test file: `crates/aperture/tests/probe_refusal_visibility.rs`
([[test]] block added to `crates/aperture/Cargo.toml`).

## Driving port

The real `aperture --config <file>` BINARY (subprocess), observed black-box
through exit code, structured stderr, and ephemeral-port connect probes. No
internal aperture type reached (Mandate 1 — hexagonal boundary). Downstream is
a real `wiremock` liar/healthy HTTP server (the 200-OPTIONS / 503-POST
substrate lie from `probe_gold_runner.rs`).

## Scenario → test-fn → US/AC map

| # | Scenario (business outcome) | Test fn | US / AC | State | Tags |
|---|---|---|---|---|---|
| 1 | A probe refusal emits a structured `health.startup.refused` stderr line | `probe_refusal_emits_health_startup_refused_on_stderr` | US-01 / AC-1 a-probe-refusal-emits-a-structured-stderr-line | RED `#[ignore]` | @walking_skeleton @driving_port @real-io @adapter-integration |
| 2 | The refusal line names the probed sink + the underlying error | `probe_refusal_line_names_the_sink_and_the_underlying_error` | US-01 / AC-2 the-line-names-the-sink-and-the-error | RED `#[ignore]` | @driving_port @real-io @error-path |
| 3 | Fail-closed exit is unchanged AND the refusal is visible (both) | `probe_refusal_is_fail_closed_and_visible` | US-01 / AC-3 fail-closed-exit-is-unchanged | RED `#[ignore]` | @driving_port @real-io @error-path @infrastructure-failure |
| 4 | A healthy downstream starts, binds, prints no refusal line | `healthy_downstream_starts_binds_and_prints_no_refusal_line` | US-01 / AC-4(a) healthy-downstream-...-unchanged | GREEN | @driving_port @real-io @negative-control |
| 5 | A config error still prints its existing line and exits 2 | `config_error_still_prints_its_existing_line_and_exits_two` | US-01 / AC-4(b) ...-config-error-paths-unchanged | GREEN | @driving_port @negative-control @no-regression |
| — | Documents the shared RED ignore-reason | `red_reason_is_documented` | (suite intent) | GREEN | — |

All four AC of US-01 are covered (Dimension 8 Check A: no untraceable AC; no
orphan scenarios).

## Gherkin (business language — Mandate 2 / Dimension 3)

### Scenario 1 — @walking_skeleton @driving_port @real-io (RED)
```gherkin
Given Priya starts aperture configured to forward to a downstream that is not
      accepting telemetry
When aperture's Earned-Trust probe refuses the start
Then aperture prints a structured-shape line to stderr carrying event
     "health.startup.refused"
```

### Scenario 2 — @error-path (RED)
```gherkin
Given Priya's configured downstream is not accepting telemetry
When aperture refuses to start because the sink probe failed
Then the stderr line identifies the sink that was probed
And the stderr line carries the underlying probe error text
```

### Scenario 3 — @error-path @infrastructure-failure (RED)
```gherkin
Given aperture refuses to start because the sink probe failed
When the process terminates
Then aperture has bound no listener
And aperture exits with a non-zero status
And the refusal is visible on stderr as event "health.startup.refused"
```

### Scenario 4 — @negative-control (GREEN)
```gherkin
Given a healthy downstream that accepts telemetry
When Priya starts aperture
Then aperture starts normally and binds its listeners
And prints no startup-refusal line
```

### Scenario 5 — @negative-control @no-regression (GREEN)
```gherkin
Given a config whose mandatory ingest-auth block is omitted
When Priya starts aperture
Then aperture prints its existing "config_validation_failed" pre-init line
And exits 2
And binds no listener
```

## Walking-skeleton litmus (Dimension 5 / 9)

- **Title = user goal**: "A probe refusal emits a structured stderr line" —
  what Priya OBSERVES, not "probe path wiring". PASS.
- **Then = user observation**: stderr line present / exit code / no bind —
  all externally observable from a terminal, no internal side-effects. PASS.
- **Strategy declared** (wave-decisions.md): Strategy C, real-local-IO
  subprocess. PASS (Dim 9a).
- **No @in-memory under Strategy C**: there are zero in-memory doubles; the
  binary, downstream, config files, and TCP are all real. PASS (Dim 9b/9d/9e).
- **Adapter integration coverage** (Dim 9c): the only NEW driven surface this
  feature exercises (the forwarding-sink probe → downstream) is covered by a
  real-I/O scenario (liar `wiremock` over real TCP). The config-loader and
  listener-bind surfaces also use real I/O. PASS.

## Error-path ratio (Dimension 1)

Behavioural scenarios: 5 (excluding the documentation stub).
Error/refusal/negative-control scenarios: 1,2,3 (refusal paths) + 5
(config-error refusal) = 4 of 5 = **80%** error/edge coverage (>= 40%). PASS.

## Mandate-7 / RED-not-BROKEN self-review

- The file COMPILES and the subprocess SPAWNS today (no missing production
  symbol — the test reads observable subprocess output only). VERIFIED
  (clippy clean, default run executed).
- The three RED scenarios FAIL behaviourally on the ABSENT line
  (`exit=Some(1) stderr: ""`), not on a panic from a missing symbol. VERIFIED
  (`--ignored` run, all three FAILED with the "must carry
  event=health.startup.refused" message).
- The `exit=Some(1)` (not 2) proves the binary got PAST mandatory auth-config
  validation and reached the forwarding-sink probe. VERIFIED.

## Driving-adapter / falsifiable self-review checklist

- [x] Enters through the driving port only (the real binary), no internal type.
- [x] Each RED assertion genuinely fails today (proven-RED evidence captured).
- [x] The liar genuinely makes the probe refuse (200 OPTIONS / 503 POST → the
      probe POSTs the lie-detector and refuses, per probe_gold contract).
- [x] Fail-closed (no bind) asserted via connect-refused on both ephemeral
      ports.
- [x] Ephemeral ports (never 4317/4318); both child + liar reaped on every
      path; temp files removed; post-run no leak / no litter VERIFIED.
- [x] Negative controls GREEN today (healthy binds; config-error exits 2).
- [x] Business-language Gherkin; technical detail confined to step/helper
      layer (subprocess, stderr parsing) — Mandate 2 three-layer model.
- [x] All four US-01 AC traced to scenarios — Dimension 8 Check A.

## Run evidence

Default (`cargo test -p aperture --test probe_refusal_visibility`):
`3 passed; 0 failed; 3 ignored`.

`--ignored` (`-- --ignored --test-threads=1`):
`0 passed; 3 failed` — each on the absent `health.startup.refused` line.
