# Acceptance Test Scenarios — cinder-wal-error-surfacing-v0 (DISTILL)

Author: Quinn (`nw-acceptance-designer`). Wave: DISTILL. Date: 2026-06-05.

WS strategy: **Strategy C (real-local-IO)** — real temp-dir WAL files on the real
local filesystem + an in-process failing `FsyncBackend` (library seam) + a real
read-only WAL substrate (subprocess seam). See `wave-decisions.md` DWD-1.

## Test files

| Path | Compiled? | Runs how today |
|---|---|---|
| `crates/cinder/tests/wal_error_surfacing_red.rs` | YES | 2 negative controls PASS; 3 failure scenarios FAIL RED (behavioural — the swallow bug) |
| `crates/sluice/tests/wal_error_surfacing_red.rs` | YES | 1 negative control PASS; 2 failure scenarios FAIL RED |
| `crates/kaleidoscope-cli/tests/wal_error_surfacing_cli_skeleton.rs` | YES | WS-A (happy subprocess) PASS; WS-B (failure subprocess) `#[ignore]`d |
| `docs/.../distill/intended-specs/cinder_wal_error_surfacing.intended.rs` | NO (docs) | the exact post-fix `Result` contract for DELIVER |
| `docs/.../distill/intended-specs/sluice_wal_error_surfacing.intended.rs` | NO (docs) | the exact post-fix `Result` contract for DELIVER |

## Scenario list (with tags and RED/ignore classification)

### R1 — walking skeleton (US-01 place + US-02 live CLI ingest/place)

| # | Scenario (business outcome) | Story | Tags | State today |
|---|---|---|---|---|
| WS-A | Priya places a tier on a healthy disk through the real binary and reads it back durable | US-01/US-02 | `@walking_skeleton @real-io @driving_port` | compiled, **PASS** (negative control at the binary boundary) |
| WS-B | Priya places onto a failing disk; the binary fails loudly (non-zero exit, `persistence failed: io:` stderr) and the placement is not durable | US-02 (D2) | `@walking_skeleton @real-io @driving_port @ignore` | `#[ignore]`d (intended D2; binary swallows today) |
| R1-1 | A failed overwrite preserves the prior durable placement (memory untouched) | US-01 #3 | `@real-io @error-path` | compiled, **FAIL RED** (got `Cold`, want `Hot`) |
| R1-2 | A fresh placement that failed to persist is not visible in memory | US-01 #2 | `@real-io @error-path` | compiled, **FAIL RED** (got `Some(Warm)`, want `None`) |
| R1-3 | A healthy disk places and persists across a reopen | US-01 #1 | `@real-io` | compiled, **PASS** (negative control) |

### R2 — policy sweep (US-03 evaluate_at / D3 fail-whole)

| # | Scenario | Story | Tags | State today |
|---|---|---|---|---|
| R2-1 | A sweep on a failing disk does not migrate items in memory without persistence (count never overstates durability) | US-03 #2 | `@real-io @error-path` | compiled, **FAIL RED** (got `Warm`, want `Hot`) |
| R2-2 | A healthy-disk sweep reports a count equal to the durably-migrated items, all durable across a reopen | US-03 #1 | `@real-io` | compiled, **PASS** (negative control) |

### R3 — sluice uniformity (US-04 / D4, unwired, separately shippable)

| # | Scenario | Story | Tags | State today |
|---|---|---|---|---|
| R3-1 | A failing-disk dequeue keeps the message pending (consistent with disk), not swallowed | US-04 #2 | `@real-io @error-path @uniformity` | compiled, **FAIL RED** (depth 0, want 1) |
| R3-2 | A failing-disk ack does not silently lose the in-flight message | US-04 #3 | `@real-io @error-path @uniformity` | compiled, **FAIL RED** (depth 0, want 1) |
| R3-3 | Healthy-disk dequeue/ack persist durably across a reopen | US-04 #1 | `@real-io @uniformity` | compiled, **PASS** (negative control) |

### Intended post-fix specs (docs; DELIVER turns these into the GREEN target)

| # | Scenario | Story | Asserts |
|---|---|---|---|
| I-1 | A failing disk makes a placement fail loudly | US-01 #1 | `place(...)` returns `Err(PersistenceFailed)` + memory untouched |
| I-2 | A failed overwrite surfaces the error and preserves the prior value | US-01 #3 | `Err(PersistenceFailed)` + `get_tier == Hot` |
| I-3 | A fail-whole sweep surfaces the error and carries no count | US-03 | `evaluate_at(...)` returns `Err`; no item migrated in memory |
| I-4 | A healthy sweep's `Ok(n)` equals the durable count | US-03 #1 | `evaluate_at == Ok(3)` |
| I-5 | A failing dequeue surfaces and keeps the message pending | US-04 #2 | `dequeue` returns `Err(PersistenceFailed)`; depth 1 |
| I-6 | A failing ack surfaces and keeps the in-flight message | US-04 #3 | `ack` returns `Err`; nack redelivers; depth 1 |
| I-7 | Healthy dequeue/ack are `Ok` | US-04 #1 | `dequeue == Ok(Some)`, `ack == Ok(())` |

## Error-path ratio

Counting the scenarios that exercise a failure / error / boundary condition vs the
total designed scenarios (excluding the docs-only intended specs, which mirror the
compiled set):

- Total compiled + WS scenarios: **10** (WS-A, WS-B, R1-1, R1-2, R1-3, R2-1, R2-2,
  R3-1, R3-2, R3-3).
- Error/failure scenarios: **6** (WS-B, R1-1, R1-2, R2-1, R3-1, R3-2).
- **Error-path ratio = 6/10 = 60%** — exceeds the 40% mandate. (Including the
  intended specs the ratio is similar: 5 failure of 9 add'l.)

## Adapter coverage table (Dimension 9c — every driven adapter has real I/O)

| Driven adapter | Real-I/O scenario(s) | Failing-substrate scenario(s) |
|---|---|---|
| `FileBackedTieringStore` (cinder, `place`) | R1-3 (healthy, durable across reopen), WS-A (via binary) | R1-1, R1-2 (failing `FsyncBackend`), WS-B (real read-only WAL via binary) |
| `FileBackedTieringStore` (cinder, `evaluate_at`) | R2-2 (healthy sweep, durable) | R2-1 (failing `FsyncBackend`) |
| `FileBackedQueue` (sluice, `dequeue`/`ack`/`nack`) | R3-3 (healthy, durable across reopen) | R3-1, R3-2 (failing `FsyncBackend`) |
| `kaleidoscope-cli` binary (driving adapter, `place`/`get-tier`) | WS-A (real subprocess, exit + stdout) | WS-B (real subprocess, exit + stderr) |

Every driven adapter the feature touches has at least one `@real-io` scenario AND
at least one failing-substrate scenario. `InMemoryTieringStore` / `InMemoryQueue`
are NOT exercised here (they never persist, so they cannot pin the write-ahead
ordering — using them would be Fixture Theater).

## Falsifiability evidence (the heart of the feature)

Each failure scenario FAILS on today's swallow bug. RUN OUTPUT (2026-06-05):

```
cinder:  failed_fresh_placement_is_not_visible_in_memory ... FAILED  (left: Some(Warm), right: None)
cinder:  failed_overwrite_preserves_prior_durable_placement_in_memory ... FAILED  (left: Some(Cold), right: Some(Hot))
cinder:  failing_sweep_does_not_migrate_in_memory_without_persistence ... FAILED  (left: Some(Warm), right: Some(Hot))
cinder:  healthy_disk_places_and_persists_across_reopen ... ok
cinder:  healthy_sweep_count_equals_durable_migrations ... ok
sluice:  failing_ack_does_not_silently_lose_the_in_flight_message ... FAILED  (left: 0, right: 1)
sluice:  failing_dequeue_keeps_message_pending ... FAILED  (left: 0, right: 1)
sluice:  healthy_queue_dequeue_then_ack_persists_across_reopen ... ok
cli:     place_then_get_tier_through_real_binary_on_healthy_disk ... ok
cli:     place_onto_failing_disk_fails_loudly_and_is_not_durable ... ignored (RED intended D2)
```

Each `FAILED` is the swallow bug exposed: memory was mutated while the WAL append
failed. The `ok` lines are the healthy negative controls (guardrails). This proves
NONE of the failure tests can pass on the swallow — they are genuinely falsifiable.

## Self-review checklist

- [x] **Mandate 7 — RED-not-BROKEN classified by RUNNING, not guessing**: the
      cinder + sluice failure tests COMPILE (they call the present signature) and
      FAIL behaviourally on the swallow bug (run output above). The intended-`Result`
      specs are docs-only (cannot compile today) — no broken build, no fake scaffold.
- [x] **No fake scaffold for an existing-but-wrong impl**: every type/trait/ctor the
      tests use already exists on the public surface. No RED scaffold symbol added
      (DWD-5 rejected a `Result`-shim as Fixture-Theater-prone).
- [x] **Driving-adapter via subprocess**: WS-A (happy) + WS-B (failure) drive the
      REAL `kaleidoscope-cli` binary via `Command` (exit code + stdout/stderr), not
      just the `place()` lib fn — satisfies the brief's driving-port mandate.
- [x] **Falsifiable substrate**: a test-local `FailingFsyncBackend` (`fsync_file`
      returns `io::Error`) makes `append_wal` return `PersistenceFailed`; the
      live-handle memory-untouched assertion FAILS on the swallow bug. Grounded by
      reading `append_wal` (the flush-before-fsync subtlety, DWD-2) — NOT guessed.
- [x] **Failing backend grounded**: confirmed NO write/fsync-failing variant exists
      in `wal-recovery` today (`Real`/`Counting` always Ok; `Lying` returns Ok and
      drops bytes). DELIVER may promote `FailingFsyncBackend` into the shared crate
      (additive, behaviour-preserving) — NOT required; the test-local double works.
- [x] **>=40% error path**: 60% (6/10).
- [x] **Business language in titles**: scenario titles describe operator outcomes
      ("a failed overwrite preserves the prior durable placement", "place onto a
      failing disk fails loudly"), not mechanics. Fn names are Rust-idiomatic; the
      doc-comment Gherkin uses domain terms (tenant, item, tier, persist, durable).
- [x] **capsys-equivalent**: N/A for Rust — the subprocess tests assert on real
      process `stdout`/`stderr`/exit via `Command::output()` (the Rust analogue);
      the library tests assert return values + observable store state.
- [x] **Story traceability**: US-01 → R1-*/WS-*; US-02 → WS-A/WS-B (D2); US-03 →
      R2-*; US-04 → R3-*. Every story has >=1 scenario. No orphans.
- [x] **Environment traceability**: tests run in `clean` + `with-pre-commit` + `ci`
      (plain `cargo test`, in-process io::Error injection, NO wall-clock threshold —
      deterministic, C-DEVOPS-3); the real-read-only-WAL subprocess test is
      `#[ignore]`d (DELIVER) so it does not flake the hook today.
- [x] **Observable assertions only**: every Then asserts a return value
      (`get_tier`, `depth`, `evaluate_at`) or an observable process outcome (exit
      code, stdout/stderr) — no private-field / mock-call assertions.
- [x] **Negative controls present**: R1-3, R2-2, R3-3, WS-A stay GREEN (the
      surfacing change must not regress the healthy path).
- [x] **No production code changed**: only `crates/*/tests/*.rs` added; zero
      `crates/*/src/**` edits; no Cargo.toml / CI / CLAUDE.md changes.
