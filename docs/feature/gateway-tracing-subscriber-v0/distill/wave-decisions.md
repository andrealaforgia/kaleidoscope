# DISTILL Decisions — gateway-tracing-subscriber-v0

Author: Scholar (nw-acceptance-designer). Scope: acceptance tests +
RED-ready scaffold for the gateway's early tracing-subscriber install.
Self-contained delivery; no peer review run; orchestrator owns the
commit. British English; no em dashes; no emoji.

## Prior wave reading (confirmation)

- + docs/feature/gateway-tracing-subscriber-v0/design/application-architecture.md
- + docs/feature/gateway-tracing-subscriber-v0/design/wave-decisions.md
- + docs/feature/gateway-tracing-subscriber-v0/discuss/user-stories.md
- + docs/feature/gateway-tracing-subscriber-v0/discuss/wave-decisions.md
- + docs/feature/gateway-tracing-subscriber-v0/discuss/story-map.md
- + crates/kaleidoscope-gateway/src/main.rs
- + crates/kaleidoscope-gateway/src/composition.rs
- + crates/kaleidoscope-gateway/src/lib.rs
- + crates/kaleidoscope-gateway/Cargo.toml
- + crates/aperture/src/observability.rs (install_subscriber pattern)
- + crates/aperture/Cargo.toml (tracing-subscriber line)
- + crates/aperture/src/config/mod.rs (fixed default bind addrs)
- + crates/aperture-storage-sink/src/lib.rs (StorageSink::probe / snapshot)
- + crates/pulse/src/fsync_probe.rs (fsync probe path)
- + crates/lumen/src/file_backed.rs (open / snapshot file mechanics)
- + crates/log-query-api/tests/slice_07_tracing_subscriber.rs (PATTERN)
- - docs/product/journeys/*.yaml (not found — pre-SSOT feature; legacy model)
- - docs/product/architecture/brief.md (not found — driving port taken from DESIGN)
- - docs/product/kpi-contracts.yaml (not found — KPI scenarios skipped, soft gate)
- - docs/feature/gateway-tracing-subscriber-v0/devops/ (not found — slim wave per DESIGN; defaults)

Reconciliation: 0 contradictions. DISCUSS, DESIGN, and the read source
agree. DISCUSS flagged the install home/posture to DESIGN; DESIGN pinned
it (DD1 inline replication, DD3 first statement of main, DD5 RUST_LOG).
No DISTILL-level contradiction to escalate.

Graceful degradation: this is a pre-SSOT (legacy `docs/feature/`) feature,
so `docs/product/` SSOT inputs are absent. Driving port is taken from the
DESIGN Verification section (the compiled binary as a child process).
KPI-contract observability scenarios are skipped (no contracts file).
DEVOPS is absent (slim wave, DESIGN DEVOPS Handoff section): default
environment posture applies, but this feature has no environment-variant
behaviour to matrix beyond the RUST_LOG filter floor, which is covered.

## DWD-01: Driving port is the compiled binary as a child process

The operability verifier (verifier-007, issue 005) and the operator
(Priya Nair) both observe the gateway through ONE port: the compiled
`kaleidoscope-gateway` binary launched as a child process with a
controlled environment, whose stderr is captured and grepped for
structured JSON `event` lines. The acceptance suite enters through
exactly that port (`CARGO_BIN_EXE_kaleidoscope-gateway`,
`std::process::Command`). An in-process test cannot assert the contract:
the subscriber is process-global, `try_init`-guarded, and writes to the
real stderr fd, so only a spawned child can observe what renders. This
matches the read tier's pinned strategy
(`log-query-api/tests/slice_07_tracing_subscriber.rs`) verbatim.

## DWD-02: Walking Skeleton Strategy — none (Strategy C, real-IO, no WS scenario)

DISCUSS Decision 2 = No walking skeleton (brownfield, isolated defect
closure); story-map confirms. There is no greenfield walking skeleton to
inherit and none to create. The acceptance suite is all `@real-io`: it
spawns the real binary against a real (temporary) filesystem pillar root
with no test doubles. There is exactly one driven boundary exercised —
the gateway-to-filesystem path through the real `FileBacked*Store` and the
real `StorageSink` probe — and it is exercised with real I/O by AC-02.
No InMemory doubles are used anywhere, so there is nothing the doubles
"cannot model" to document.

## DWD-03: AC-02 (fail-closed) is the PRIMARY always-run RED anchor; AC-01 is RED-ready and ignored

The pin (read from source, not assumed):

- The gateway's `main` builds its aperture `Config` with
  `Config::builder().build()` and reads NO env override for the listener
  bind address. The defaults are the FIXED operator ports
  `0.0.0.0:4317` (grpc) and `0.0.0.0:4318` (http)
  (`crates/aperture/src/config/mod.rs:214-215`). Unlike the read tier's
  `log-query-api` (which honours `KALEIDOSCOPE_LOG_QUERY_ADDR=127.0.0.1:0`
  for an ephemeral port), the gateway has no ephemeral knob. A clean-start
  scenario (AC-01) therefore binds fixed ports and is fixed-port-flake-
  prone inside the deterministic pre-commit hook.
- AC-02 (fail-closed) needs no socket and no kill: the sink probe refuses,
  the process exits non-zero on its own, and `.output()` blocks until it
  does. It is fully deterministic.

Decision: AC-02 (and its filter-floor sibling) are NOT `#[ignore]`d — they
are the always-run RED anchor under `cargo test`. AC-01 (clean start) and
the `listener_bound` regression guard ARE `#[ignore]`d with a documented
reason (fixed-port bind). DELIVER de-ignores AC-01 only if it can
guarantee deterministic binding (serialisation or a future ephemeral-port
knob); otherwise they remain an explicit `cargo test -- --ignored` check.
This honours the verifier's priority: the refusal half of issue 005 (the
operator understanding WHY the gateway refused) is the sharper half and is
the always-run anchor.

## DWD-04: The deterministic black-box fail-closed lever — read-only pillar root after WAL pre-stage

Establishing a deterministic, infra-free condition that lands SPECIFICALLY
on the `probe_or_refuse` sink-probe arm (and not on an earlier `?`) took
source reading. The findings:

- `main` runs `std::fs::create_dir_all(pillar_root)` first; it returns Ok
  on an already-existing directory regardless of its mode.
- `FileBacked*Store::open` opens each WAL with
  `OpenOptions::create(true).append(true)` on a path SIBLING to the pillar
  root (`<root>/lumen.wal`, etc. — the stores name WAL/snapshot as
  `base + ".wal"` / `base + ".snapshot"`, flat in the pillar root, NOT in
  a subdir). A statically read-only pillar root would fail this open
  (WAL create) and trip the early `?` rather than the probe.
- The fix: PRE-CREATE the three `.wal` files, THEN `chmod 0o555` the
  pillar root. On Unix, append-opening an EXISTING file needs write on the
  FILE (mode 0644, granted), not on the directory; the read-only directory
  only blocks CREATING a NEW entry. So the store opens succeed.
- The sink probe (DD5 / ADR-0041) then ingests one sentinel (WAL append —
  succeeds) and calls `snapshot()`, whose `File::create(<root>/lumen.snapshot)`
  must create a NEW directory entry. On the `0o555` root that fails with
  `Permission denied`, so the probe returns `ProbeError::Unreachable` ->
  `CompositionError::SinkProbe` -> `substrate=sink`. This is exactly the
  catalogued "opens but is not writable" substrate lie the snapshot check
  exists to catch (`aperture-storage-sink/src/lib.rs:370-394`).

The RED run confirmed the lever empirically: captured stderr (today's bare
`Err`) reads `storage sink probe failed: ... probe snapshot check failed:
... Permission denied (os error 13)`, the process exited non-zero, and the
structured `health.startup.refused` JSON line was absent (RED). The lever
is Unix-only, so the whole suite is `#![cfg(unix)]`.

The fsync-honesty probe lever was rejected: the gateway injects
`RealFsyncBackend` (honest on a real disk) and exposes no env knob to
swap in a `LyingFsyncBackend`, so the only black-box lever to the refusal
arm is the sink-probe snapshot-create failure above. The fsync substrate
classes (`fsync-noop`, etc.) are covered by the existing `composition.rs`
unit tests, not reachable black-box.

## DWD-05: RED-not-BROKEN scaffold (Mandate 7) — wired no-op `init_tracing`

`init_tracing()` is added to `crates/kaleidoscope-gateway/src/main.rs`:

- WIRED as the FIRST statement of `main` (before `resolve_pillar_root`,
  before `create_dir_all`), matching DESIGN DD3 (install point precedes
  the `gateway_starting` emission and the `health.startup.refused` fail
  arm, shrinking the pre-subscriber window to empty).
- The BODY is a deliberate NO-OP carrying both the machine-detectable
  `SCAFFOLD: true` marker (Rust convention) and the `__SCAFFOLD__`
  textual marker. It installs no subscriber, never panics, never returns
  an error. The gateway boots exactly as it does today.
- Consequence: every existing gateway test (6 lib/composition unit tests)
  stays GREEN, and AC-02 is RED (the awaited JSON refusal line never
  reaches stderr) — RED, never BROKEN. No `ImportError`/missing-symbol,
  no panic, no bind error. The RED run proved this classification.
- `tracing-subscriber 0.3` is added to the gateway `Cargo.toml` (verbatim
  feature set from `aperture/Cargo.toml:60`: `fmt`, `json`, `env-filter`,
  `registry`, `default-features = false`). Build verified clean with NO
  warnings: the gateway crate does not enable `unused_crate_dependencies`,
  so the as-yet-unused dep produces no warning under the active lints
  (`unsafe_code = forbid`, clippy `all = warn`). `serde_json` added as a
  dev-dependency for the test's JSON line interrogation.
- DELIVER completion signal: Crafty replaces the no-op body with
  aperture's posture replicated inline (OnceLock + `EnvFilter("RUST_LOG")`
  + JSON-to-stderr `fmt::layer()` + `try_init`, per DESIGN "The fix") and
  removes the `SCAFFOLD: true` marker. AC-02 then turns GREEN; AC-01 may
  be de-ignored per DWD-03.

## Files written (uncommitted, for Crafty)

- `crates/kaleidoscope-gateway/Cargo.toml` — `tracing-subscriber` dep +
  `serde_json` dev-dep + `[[test]]` registration for the new slice.
- `crates/kaleidoscope-gateway/src/main.rs` — early `init_tracing()` call
  + the no-op `init_tracing` scaffold fn.
- `crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs` —
  the black-box subprocess acceptance suite (AC-01, AC-02).
- `docs/feature/gateway-tracing-subscriber-v0/distill/wave-decisions.md`
  (this file) and `distill/test-scenarios.md`.

## Verification at DISTILL close

- `cargo build --workspace --all-targets`: GREEN, no warnings.
- `cargo test -p kaleidoscope-gateway`: 6 lib/composition tests GREEN;
  AC-01 (2 scenarios) `ignored`; AC-02 (2 scenarios) RED with the expected
  "structured refusal absent" assertion failure (the captured stderr is
  today's bare `Err`, proving the lever reached the sink-probe arm).
- `cargo test --workspace --no-run`: GREEN (no other crate disturbed).
- `cargo tree -p kaleidoscope-gateway | grep query-http-common`: 0 edges
  (US-03 anti-coupling invariant holds).
- NOT committed; Crafty does the atomic commit in DELIVER.

## Scope / process notes

- Peer review not run (self-contained delivery, per orchestrator
  instruction). Orchestrator owns the commit; this wave does not commit.
- US-04 (pre-subscriber `eprintln!` fallback) is collapsed into
  US-01/US-02 by DESIGN DD3 (empty pre-subscriber window); no separate
  DISTILL scenario for it. The bare-`Err` path that AC-02 currently
  observes becomes a structured `tracing::error` line once Crafty installs
  the early subscriber, which is the DD3 outcome.
