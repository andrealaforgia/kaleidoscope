<!-- markdownlint-disable MD024 -->

# User Stories — `cli-ingest-atomic-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits
  where polymorphism is genuinely needed. This change modifies the
  commit discipline of ONE existing free function
  (`kaleidoscope_cli::ingest`,
  `crates/kaleidoscope-cli/src/lib.rs:157-246`); it introduces no
  new trait, no new crate, no new external dependency, and no new
  CLI subcommand.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions
  with `// Given / // When / // Then` comment blocks, not Gherkin
  `.feature` files. The Given/When/Then text in the UAT Scenarios
  section below is the specification; DISTILL translates it into
  `#[test]` functions in
  `crates/kaleidoscope-cli/tests/ingest_atomic.rs` (NEW file, per
  `wave-decisions.md` Handoff). The harness mirrors the pattern in
  `crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs` (the
  existing ingest acceptance file with `tenant`, `record`,
  `temp_data_dir`, `cleanup`, `ndjson` helpers).
- All-or-nothing contract: NO batch is committed to Lumen and NO
  Cinder Hot-tier entry is placed until EVERY line of the input has
  been successfully parsed as a `LogRecord`. On the first parse
  failure, the call returns `Err(Error::ParseRecord { line, source })`
  (`crates/kaleidoscope-cli/src/lib.rs:87-90`) naming the 1-based
  line number of the first offending line, and the store count for
  the tenant is UNCHANGED from before the invocation (zero records
  committed by this call). On full success, every record is
  committed exactly once and the call returns `Ok(IngestStats)`.
- Count read-back contract: "store count" is observed via the
  shipped read surfaces — `kaleidoscope-cli stats <tenant>
  <data_dir>` (first stdout line `records=N`, `lib.rs:367-400`) or
  `kaleidoscope-cli read <tenant> <data_dir>` (returns the matched
  record count, `lib.rs:279-311`). The acceptance test reads the
  count back through one of these surfaces (or the equivalent
  in-process `read(...)` / `stats_with_tiers(...)` call against the
  same `data_dir`), NOT by inspecting Lumen's internal files.
- Typed-error preservation: the malformed-line abort preserves the
  existing `Error::ParseRecord { line, source }` shape and its
  `Display` (`parse record at line {line}: {source}`,
  `crates/kaleidoscope-cli/src/lib.rs:112-114`). The CLI binary maps
  this (any non-`UnknownFlag` `Error`) to `ExitCode::FAILURE`
  (`main.rs:90`) and prints `kaleidoscope-cli: {e}` to stderr
  (`main.rs:80`). The operator stderr experience on a malformed line
  is unchanged EXCEPT the store count after the failed run is now 0,
  not a partial commit.
- Negative-control byte-equivalence: for a fully-valid input (no
  parse error), the `IngestStats` return
  (`records_ingested`/`batches_flushed`/`tier_items_placed`,
  `crates/kaleidoscope-cli/src/lib.rs:128-134`) and the stderr
  summary line `ingest ok: records=N batches=M tier_items=K`
  (`main.rs:275-278`) MUST be byte-equivalent before and after this
  change. The all-valid ingest path's observable behaviour does not
  regress; every existing locked test in
  `crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs`
  continues to pass green unmodified.
- Blank-line handling preserved: blank lines continue to be skipped
  (`crates/kaleidoscope-cli/src/lib.rs:207-209`) and the line-number
  basis for `Error::ParseRecord` is unchanged (the raw
  `reader.lines()` enumeration index + 1, so the existing
  malformed-line test reporting `line: 2`
  (`crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs:244-263`)
  still reports `line: 2`).
- Out of scope (this wave): success-case re-run dedup. Ingesting the
  SAME fully-valid file twice still adds the records twice (Lumen has
  no idempotency key). This is a SEPARATE, LARGER concern (ingest
  dedup) deferred to a future feature per `wave-decisions.md`
  D-DedupFuture. THIS feature closes only the parse-error
  partial-commit + parse-error re-run double-count.

---

## US-01: Operator ingests a file with a malformed line and the command commits nothing, names the bad line, and survives a re-run without double-counting

### Elevator Pitch

- **Before**: Priya the platform operator pipes a 101-line NDJSON
  file into the CLI — 100 valid `LogRecord`s followed by one
  malformed line at line 101 (a truncated write, a hand-edit typo, a
  bad export):

  ```text
  cat acme-logs.ndjson | kaleidoscope-cli ingest acme /tmp/data
  ```

  Today, with `DEFAULT_BATCH_SIZE=100`
  (`crates/kaleidoscope-cli/src/lib.rs:70`), the ingest loop flushes
  the first full batch (lines 1-100) to Lumen
  (`crates/kaleidoscope-cli/src/lib.rs:215-226`, calling `flush` →
  `lumen.ingest` at `:259`) BEFORE it reaches line 101 and aborts on
  the parse error (`:210-213`). So the command exits non-zero and
  prints, honestly, the bad line:

  ```text
  kaleidoscope-cli: parse record at line 101: ...
  ```

  but the store now holds **100** records, not 0. Priya checks:

  ```text
  kaleidoscope-cli stats acme /tmp/data
  records=100
  ```

  The command told her it FAILED, yet it committed 100 records. Her
  reflex — the universal operator reflex on a failed ingest — is to
  fix nothing yet and just re-run the same file to see if it was a
  transient hiccup:

  ```text
  cat acme-logs.ndjson | kaleidoscope-cli ingest acme /tmp/data
  kaleidoscope-cli stats acme /tmp/data
  records=200
  ```

  It DOUBLE-INGESTED the prefix. Lumen has no dedup, so the
  already-committed 100 records were added again. Priya now has 200
  records from a file that was never successfully ingested even once,
  and no way to tell which 100 are duplicates. The command
  acknowledged a partial ingest as if it had failed cleanly, then
  punished the obvious recovery with silent duplication.

- **After**: Priya runs the exact same command against the exact
  same malformed file:

  ```text
  cat acme-logs.ndjson | kaleidoscope-cli ingest acme /tmp/data
  kaleidoscope-cli: parse record at line 101: ...
  ```

  Exit non-zero, line 101 named — same honest error as before. But
  now:

  ```text
  kaleidoscope-cli stats acme /tmp/data
  records=0
  ```

  The store count is UNCHANGED: the command committed NOTHING,
  because it validated every line before committing any batch and
  line 101 failed. Priya re-runs the still-malformed file (her
  reflex):

  ```text
  cat acme-logs.ndjson | kaleidoscope-cli ingest acme /tmp/data
  kaleidoscope-cli: parse record at line 101: ...
  kaleidoscope-cli stats acme /tmp/data
  records=0
  ```

  Still 0. No partial, no double. The re-run of a still-bad input is
  a no-op on the store count. Now she opens the file, fixes line 101
  (the one the command named), and re-runs the corrected file:

  ```text
  cat acme-logs-fixed.ndjson | kaleidoscope-cli ingest acme /tmp/data
  ingest ok: records=101 batches=2 tier_items=2
  kaleidoscope-cli stats acme /tmp/data
  records=101
  ```

  Exit 0. All 101 records committed exactly once. The ingest is
  all-or-nothing: it took the whole file or none of it.

- **Decision enabled**: Priya can trust the store count after a
  failed ingest and can safely retry. Concretely:
  1. "The ingest failed — is my store dirty?" She runs `stats` and
     sees `records=0` (or the unchanged pre-ingest count), so she
     KNOWS the failed file committed nothing. She decides to fix the
     named line, not to hunt for a partial commit to clean up.
  2. "Can I just re-run this?" Yes — re-running a still-bad input is
     a no-op on the count, so her reflex is safe. She decides to
     re-run freely while diagnosing, with no risk of doubling.
  3. "Is the corrected file now fully in?" She runs `stats` after the
     corrected ingest exits 0 and sees the full record count, so she
     decides the ingest is complete and moves on.

### Problem

Priya the platform operator ingests operator-provided NDJSON files
via `kaleidoscope-cli ingest <tenant> <data_dir>` daily. When a line
partway through a file is malformed — a truncated write, a hand-edit
typo, a bad upstream export — she finds it dangerous to recover,
because the `ingest` command is NON-ATOMIC:

1. **Partial commit on parse error**: the ingest loop flushes
   completed batches to Lumen DURING the read
   (`crates/kaleidoscope-cli/src/lib.rs:215-226`), so a malformed
   line at position N leaves every batch before N already committed.
   With `DEFAULT_BATCH_SIZE=100`, a malformed line 101 commits the
   first 100 records and then aborts naming line 101 — the command
   exits non-zero while having committed a partial prefix. The error
   message is honest about the bad line but silent about the partial
   commit.
2. **Double-count on the obvious recovery**: the natural operator
   reaction to a failed ingest is to re-run it. Re-running the same
   still-malformed file commits the prefix AGAIN (Lumen has no
   dedup; `flush` builds a fresh `LogBatch` and calls `lumen.ingest`
   unconditionally, `crates/kaleidoscope-cli/src/lib.rs:258-259`).
   So run 1 → count 100, re-run → count 200, from a file that never
   ingested even once.

This is the acked-but-wrong shape the project's durability/honesty
posture forbids: a command that says it failed must not have
committed anything, and the obvious recovery must not corrupt. The
verifier pinned this as K13 / issue 009 (RED-ish footgun) and the
four-quadrants assessment named it (kaleidoscope-cli Q2-MEDIUM).

The existing acceptance test
`malformed_json_line_returns_typed_error_with_line_number`
(`crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs:244-263`)
does NOT catch this: it uses only 2 lines at `DEFAULT_BATCH_SIZE`, so
the single good record never reaches a full batch and nothing is
flushed before the abort — the partial-commit footgun is invisible
to the current suite. The footgun appears only when at least one
full batch flushes BEFORE the malformed line (100 valid + malformed
at `DEFAULT_BATCH_SIZE=100`, or 3 valid + malformed at `batch_size=3`).

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | uses `kaleidoscope-cli ingest`,
`kaleidoscope-cli read`, `kaleidoscope-cli stats`,
`kaleidoscope-cli migrate` daily (inherited from the
`kaleidoscope-cli` cluster) | ingests operator-provided NDJSON files
(thousands to low-millions of records) that occasionally contain a
malformed line from a truncated write or a bad export | expects a
failed ingest to commit NOTHING (so the store count after a failure
is trustworthy) | expects re-running a failed ingest to be SAFE (her
universal reflex on any failed batch job) | reads the store count
back via `stats` / `read` | does NOT expect (in v0) that re-ingesting
the SAME fully-valid file is deduplicated — she knows that re-running
a SUCCESSFUL ingest of a valid file still adds the records again
(out of scope this wave, per `wave-decisions.md` D-DedupFuture).

### Solution

Change the commit discipline of the existing `ingest` library
function (`crates/kaleidoscope-cli/src/lib.rs:157-246`) so that it
validates the ENTIRE input — parsing every NDJSON line into a
`LogRecord` — BEFORE committing any batch to Lumen (and before
placing any Cinder Hot-tier entry). If every line parses, ingest all
batches exactly as today (the all-valid path is byte-equivalent). If
ANY line fails to parse, return the existing typed
`Error::ParseRecord { line, source }`
(`crates/kaleidoscope-cli/src/lib.rs:87-90`) naming the first
offending 1-based line number, and commit NOTHING — the store count
for the tenant is unchanged.

The exact mechanism is DESIGN-locked per `wave-decisions.md`
D-BufferVsStream. The two candidate mechanisms are:

1. **Buffer-all-then-flush (recommended for v0)**: parse every line
   into an in-memory `Vec<LogRecord>` first; if a parse error
   occurs, return it before opening / writing any store. Only after
   the whole input validates, open the stores and flush the buffered
   records in batches of `batch_size`. The code already buffers up to
   `batch_size` records at a time
   (`crates/kaleidoscope-cli/src/lib.rs:200`); this widens the buffer
   to the whole input. Acceptable for v0 operator files (bounded
   size).
2. **Two-pass read**: pass 1 parses every line and detects the first
   parse error; pass 2 re-reads and ingests. This requires the input
   to be re-readable — stdin (`main.rs:266-267`) is a one-shot
   stream, so this would need a file-path positional argument (a CLI
   shape change). DESIGN decides whether this is warranted.

Either mechanism satisfies the all-or-nothing behaviour requirement.
The wire-observable contract this wave locks is the four AC below
(parse-error-commits-nothing, re-run-no-double,
corrected-file-ingests-once, valid-file-negative-control). The
`IngestStats` shape and the stderr summary line for the all-valid
path are unchanged.

The `Error` type adds NO new variants — the malformed-line case
reuses the existing `Error::ParseRecord { line, source }`
(`crates/kaleidoscope-cli/src/lib.rs:87-90`).

### Domain Examples

#### 1. Happy path / the pinned footgun — 100 valid records + malformed line 101, then re-run, then fix-and-ingest

Priya pipes `acme-logs.ndjson` (100 valid `LogRecord`s, then a
malformed line 101 `{not valid json}`) into the CLI under a fresh
empty `/tmp/data` for tenant `acme`:

```text
cat acme-logs.ndjson | kaleidoscope-cli ingest acme /tmp/data
```

The command exits non-zero, prints `parse record at line 101: ...`
to stderr. She checks the count — `kaleidoscope-cli stats acme
/tmp/data` reports `records=0`. NOTHING was committed (today it
would report `records=100`). She re-runs the SAME still-malformed
file; it exits non-zero again, names line 101 again, and `stats`
still reports `records=0` (today it would report `records=200`). She
fixes line 101 (replaces it with a valid `LogRecord`), saving
`acme-logs-fixed.ndjson` (now 101 valid lines), and ingests it:

```text
cat acme-logs-fixed.ndjson | kaleidoscope-cli ingest acme /tmp/data
ingest ok: records=101 batches=2 tier_items=2
```

Exit 0. `kaleidoscope-cli stats acme /tmp/data` reports
`records=101`. Every record committed exactly once.

#### 2. Edge case (malformed line at a batch boundary, small batch size) — 3 valid + malformed line 4 at batch_size=3

To exercise the footgun without needing 100+ records, the test uses
`batch_size=3`: 3 valid records then a malformed line 4. At
`batch_size=3`, the first batch (lines 1-3) would flush BEFORE line 4
under today's code (`crates/kaleidoscope-cli/src/lib.rs:215-226`),
making the partial commit observable. Under the fix, the call to the
library `ingest(&acme, &dir, 3, reader, None)` returns
`Err(Error::ParseRecord { line: 4, .. })` and a follow-up
`read(&acme, &dir, ...)` / `stats_with_tiers(...)` against the same
`dir` reports a count of 0 — no batch was committed even though the
first three records parsed cleanly. This is the minimal witness of
"a full batch parsed-and-would-have-flushed, but the all-or-nothing
discipline held it back because a later line failed."

#### 3. Edge case (re-run of still-malformed input is a no-op on the count) — same input twice, count stays 0

Priya ingests the `batch_size=3` malformed input from Example 2
twice in succession, never fixing the file. After the first run the
count is 0; after the second run the count is STILL 0. The re-run of
a still-malformed input adds nothing — there is no prefix to double
because nothing was committed on either run. (Under today's code, run
1 → count 3, run 2 → count 6.)

#### 4. Negative control (fully-valid file) — every record ingested exactly once, exit 0

Priya ingests a fully-valid file of 250 records under a fresh
`/tmp/data` for tenant `acme` (no malformed line anywhere). The
command exits 0, prints `ingest ok: records=250 batches=3
tier_items=3` (250 / 100 = 3 batches: 100 + 100 + 50, identical to
the existing `ingest_survives_a_simulated_restart_via_separate_read_call`
test at
`crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs:124-153`).
A follow-up `read` / `stats` reports `records=250`. Every record
committed exactly once. The all-valid path's `IngestStats` and stderr
summary are byte-equivalent to today's behaviour — this is the
no-regression control.

#### 5. Edge case (malformed FIRST line) — line 1 bad, nothing committed

Priya ingests a file whose VERY FIRST line is malformed (no valid
prefix at all). The call returns `Err(Error::ParseRecord { line: 1,
.. })` and the count is 0. This is the degenerate all-or-nothing case
(no batch ever could have flushed) and confirms the error names line
1. It is the trivial boundary that both the old and new code already
handle identically on the count (0 either way), but the new code's
contract makes the "names the bad line" half explicit at the
boundary.

### UAT Scenarios (BDD)

> Scenario titles describe WHAT the operator achieves, not HOW the
> validation is implemented. The DISTILL test file
> `crates/kaleidoscope-cli/tests/ingest_atomic.rs` realises these as
> `#[test]` functions calling the library `ingest(...)` and reading
> the count back via `read(...)` / `stats_with_tiers(...)` against the
> same `data_dir`.

#### Scenario: A malformed line aborts the ingest and commits nothing — the store count is unchanged (parse-error-commits-nothing)

```text
Given a fresh empty data_dir /tmp/data and tenant acme with zero records currently stored
And an NDJSON input of 3 valid LogRecords followed by a malformed line 4 ("{not valid json}"), ingested at batch_size=3 so that the first batch (lines 1-3) would flush before line 4 under non-atomic behaviour
When Priya invokes ingest(acme, /tmp/data, batch_size=3, reader, None)
Then the call returns Err(Error::ParseRecord { line, .. }) with line == 4
And a follow-up read(acme, /tmp/data, ...) against the same data_dir returns a record count of 0
And a follow-up stats query reports records=0 (the store count is unchanged from the pre-ingest zero — NO partial commit)
```

#### Scenario: Re-running the same still-malformed input does not double-count — the store count stays unchanged (re-run-no-double)

```text
Given the same 3-valid-plus-malformed-line-4 input from the previous scenario, ingested at batch_size=3
And the first ingest invocation has already returned Err(ParseRecord{line:4}) leaving the store count at 0
When Priya invokes ingest(acme, /tmp/data, batch_size=3, reader, None) a SECOND time with the same still-malformed input
Then the second call also returns Err(Error::ParseRecord { line, .. }) with line == 4
And a follow-up read/stats against the same data_dir still returns a record count of 0 (no partial from the first run to double; no new partial from the second run)
```

#### Scenario: After the operator fixes the named line, the corrected file ingests exactly once (corrected-file-ingests-once)

```text
Given the store count for tenant acme is 0 after one or more failed ingests of a malformed input naming line 4
And Priya has corrected line 4 so the input is now 4 valid LogRecords (the malformed line replaced with a valid one), ingested at batch_size=3
When Priya invokes ingest(acme, /tmp/data, batch_size=3, reader, None) on the corrected input
Then the call returns Ok(IngestStats) with records_ingested == 4 and batches_flushed == 2 (3 + 1) and tier_items_placed == 2
And a follow-up read/stats against the same data_dir returns a record count of exactly 4 (every record committed exactly once — not 0, not 8)
```

#### Scenario: A fully-valid file ingests every record exactly once and exits successfully (valid-file-negative-control)

```text
Given a fresh empty data_dir /tmp/data and tenant acme with zero records currently stored
And a fully-valid NDJSON input of 250 LogRecords with no malformed line, ingested at DEFAULT_BATCH_SIZE=100
When Priya invokes ingest(acme, /tmp/data, DEFAULT_BATCH_SIZE, reader, None)
Then the call returns Ok(IngestStats) with records_ingested == 250 and batches_flushed == 3 (100 + 100 + 50) and tier_items_placed == 3
And the IngestStats and the binary's stderr summary line "ingest ok: records=250 batches=3 tier_items=3" are byte-equivalent to the pre-change behaviour (no regression on the all-valid path)
And a follow-up read/stats against the same data_dir returns a record count of exactly 250 (every record committed exactly once)
```

#### Scenario: A malformed first line commits nothing and names line 1 (boundary)

```text
Given a fresh empty data_dir /tmp/data and tenant acme with zero records currently stored
And an NDJSON input whose very first line is malformed ("{not valid json}"), with no valid prefix, ingested at DEFAULT_BATCH_SIZE
When Priya invokes ingest(acme, /tmp/data, DEFAULT_BATCH_SIZE, reader, None)
Then the call returns Err(Error::ParseRecord { line, .. }) with line == 1
And a follow-up read/stats against the same data_dir returns a record count of 0
```

### Acceptance Criteria

- [ ] **parse-error-commits-nothing**: When tenant `acme` starts
  with zero stored records and an input of 3 valid `LogRecord`s
  followed by a malformed line 4 is ingested at `batch_size=3`,
  `ingest(...)` returns `Err(Error::ParseRecord { line, .. })` with
  `line == 4`, and a follow-up `read`/`stats` against the same
  `data_dir` reports a record count of **0** (no partial commit —
  the first batch that would have flushed under non-atomic behaviour
  is held back).
- [ ] **re-run-no-double**: Invoking `ingest(...)` a SECOND time with
  the SAME still-malformed input (3 valid + malformed line 4 at
  `batch_size=3`) again returns `Err(Error::ParseRecord { line: 4,
  .. })`, and a follow-up `read`/`stats` STILL reports a record count
  of **0** (no double — neither run committed anything to double).
- [ ] **corrected-file-ingests-once**: After the malformed line 4 is
  corrected to a valid `LogRecord` (input now 4 valid lines at
  `batch_size=3`), `ingest(...)` returns
  `Ok(IngestStats { records_ingested: 4, batches_flushed: 2,
  tier_items_placed: 2 })`, and a follow-up `read`/`stats` reports a
  record count of exactly **4** (every record committed exactly once
  — not 0, not 8).
- [ ] **valid-file-negative-control**: A fully-valid input of 250
  `LogRecord`s with no malformed line, ingested at
  `DEFAULT_BATCH_SIZE=100`, returns `Ok(IngestStats {
  records_ingested: 250, batches_flushed: 3, tier_items_placed: 3 })`
  (100 + 100 + 50), a follow-up `read`/`stats` reports exactly
  **250**, and the `IngestStats` plus the binary's stderr summary
  line `ingest ok: records=250 batches=3 tier_items=3` are
  byte-equivalent to the pre-change behaviour (no regression).
- [ ] **malformed-first-line boundary**: An input whose first line is
  malformed returns `Err(Error::ParseRecord { line, .. })` with
  `line == 1` and a follow-up `read`/`stats` reports a record count
  of **0**.
- [ ] The existing locked acceptance tests in
  `crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs`
  (`ingest_then_read_round_trips_records_byte_stable`,
  `ingest_survives_a_simulated_restart_via_separate_read_call`,
  `two_tenants_data_is_isolated_in_the_same_data_dir`,
  `empty_stdin_produces_zero_records_zero_batches`,
  `blank_lines_in_input_are_skipped`,
  `malformed_json_line_returns_typed_error_with_line_number`,
  `small_batch_size_splits_into_multiple_batches`) continue to pass
  green UNMODIFIED under `cargo test --package kaleidoscope-cli`.
  (Note: `malformed_json_line_returns_typed_error_with_line_number`
  uses 2 lines at `DEFAULT_BATCH_SIZE`, so it never flushed a batch
  before the abort — it still passes because the typed error and line
  number are preserved.)
- [ ] The new acceptance test file
  `crates/kaleidoscope-cli/tests/ingest_atomic.rs` is added (NEW
  file, mirroring the harness pattern of
  `tests/ingest_and_read_roundtrip.rs`) with assertions covering the
  five UAT scenarios above.
- [ ] No new external crate dependency is added to
  `crates/kaleidoscope-cli/Cargo.toml`. The only new `Cargo.toml`
  change is one `[[test]]` entry for the new test file.
- [ ] No new `Error` variant is added — the malformed-line case
  reuses the existing `Error::ParseRecord { line, source }` at
  `crates/kaleidoscope-cli/src/lib.rs:87-90`.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the store-count
  level (via `read`/`stats`) and the exit-code/stderr level on the
  `kaleidoscope-cli ingest <tenant> <data_dir>` invocation.
- **Does what**: after a failed ingest (a parse error on any line),
  observes a store count UNCHANGED from before the invocation (zero
  committed by the failed run) and a non-zero exit naming the
  offending line; after re-running a still-malformed input, observes
  the count STILL unchanged (no double); after fixing the named line
  and re-ingesting, observes every record committed exactly once and
  exit 0; for a fully-valid file, observes every record committed
  exactly once and exit 0 with no behaviour regression.
- **By how much**: 100% of ingests that hit a parse error commit
  ZERO records (the store count is byte-identical before and after
  the failed run) and exit non-zero naming the first offending line
  (OK1); 100% of re-runs of a still-malformed input leave the store
  count unchanged (no double, OK2); 100% of corrected-file ingests
  commit every record exactly once and exit 0 (OK3); 100% of
  fully-valid ingests are byte-equivalent in `IngestStats` and
  stderr summary to the pre-change behaviour (OK4, no regression).
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/ingest_atomic.rs` covering the four
  KPIs across the five UAT scenarios, PLUS the existing locked
  acceptance tests in `tests/ingest_and_read_roundtrip.rs` continuing
  to pass green UNMODIFIED.
- **Baseline**: 0% atomic today — the verifier reproduced (HEAD
  2e2ed58) run 1 committing 100 records on a 100-valid + malformed-101
  file (partial commit), and a re-run committing 200 (double-count).
  Both are 0% of the desired all-or-nothing behaviour.

Maps to OK1-ingest-parse-error-commits-nothing (principal),
OK2-ingest-rerun-no-double, OK3-ingest-corrected-file-once, and
OK4-ingest-valid-file-no-regression in `outcome-kpis.md`.

### Technical Notes

- The all-or-nothing mechanism is DESIGN-locked per
  `wave-decisions.md` D-BufferVsStream: (a) buffer all parsed
  `LogRecord`s in memory and flush only after the whole input
  validates (recommended for v0; the code already buffers per-batch
  at `crates/kaleidoscope-cli/src/lib.rs:200`), OR (b) a two-pass
  read (which needs a re-readable input — stdin at `main.rs:266-267`
  is one-shot, so this would require a file-path positional argument,
  a CLI shape change). The behaviour requirement (the four AC) is
  fixed; the mechanism is DESIGN's call.
- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — the `ingest`
  function's commit discipline (`:157-246`). The parse-then-flush
  interleaving at `:205-239` is replaced by validate-all-then-commit.
  The `flush` helper (`:248-266`), the `IngestStats` struct
  (`:128-134`), and the `Error::ParseRecord` variant (`:87-90`) are
  reused unchanged in shape.
- Possibly unmodified: `crates/kaleidoscope-cli/src/main.rs` — if
  DESIGN picks option (a) buffering, the binary's `run_ingest`
  (`:262-280`) and its stdin reader are unchanged. If DESIGN picks
  option (b) two-pass with a file-path argument, `run_ingest` and
  `print_usage` change — that is recorded against D-BufferVsStream,
  not mandated here.
- New test file: `crates/kaleidoscope-cli/tests/ingest_atomic.rs`,
  mirroring the harness of `tests/ingest_and_read_roundtrip.rs`
  (`tenant`, `record`, `temp_data_dir`, `cleanup`, `ndjson` helpers
  duplicated inline at v0 — rule-of-three extraction deferred, per
  cluster precedent).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains one
  `[[test]]` entry `name = "ingest_atomic", path =
  "tests/ingest_atomic.rs"`. No new dependency.
- Per-feature mutation testing: kill rate gate 100% on the modified
  files (`CLAUDE.md`, ADR-0005 Gate 5). The commit-discipline change
  is mutation-rich (the new ordering of parse-vs-commit); DESIGN /
  DELIVER must seed mutation-killing witnesses for the
  "commit-nothing-on-error" branch.
- Slice tag: not `@infrastructure` — this story directly changes an
  operator-visible behaviour (the store count after a failed ingest,
  and the safety of re-running) on a real CLI surface
  (`kaleidoscope-cli ingest <tenant> <data_dir>`).

### Dependencies

- `lumen::FileBackedLogStore::open` / `LogStore::ingest` / `query`
  already exist and are already used by `ingest`/`read`
  (`crates/kaleidoscope-cli/src/lib.rs:195-196, :259, :302`). No
  Lumen-side change is required (no dedup is added — that is
  D-DedupFuture, out of scope).
- `cinder::FileBackedTieringStore::open` / `place` already used by
  `ingest`'s `flush` (`crates/kaleidoscope-cli/src/lib.rs:197-198,
  :262`). The Cinder Hot-tier placement is held back along with the
  Lumen commit under the all-or-nothing discipline.
- `kaleidoscope_cli::Error::ParseRecord { line, source }` already
  exists (`crates/kaleidoscope-cli/src/lib.rs:87-90`) with the
  line-naming `Display` (`:112-114`). Reused; no new variant.
- `kaleidoscope_cli::IngestStats` already exists
  (`crates/kaleidoscope-cli/src/lib.rs:128-134`). Return shape
  unchanged.
- `aegis::TenantId`, `serde_json` already dependencies.
- No new external dependencies. No new internal crate dependencies.

### Out of Scope (deferred)

- **Success-case re-run dedup** (`wave-decisions.md` D-DedupFuture):
  ingesting the SAME fully-valid file twice still adds the records
  twice, because Lumen has no idempotency key. This is a SEPARATE,
  LARGER concern (ingest dedup) that touches the `lumen` bounded
  context, not just `kaleidoscope-cli`. Recommended as a future
  feature (`ingest-dedup-v0` or similar). THIS feature closes only
  the parse-error partial-commit and the parse-error re-run
  double-count — exactly what the verifier's K13 pins.
- **Mid-commit I/O / store-write failure atomicity**: this feature
  makes ingest all-or-nothing for the PARSE-FAILURE case (the K13
  footgun). It does NOT add transactional rollback for a Lumen/Cinder
  WRITE failure that occurs AFTER full parse validation has passed
  and committing has begun (e.g. a disk-full error on the second of
  three batches). That is a pre-existing failure mode of the
  underlying `LogStore::ingest` durability contract, not introduced
  or changed by this wave, and is out of scope here. Under the
  recommended buffer-all-then-flush mechanism the parse-error class
  (by far the most common, and the one the verifier pinned) is fully
  closed; mid-commit write-failure atomicity is a distinct and much
  rarer concern that belongs to the store-durability features (e.g.
  the shipped `store-fsync-durability-v0` /
  `wal-torn-tail-recovery-v0` line of work), not to CLI ingest
  validation.
