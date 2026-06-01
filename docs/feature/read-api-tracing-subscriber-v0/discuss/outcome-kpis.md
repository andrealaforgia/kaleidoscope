# Outcome KPIs — read-api-tracing-subscriber-v0

These KPIs are operability outcomes: a read-tier operator (Priya Nair) and
the EDD-verifier can observe startup lifecycle and fail-closed refusals on
process stderr. All are black-box observable so the verifier can assert
them from outside the process.

## K1: All three read binaries install a tracing subscriber

- **Who**: the three read crates (query-api, log-query-api,
  trace-query-api).
- **Does what**: install a tracing subscriber as the first action in
  `main`, before the first `tracing::` call.
- **By how much**: 3 of 3 read binaries.
- **Measured by**: code review / grep — each `main.rs` installs the
  subscriber before the first `tracing::info!`/`tracing::error!`. (The
  init expression precedes `*_starting`.)
- **Baseline**: 0 of 3 (no subscriber installed today).
- **Target**: 3 of 3.

## K2: Lifecycle events are visible on stderr at startup

- **Who**: read-tier operators.
- **Does what**: see `*_starting` and `listener_bound` events on stderr
  for a clean start.
- **By how much**: 100% of clean starts, all three binaries.
- **Measured by**: black-box acceptance test spawns each binary, captures
  stderr, greps `*_starting` and `listener_bound`.
- **Baseline**: 0% (stderr empty today).
- **Target**: 100%.

## K3: Fail-closed refusal event is visible on stderr before non-zero exit

- **Who**: operators triaging a refusing read service.
- **Does what**: read `health.startup.refused` with its reason on stderr
  before the process exits non-zero.
- **By how much**: 100% of fail-closed starts, all three binaries.
- **Measured by**: acceptance test forces a fail-closed start (tenant
  unset / unprobeable store), captures stderr, greps
  `health.startup.refused`, asserts non-zero exit.
- **Baseline**: 0% (only the bare `Err` printed by the runtime today).
- **Target**: 100%.

## K4: Read tier is uniform with aperture

- **Who**: operators and the EDD-verifier.
- **Does what**: apply one subscriber format (JSON to stderr) and one
  filter (EnvFilter / RUST_LOG) across the read tier.
- **By how much**: 4 of 4 read-tier binaries (aperture + the three read
  APIs) share one init pattern.
- **Measured by**: code review confirms identical init expression /
  shared helper; the harness parses all four with one JSON line parser.
- **Baseline**: 1 of 4 (only aperture).
- **Target**: 4 of 4.

## K5: The EDD-verifier can assert the structured refusal event

- **Who**: the EDD-verifier (issue 005).
- **Does what**: tighten LQ01/Q01/TQ01 and the fails-closed assertions to
  require the structured `health.startup.refused` event on stderr instead
  of the bare `Err`.
- **By how much**: issue 005 moves to resolvable; the three tightened
  assertions pass against the fixed binaries.
- **Measured by**: the verifier's black-box run captures stderr on
  fail-closed starts of all three binaries and finds the structured event.
- **Baseline**: blocked (empty stderr; only bare `Err` available to
  assert on).
- **Target**: unblocked; all three tightened assertions pass.
