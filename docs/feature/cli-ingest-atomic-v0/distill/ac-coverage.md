# AC Coverage — `cli-ingest-atomic-v0` / DISTILL

Each US-01 acceptance criterion mapped to its observable and its test, with
the RED-ignored vs GREEN-today classification **verified by running** (not
guessed) against HEAD (today's interleaved parse-and-flush loop).

| AC | Observable (through driving port + read-back) | Test fn | Status today | Verified-by-running evidence |
|----|-----------------------------------------------|---------|--------------|------------------------------|
| **parse-error-commits-nothing** | `ingest(...)` returns `Err(ParseRecord{line:4})` AND `read(...)` count == 0 | `parse_error_commits_nothing` | **RED — ignored** | `--ignored` run: count == **3** (today flushes batch 1 of 3 before line 4) vs expected 0 |
| **re-run-no-double** | second `ingest(...)` of same bad input again `Err(ParseRecord{line:4})` AND count STILL == 0 | `re_run_of_still_malformed_input_does_not_double_count` | **RED — ignored** | `--ignored` run: count == **3** after first run already (would be 6 after second) vs expected 0 |
| **corrected-file-ingests-once** | `ingest(...)` of corrected 4-valid input returns `Ok(IngestStats{4,2,2})` AND count == 4 | `corrected_file_ingests_every_record_exactly_once` | **RED — ignored** | run: count == **7** (failed run left a partial 3 on disk; 3 + 4 = 7) vs expected 4 |
| **valid-file-negative-control** | `ingest(...)` of 250 valid at `DEFAULT_BATCH_SIZE` returns `Ok(IngestStats{250,3,3})` AND count == 250 | `fully_valid_file_ingests_every_record_exactly_once_no_regression` | **GREEN** | run: passed — the all-valid path already commits correctly; this guards no-regression |
| **malformed-first-line boundary** | `ingest(...)` returns `Err(ParseRecord{line:1})` AND count == 0 | `malformed_first_line_commits_nothing_and_names_line_one` | **GREEN** | run: passed — no batch can flush before line 1 fails, so count is 0 today too |

## Why the RED tests are RED-NOT-BROKEN

All three ignored tests COMPILE against the existing public surface
(`ingest`, `read`, `Error::ParseRecord`, `IngestStats`, `DEFAULT_BATCH_SIZE`
— no not-yet-existing symbol, no scaffold) and FAIL only on a **behavioural
assertion** (the committed store COUNT is wrong under today's partial-commit
loop). The `cargo test ... -- --ignored` run shows three `assertion left ==
right failed` panics on the count, NOT compile errors and NOT setup errors.
That is the definition of RED-not-BROKEN: the test is a correct executable
specification of behaviour the production code does not yet have.

## Classification correction (verified, not guessed)

The DISTILL brief flagged that corrected-file "should already work today
actually; check." Running it disproved that guess: because AC3 builds on a
store already dirtied by the failed run's partial commit (3 records on disk),
the corrected 4-record ingest lands on 7, not 4. AC3 therefore depends on
the new all-or-nothing behaviour (the failed run must leave the store clean)
and is correctly `#[ignore]`d. This is exactly why the brief mandated
classify-by-running rather than by inference.

## Negative-control / safety-property first-class

The safety properties — commit-nothing (AC1), no-double (AC2), and the
no-regression guardrail (AC4) — are first-class `#[test]` functions, not
afterthoughts. AC4 and AC5 are GREEN guardrails that MUST keep passing
through DELIVER; if the re-ordering breaks the all-valid path or the
first-line boundary, they go RED and catch the regression immediately.

## Existing locked suite (must stay GREEN unmodified)

The seven tests in `tests/ingest_and_read_roundtrip.rs` — including
`malformed_json_line_returns_typed_error_with_line_number` (2 lines at
`DEFAULT_BATCH_SIZE`, never flushes, still reports `line: 2`) — are NOT
modified by this wave and continue to pass green. Confirmed by the full
`cargo test --workspace --all-targets --locked` run staying green at the
DISTILL commit.

## DELIVER handoff — un-ignore sequence

After DELIVER re-orders `ingest` to parse-all-then-flush-all
(ADR-0064 DD-1, buffer-all-then-flush), remove the `#[ignore = "RED until
DELIVER: ..."]` attribute from these three, one at a time as each goes
GREEN under the new code:

1. `parse_error_commits_nothing`
2. `re_run_of_still_malformed_input_does_not_double_count`
3. `corrected_file_ingests_every_record_exactly_once`

AC4 and AC5 stay un-ignored throughout (they guard no-regression). When all
five are GREEN with the `#[ignore]`s removed, US-01 is demonstrably
complete.
