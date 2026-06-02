# Test Scenarios — gateway-tracing-subscriber-v0

Black-box subprocess acceptance suite for the gateway's early
tracing-subscriber install. Driving port: the compiled
`kaleidoscope-gateway` binary launched as a child process, stderr
captured and grepped for structured JSON `event` lines (DWD-01). All
scenarios are `@real-io` (real binary, real temporary filesystem, no
doubles). Suite is `#![cfg(unix)]` (the fail-closed lever uses Unix
permission bits — DWD-04).

Test file: `crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs`

## Story-to-AC traceability

| Story | AC source | Scenario(s) | Status |
|---|---|---|---|
| US-01 | gateway_starting renders with `pillar_root` | AC-01a clean start announces gateway_starting | RED-ready, `#[ignore]` (fixed-port bind) |
| US-01 | listener_bound renders with `transport`+`addr` (regression) | AC-01b bound listener address | RED-ready, `#[ignore]` (fixed-port bind) |
| US-01 | info events absent under raised floor | covered for the refusal path by AC-02b; clean-start warn variant folded into AC-01 (ignored) | n/a |
| US-02 | health.startup.refused renders with `substrate`+`reason`, before non-zero exit | AC-02a fail-closed refusal | ACTIVE, RED |
| US-02 | substrate names the refusal class (`sink`) | AC-02a (asserts `substrate=sink`) | ACTIVE, RED |
| US-02 | refusal survives raised log floor | AC-02b refusal survives RUST_LOG=warn | ACTIVE, RED |
| US-03 | no `query-http-common` dependency edge | verified by `cargo tree` (0 edges) — dependency-audit, not a runtime scenario | PASS (build-time) |
| US-03 | same JSON line shape as aperture | shared envelope asserted via the JSON `event`/field shape AC-02 and AC-01 grep against | covered by GREEN shape post-DELIVER |
| US-04 | pre-subscriber failure named line | collapsed into US-01/US-02 by DESIGN DD3 (empty pre-subscriber window) | n/a |

## Scenario table

| ID | Scenario | Given / When / Then (business) | Driving port | Tags | Active? | RED reason at DISTILL |
|---|---|---|---|---|---|---|
| AC-02a | Fail-closed refusal is visible before the non-zero exit | Given a pillar root that opens but cannot accept a fresh snapshot write; When the startup probe refuses; Then stderr shows `health.startup.refused` with `substrate=sink` and a `reason`, AND the process exits non-zero | compiled binary (child process) | `@real-io @driving_port` | YES (always-run anchor) | no-op subscriber drops the refusal; stderr carries only today's bare `Err` |
| AC-02b | Refusal survives a raised log floor | Given `RUST_LOG=warn` and the refusing substrate; When the probe refuses; Then the error-level `health.startup.refused` line is still present on stderr | compiled binary (child process) | `@real-io @driving_port` | YES | same — no subscriber, nothing renders |
| AC-01a | Operator sees the gateway announce itself at startup | Given an explicit pillar root and a default tenant; When the process starts; Then stderr shows `gateway_starting` naming the `pillar_root` | compiled binary (child process) | `@real-io @driving_port` | `#[ignore]` (fixed-port bind, DWD-03) | gateway_starting fires before the no-op install and is dropped |
| AC-01b | Operator sees the bound listener address (regression guard) | Given an explicit pillar root and a default tenant; When the listeners bind; Then stderr shows `listener_bound` naming `transport` and `addr` | compiled binary (child process) | `@real-io @driving_port` | `#[ignore]` (fixed-port bind, DWD-03) | aperture emits this after its own in-spawn install; the guard is that the early install preserves the stream and shape |

## Error / edge path ratio

Active scenarios: 2 (AC-02a, AC-02b) — BOTH are error/fail-closed paths
(100% of the always-run set). Including the ignored RED-ready scenarios,
4 total: 2 error/refusal + 2 happy/clean-start = 50% error path. Both
figures clear the 40% mandate. This is expected for an operability defect
closure whose sharpest half is the refusal diagnosis (US-02).

## Adapter coverage (Mandate 6)

| Driven boundary | @real-io scenario | Covered by |
|---|---|---|
| gateway -> filesystem (FileBacked*Store open + StorageSink snapshot probe) | YES | AC-02a/AC-02b (real temp pillar root, real permission failure on the real snapshot create) |
| gateway -> aperture spawn -> listener bind | YES (ignored, fixed-port) | AC-01b (real bind, real `listener_bound`) |

No InMemory doubles anywhere; nothing to document as un-modellable.

## Self-review checklist (Dimension 9 + Mandate 7)

- [x] 1. WS strategy declared in wave-decisions.md (DWD-02: none / Strategy C real-IO, no WS scenario — justified by DISCUSS D2).
- [x] 2. Scenarios tagged correctly: all `@real-io` (no InMemory used).
- [x] 3. Every driven boundary has a `@real-io` scenario (filesystem probe: AC-02; listener bind: AC-01b).
- [x] 4. No InMemory doubles, so nothing to document as un-modellable.
- [x] 5. Container preference: not applicable (host process + temp dir; no container).
- [x] 6. Mandate 7: the production symbol the tests rely on (`init_tracing`) has a wired scaffold in `main.rs`.
- [x] 7. Mandate 7: scaffold carries `SCAFFOLD: true` and `__SCAFFOLD__` markers.
- [x] 8. Mandate 7: the scaffold is a no-op (never panics, never errors) — the RED comes from the missing observable behaviour, not an exception. For a wired-no-op binary this is the correct Rust shape: the binary must stay launchable, so a `panic!` scaffold would be BROKEN (process aborts before the probe), not RED. The RED signal is the ABSENCE of the JSON line on a process that still runs to its real refusal.
- [x] 9. Mandate 7: tests are RED, not BROKEN — proven by the RED run (assertion failure on "refusal event absent", process exited non-zero on its own, no panic / no missing symbol / no bind error).
- [x] 10. Driving port: every scenario enters via the compiled binary subprocess (the operator's and verifier's actual path), never an in-process function call.
- [x] 11. At least one `@real-io @adapter-integration` scenario per driven boundary (see adapter coverage table).
- [x] Business-language purity: Gherkin-in-doc-comments uses operator language (gateway start, refusal, substrate class, log floor); technical JSON/field mechanics live in the step helpers, not the scenario prose.
- [x] Observable-behaviour assertions (Dim 7): every assertion checks stderr content the operator sees or the process exit status the operator observes — no internal-state or method-call assertions.

## Mandate compliance evidence (for DELIVER handoff)

- CM-A (hexagonal boundary): the test imports only `std::process`,
  `std::fs`, `serde_json`, and `env!("CARGO_BIN_EXE_kaleidoscope-gateway")`.
  Zero imports of gateway internal modules — it enters purely through the
  compiled binary (the driving port).
- CM-B (business language): scenario doc-comments speak operator language;
  no `assert response.status_code`-style technical leakage in the
  Given/When/Then prose.
- CM-C (walking skeleton + focused counts): no walking skeleton (DWD-02);
  2 active focused scenarios + 2 ignored RED-ready focused scenarios.
- CM-D (pure function extraction): not applicable — the feature adds an
  impure I/O seam (`init_tracing`, process-global subscriber install). It
  is correctly tested only through the binary boundary (DWD-01); there is
  no business logic to extract to a pure function. The idempotence /
  OnceLock guard is pinned by a unit test in DELIVER (mirroring the read
  tier), not by this acceptance suite.
