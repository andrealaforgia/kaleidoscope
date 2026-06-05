# Evolution archive — cli-ingest-atomic-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md` and
`beacon-sighup-reload-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0064
  (`docs/product/architecture/adr-0064-cli-ingest-all-or-nothing-on-parse-error.md`),
  in the Earned-Trust lineage of ADR-0049 (write path honours fsync),
  ADR-0059 (WAL torn-tail recovery) and ADR-0060 (store fsync durability).
- Closes: the black-box verifier's issue 009 (K13), the four-quadrants
  Q2-MED finding. It was the NEXT item on the carried project-wide
  follow-up list.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `f03b22e` | CLI ingest must be all-or-nothing, no partial commit |
| design | `8f388a5` | ADR-0064, buffer-all-parsed-then-flush |
| devops | `6550dac` | slim wave, existing CLI gates cover |
| distill | `535882d` | 5 ingest_atomic acceptance scenarios |
| feat | `fdfbc28` | ingest is all-or-nothing, parse-all then flush-all |
| docs | `2ae2b95` | narrative + slide closure |

## The problem, in Earned-Trust framing

`kaleidoscope-cli`'s ingest read its NDJSON input and flushed each full
batch to lumen DURING the read (`crates/kaleidoscope-cli/src/lib.rs`,
around line 259), before the later lines of the file had been parsed. A
single malformed line midway through a file therefore left the earlier
batches already committed to lumen while the command exited `Err` as if
it had failed. Two distinct lies followed from this one ordering.

- A partial commit acked as a clean failure. The store held the good
  prefix, but the operator saw a non-zero exit and a parse error and was
  entitled to believe nothing had landed. A command that says it failed
  while having committed half its input is the substrate lie the
  project's Earned-Trust principle exists to forbid.
- A re-run double-ingested the good prefix. lumen has no dedup, so an
  operator who fixed the malformed line and re-ran the corrected file
  ingested the already-committed prefix a second time. The honest
  recovery action (fix and re-run) silently corrupted the store.

This is the verifier's issue 009 (K13) and the four-quadrants Q2-MED
finding. It sits in the same Earned-Trust lineage as the fsync and
WAL-recovery features before it: ADR-0049, ADR-0059 and ADR-0060. The
contract the feature restores is all-or-nothing ingest: the command
either commits the whole input or commits nothing, and never acks a
partial commit as a failure.

## The architecture decision

### ADR-0064, buffer-all-parsed-then-flush

The fix is a pure re-ordering of the ingest function body into two
phases.

- Phase 1 parses the ENTIRE NDJSON input into a validated
  `Vec<LogRecord>` first. The first bad line returns
  `Err(ParseRecord{line})` naming the 1-based line number, with NOTHING
  committed to any store.
- Phase 2 runs only after the whole input validates. It is the existing
  batch loop, unchanged: `lumen.ingest` plus `cinder.place` plus the
  pulse self-observation, per batch, over the already-validated records.

Because phase 2 is reached only with a fully valid `Vec<LogRecord>` in
hand, there is no control-flow path that commits a partial input. The
parse-error case never reaches the flush loop; the flush loop runs only
on a wholly valid input.

### Streaming-with-rollback rejected

A streaming ingest that committed batches as it read and then deleted
them on a later parse error was rejected. lumen exposes no ingest delete
API, so the rollback would require a three-store compensation saga across
lumen, cinder and pulse. A compensation saga is the wrong v0 trade for a
CLI ingest: it adds a delete path and a partial-failure recovery surface
to three stores to avoid holding the input in memory, when the input is
operator-supplied files that fit in memory comfortably.

### Two-pass rejected

A two-pass approach (read once to validate, read again to commit) was
rejected because ingest reads stdin, which is a one-shot stream. There is
no second pass over stdin to take. The single read must therefore hold
its parsed result, which is exactly what buffer-all-then-flush does.

## The accepted honest consequence

The chosen design holds the whole input's parsed records in RAM before
committing any of them. This is recorded plainly as the accepted cost,
not hidden: for operator-supplied files it is fine, and it is the
honest, simplest v0 trade against the rejected saga. For very large
inputs it is a real bound, and the future ingest-bounded-memory feature
(a temp-WAL staging stage, or a max-records cap that streams in bounded
chunks) is the place to lift it. The CLI help text and the README should
carry a file-size note so an operator is not surprised by the memory
profile of a very large ingest. This is the consequence accepted
knowingly, with its mitigation already named.

## The happy-path latency note

Parse-all defers the first flush until the whole input has parsed, so the
first batch lands later in wall-clock time than it did under the
streaming order. `IngestStats` is byte-equivalent before and after: the
same records, the same counts, the same batches. Only the timing of the
first flush shifts. There is no observable change to the success-path
result, only to when within the run the first commit occurs.

## The test lesson worth recording

This is the load-bearing lesson of the feature, recorded in the same
spirit as the prior archives' honest-finding sections: a test can be true
and prove nothing.

The existing malformed-line test
(`crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs`, around
lines 244 to 263) was GREEN the whole time the footgun was REAL. It used
2 input lines against `DEFAULT_BATCH_SIZE = 100`, so the malformed line
aborted the read before any batch reached the flush threshold. Nothing
was ever flushed before the abort, so the test never exercised the
partial-commit path it appeared to guard. It asserted a true fact (a
malformed line returns an error) while proving nothing about the
all-or-nothing property, because its fixture could not reach the bug.

The new tests use a `batch_size = 3` witness with the malformed line at
position 4, so the first batch (lines 1 to 3) flushes BEFORE the abort at
line 4. This fixture actually exercises the partial commit: under the old
ordering the store would hold 3 records after a failing run, and the test
can see it.

The DISTILL run-not-guess classification earned its keep here. It caught
that the corrected-file-ingests-once scenario is RED today as well, not
only the partial-commit scenario, because a failed run dirties the store:
without the all-or-nothing fix, a failing first run leaves the good
prefix committed, so a later corrected-file run double-counts the prefix.
Classifying the scenario by running it rather than guessing its colour is
what surfaced that the dedup-on-re-run expectation was entangled with the
partial-commit defect and not a separate, already-green property.

## Verification

- 5 `ingest_atomic` acceptance scenarios
  (`crates/kaleidoscope-cli/tests/ingest_atomic.rs`). Three were
  RED-`#[ignore]`d at DISTILL and un-ignored at DELIVER as the fix landed
  (`parse_error_commits_nothing`,
  `re_run_..._does_not_double_count`,
  `corrected_file_ingests_every_record_exactly_once`); two are GREEN
  guardrails (`fully_valid...`, `malformed_first_line...`). The three
  un-ignored scenarios use the `batch_size = 3` witness so they genuinely
  exercise the partial-commit path the old test could not reach.
- The existing roundtrip suite stays green, with 1-based line numbering
  preserved (`malformed_json_line_returns_typed_error_with_line_number`
  stays green): the re-ordering changed when records flush, not how a bad
  line is reported.
- 100% mutation kill on `crates/kaleidoscope-cli/src/lib.rs` (ADR-0005
  Gate 5; CLAUDE.md per-feature 100%): `cargo mutants` reported 169/169
  viable mutants caught, 0 missed. The existing CLI `--in-diff` mutation
  job picks up the diff; no new CI job was needed.
- Gate 2 (`cargo public-api`) unchanged: the ingest signature and the
  `IngestStats` type are byte-identical before and after. No surface
  change, zero new components, `main.rs` untouched. The whole feature is
  a re-ordering of one function body.

## The honest finding

The honest finding is the test lesson above, stated as a finding. The
production change was small and self-contained: a re-ordering of one
function into parse-all then flush-all, no new component, no signature
change. The difficulty was not in the fix. It was in the realisation
that the codebase already carried a green test over the exact failure the
feature exists to close, and that the green was an artefact of a fixture
too small to reach the bug. The value of recording this is the same as
the prior archives': the difficulty was not where a first glance put it.
It was in building a fixture that could actually witness the partial
commit, and in the run-not-guess classification that caught a second RED
scenario hiding behind the first.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted.

1. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. This feature
   removed the partial-commit double-ingest (the failure path), but the
   success-path re-run double is a separate, still-open defect. The
   verifier was told to scope any re-run-doubles expectation to the
   SUCCESS path as 009-adjacent, NOT as issue 009 itself. This is the
   designed extraction (DD-3): success-case dedup earns its own slice.
   Open.

2. cinder-wal-error-surfacing-v0. cinder's `place()` and `evaluate_at()`,
   and sluice, swallow the result of `append_wal` rather than surfacing
   it (`crates/cinder/src/file_backed.rs`, the `if let Err(_e)` and
   `let _ =` sites). A failed durable append on these paths is silently
   dropped, itself a residual substrate lie now that the append is
   fsync-honest. Four-quadrants backlog item 4. This is the NEXT item.
   Open.

3. ingest-bounded-memory. The buffer-all-then-flush design holds the
   whole input's records in RAM before commit. For very large inputs this
   is a real bound; a future feature lifts it with a temp-WAL staging
   stage or a max-records streaming cap, and adds the file-size note to
   the CLI help and README. Recorded as the accepted consequence of
   ADR-0064. Open.

4. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

5. ADR-0059 Decision 8 layer b, the AST structural check, remains
   UNWIRED. The structural pre-commit check asserting in-scope stores
   delegate to the shared wal-recovery routine and retain no inline
   replay loop; the tool choice was deferred to DELIVER and remains
   deferred. It is feedback, not a gate, consistent with the pure
   trunk-based, no-required-checks posture; when wired it belongs in the
   local pre-commit stage. Open.

6. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

7. beacon SLO unreachable (B06). The beacon SLO as specified is not
   reachable by the current implementation; the SLO MWMBR synthesis the
   verifier left for later is still outstanding. Open.

8. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

9. The two claims-honesty DOCUMENT items remain future features if
   wanted. The actual Prometheus-stepped grid for `query_range` (a
   query-api feature) and real gRPC-prefix honouring for `harness`
   (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
   built; each would retire its respective pin. Open only if wanted.
