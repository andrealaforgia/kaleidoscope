# DISTILL Decisions — read-api-tracing-subscriber-v0

Scholar (nw-acceptance-designer). Outer-loop acceptance design for the
read-tier tracing-subscriber operability fix. Origin: EDD black-box
verifier issue 005 (medium, operability). No HTTP contract change,
no new crate, no new ADR (DESIGN DD4).

## Reconciliation (pre-scenario gate)

Read all prior-wave artefacts: DISCUSS (`user-stories.md`,
`story-map.md`, `wave-decisions.md`), DESIGN
(`application-architecture.md`, `wave-decisions.md`). No SSOT
`docs/product/` model exists for this feature (it originates from a
verifier issue, not a journey); driving ports are taken from DESIGN.

Checked each DISCUSS decision against DESIGN: **zero contradictions.**
The one grounding correction (DD3: aperture keys off `APERTURE_LOG`, the
read tier keys off `RUST_LOG`) is already what the user stories pin
(US-01..04 assert `RUST_LOG` by name), so it is alignment, not conflict.
Reconciliation passed — 0 contradictions. Proceeded to scenario design.

## Story-to-AC traceability

| Story | Acceptance criterion | Scenario (slice_07_tracing_subscriber.rs) | Status |
|---|---|---|---|
| US-01 | Clean start prints `log_query_api_starting` | `clean_startup_announces_log_query_api_starting_on_stderr` | RED-ready, `#[ignore]` (poll-then-kill) |
| US-01 | `listener_bound` with bound address on stderr | `clean_startup_reports_bound_listener_address_on_stderr` | RED-ready, `#[ignore]` |
| US-01 | `RUST_LOG=warn` suppresses info events | `rust_log_warn_suppresses_info_startup_events` | RED-ready, `#[ignore]` |
| US-02 | Fail-closed writes `health.startup.refused` + reason | `fail_closed_startup_writes_health_startup_refused_to_stderr_before_nonzero_exit` | **RED (always-run anchor)** |
| US-02 | Event precedes non-zero exit | same scenario (asserts both event presence and `!status.success()`) | **RED (always-run anchor)** |
| US-02 | Refusal survives `RUST_LOG=error` | `refusal_event_survives_rust_log_error_filter` | **RED (always-run anchor)** |
| US-03 | query-api start/bind/refuse on stderr | covered by the shared `init_tracing` helper + wiring; the log-query-api subprocess suite is the representative black-box proof for the one shared helper (US-05). DELIVER may mirror the suite per binary if desired. | helper wired in `query-api/src/main.rs` |
| US-04 | trace-query-api start/bind/refuse on stderr | same shared helper; wiring landed in `trace-query-api/src/main.rs` | helper wired |
| US-05 | All three binaries match aperture's posture | one shared `query_http_common::init_tracing()` is the single source of truth; the subprocess suite parses each stderr line with ONE JSON helper (`stderr_has_event`), proving the one-parser-covers-all contract | scaffold landed; verified by code review + the shared helper |
| US-06 | Pre-init failures via `eprintln!` before exit | `stderr_has_event` deliberately skips non-JSON lines so the pre-init `eprintln!` window is tolerated; DELIVER adds the explicit pre-init scenarios (malformed addr / unopenable store) once it converts the `?` arms to `eprintln!` | DELIVER scenario stub noted below |

US-03 and US-04 share ONE helper with US-01/US-02 (US-05 is the whole
point: one pattern, four binaries). The acceptance proof is deliberately
concentrated on `log-query-api` because the behaviour under test is the
shared `init_tracing` helper, not per-binary logic; the wiring into
`query-api` and `trace-query-api` is identical one-line calls verified by
`cargo build` + code review. DELIVER may copy the subprocess suite to the
other two crates if the EDD verifier wants per-binary black-box coverage
(Q01/TQ01); the helper guarantees identical behaviour.

## DWD-01 — Verification strategy: BOTH (subprocess acceptance + in-process idempotence unit)

DESIGN DD5 pins black-box subprocess + stderr grep as the primary and
less-fragile acceptance approach. Scholar adopts it AND adds the
in-process idempotence unit test (the two options in the brief), because
they assert disjoint, non-overlapping contracts:

- **Option A (subprocess acceptance, the user-observable behaviour):**
  `crates/log-query-api/tests/slice_07_tracing_subscriber.rs`. Spawns the
  COMPILED binary via `env!("CARGO_BIN_EXE_log-query-api")` (the
  established repo idiom — see `aperture/tests/cli_smoke.rs`,
  `kaleidoscope-cli/tests/*`), controls the environment
  (`KALEIDOSCOPE_PILLAR_ROOT`, `KALEIDOSCOPE_LOG_QUERY_TENANT`,
  `KALEIDOSCOPE_LOG_QUERY_ADDR`, `RUST_LOG`), captures stderr, parses each
  line as `serde_json::Value`, and greps the `event` field. This is the
  ONLY way to assert "the subscriber installed and wrote to the real
  stderr fd", because the subscriber is process-global and
  `try_init`-guarded — an in-process test cannot observe it. It is also
  byte-for-byte the shape the EDD verifier uses (LQ02/LQ03/TQ01 capture
  empty stderr today). This is the driving-port test (the operator's real
  invocation path).

- **Option B (in-process idempotence unit):**
  `test_init_tracing_is_idempotent_and_never_panics` in
  `crates/query-http-common/src/lib.rs`. Asserts the ONE in-process
  invariant that holds for BOTH the scaffold no-op AND the real DELIVER
  body: the `OnceLock` guard makes a second call a safe no-op (without it,
  a second global `try_init` would error). This test is GREEN now and
  stays GREEN after DELIVER, because it guards the idempotence CONTRACT,
  not the unimplemented behaviour — so it is correctly NOT `#[ignore]`d.
  It exists so the mutation surface of the `OnceLock` guard (DELIVER will
  add) is killable in-crate.

Rationale for BOTH: the subprocess suite proves the observable operator
outcome (RED until DELIVER); the unit test pins the idempotence contract
that every `main` and every shared-process test relies on. Neither
subsumes the other.

## DWD-02 — Fail-closed scenario is the always-run RED anchor; clean-start scenarios are `#[ignore]`d

The fail-closed scenario (tenant unset) is the highest-value and the most
deterministic assertion (DD5 agrees):

- the binary runs `create_dir_all` + `FileBackedLogStore::open` in a tmp
  pillar root (both succeed), `resolve_tenant` returns `None`, `probe`
  returns `Err`, `health.startup.refused` is emitted, and the process
  exits non-zero — all WITHOUT binding a socket and WITHOUT needing a
  kill. `Command::output()` blocks until the child exits on its own.

This makes it a clean always-run RED anchor: it RUNS under `cargo test`
and FAILS today because the no-op subscriber emits nothing (stderr shows
only the bare Rust `Error: "..."` line), turning GREEN the moment DELIVER
installs the real subscriber.

The clean-start and filter scenarios (US-01) bind a socket and block on
`axum::serve` forever, so they must poll stderr then kill the child. They
are `#[ignore]`d for pre-commit safety (a spawned server in the default
test run is undesirable) and use `KALEIDOSCOPE_LOG_QUERY_ADDR=127.0.0.1:0`
(ephemeral OS-assigned port, no fixed-port flake). **DELIVER de-ignores
them one at a time** as it lands the real subscriber (outer-loop
one-at-a-time convention). They are RED-ready: the bodies compile and
assert the correct observable outcome; only the `#[ignore]` attribute and
the missing subscriber body stand between them and GREEN.

## DWD-03 — Mandate 7 RED-not-BROKEN: wired NO-OP scaffold, never panic

`init_tracing` is called by all three `main`s. A `panic!`/`unimplemented!`
scaffold body would make EVERY binary launch panic, which would BREAK
every existing test that spawns or boots a read binary (and the EDD
verifier). That is BROKEN, not RED.

Scholar's pinned approach — the **wired no-op scaffold** (the brief's
RECOMMENDED option):

1. `query_http_common::init_tracing()` exists in
   `crates/query-http-common/src/lib.rs` with a deliberate NO-OP body
   (`let _ = ();`) carrying the `// __SCAFFOLD__ read-api-tracing-subscriber-v0`
   marker. It compiles, installs nothing, never panics.
2. The call is wired as the FIRST statement of all three `main`s
   (`query-api`, `log-query-api`, `trace-query-api`). The wiring is
   therefore already RED-ready: the binaries boot exactly as today (events
   discarded), so all 6 existing log-query-api slices and the rest of the
   workspace stay GREEN.
3. The new subprocess acceptance test is RED because the no-op emits no
   structured event to stderr. RED (behaviour unimplemented), never BROKEN
   (no panic, no missing symbol, imports resolve).
4. DELIVER (Crafty) fills ONLY the `init_tracing` body with aperture's
   `OnceLock`-guarded JSON-to-stderr `EnvFilter("RUST_LOG")` builder (DD3);
   the wiring is already in place. The acceptance test turns GREEN and the
   `__SCAFFOLD__` marker is removed.

This is the cleanest RED-not-BROKEN: the RED comes purely from the empty
helper body, isolated to one function, with the wiring already proven.

Verified empirically:
- `cargo build --workspace --all-targets`: GREEN (23s).
- `cargo test --workspace --no-fail-fast`: the ONLY failing binary is
  `slice_07_tracing_subscriber` (2 failed always-run, 3 ignored). Every
  pre-existing test (including log-query-api slice_01..06,
  query-http-common's 25 tests incl. the new idempotence unit) is GREEN.

## DWD-04 — Walking-skeleton strategy: Strategy C (real local), `@real-io`

Per the WS decision tree: the feature exercises only LOCAL resources — a
spawned in-process subprocess (the compiled binary) and a real local
filesystem (tmp pillar root, real `FileBackedLogStore::open`). No costly
external dependency, no paid API. Therefore **Strategy C (Real local)**:
the subprocess acceptance suite uses REAL adapters throughout — real
process spawn, real filesystem store, real stderr fd. No InMemory double
appears anywhere in the suite; deleting any "fake" would not let the test
pass because there is no fake. The fail-closed anchor is the de-facto
walking skeleton: the operator's real invocation path, real store open,
real refusal, real non-zero exit, real stderr.

## DWD-05 — Adapter coverage

The only driven "adapter" this feature introduces is the effectful
`init_tracing` install seam (a process-global subscriber writing to the
real stderr fd). It is exercised with REAL I/O by the subprocess suite
(real child process, real stderr capture) — there is no InMemory
substitute and none is wanted (DD5: an in-process test cannot assert the
real-fd write). Coverage table:

| Adapter / seam | Real-I/O scenario | Covered by |
|---|---|---|
| `init_tracing` subscriber install (stderr fd) | YES | subprocess suite (real child, real stderr) — RED until DELIVER |
| `FileBackedLogStore` open (tmp pillar root) | YES (real fs) | fail-closed anchor opens a real store before the probe |

Zero `NO — MISSING` rows.

## DELIVER notes (handoff)

- Fill `query_http_common::init_tracing` body with aperture's builder
  (`observability::install_subscriber`, verbatim) with `RUST_LOG` instead
  of `APERTURE_LOG` (DD3); keep the `OnceLock` guard; remove the
  `__SCAFFOLD__` marker.
- De-ignore the three `#[ignore]`d clean-start/filter scenarios one at a
  time as the subscriber lands; they will pass once info/`listener_bound`
  events render and the `RUST_LOG=warn` filter drops the info events.
- Convert the pre-init `?` arms (`resolve_addr`, `*Store::open`,
  `create_dir_all`) in each `main` to `eprintln!("{binary}: ...: {e}")`
  before the non-zero return (US-06), matching aperture's convention; add
  the two US-06 subprocess scenarios (malformed addr; unopenable store)
  asserting a plain stderr line precedes a non-zero exit.
- Optionally mirror the subprocess suite into `query-api` and
  `trace-query-api` tests if per-binary EDD black-box coverage is wanted
  (Q01/TQ01); the shared helper makes the behaviour identical.
- C6: extend the `#[mutants::skip]` posture to `init_tracing` (unkillable
  global-install wiring), as DESIGN flags.
- C7: no crate bumped to 1.0.0. No commit in DISTILL (DELIVER lands the
  atomic GREEN).
</content>
