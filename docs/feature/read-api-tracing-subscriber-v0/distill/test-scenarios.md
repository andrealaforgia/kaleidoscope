# Test Scenarios — read-api-tracing-subscriber-v0

Outer-loop acceptance scenarios for the read-tier tracing-subscriber
operability fix. Verification strategy: BOTH — black-box subprocess
acceptance (Option A, the user-observable behaviour) plus an in-process
idempotence unit test (Option B). See `wave-decisions.md > DWD-01`.

SSOT for scenarios is the `.feature`-equivalent Rust test file
`crates/log-query-api/tests/slice_07_tracing_subscriber.rs`; this table
is the human-readable map.

## Driving port

The operator's real invocation path: the COMPILED binary launched as a
child process with a controlled environment, whose stderr is captured and
grepped for structured JSON `event` lines. Entered via
`env!("CARGO_BIN_EXE_log-query-api")` (the repo idiom). No in-process
function call substitutes for this (the subscriber is process-global and
writes to the real stderr fd, DD5).

## Scenario table

| # | Scenario | Story / AC | Category | Tags | Driving port | Status at DISTILL close |
|---|---|---|---|---|---|---|
| 1 | Fail-closed startup writes `health.startup.refused` to stderr before non-zero exit | US-02 (AC1+AC2) | error path (the anchor) | `@walking_skeleton @real-io @driving_port` | compiled `log-query-api` child | **RED (always-run)** |
| 2 | Refusal event survives `RUST_LOG=error` filter | US-02 (AC3) | error / boundary | `@real-io @driving_port` | compiled `log-query-api` child | **RED (always-run)** |
| 3 | Clean startup announces `log_query_api_starting` on stderr | US-01 (AC1) | happy path | `@real-io @driving_port` | compiled `log-query-api` child | RED-ready, `#[ignore]` |
| 4 | Clean startup reports bound listener address on stderr | US-01 (AC2) | happy path / edge | `@real-io @driving_port` | compiled `log-query-api` child | RED-ready, `#[ignore]` |
| 5 | `RUST_LOG=warn` suppresses info-level startup events | US-01 (AC3) | boundary (filter) | `@real-io @driving_port` | compiled `log-query-api` child | RED-ready, `#[ignore]` |
| 6 | `init_tracing` is idempotent and never panics | US-05 substrate / DWD-01 Option B | invariant | `@property` (idempotence) | `query_http_common::init_tracing` (in-process) | GREEN (contract holds for scaffold + real body) |

Error/edge ratio: scenarios 1, 2, 5 are error/boundary (the fail-closed
refusal and the two filter cases); 6 is an idempotence invariant. Of the
5 observable subprocess scenarios, 3 are error/boundary (60% > 40%
target). The fail-closed path is the highest-value assertion (DD5).

## DELIVER-pending scenarios (US-06, not yet authored)

| Scenario | Story / AC | Why deferred |
|---|---|---|
| Malformed `KALEIDOSCOPE_LOG_QUERY_ADDR` prints an `eprintln!` line before non-zero exit | US-06 (AC1) | DELIVER first converts the pre-init `?` arms to `eprintln!`; the scenario lands in the same DELIVER step (the `stderr_has_event` helper already tolerates the non-JSON pre-init line) |
| Unopenable store prints an `eprintln!` line before non-zero exit | US-06 (AC2) | same — depends on the DELIVER pre-init `eprintln!` conversion |

## Self-review checklist (DISTILL Dimension 9 + Mandate 7)

- [x] 1. WS strategy declared in `wave-decisions.md` (DWD-04: Strategy C, real local).
- [x] 2. WS / scenarios tagged correctly: all subprocess scenarios `@real-io` (Strategy C); no `@in-memory` anywhere.
- [x] 3. Every driven seam has a `@real-io` scenario: the `init_tracing` stderr install + real `FileBackedLogStore` open are both exercised with real I/O by the fail-closed anchor (DWD-05 coverage table, zero MISSING).
- [x] 4. For InMemory doubles: N/A — no InMemory doubles used (Strategy C); the suite would not pass if any adapter were faked.
- [x] 5. Container preference: none (real adapters on host; subprocess + tmp filesystem).
- [x] 6. Mandate 7: the production module imported by the test (`query_http_common::init_tracing`) has its scaffold present and compiling.
- [x] 7. Mandate 7: scaffold carries the `__SCAFFOLD__ read-api-tracing-subscriber-v0` marker (Rust comment form).
- [x] 8. Mandate 7: scaffold body does NOT panic — deliberate NO-OP (`let _ = ();`). RED comes from the absent observable behaviour (no event on stderr), not from a panic. This is the correct posture HERE precisely because the helper is called by every `main`; a panicking body would BREAK every binary-spawning test (DWD-03).
- [x] 9. Mandate 7: the always-run scenarios are RED (2 failed), not BROKEN — verified by `cargo test --workspace --no-fail-fast`: the only failing binary is `slice_07_tracing_subscriber`; all imports resolve, no panic, no missing symbol.
- [x] 10. Driving adapter: the binary entry point (the operator's `CARGO_BIN_EXE_log-query-api` subprocess) is exercised via its real protocol (process spawn + stderr capture), exit-code asserted — not via an in-process service call.
- [x] 11. At least one `@real-io @adapter-integration` scenario per driven seam: the fail-closed anchor is real subprocess + real filesystem + real stderr fd.
- [x] 12. (pytest `capsys` rule) N/A — Rust suite; stderr captured via `Stdio::piped` on the child, not a step-scoped fixture.
- [x] 13. (driving-port boundary) the test imports ONLY the compiled binary (via `CARGO_BIN_EXE`) and `serde_json` for parsing; no internal `log_query_api::*` module is reached.
- [x] 14. Timing: no wall-clock timing assertion; the poll-then-kill helper uses a generous 5s / 2s bound only as a liveness ceiling, not a perf budget.
- [x] 15. (BDD import noqa) N/A — Rust.

## Mandate compliance evidence (handoff)

- **CM-A (hexagonal boundary):** the acceptance suite enters ONLY through
  the compiled binary (`env!("CARGO_BIN_EXE_log-query-api")`) — the
  operator's real driving port — and `serde_json` for stderr parsing. Zero
  imports of internal `log_query_api` modules. The idempotence unit test
  enters through the public `query_http_common::init_tracing` free
  function (a public surface, not an internal component).
- **CM-B (business language):** scenario names and the Gherkin in the
  story comments speak operator language ("operator learns why the service
  refused to start", "operator sees the bound listener address"). The
  structured `event` strings (`health.startup.refused`, `listener_bound`,
  `log_query_api_starting`) are the OPERATOR-FACING contract the verifier
  greps, not internal jargon — they are the observable outcome itself.
- **CM-C (walking skeleton / user journey):** the fail-closed anchor is a
  complete operator journey — start the service with a misconfiguration,
  read the reason off stderr, observe the non-zero exit. Demo-able to a
  platform on-call operator (persona Priya Nair).
- **CM-D (pure function extraction):** the only effectful seam
  (`init_tracing`) is isolated behind one free function; the parsing logic
  in the test (`stderr_has_event`) is a pure function over `&str`. No
  fixture parametrisation across environments is needed (Strategy C, real
  local only).
