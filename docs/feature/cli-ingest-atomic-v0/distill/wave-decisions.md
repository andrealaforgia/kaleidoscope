# Wave Decisions — `cli-ingest-atomic-v0` / DISTILL

> **Author**: `nw-acceptance-designer` (Quinn), DISTILL wave, 2026-06-05.
> **Mode**: SLIM. One driving port, US-01, five AC, one new test file.
> **nWAVE-ORDER**: DISCUSS -> DESIGN -> DEVOPS -> DISTILL -> DELIVER. DISTILL
> writes the acceptance tests BEFORE the DELIVER production change. The
> all-or-nothing behaviour does not exist yet (today's `ingest` flushes each
> full batch DURING the read), so the tests asserting it are behaviourally
> RED today and `#[ignore]`d. This is the EXPECTED, CORRECT state — the
> ignored tests are NOT a defect.

## Inputs read

- DISCUSS `discuss/user-stories.md` (US-01, the 5 AC, the System
  Constraints + the byte-equivalence + blank-line-preservation contracts).
- DESIGN `design/wave-decisions.md` (ADR-0064: DD-1 buffer-all-then-flush,
  DD-6 For-Acceptance-Designer — the driving port, per-AC observables, and
  the `batch_size=3` + malformed-line-4 witness).
- DEVOPS `devops/wave-decisions.md` (A2 in-process deterministic
  typed-error + count-readback, no subprocess/signals/p95;
  `environments.yaml` = `clean` + `ci`).
- The existing harness `tests/ingest_and_read_roundtrip.rs` (reused shape:
  `tenant`, `record`, `temp_data_dir`, `cleanup`, `ndjson`; the in-process
  `ingest(...)` call + `read(...)` count-readback).
- The public surface `src/lib.rs` (`ingest` signature with `batch_size`,
  `Error::ParseRecord{line}`, `IngestStats`, `read` returning the count).

## D-1 — Test file shape: mirror the existing roundtrip harness

`tests/ingest_atomic.rs` is a NEW file that duplicates the
`tenant`/`record`/`temp_data_dir`/`cleanup`/`ndjson` helpers inline (v0
rule-of-three extraction deferred, per cluster precedent and US-01 Technical
Notes), plus two local helpers: `stored_count(...)` (the count-readback
through `read`) and `valid_prefix_then_malformed(n)` (the witness builder).
One `[[test]]` manifest entry added to `Cargo.toml`; no new dependency.

## D-2 — Count read-back via `read(...)`, not a private file

The committed store count is observed through `read(tenant, data_dir, sink,
None, TimeRange::all())` against the same `data_dir` — the in-process
equivalent of `kaleidoscope-cli read`/`stats records=N`. No Lumen internal
file is inspected (Mandate 1 / DD-6). `stats_with_tiers` would work equally;
`read` is chosen because it returns the count directly and is already in the
roundtrip harness's vocabulary.

## D-3 — Classify by RUNNING, not by inference (corrected a wrong guess)

The DISTILL brief flagged corrected-file as "should already work today
actually; check." Running it disproved the guess: AC3 builds on a store
already dirtied by the failed run's partial commit (3 records on disk), so
the corrected 4-record ingest lands on **7**, not 4. AC3 therefore depends
on the new all-or-nothing behaviour and is correctly `#[ignore]`d. Final
classification (all verified by `cargo test ... -- --ignored`):

| AC | Test | Today | Why |
|----|------|-------|-----|
| AC1 | `parse_error_commits_nothing` | RED (ignored) | count 3 not 0 — batch 1 flushes before line 4 |
| AC2 | `re_run_of_still_malformed_input_does_not_double_count` | RED (ignored) | count 3 (then 6) not 0 |
| AC3 | `corrected_file_ingests_every_record_exactly_once` | RED (ignored) | count 7 not 4 — failed run left a partial |
| AC4 | `fully_valid_file_..._no_regression` | GREEN | all-valid path already commits correctly |
| AC5 | `malformed_first_line_..._names_line_one` | GREEN | nothing flushes before line 1 fails |

## D-4 — RED-not-BROKEN, hook-green, never --no-verify

The three RED tests COMPILE against the existing public surface (no scaffold,
no not-yet-existing symbol) and FAIL only on a behavioural count assertion —
RED-not-BROKEN. They are `#[ignore = "RED until DELIVER: cli-ingest-atomic-v0
..."]` so `cargo test --workspace --all-targets --locked` stays GREEN at the
DISTILL commit (the same command the pre-commit hook Step 4 runs — no
`--no-verify` needed, no flake surface per DEVOPS A2).

## D-5 — KPI contracts: none present (soft gate, proceed)

`docs/product/kpi-contracts.yaml` does not exist. Per the soft-gate rule,
proceed with a warning. The feature's outcome KPIs (OK1-OK4) are verified at
BUILD time by this test file plus the locked roundtrip suite (DEVOPS A3 — no
runtime telemetry, no `@kpi` observability scenario warranted). The four
KPIs map directly onto the five AC tests; no separate `@kpi` scenario is
added.

## D-6 — No Fixture Theater

The Given steps set up PRECONDITIONS only (empty store, valid-prefix +
malformed input, a prior failed run). NO Given step seeds the expected
output. The three RED tests genuinely fail today and will pass only when
DELIVER changes the commit discipline — confirmed by the `--ignored` run
showing real count mismatches, not fixtures doing the feature's work.

## Self-review + reviewer dispatch

`@nw-acceptance-designer-reviewer` dispatched; if not invocable from this
subagent context, a structured self-review was performed and a top-level
reviewer run is flagged in the parent report (WITH the nWAVE-order reminder,
so the reviewer does not mistake the `#[ignore]`d RED tests or the
not-yet-existing `lib.rs` re-ordering for a defect). Self-review found no
blocker/high issues: hexagonal boundary held (CM-A, driving port only),
business language clean (CM-B), journeys complete (CM-C), real-I/O adapter
coverage (Dim 9, `@real-io`), error-path ratio 80% (Dim 1), every assertion
observable (Dim 7), full US-01 traceability (Dim 8 Check A — all 5 AC
mapped). Do NOT proceed into DELIVER from this wave.
