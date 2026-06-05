# ADR-0064 — CLI ingest is all-or-nothing on a parse error: validate the whole input before committing any batch

- **Status**: Accepted
- **Date**: 2026-06-05
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `cli-ingest-atomic-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0049 (`earned-trust-fsync-probe-v0`), ADR-0059
  (`wal-torn-tail-recovery-v0`), ADR-0060 (`store-fsync-durability-v0`) — the
  Earned-Trust durability/honesty lineage this ADR extends to the CLI ingest
  *commit discipline*. Those ADRs make an acknowledged write **durable** (bytes
  on stable storage; the intact acked prefix recoverable past a torn tail). This
  ADR closes the dual lie one layer up: a command that **acknowledges failure**
  (non-zero exit, named line) must not have **committed a partial success**. A
  partial commit acked-as-failed is the dishonesty this removes; it is the same
  family of "the system must not lie to the operator" the fsync line establishes
  on the durability axis. ADR-0005 (the five CI gates including Gate 5 100%
  mutation kill on modified files and Gate 2 `cargo public-api` byte identity —
  the `ingest` public signature is unchanged, so Gate 2 holds by construction).

## Context

`kaleidoscope-cli ingest <tenant> <data_dir>` reads NDJSON `lumen::LogRecord`
from **stdin**, batches them in groups of `DEFAULT_BATCH_SIZE = 100`, and for
each full batch calls `flush` — which `lumen.ingest`s the batch, places one
Cinder Hot-tier entry, and records a Pulse self-observation
(`crates/kaleidoscope-cli/src/lib.rs:157-266`).

The defect (verifier issue 009 / K13, four-quadrants `kaleidoscope-cli`
Q2-MEDIUM): the single `ingest` loop **interleaves** parse and flush
(`lib.rs:205-239`). A full batch is `flush`ed and **committed to Lumen DURING
the read** (`lib.rs:215-226` → `flush` → `lumen.ingest` at `:259`), *before* the
loop reaches and parses a later line. A malformed line at position N therefore
aborts via `?` (`:210-213`) *after* every batch before N is already committed.

Concretely, a file of 100 valid records + a malformed line 101 at
`DEFAULT_BATCH_SIZE=100`:

1. **Partial commit**: run 1 exits non-zero with `Error::ParseRecord { line:
   101, .. }` (honest about the bad line) but the store now holds **100**
   records, not 0 — the first batch was already committed.
2. **Double-count on the obvious recovery**: the universal operator reflex on a
   failed batch job is to re-run it. Re-running the still-malformed file commits
   the prefix **again** (Lumen has no idempotency key on the ingest path), so the
   count goes to **200** — from a file that never ingested even once.

The command acknowledged a partial ingest as if it had failed cleanly, then
punished the recovery with silent duplication. This is precisely the
acked-but-wrong shape the project's durability/honesty posture forbids.

The existing acceptance test
`malformed_json_line_returns_typed_error_with_line_number`
(`tests/ingest_and_read_roundtrip.rs:244-263`) does **not** catch this: it uses
only 2 lines at `DEFAULT_BATCH_SIZE`, so the single good record never reaches a
full batch and nothing flushes before the abort. The footgun is invisible to the
current suite — it surfaces only when at least one full batch flushes BEFORE the
malformed line.

The required behaviour (DISCUSS D-ValidateBeforeCommit): NO batch is committed to
Lumen and NO Cinder Hot-tier entry is placed until EVERY line of the input has
parsed as a `LogRecord`. The first parse failure aborts non-zero naming the
1-based line, leaving the store count UNCHANGED. The mechanism was flagged to
DESIGN (D-BufferVsStream). This ADR records the mechanism decision.

## Decision

**Make `ingest` all-or-nothing for the parse-failure case by buffering all
parsed records first, then flushing — a two-phase rewrite of the function body
with no CLI shape change and no new public surface.**

The single interleaved loop becomes two sequential phases:

- **Phase 1 — parse-all (no commit).** Drain `reader.lines()`, skip blank lines
  exactly as today (`line.trim().is_empty() { continue }`), and parse every
  non-blank line into an in-memory `Vec<LogRecord>`. On the first parse failure,
  return `Error::ParseRecord { line: idx + 1, source }` **immediately** — using
  the same raw `reader.lines()` enumeration index basis as today (so the reported
  line number, including blank lines in the count, is byte-identical). At this
  point **nothing has been committed**: the Lumen and Cinder stores have been
  opened (their WAL/snapshot files may exist) but no `lumen.ingest` and no
  `cinder.place` has run, so the per-tenant record count is unchanged.
- **Phase 2 — flush-all (commit).** Only after the whole input has parsed, run
  the existing batch loop over the validated `Vec<LogRecord>`: chunk into groups
  of `batch_size`, calling the **unchanged** `flush` helper (`lib.rs:248-266`:
  `lumen.ingest` + `cinder.place` + Pulse self-observe + the three counters) once
  per chunk, in the same order, producing the same `IngestStats`.

The store-open calls (`FileBackedLogStore::open`, `FileBackedTieringStore::open`,
the recorder wiring incl. the `otlp_log_path` branch) stay where they are; they
are pure `open`s, not commits. The function signature, the `IngestStats` shape,
the `Error` enum, and `main.rs` are all unchanged.

### Why buffer-all-then-flush (and not the two alternatives)

**Alternative A — two-pass read (rejected).** Pass 1 parses every line and
detects the first error; pass 2 re-reads and ingests. This requires the input to
be **re-readable**. The CLI reads from `stdin` (`main.rs:266-267`,
`BufReader::new(stdin.lock())`) and the library takes `reader: impl BufRead`
(`lib.rs:157-163`) — both one-shot. Two-pass would force either (a) a new
file-path positional argument (a CLI shape change DISCUSS D-NoFlag declined for
v0), or (b) buffering the bytes/records anyway — which is just buffer-all with
extra ceremony. Rejected: it buys nothing over buffer-all for a one-shot stdin
and costs a CLI surface change.

**Alternative B — streaming-with-rollback (rejected).** Keep the interleaved
flush, but on a later parse error *compensate* the already-committed batches
(delete from Lumen, un-place from Cinder, un-observe in Pulse). Rejected for v0:
rolling back committed `lumen.ingest` + `cinder.place` + Pulse writes is a
distributed-compensation (saga) problem across three stores with no delete API on
the ingest path — far more complex, far more failure modes (a rollback can itself
fail mid-way, leaving a worse state than the partial commit it was undoing) than
buffer-then-commit, and it inverts the simplest-solution-first default. It is the
wrong v0 trade by a wide margin: it adds machinery to *undo* a commit we can
simply *defer*.

**Chosen — buffer-all-then-flush.** The validate-then-commit ordering makes the
parse-failure case all-or-nothing **structurally** — there is no commit to undo
because no commit happens until validation passes. It needs no CLI change (stdin
stays one-shot; the function already takes a single-pass reader), no new
dependency, no new `Error` variant, and no new public API. The code already
buffers up to `batch_size` records at a time (`lib.rs:200`); this widens that
buffer to the whole input. It is the smallest change that satisfies the contract.

## Consequences

### Positive

- **All-or-nothing on parse error, structurally.** A parse error commits ZERO
  records (the count is byte-identical before and after the failed run); the
  re-run of a still-bad input is a no-op on the count (no partial to double); a
  corrected file commits every record exactly once. Closes K13 exactly.
- **No CLI shape change, no new surface.** `ingest`'s signature, `IngestStats`,
  the `Error` enum, and `main.rs`/`run_ingest` are untouched. Gate 2 (`cargo
  public-api` byte identity) holds by construction. The all-valid path's
  `IngestStats` and the stderr summary `ingest ok: records=N batches=M
  tier_items=K` are byte-equivalent before and after — the negative control.
- **Typed error preserved.** The malformed-line case reuses
  `Error::ParseRecord { line, source }` (`lib.rs:87-90`) with the same `idx + 1`
  line-number basis (blank lines still count toward the number, per
  D-BlankLinesStillSkipped). The operator's stderr experience is unchanged except
  the post-failure store count is now 0, not a partial.
- **Reuse, not reinvention.** `flush`, `lumen.ingest`, `cinder.place`, the Pulse
  self-observe, and the `otlp_log_path` recorder wiring are all reused unchanged;
  the only change is the *ordering* of parse-vs-flush inside one function body.

### Negative (accepted, flagged)

- **Whole-input memory cost.** The entire input's `Vec<LogRecord>` is held in RAM
  before any commit, rather than a single `batch_size` window. **Accepted for
  v0**: operator ingest files are bounded (the verifier reproduction is 101
  lines; realistic files are thousands to low-millions of records, comfortably in
  RAM), and the code already buffered a batch — this extends that buffer to the
  file. If a future feature must ingest unbounded streams, revisit with a
  bounded-memory mechanism (e.g. staging to a temp WAL then atomic promote);
  recorded here as the known consequence, not a v0 concern.

### Out of scope (recorded, not addressed here)

- **Success-case re-run dedup (D-DedupFuture).** Re-ingesting the SAME
  *fully-valid* file twice still doubles the records, because Lumen has no
  idempotency key. That is a SEPARATE, LARGER concern touching the `lumen`
  bounded context's `LogStore` contract (a content hash or per-file ingest-id),
  not a CLI-local change. Deferred to a future `ingest-dedup-v0`. THIS feature
  closes only the parse-error partial-commit and the parse-error re-run
  double-count — exactly what K13 pins.
- **Mid-commit write-failure atomicity.** This feature makes ingest all-or-nothing
  for the PARSE-FAILURE case. It does NOT add transactional rollback for a
  Lumen/Cinder *write* failure that occurs AFTER full parse validation has passed
  and Phase 2 committing has begun (e.g. disk-full on the second of three
  batches). That is a pre-existing property of the underlying store durability
  contract (the ADR-0059/0060 line of work), unchanged by this wave.

## Enforcement

No new architectural-rule tooling is warranted: the change is a single-function
body rewrite inside one crate, with no new module boundary or dependency
direction to enforce. The behavioural contract is enforced by the new acceptance
file `crates/kaleidoscope-cli/tests/ingest_atomic.rs` (the five UAT scenarios)
plus the existing locked suite in `tests/ingest_and_read_roundtrip.rs` continuing
green unmodified (the negative control), and by per-feature mutation testing (Gate
5, 100% kill on the modified file) — the parse-vs-commit re-ordering is
mutation-rich, so DISTILL/DELIVER must seed a witness that a full batch
"would-have-flushed" under the old interleaving is held back (the `batch_size=3`,
3-valid-plus-malformed-line-4 case is the minimal such witness).

## Review follow-ups (nw-solution-architect-reviewer, iteration 1)

The independent review APPROVED the decision (0 critical, no blockers) and
recorded three follow-ups, none of which change the v0 mechanism. They are
captured here so the trade-offs are explicit.

1. Unbounded in-memory buffer (the reviewer's one HIGH, accepted for v0).
   Phase 1 holds the whole input's parsed records in a Vec before Phase 2
   commits. For the operator-provided files this command targets (thousands
   to low-millions of records) this sits comfortably in RAM, and the prior
   code already buffered a batch; this widens that buffer to the file. A
   very large input (order 100M+ records) is an OOM risk. Acceptable for v0,
   not hidden: a future ingest-bounded-memory feature (a temp-WAL stage then
   atomic promote, or a max-records cap) revisits it, and the CLI help/README
   should carry a file-size guidance note. The streaming-with-rollback
   alternative was rejected because rolling back already-committed
   lumen/cinder/pulse writes is far more complex than deferring the commit.

2. Happy-path latency profile (MEDIUM, a note not a blocker). Validating the
   whole input before the first flush defers all lumen/cinder writes to after
   the parse, where the prior code interleaved them. The IngestStats and the
   stderr summary are byte-identical for a fully-valid file (the negative
   control), so there is no observable-output regression; only the timing of
   the writes shifts. Acceptable for operator-provided files; a latency-
   sensitive-ingest feature would revisit if streaming commit timing ever
   matters.

3. Blank-line line numbering (LOW, confirmed correct). Phase 1 keeps the
   1-based line number from reader.lines().enumerate() including skipped
   blank lines, so the typed ParseRecord{line} and the existing
   line-number test are preserved unchanged.
