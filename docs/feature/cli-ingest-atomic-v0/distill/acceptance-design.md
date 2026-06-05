# Acceptance Design — `cli-ingest-atomic-v0` / DISTILL

> **Author**: `nw-acceptance-designer` (Quinn), DISTILL wave, 2026-06-05.
> **Mode**: SLIM. One driving port, one user story (US-01), five AC.
> **nWAVE-ORDER**: DISCUSS -> DESIGN -> DEVOPS -> DISTILL -> DELIVER.
> DISTILL writes the acceptance tests BEFORE the DELIVER production change.
> The all-or-nothing behaviour DOES NOT EXIST YET — today `ingest` flushes
> each full batch DURING the read, so a parse error after a flushed batch
> leaves a partial commit. The tests that assert the new behaviour are
> therefore behaviourally RED today and `#[ignore]`d. This is the EXPECTED,
> CORRECT state at this wave; the ignored tests are NOT a defect.

## Driving port

`kaleidoscope-cli ingest <tenant> <data_dir>` — exercised in-process through
its library entry point `kaleidoscope_cli::ingest(tenant, data_dir,
batch_size, reader, otlp_log_path)` (ADR-0064 DD-6). This is the CLI
driving port; calling the library with a `Cursor`-backed reader is the
in-process equivalent of spawning the binary and piping NDJSON on stdin,
and strictly simpler (no subprocess, no signals, deterministic). The
committed store count is read back through the shipped `read(...)` surface
against the SAME `data_dir` — the in-process equivalent of
`kaleidoscope-cli read`/`stats` reporting `records=N`. No private Lumen
helper is touched (Mandate 1 — hexagonal boundary held).

## Strategy: real local I/O (@real-io)

Per DEVOPS `wave-decisions.md` A2 and ADR-0064 DD-6, the proving test is a
deterministic typed-error + count-readback in-process test with **no flake
surface**. Every test drives a REAL `FileBackedLogStore` (Lumen) and a REAL
`FileBackedTieringStore` (Cinder) on a per-test tmp `data_dir`. There is no
InMemory double anywhere on the path — the store is the real file-backed
adapter, so the test catches the real commit-discipline behaviour (a
partial commit really lands on disk and is really read back). This is why
the tests are tagged `@real-io`: deleting the real adapter is not possible
without the count-readback failing, so the tests genuinely prove the
wiring, not a fake.

Observables only: a typed `Result` (the library-boundary equivalent of an
exit code) and a committed-state COUNT read back through `read(...)`. No
signals, no crash target, no wall-clock/p95/sleep, no concurrency — sidesteps
the overnight p95 flake class entirely.

## The witness (why batch_size=3)

The shipped suite's `malformed_json_line_returns_typed_error_with_line_number`
uses only 2 lines at `DEFAULT_BATCH_SIZE=100`, so the single good record
never reaches a full batch and nothing flushes before the abort — the
partial-commit footgun is INVISIBLE to it. The headline witness here uses
`batch_size=3` with 3 valid records + a malformed line 4, so the first
batch (lines 1-3) WOULD flush before line 4 under today's non-atomic loop.
This is the minimal witness of "a full batch parsed-and-would-have-flushed,
held back by the all-or-nothing discipline because a later line failed."

## Scenario inventory (5 tests, all US-01)

| # | Test fn | Category | Today |
|---|---------|----------|-------|
| AC1 | `parse_error_commits_nothing` | error / headline | RED (ignored) |
| AC2 | `re_run_of_still_malformed_input_does_not_double_count` | error / safety | RED (ignored) |
| AC3 | `corrected_file_ingests_every_record_exactly_once` | recovery | RED (ignored) |
| AC4 | `fully_valid_file_ingests_every_record_exactly_once_no_regression` | happy / negative control | GREEN |
| AC5 | `malformed_first_line_commits_nothing_and_names_line_one` | error / boundary | GREEN |

Error-path ratio: 4 of 5 scenarios (AC1, AC2, AC3-recovery-after-error,
AC5) exercise the parse-error / failed-recovery path = 80%, well above the
40% target. This is a correctness-hardening wave on a single failure mode,
so the error-path weighting is intentional and proportionate.

## Walking skeleton

This is a SLIM correctness wave on an already-shipped driving port, not a
greenfield feature, so there is no NEW walking skeleton: the operator's
end-to-end journey (pipe NDJSON -> ingest -> read the count back) already
walks through the real CLI library + real file-backed store + real read
surface and is exercised by every test here. AC1 (commit-nothing) plus AC3
(fix-and-ingest-once) together trace the complete demo-able operator
journey from the elevator pitch: failed ingest commits nothing -> re-run is
safe -> fix the named line -> corrected file ingests exactly once. AC4 is
the no-regression guardrail proving the all-valid path is unchanged.

## Out of scope (mirrors US-01)

- Success-case re-run dedup (re-ingesting the SAME valid file twice still
  doubles — Lumen has no idempotency key; D-DedupFuture).
- Mid-commit write-failure atomicity (disk-full on batch 2 of 3) — a
  pre-existing store-durability concern, not changed by this wave.
