# Wave Decisions — `cli-ingest-atomic-v0` / DESIGN

> **Author**: `nw-solution-architect` (Morgan), DESIGN wave, 2026-06-05.
> **Mode**: PROPOSE (light). Scope: Application/components — a contained
> correctness change to the commit discipline of one existing free function,
> `kaleidoscope_cli::ingest` (`crates/kaleidoscope-cli/src/lib.rs:157-246`). No
> over-build: no new crate, no new trait, no new dependency, no new CLI
> subcommand, no new `Error` variant, no new public surface.
> **Inputs read**: `discuss/{user-stories.md, wave-decisions.md}`; the real code
> (`crates/kaleidoscope-cli/src/lib.rs` `ingest`/`flush`, `src/main.rs`
> `run_ingest`, `tests/ingest_and_read_roundtrip.rs`).

## DD-1 — D-BufferVsStream resolved: BUFFER-ALL-PARSED-THEN-FLUSH

**Decision.** Parse and validate the **entire** NDJSON input into an in-memory
`Vec<LogRecord>` FIRST; only AFTER the whole input parses successfully run the
existing batch-flush loop over the validated records. Buffer-all, not two-pass,
not streaming-with-rollback.

**One-line rationale.** It makes the parse-failure case all-or-nothing
*structurally* (no commit happens until validation passes, so there is nothing to
roll back), with no CLI shape change (stdin is one-shot and the library `ingest`
already takes a single-pass `reader: impl BufRead`), no new dependency, and no new
public surface — the smallest change that satisfies the locked contract.

**Alternatives weighed and rejected** (full text in ADR-0064):

- **Two-pass read — rejected.** Requires a re-readable input; stdin
  (`main.rs:266-267`) is one-shot, so two-pass would force a file-path positional
  argument (a CLI shape change DISCUSS D-NoFlag declined) or buffer the records
  anyway (= buffer-all with extra ceremony). Buys nothing over buffer-all.
- **Streaming-with-rollback — rejected.** Keep the interleaved flush and
  *compensate* committed batches on a later parse error. Rolling back committed
  `lumen.ingest` + `cinder.place` + Pulse writes is a three-store saga with no
  delete API on the ingest path: far more complex, far more failure modes (a
  rollback can itself fail mid-way), and inverts simplest-solution-first. Wrong v0
  trade — it adds machinery to *undo* a commit we can simply *defer*.

**Memory cost — known consequence, accepted for v0.** The whole input's
`Vec<LogRecord>` is held in RAM before any commit, vs a single `batch_size`
window. Acceptable: operator files are bounded (verifier reproduction is 101
lines; realistic files thousands to low-millions of records, comfortably in RAM);
the code already buffered a batch (`lib.rs:200`), this widens that buffer to the
file. A bounded-memory streaming-with-rollback alternative was weighed and
rejected above; if a future feature ingests unbounded streams, revisit (e.g.
stage to a temp WAL then atomic promote). Recorded, not a v0 concern.

## DD-2 — The exact new structure of `ingest` (two phases)

The single interleaved loop (`lib.rs:205-239`) becomes two sequential phases. The
store-open block (`lib.rs:164-198`: `create_dir_all`, the `otlp_log_path`
recorder wiring, `FileBackedLogStore::open`, `FileBackedTieringStore::open`) is
unchanged and stays first — these are pure `open`s, NOT commits.

**Phase 1 — parse-all (commits NOTHING).**

- Drain `reader.lines().enumerate()`; skip blank lines with the existing
  `if line.trim().is_empty() { continue; }` (D-BlankLinesStillSkipped preserved).
- Parse each non-blank line via `serde_json::from_str::<LogRecord>(&line)`.
- On the FIRST parse failure, return `Error::ParseRecord { line: idx + 1, source
  }` immediately — same `idx + 1` raw-enumeration basis as today (blank lines
  still count toward the reported number; the existing malformed test still
  reports `line: 2`). At this point no `flush`/`lumen.ingest`/`cinder.place`/Pulse
  has run, so the per-tenant store count is UNCHANGED.
- On success, accumulate into a `Vec<LogRecord>` holding the whole validated
  input.

**Phase 2 — flush-all (commits).**

- Only after Phase 1 succeeds, iterate the validated `Vec<LogRecord>` in chunks
  of `batch_size`, calling the **unchanged** `flush` helper (`lib.rs:248-266`)
  once per chunk: `lumen.ingest` + `cinder.place` (Hot, `batch-{seq:05}`) + Pulse
  self-observe + the three counters (`records_ingested`, `batches_flushed`,
  `tier_items_placed`), in the same order as today.
- Return `IngestStats { records_ingested, batches_flushed, tier_items_placed }`,
  same shape, same values for any given input.

**Line-number confirmation.** The typed parse error still names the 1-based line
via the unchanged `idx + 1` from `reader.lines().enumerate()`. The five AC and the
existing `malformed_json_line_returns_typed_error_with_line_number` test depend on
this; it is preserved.

**Byte-equivalence confirmation (negative control).** For a fully-valid file, the
chunk-of-`batch_size` Phase-2 loop produces the identical sequence of `flush`
calls in the identical order as today's full-batch-during-read plus trailing
flush — same number of batches (e.g. 250 → 3: 100+100+50), same `tier_items`,
same Pulse observations. Therefore `IngestStats` and the binary's stderr summary
`ingest ok: records=N batches=M tier_items=K` (`main.rs:275-278`) are
byte-equivalent before and after. Every locked test in
`tests/ingest_and_read_roundtrip.rs` passes green unmodified.

## DD-3 — D-DedupFuture confirmed: success-case dedup OUT OF SCOPE

Confirmed deferred. Re-ingesting the SAME *fully-valid* file twice still doubles
the records because Lumen has no idempotency key on the ingest path. That is a
SEPARATE, LARGER concern: a minimal idempotency key (content hash per batch, or a
per-file ingest-id) is a new persistent concept on the **`lumen` bounded
context's `LogStore` contract**, not a CLI-local change. Recommended as a future
feature `ingest-dedup-v0` (or similar). THIS feature closes only the
parse-error partial-commit and the parse-error re-run double-count — exactly what
the verifier's K13 pins. Buffer-all does NOT and is not intended to dedup the
fully-valid re-run path.

## DD-4 — Reuse Analysis (MANDATORY — re-ordering, not new components)

This is a **re-ordering** of machinery that already exists, not the addition of
any new component. No new crate, no new trait, no new module, no new dependency,
no new `Error` variant, no new public function.

| Existing machinery | Path | Decision |
|---|---|---|
| The parse step (`serde_json::from_str::<LogRecord>` + `Error::ParseRecord { line: idx+1 }`) | `lib.rs:210-213` | **REUSE verbatim**, moved into Phase 1. |
| The blank-line skip | `lib.rs:207-209` | **REUSE verbatim** in Phase 1. |
| `flush` (`lumen.ingest` + `cinder.place` Hot + Pulse self-observe + 3 counters) | `lib.rs:248-266` | **REUSE UNCHANGED**, called in Phase 2. |
| The per-batch buffering (`Vec<LogRecord>`, `buffer.len() >= batch_size`) | `lib.rs:200,215-239` | **EXTEND** — widen the buffer from one `batch_size` window to the whole input; the chunk-of-`batch_size` flush loop is the same flush logic, re-sequenced after parse-all. |
| Store opens + `otlp_log_path` recorder wiring | `lib.rs:164-198` | **REUSE UNCHANGED**, stays first (pure opens, not commits). |
| `IngestStats` struct | `lib.rs:128-134` | **REUSE UNCHANGED** — return shape and values identical. |
| `Error::ParseRecord { line, source }` + its `Display` | `lib.rs:87-90, 112-114` | **REUSE UNCHANGED** — no new variant. |
| `main.rs` `run_ingest` (stdin reader, stderr summary) | `main.rs:262-280` | **UNCHANGED** — buffer-all is internal to `ingest`; no CLI shape change. |

**Net new surface: NONE.** Only the parse-vs-flush ordering inside one function
body changes. `lumen.ingest`, `cinder.place`, the Pulse self-observe, and the
recorder wiring are reused unchanged.

## DD-5 — ADR warranted: ADR-0064 authored

An ADR **is** warranted and was authored:
`docs/product/architecture/adr-0064-cli-ingest-all-or-nothing-on-parse-error.md`.

Judgement: although the change is a contained single-function rewrite, it
realises an **Earned-Trust all-or-nothing commit contract** — a partial commit
acked-as-failed is the lie this removes — which sits squarely on the project's
durability/honesty lineage (ADR-0049/0059/0060). That lineage argues for an ADR
recording the contract ("CLI ingest is all-or-nothing on a parse error: validate
the whole input before committing any batch; a parse error commits nothing") and
the mechanism choice with its two rejected alternatives, so a future maintainer
can see *why* the function validates-before-committing rather than reverting to
the simpler interleaved loop. The ADR carries the two-alternatives-rejected
analysis (two-pass, streaming-rollback), the memory-cost consequence, and the
two out-of-scope items (dedup, mid-commit write atomicity).

## DD-6 — For Acceptance Designer (handoff to DISTILL)

**Driving port** (primary/inbound): `kaleidoscope-cli ingest <tenant> <data_dir>`
with NDJSON on **stdin**; equivalently the in-process library call
`ingest(tenant, data_dir, batch_size, reader, otlp_log_path)`. **Driven ports**
(outbound, all reused unchanged): Lumen `FileBackedLogStore` (`ingest`/`query`),
Cinder `FileBackedTieringStore` (`place`/`list_by_tier`), Pulse self-observe.
Count read-back surface: `kaleidoscope-cli stats <tenant> <data_dir>` (first line
`records=N`) / `read` (returns the count) / in-process `read(...)` /
`stats_with_tiers(...)` against the same `data_dir`. **Black-box only** — never
reach into private helpers; the count is observed via the shipped read surfaces.

Per-AC observables (the contract DISTILL realises in the NEW file
`crates/kaleidoscope-cli/tests/ingest_atomic.rs`, mirroring the harness of
`tests/ingest_and_read_roundtrip.rs`):

- **parse-error-commits-nothing** — 3 valid + malformed line 4 at `batch_size=3`
  (so the first batch WOULD flush before line 4 under the old interleaving):
  `ingest(...)` returns `Err(Error::ParseRecord { line, .. })` with `line == 4`,
  AND a follow-up `read`/`stats` against the same `data_dir` reports count **0** —
  the store record count is UNCHANGED from the pre-ingest zero (no partial). This
  is the minimal witness of "a full batch parsed-and-would-have-flushed, held
  back by the all-or-nothing discipline."
- **re-run-no-double** — invoking the same still-malformed input a SECOND time
  again returns `Err(ParseRecord { line: 4, .. })` and a follow-up `read`/`stats`
  STILL reports **0** (no partial from either run to double).
- **corrected-file-ingests-once** — line 4 corrected to a valid record (4 valid
  lines at `batch_size=3`): returns `Ok(IngestStats { records_ingested: 4,
  batches_flushed: 2, tier_items_placed: 2 })`, exit 0, follow-up `read`/`stats`
  reports exactly **4** (committed once — not 0, not 8).
- **valid-file-negative-control** — 250 valid records at `DEFAULT_BATCH_SIZE=100`:
  returns `Ok(IngestStats { records_ingested: 250, batches_flushed: 3,
  tier_items_placed: 3 })`, exit 0, follow-up reports **250**, AND `IngestStats`
  plus the stderr summary `ingest ok: records=250 batches=3 tier_items=3` are
  byte-equivalent to the pre-change behaviour (no regression).
- **malformed-first-line boundary** — first line malformed: returns
  `Err(ParseRecord { line, .. })` with `line == 1`, follow-up reports **0**.

Plus: the seven existing locked tests in `tests/ingest_and_read_roundtrip.rs`
pass green UNMODIFIED; no new external crate dependency (only one `[[test]]`
manifest entry); no new `Error` variant.

**No external integration; no contract-test recommendation.** The operator entry
point is the local CLI over stdin; the only dependencies the change reaches are
the local Lumen/Cinder/Pulse stores (already in-tree, already tested). No
third-party API, webhook, or OAuth provider — nothing to annotate for
consumer-driven contract tests.

## Artefacts written

- `docs/product/architecture/adr-0064-cli-ingest-all-or-nothing-on-parse-error.md`
  (NEW).
- `docs/product/architecture/brief.md` — appended
  `## Application Architecture — cli-ingest-atomic-v0` section (note + For
  Acceptance Designer + Reuse Analysis; no C4 update — no new container, port, or
  external system; the existing kaleidoscope-cli C4 stands).
- `docs/feature/cli-ingest-atomic-v0/design/wave-decisions.md` (this file).

## Self-review (reviewer dispatch)

`@nw-solution-architect-reviewer` was attempted; if not invocable from this
subagent context, a structured self-review against `nw-sa-critique-dimensions`
was performed and a top-level reviewer run is flagged (see the parent report).
Summary of the self-review: no resume-driven complexity (the change REMOVES
machinery-creep risk by deferring two larger concerns); ADR-0064 carries context,
2 rejected alternatives, and consequences; the simplest-solution default holds
(buffer-all is the minimal mechanism, streaming-rollback explicitly rejected as
over-built); testability is strong (black-box through the driving port + read
surfaces; the `batch_size=3` witness exercises the previously-invisible footgun);
priority is data-justified (the verifier's HEAD-2e2ed58 reproduction quantifies
the defect: run 1 → 100, re-run → 200, both 0% of the all-or-nothing target).
