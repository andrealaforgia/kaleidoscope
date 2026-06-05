# Wave Decisions — `cli-ingest-atomic-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus
decisions recorded during DISCUSS. Origin: the black-box verifier
filed issue 009 (K13, RED-ish footgun) and the four-quadrants
assessment named it (kaleidoscope-cli Q2-MEDIUM). `kaleidoscope-cli
ingest` is NON-ATOMIC on a mid-stream parse error.

## Verifier reproduction (the pinned defect, HEAD 2e2ed58)

A file of 100 valid NDJSON `LogRecord`s followed by a malformed
line 101, ingested via `kaleidoscope-cli ingest <tenant> <data_dir>`
on stdin (`DEFAULT_BATCH_SIZE=100`, `crates/kaleidoscope-cli/src/lib.rs:70`):

1. Run 1 exits 1 (typed `Error::ParseRecord { line: 101, .. }`,
   `crates/kaleidoscope-cli/src/lib.rs:210-213`), naming the bad
   line — no corruption, clean typed abort. BUT the store count
   after the run is **100**, not 0: the first batch (lines 1-100)
   was already `flush`ed and committed to Lumen
   (`crates/kaleidoscope-cli/src/lib.rs:215-226` calls `flush`,
   which calls `lumen.ingest`, `crates/kaleidoscope-cli/src/lib.rs:259`)
   BEFORE the loop reached line 101 and aborted. This is the
   **partial commit**.
2. Re-running the SAME (still-malformed) input — the natural
   operator reaction to a failed ingest — exits 1 again AND the
   count after is now **200**: it DOUBLE-INGESTS the already-committed
   prefix, because Lumen has no dedup (each `LogBatch` ingest adds
   records unconditionally; there is no idempotency key on the
   ingest path).

So a malformed line midway through a file leaves earlier batches
committed, and the obvious recovery (re-run) silently doubles the
prefix. The command acknowledges a partial ingest as if it failed
cleanly (exit 1, named line) while having committed 100 records —
the acked-but-wrong shape the project's durability/honesty posture
forbids.

## Code verification (done in this wave, against HEAD)

| Fact | Location | Confirmed |
|------|----------|-----------|
| `DEFAULT_BATCH_SIZE = 100`, no flag to override it from the CLI | `crates/kaleidoscope-cli/src/lib.rs:70`; `main.rs:271` passes it as a literal | YES |
| The ingest loop parses each line, and on parse error returns immediately via `?` | `crates/kaleidoscope-cli/src/lib.rs:205-213` | YES |
| Batches that reach `buffer.len() >= batch_size` are `flush`ed (committed to Lumen + one Cinder Hot entry) DURING the loop, BEFORE later lines are parsed | `crates/kaleidoscope-cli/src/lib.rs:215-226`, `flush` at `:248-266` calls `lumen.ingest` at `:259` | YES — this is the partial-commit site |
| Lumen has no dedup: a re-ingest of the same records adds them again | `flush` builds a fresh `LogBatch::with_records` and calls `lumen.ingest` unconditionally; no idempotency key anywhere on the path | YES |
| The existing `malformed_json_line_returns_typed_error_with_line_number` test does NOT catch the footgun | `crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs:244-263`: only 2 lines, `DEFAULT_BATCH_SIZE` — the good record never reaches a full batch, so nothing flushes before the abort | YES — the bug is invisible to the current suite |
| The operator-invocable entry point | `kaleidoscope-cli ingest <tenant_id> <data_dir>`, NDJSON on stdin, stats to stderr `ingest ok: records=N batches=M tier_items=K` (`main.rs:262-279`) | YES |
| Count read-back surface | `kaleidoscope-cli stats <tenant> <data_dir>` first line `records=N` (`main.rs:359-380`, `stats_with_tiers` at `lib.rs:367-400`); or `read` returning a count (`lib.rs:279-311`) | YES |

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| D1 `feature_type` | `backend` (CLI ingest correctness) | No new persona, no new crate, no new subcommand, no new external dependency. The change is to the existing `ingest` library function's commit discipline. |
| D2 `walking_skeleton` | `no` | Brownfield. The CLI exists; `ingest` works for the all-valid case; the four-plus subcommands ship. This feature changes WHEN `ingest` commits, not WHAT the CLI is. |
| D3 `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the `kaleidoscope-cli` cluster). Single job: an ingest is all-or-nothing on a parse error. The behaviour change is precisely pinned by the verifier's K13 reproduction. |
| D4 `jtbd_analysis` | the all-or-nothing ingest job (below) | The job is singular and Earned-Trust framed. No DIVERGE wave artefacts; recorded as a LOW risk below. |

## The operator job (JTBD, Earned-Trust framing)

> When I ingest a file and a line partway through is malformed, the
> command commits NOTHING and tells me which line broke, so I can
> fix that line and re-run without either losing the good records or
> double-counting the ones that already went in. An ingest is
> all-or-nothing: it either takes the whole file or none of it.

- **Push**: a malformed line midway through a file leaves earlier
  batches committed (run 1 → count 100), and re-running doubles the
  prefix (run 2 → count 200). The operator cannot trust the count
  after a failed ingest and cannot safely retry.
- **Pull**: a single all-or-nothing ingest — a parse error commits
  zero, names the bad line; re-running the still-bad input commits
  zero again; fixing the line and re-running commits the whole file
  exactly once.
- **Anxiety**: "if I re-run after a failure, will I double-count?"
  (today: yes). The fix removes the anxiety: re-running a still-bad
  input is a no-op on the store count.
- **Habit**: the operator's reflex on a failed ingest is to re-run
  it. The fix makes that reflex SAFE rather than corrupting.

## The fix direction (decided; DESIGN owns the exact mechanism)

Make `ingest` ALL-OR-NOTHING for the parse-failure case by
VALIDATING the entire input (parsing every NDJSON line) BEFORE
committing any batch. If every line parses, ingest all batches; if
ANY line fails to parse, exit non-zero naming the offending line,
and commit NOTHING.

This closes K13 exactly:

- Run 1 against `100 valid + malformed line 101` commits **0** (not
  100), exits non-zero, names line 101.
- A re-run of the still-malformed input commits **0** again (no
  partial, no double).
- After the operator fixes line 101, the corrected file ingests
  **once** (every record committed exactly once), exits 0.
- Negative control: a fully-valid file ingests every record exactly
  once and exits 0 (no behaviour change for the all-valid path).

## In-wave decisions

### D-ValidateBeforeCommit: parse-validate the whole input before any commit

The required behaviour is: NO batch is committed to Lumen (and no
Cinder Hot-tier entry is placed) until EVERY line of the input has
been successfully parsed as a `LogRecord`. The first parse failure
aborts with a non-zero exit, names the offending 1-based line
number (preserving the existing `Error::ParseRecord { line, source }`
shape at `crates/kaleidoscope-cli/src/lib.rs:87-90, :112-114`), and
leaves the store count UNCHANGED from before the invocation.

This is a behaviour requirement, not a mechanism. DESIGN owns the
mechanism (see D-BufferVsStream).

### D-BufferVsStream: the memory-vs-streaming trade-off is DESIGN's call (FLAGGED)

> **Flag for DESIGN.** Validating the whole input before committing
> requires either (a) buffering all parsed `LogRecord`s in memory
> and then flushing them in batches once validation succeeds, or
> (b) a two-pass read (pass 1: parse every line, discard records,
> detect the first parse error; pass 2: re-read and ingest) which
> requires the input to be re-readable (a file path or a seekable
> reader), NOT a one-shot stdin pipe.

Constraints DESIGN must weigh:

- The current CLI reads NDJSON from **stdin** (`main.rs:266-267`,
  `BufReader::new(stdin.lock())`). A one-shot stdin stream is NOT
  re-readable, so option (b) two-pass either requires the operator
  to pass a file path argument (a CLI shape change) or requires
  buffering the raw bytes / parsed records anyway. The library
  `ingest` function takes `reader: impl BufRead`
  (`crates/kaleidoscope-cli/src/lib.rs:157-163`), which is
  single-pass.
- For v0 CLI ingest of an operator-provided file, buffering the
  whole input (option a) is **acceptable**: operator ingest files
  are bounded (the verifier's reproduction is 101 lines; realistic
  operator files are thousands to low-millions of records, not
  unbounded streams). The existing code already buffers up to
  `batch_size` records at a time
  (`crates/kaleidoscope-cli/src/lib.rs:200`); option (a) widens
  that buffer to the whole input.
- DESIGN decides whether to (a) buffer all parsed records and flush
  after full validation, or (b) do a two-pass read (and, if so,
  whether to change the CLI to accept a file-path positional
  argument instead of / in addition to stdin). Either mechanism
  satisfies the all-or-nothing behaviour requirement above; the
  wire-observable contract (run 1 commits 0, re-run commits 0,
  corrected file commits once, valid file commits once) is what
  this wave locks.
- Whichever mechanism DESIGN picks, the `IngestStats` return shape
  (`records_ingested`, `batches_flushed`, `tier_items_placed`,
  `crates/kaleidoscope-cli/src/lib.rs:128-134`) and the stderr
  summary line `ingest ok: records=N batches=M tier_items=K`
  (`main.rs:275-278`) MUST remain byte-equivalent for the all-valid
  path (the negative control), so no locked test regresses.

### D-DedupFuture: success-case re-run idempotency (dedup) is OUT OF SCOPE — a separate, larger future concern (FLAGGED)

> **Flag for DESIGN / future.** A SUCCESSFUL ingest re-run still
> double-ingests, because Lumen has no dedup. This wave does NOT
> address that.

This feature is scoped to the **all-or-nothing-on-parse-error** fix
the verifier's K13 pins. The success-case re-run problem (ingesting
the SAME fully-valid file twice still adds the records twice,
because there is no idempotency key on the Lumen ingest path) is a
SEPARATE, LARGER concern — ingest dedup — that belongs to a future
feature, for these reasons:

- A minimal idempotency key (e.g. a content hash per batch, or a
  per-file ingest-id recorded in Lumen) is a new persistent concept
  on the Lumen ingest path, not a CLI-local change. It touches the
  `lumen` crate's `LogStore` contract, not just `kaleidoscope-cli`.
- The verifier's K13 is specifically about the PARSE-ERROR case (run
  1 commits 100, re-run of the STILL-MALFORMED input commits 200).
  The all-or-nothing fix closes that exactly: re-running the
  still-malformed input commits 0, so the double-count on the
  parse-error path disappears. The remaining double-count is only on
  the fully-VALID re-run path — a different problem with a different
  shape and a much larger blast radius.
- Recommendation (this wave's judgement): scope THIS feature to
  all-or-nothing on parse error; flag success-case dedup as a future
  feature. A minimal idempotency key is NOT judged to be in scope
  here — it would balloon the slice past right-sized and pull in the
  `lumen` bounded context. DESIGN and the product owner can open a
  separate `ingest-dedup-v0` (or similar) when that problem is
  prioritised.

### D-ErrorShapePreserved: the typed parse error and its line number are preserved

The all-or-nothing change MUST preserve the existing typed error on
a malformed line: `Error::ParseRecord { line, source }`
(`crates/kaleidoscope-cli/src/lib.rs:87-90`), whose `Display`
renders `parse record at line {line}: {source}`
(`crates/kaleidoscope-cli/src/lib.rs:112-114`), with `line` being
the 1-based line number of the first offending line
(`crates/kaleidoscope-cli/src/lib.rs:210-212`, `idx + 1`). The
operator's stderr experience on a malformed line is unchanged
EXCEPT that the store count after the failed run is now 0 instead of
a partial commit. Exit code stays non-zero (the binary maps any
non-`UnknownFlag` `Error` to `ExitCode::FAILURE`, `main.rs:90`).

### D-BlankLinesStillSkipped: blank-line skipping is preserved

The existing blank-line skip (`crates/kaleidoscope-cli/src/lib.rs:207-209`,
`if line.trim().is_empty() { continue; }`) is preserved. Blank lines
are not records and do not count toward the line number of a later
malformed line in a way that changes the existing `blank_lines_in_input_are_skipped`
behaviour (`crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs:221-241`).
Note: the existing line-number reporting uses the raw `reader.lines()`
enumeration index (`idx + 1`), so blank lines DO count toward the
reported line number — that behaviour is preserved as-is (the
malformed-line test at `:244-263` reports `line: 2` for a malformed
second physical line). DESIGN must not silently change the
line-numbering basis.

### D-NoFlag: no new flag in v0

No `--batch-size`, no `--strict`, no `--validate-only`, no
`--dry-run`, no file-path positional argument is REQUIRED by this
wave. The all-or-nothing behaviour is the new DEFAULT (and only)
behaviour. (If DESIGN's chosen mechanism for D-BufferVsStream
needs a file-path positional argument for a two-pass read, that is a
DESIGN decision recorded against D-BufferVsStream, not a flag this
wave mandates.)

### D-NoSSOT: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as the predecessor features in the `kaleidoscope-cli`
cluster. This ingest-correctness fix serves the operator's
all-or-nothing ingest job, which is operationally important but does
not rise to an SSOT journey modification. The feature-local
artefacts are NOT promoted to `docs/product/journeys/` or
`docs/product/jobs.yaml`.

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. The job is implicit, singular, and pinned by the verifier's K13 reproduction: an ingest must be all-or-nothing on a parse error. There is exactly one reasonable behaviour (commit zero on any parse failure). | DIVERGE skipped. The behaviour is doubly constrained: by the verifier's exact reproduction and by the project's durability/honesty posture (an acked-but-wrong partial commit is forbidden). |
| No formal JTBD workshop | LOW. Persona, push (partial commit + double-count on re-run), pull (all-or-nothing ingest), anxiety ("will re-run double-count?"), habit (operator re-runs failed ingests) are all derivable from the verifier's reproduction and the cluster's inherited operator persona. | Persona + emotional-arc inherited from the `kaleidoscope-cli` cluster (`cli-list-items-subcommand-v0`, `cli-migrate-subcommand-v0`). |
| No standalone Three Amigos session | LOW. The reviewer pass at handoff replaces the workshop. The behaviour shape is constrained by the K13 reproduction (four checkable observables) and the existing `Error::ParseRecord` typed-error surface. | Peer review against `nw-po-review-dimensions` before handoff. |

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate),
1 modified file in `src/` (`lib.rs`'s `ingest` function — the
commit-discipline change), possibly 0 changes to `main.rs` (the
all-or-nothing behaviour is internal to `ingest` unless DESIGN's
D-BufferVsStream mechanism requires a CLI shape change), 1 new
acceptance test file
(`crates/kaleidoscope-cli/tests/ingest_atomic.rs`), 1 manifest
line-level change (`Cargo.toml` for the new `[[test]]` entry).
Estimated effort: well under 1 day. PASSES the right-sized gate. No
oversized signal: does not touch >3 bounded contexts (the dedup
concern that WOULD pull in the `lumen` context is explicitly
deferred per D-DedupFuture), does not need >10 stories, walking
skeleton not required (brownfield).

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions to lock:

- **D-BufferVsStream**: buffer-all-parsed-records-then-flush (option
  a) vs two-pass read (option b, which may need a file-path
  positional argument because stdin is not re-readable). The
  all-or-nothing behaviour requirement is fixed; the mechanism is
  DESIGN's call. Recommendation: option (a) buffering for v0 (the
  operator file is bounded; the code already buffers per-batch).
- **D-DedupFuture confirmation**: confirm success-case re-run dedup
  stays OUT of this feature and is opened as a separate future
  feature (`ingest-dedup-v0` or similar). Recommendation: confirm
  the deferral.
- **`IngestStats` / stderr summary byte-equivalence** for the
  all-valid path: confirm the negative-control path (`ingest ok:
  records=N batches=M tier_items=K`) is byte-equivalent before and
  after the change, so no locked test regresses.
