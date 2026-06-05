# Outcome KPIs — `cli-ingest-atomic-v0`

## Feature: all-or-nothing ingest on parse error

### Objective

When an operator ingests a file with a malformed line, the command
commits NOTHING and names the bad line, so the operator can re-run
safely and fix-and-re-ingest without losing the good records or
double-counting the prefix. An ingest takes the whole file or none of
it.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1 | Platform operator (Priya) | After an ingest hits a parse error, observes a store count UNCHANGED from before the run (zero committed) and a non-zero exit naming the first offending line | 100% of parse-error ingests commit zero records | 0% — verifier reproduced run 1 on 100-valid+malformed-101 committing 100 records (partial commit) | New acceptance test `tests/ingest_atomic.rs` (parse-error-commits-nothing): assert `Err(ParseRecord{line})` + post-call `read`/`stats` count == pre-call count | Leading (Outcome) |
| OK2 | Platform operator (Priya) | After re-running a still-malformed input (the operator's reflex on a failed ingest), observes the store count STILL unchanged (no double) | 100% of re-runs of a still-malformed input leave the count unchanged | 0% — verifier reproduced re-run committing 200 (double-count) | `tests/ingest_atomic.rs` (re-run-no-double): assert two successive `ingest` calls on the same malformed input both `Err`, count stays 0 | Leading (Outcome) |
| OK3 | Platform operator (Priya) | After fixing the named line and re-ingesting the corrected file, observes every record committed exactly once and exit 0 | 100% of corrected-file ingests commit every record exactly once | n/a (today the corrected file would also stack on top of any earlier partial commit, so the final count would be wrong) | `tests/ingest_atomic.rs` (corrected-file-ingests-once): assert `Ok(IngestStats{records_ingested:4,..})` + post-call count == 4 | Leading (Outcome) |
| OK4 | Platform operator (Priya) | For a fully-valid file (no malformed line), observes every record committed exactly once and exit 0 with no regression in `IngestStats` or stderr summary | 100% byte-equivalence with pre-change behaviour on the all-valid path | 100% (already correct today — this is the no-regression guardrail) | `tests/ingest_atomic.rs` (valid-file-negative-control) + existing locked `tests/ingest_and_read_roundtrip.rs` passing UNMODIFIED | Guardrail |

### Metric Hierarchy

- **North Star**: parse-error ingests that commit ZERO records (OK1)
  — the single behaviour that closes the verifier's K13 footgun. If
  this holds, the partial commit disappears, and with it the
  parse-error-path double-count on re-run (OK2 is a corollary).
- **Leading Indicators**: re-run-no-double (OK2) and
  corrected-file-ingests-once (OK3) — the two downstream operator
  experiences that the all-or-nothing discipline enables. They
  predict the north star: if OK1 holds, OK2 and OK3 follow from the
  same single discipline change.
- **Guardrail Metrics**: valid-file-negative-control (OK4) — the
  all-valid ingest path's `IngestStats`
  (`records_ingested`/`batches_flushed`/`tier_items_placed`,
  `crates/kaleidoscope-cli/src/lib.rs:128-134`) and stderr summary
  line (`ingest ok: records=N batches=M tier_items=K`,
  `main.rs:275-278`) MUST NOT change. The seven existing locked tests
  in `tests/ingest_and_read_roundtrip.rs` MUST continue to pass green
  unmodified.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| OK1 parse-error-commits-nothing | `tests/ingest_atomic.rs` | `cargo test --package kaleidoscope-cli` (CI per ADR-0005; feedback, not gate, per `project_kaleidoscope_pure_trunk_based`) | Every push / PR | DELIVER (crafter) |
| OK2 re-run-no-double | `tests/ingest_atomic.rs` | `cargo test --package kaleidoscope-cli` | Every push / PR | DELIVER (crafter) |
| OK3 corrected-file-ingests-once | `tests/ingest_atomic.rs` | `cargo test --package kaleidoscope-cli` | Every push / PR | DELIVER (crafter) |
| OK4 valid-file-no-regression | `tests/ingest_atomic.rs` + locked `tests/ingest_and_read_roundtrip.rs` | `cargo test --package kaleidoscope-cli` (locked files must stay green unmodified) | Every push / PR | DELIVER (crafter) |
| Mutation kill rate (Gate 5) | `cargo mutants` scoped to modified files | Per-feature mutation run after refactoring (`CLAUDE.md`, ADR-0005 Gate 5) | Per delivery | DELIVER (crafter) |

### Hypothesis

We believe that making `kaleidoscope-cli ingest` validate the whole
input before committing any batch (all-or-nothing on parse error) for
the platform operator will achieve a trustworthy store count after a
failed ingest and a safe re-run. We will know this is true when the
operator's parse-error ingests commit zero records (OK1), re-runs of
still-malformed input do not double the count (OK2), corrected files
ingest exactly once (OK3), and the fully-valid ingest path is
byte-equivalent to today (OK4) — all four asserted by
`tests/ingest_atomic.rs` and the locked roundtrip suite.

### Handoff to DEVOPS

No new runtime instrumentation is required for this feature. The KPIs
are verified by acceptance tests at build time, not by production
telemetry. The platform-architect needs nothing new here beyond the
existing `cargo test` / `cargo mutants` CI feedback already in place
for the `kaleidoscope-cli` crate. (The `--observe-otlp` metric stream
on `ingest` is unaffected — it emits per-batch on commit; under
all-or-nothing, a failed ingest emits zero commit-side metric lines,
consistent with committing zero records.)
