# I/O Strategy — beacon-sighup-reload-v0 (DISTILL)

British English throughout, no em dashes.

## Strategy: C (real I/O), tagged `@real-io`

The DEVOPS wave specified a **real subprocess + real POSIX signal + mock
backend + sink catcher** proving test (`devops/wave-decisions.md`,
Decision 2 and 3, and `environments.yaml`). DISTILL implements exactly that.
Every scenario in `crates/beacon-server/tests/sighup_reload.rs` uses real
I/O end to end. No InMemory double appears.

## What is real, and why InMemory cannot substitute

| Element | Realisation | Why real (what an InMemory double would miss) |
|---|---|---|
| The orchestrator under test | the real `beacon-server` binary spawned via `CARGO_BIN_EXE_beacon-server` | the feature IS the binary's signal-handling + atomic swap; a library-level double would bypass the very wiring being proven (signal install order, `tokio::select!` arm) |
| The operator action | a real POSIX SIGHUP sent by pid (`rustix::process::kill_process`) | SIGHUP delivery, the OS default-disposition trap, and the install-before-spawn ordering only exist for a real signal to a real process |
| The rules directory | a real writable tmp dir under `env::temp_dir()` (project's established style) | beacon-server writes its durable store at `<rules>/.beacon-state/store` (main.rs:107); the edit-then-SIGHUP step and the store both need a real writable, test-owned dir |
| The PromQL backend | a real `wiremock` HTTP server returning an Active instant vector | the per-rule eval loop issues a real HTTP GET `/query`; the firing lifecycle only runs against a real responder |
| The incident sink | a real `wiremock` HTTP webhook catcher; incident POSTs recorded and polled | a firing is observable only as a real webhook POST; the `started_at` (`since` proxy) rides the real JSON body |
| The reload events | the child's real stderr drained into a buffer and polled | the two structured `tracing` events are the operator-visible observables and the happen-before anchor; they exist only on the real stream |

## Determinism (no p95), per DEVOPS Decision 2

- **Happen-before anchor**: `wait_for_event(&stderr, "beacon.reload.
  succeeded" | "beacon.reload.refused")` blocks (by polling) until the
  structured event appears, BEFORE any downstream assertion.
- **Poll-under-bound**: `wait_for_firing` / `wait_for_event` poll every
  `POLL_STEP` (50 ms) and return on first appearance, failing only if the
  single `GENEROUS_BOUND` (20 s) elapses with the observable absent.
- **Short interval for SPEED only**: rule TOML seeds `interval = "100ms"`,
  `for_duration = "100ms"`. This makes the awaited observable arrive
  quickly; it is never the asserted quantity.
- **Assertion form**: presence-under-a-bound (boolean "was observed"),
  never a latency, never a percentile, never a `sleep`-as-sole-sync. The
  few fixed `sleep(500ms)` calls exist only to let extra ticks accrue
  BEFORE a *negative* assertion ("no SECOND firing"), and are bounded above
  by the generous bound logic, never used as the positive sync.

## Portability (POSIX-only), per DEVOPS Decision 3 + reviewer condition 3

- Module gated `#![cfg(unix)]`. SIGHUP is POSIX-only; a future Windows CI
  compiles the module to empty and does not fail on the absent signal.
- Signal sent by pid with the **safe** `rustix::process::kill_process(
  Pid::from_child(&child), Signal::HUP)`. No `unsafe` block, so the test
  target honours the crate's `forbid(unsafe_code)` lint. `rustix` is
  already in the workspace lock (1.1.4); the dev-dep adds only its
  `process` feature.

## Pure-function extraction note (Mandate 4 / CM-D)

This is a black-box subprocess proving test: there is no business logic in
the test to extract, and the test parametrises no environment fixtures
(one substrate: a real Unix process + tmp dir + mock HTTP, identical on
Linux CI and macOS local). The pure evaluation logic
(`transition`/`evaluate_once`) is already unit-tested in `beacon` and
`smoke.rs` and is untouched by this feature (ADR-0037 keeps `transition`
pure). The reload orchestration is inherently impure (signal + I/O) and is
exercised only through the driving port, never extracted into the test.
Mandate 4 is satisfied vacuously: no fixture matrix, no business logic in
steps.

## Clean target

Each scenario owns a `TmpRules` guard whose `Drop` removes the tmp tree
(rules dir + durable store) at test end; the child is killed and reaped via
`shutdown`. The clean state is the absence of both.
