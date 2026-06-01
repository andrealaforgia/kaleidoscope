# Outcome KPIs — gateway-tracing-subscriber-v0

All KPIs are verified black-box: spawn `kaleidoscope-gateway` as a child
process with controlled env, capture stderr, parse each line as JSON,
grep on the `event` field. This is the verifier's G01 method.

## K1: gateway installs a tracing subscriber

- **Who**: operators of `kaleidoscope-gateway`.
- **Does what**: a JSON-to-stderr tracing subscriber is active during the
  gateway's own startup, not only inside aperture's spawn.
- **By how much**: at least one gateway-origin event renders on stderr on
  every clean start (was 0 from the gateway's own main).
- **Measured by**: stderr capture shows a `gateway_starting` JSON line.
- **Baseline**: today only aperture-origin events (`listener_bound`,
  aperture `startup`/`ready`) render; the gateway's own `gateway_starting`
  is dropped.

## K2: gateway_starting + listener_bound (with addr) visible on start

- **Who**: SREs confirming the node is up.
- **Does what**: observe start and bind on stderr.
- **By how much**: `gateway_starting` present AND at least one
  `listener_bound` line carrying `transport` and `addr` present, on 100%
  of clean starts.
- **Measured by**: stderr JSON grep for both `event` values; assert
  `addr` field present on `listener_bound`.
- **Baseline**: `listener_bound` renders today; `gateway_starting` does
  not.

## K3: fail-closed event visible before exit

- **Who**: SREs diagnosing a refused boot.
- **Does what**: read the structured refusal reason and substrate class.
- **By how much**: `health.startup.refused` (with `substrate` + `reason`)
  present on 100% of fail-closed exits, emitted before the non-zero exit.
- **Measured by**: spawn against a lying-fsync or unwritable substrate;
  grep `event=health.startup.refused`; assert `substrate` field; assert
  non-zero exit; assert refusal line precedes exit.
- **Baseline**: today the refusal is emitted before subscriber install
  and dropped; operator sees a bare non-zero exit.
- **Applicability**: confirmed applicable. The gateway HAS a fail-closed
  arm via `probe_or_refuse` (`composition.rs`), emitting
  `health.startup.refused` at main.rs line 102.

## K4: uniformity with aperture write-side posture

- **Who**: operators running one log pipeline over the ingest tier.
- **Does what**: parse aperture and gateway stderr with one schema.
- **By how much**: 0 gateway-specific parser branches; 0 dependency edges
  from `kaleidoscope-gateway` to `query-http-common`.
- **Measured by**: line-shape diff of captured stderr against aperture;
  `cargo tree -p kaleidoscope-gateway | grep query-http-common` returns
  nothing.
- **Baseline**: gateway renders an incomplete subset today; no read-tier
  edge exists and none must be introduced.

## K5: verifier G01 can assert the structured event → issue 005 RESOLVED

- **Who**: the black-box operability verifier (issue 005).
- **Does what**: asserts the gateway's structured lifecycle events on
  stderr, completing the fourth-binary coverage the read tier left open.
- **By how much**: issue 005 moves from `partial` (read tier resolved,
  gateway open) to RESOLVED.
- **Measured by**: verifier re-run after landing; G01 finds
  `gateway_starting` and `health.startup.refused` on the gateway's
  stderr with the agreed field shape.
- **Baseline**: issue 005 is `partial`; the gateway is the last open
  binary.
