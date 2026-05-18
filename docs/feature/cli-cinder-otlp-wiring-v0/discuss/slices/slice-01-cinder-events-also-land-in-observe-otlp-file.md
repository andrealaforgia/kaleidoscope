# Slice 01 — Cinder events also land in the `--observe-otlp` file

**Story**: US-01
**Outcome KPIs**: OK6-CLI-cross-writer-ndjson (principal),
OK7-CLI-cinder-events-present, OK8-CLI-no-regression
**Tag**: operator-visible (not `@infrastructure` — the CLI surface is
the real user-invocable entry point)
**Estimated effort**: well under 1 day

## Goal

Replace the `cinder::NoopRecorder` constructed in the `Some(path) =>
{ … }` arm of the `otlp_log_path` match inside `kaleidoscope_cli::ingest`
(`crates/kaleidoscope-cli/src/lib.rs:147-163`) with a
`CinderToOtlpJsonWriter` constructed against the same operator-supplied
file path the Lumen writer is already wired against. The result: a
single `kaleidoscope-cli ingest acme /tmp/data --observe-otlp
/tmp/foo.ndjson < records.json` invocation produces a file whose lines
interleave `lumen.ingest.count` and `cinder.place.count`,
remains valid line-by-line JSON terminated by `\n`, and crosses the
concurrent-random-pause probe mandated by ADR-0039 §7.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | The `Some(path) => { … }` arm of the `otlp_log_path` match (currently lines 147-160) gains a parallel construction site for `CinderToOtlpJsonWriter`. The Cinder recorder at line 163 becomes a conditional `Box<dyn cinder::MetricsRecorder + Send + Sync>` driven by the same `otlp_log_path` value (DESIGN picks the file-sharing mechanism). |
| `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` | NEW. Mirrors the harness pattern in `observe_otlp_flag.rs`. Hosts the happy-path test (OK7) and the concurrent-random-pause test (OK6). |
| `crates/kaleidoscope-cli/Cargo.toml` | New `[[test]] name = "observe_otlp_cinder_wiring", path = "tests/observe_otlp_cinder_wiring.rs"`. No new external dep — `self-observe` is already a dependency and re-exports `CinderToOtlpJsonWriter` after `cinder-to-otlp-json-bridge-v0` shipped. |

## IN scope

- The `ingest` subcommand only.
- The `place` event only (the CLI's ingest loop only invokes `place` at
  `crates/kaleidoscope-cli/src/lib.rs:228`).
- Single-process concurrency: two threads inside one `ingest`
  invocation (or the two-thread acceptance-test scenario that mimics it).
- The cross-writer NDJSON-validity guarantee under concurrent emission
  against a real `File`.

## OUT of scope

- `read` subcommand wiring (`wave-decisions.md` D5).
- `cinder.migrate` and `cinder.evaluate` wiring (`wave-decisions.md` D1
  — the ingest loop does not reach them; CLI usage would need a
  separate tier-management subcommand, which does not exist yet).
- Multi-process scenarios (two CLI processes against the same path)
  (`wave-decisions.md` D7).
- Changes to either writer's public API (ADR-0039 §1 is locked).
- A separate `--observe-cinder-otlp` flag (`wave-decisions.md` D4).

## Learning hypothesis

Disproves the assumption that cross-writer NDJSON validity is automatic
if we naively pass the same `File` path to both writers. If both
writers were constructed against independent `File::open(path)` handles
with `O_APPEND`, the kernel's per-write atomicity for sub-`PIPE_BUF`
writes is the only thing defending against interleaving. The
concurrent-random-pause acceptance test is the empirical probe; its
pass/fail tells DESIGN what shape the file-sharing mechanism actually
needs to take.

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `cinder_place_lines_appear_in_observe_otlp_file_one_per_batch_flush`:
  6 records at batch_size 3 produce exactly 2 lines with
  `metrics[0].name == "cinder.place.count"` under tenant `acme` with
  `tier == "hot"` and `asInt == "1"`.
- `lumen_lines_continue_to_appear_alongside_cinder_lines`: the same
  invocation produces exactly 2 lines with
  `metrics[0].name == "lumen.ingest.count"` and `asInt == "3"`;
  total non-empty line count is 4.
- `cross_writer_ndjson_validity_under_concurrent_random_pauses`: spawn
  two threads emitting 100 lines each (Lumen + Cinder) against handles
  onto the same file path with random `[0, 5]` ms pauses between calls;
  after both join, every non-empty line parses as
  `serde_json::Value`, the file ends with `\n`, exactly 100 lines have
  metric name `lumen.ingest.count`, and exactly 100 lines
  have metric name `cinder.place.count`.
- `flag_absent_creates_no_file_and_does_not_change_recorders`:
  unchanged from the existing `no_observe_otlp_means_no_otlp_file_created`
  test; the new test asserts the same behaviour against the new code
  path (recorders construction does not panic when `otlp_log_path = None`).
- `existing_observe_otlp_flag_tests_continue_to_pass_byte_equivalently`
  (meta-AC, verified by CI): `cargo test --package kaleidoscope-cli
  --test observe_otlp_flag` exits 0 with zero assertion edits to that
  file's source.

## Dependencies

- `cinder-to-otlp-json-bridge-v0` shipped (`CinderToOtlpJsonWriter` is
  publicly re-exported from `self-observe`).
- Prior `--observe-otlp` Lumen wiring shipped (commit `3af7e82`).

## Reference class

This is the third small feature in a row in the `self-observe` /
`kaleidoscope-cli` cluster, after `cinder-to-pulse-bridge-v0` and
`cinder-to-otlp-json-bridge-v0`. Both reference features landed new
writer types; this feature does not — it routes one already-shipped
writer to a place that previously held a `NoopRecorder`. The change
surface is strictly smaller than either reference: one match-arm
substitution plus one new test file. The existing
`observe_otlp_flag.rs` test pattern is the exact harness shape for the
new test; the `CinderToOtlpJsonWriter::new(file)` construction site is
the exact mirror of the `LumenToOtlpJsonWriter::new(file)` site already
at line 153.

## Effort estimate

Well under 1 day for the crafter. Breakdown: 30 minutes for the wiring
change inside `ingest` (DESIGN-picked file-sharing mechanism applied);
1-2 hours for the new acceptance test (happy-path + concurrent-random-
pause); 30 minutes for the `Cargo.toml` `[[test]]` entry and a local
green run; remainder reserved for unanticipated `File` handle ownership
surprises that the DESIGN wave's file-sharing pick may surface.

## Definition of Done for this slice

- All AC above green under `cargo test --package kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean (no new warnings).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli -- ingest
  acme /tmp/kdata --observe-otlp /tmp/kobs.ndjson < some_records.ndjson`
  in one pane, `tail -f /tmp/kobs.ndjson | jq .` in another, every line
  parsed by `jq` without error.
