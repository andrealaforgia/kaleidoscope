# Slice 01 — `read` emits OTLP-JSON on `--observe-otlp` flag

**Story**: US-01
**Outcome KPIs**: OK1-CLI-read-lumen-query-events-present (principal),
OK2-CLI-read-no-side-channel, OK3-CLI-read-ingest-symmetry
**Tag**: operator-visible (not `@infrastructure` — the CLI surface is
the real user-invocable entry point)
**Estimated effort**: well under 1 day

## Goal

Extend `kaleidoscope_cli::read` (currently
`crates/kaleidoscope-cli/src/lib.rs:252-269`) so that, when the
operator passes `--observe-otlp <path>`, the function constructs
`LumenToOtlpJsonWriter::new(file)` (against an
`OpenOptions::new().create(true).append(true).open(path)` handle, the
same way `ingest` already does at lines 158-161) instead of the
current `LumenToPulseRecorder` over an in-process Pulse sink. The
result: a single `kaleidoscope-cli read acme /tmp/data --observe-otlp
/tmp/foo.ndjson > /dev/null` invocation leaves one new
`lumen.query.count` OTLP-JSON line at `/tmp/foo.ndjson`, with the
tenant id (`acme`) carried as the resource attribute. Stdout behaviour
is unchanged.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | The `read` function gains an `otlp_log_path: Option<&Path>` parameter (structurally identical to `ingest`'s fifth parameter at line 144). The recorder construction inside `read` becomes conditional on that value; the `Some(path)` arm constructs `LumenToOtlpJsonWriter::new(file)` against an `OpenOptions::new().create(true).append(true).open(path)` handle; the `None` arm preserves the existing `LumenToPulseRecorder::new(pulse)` construction. |
| `crates/kaleidoscope-cli/src/main.rs` | `run_read` (currently lines 121-128) gains a `parse_observe_otlp(args)?` call mirroring `run_ingest`'s call at line 88; the parsed `Option<PathBuf>` is forwarded into `read()`'s new fifth parameter as `otlp_path.as_deref()`. `print_usage` (lines 68-84) gains a one-line note that `read` also accepts `--observe-otlp <path>` with the same semantics it has for `ingest`. |
| `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` | NEW. Mirrors the harness pattern in `observe_otlp_flag.rs`. Hosts the three acceptance tests below (OK1 happy path, OK2 no-flag quiescence, OK3 ingest-then-read symmetry). |
| `crates/kaleidoscope-cli/Cargo.toml` | New `[[test]] name = "observe_otlp_read_flag", path = "tests/observe_otlp_read_flag.rs"`. No new external dep — `self-observe` already re-exports `LumenToOtlpJsonWriter` and is already a `kaleidoscope-cli` dependency. |

## IN scope

- The `read` subcommand only.
- The single `lumen.query` event per invocation (the `read()` function
  invokes `lumen.query(tenant, TimeRange::all())` exactly once at
  `crates/kaleidoscope-cli/src/lib.rs:258-260`).
- Single-process scope: one thread inside one
  `kaleidoscope-cli read` invocation. No concurrency probe — only one
  writer participates.
- The OK3 ingest-then-read symmetry against one shared file across two
  sequential function calls in the same test.

## OUT of scope

- `ingest` subcommand wiring (already shipped at commit `3af7e82` plus
  `cli-cinder-otlp-wiring-v0`).
- Cinder events from `read` (`wave-decisions.md` D2 — the `read()`
  function does not construct a Cinder store at all).
- Multi-process scenarios (two CLI processes writing to the same path
  concurrently) (`wave-decisions.md` D4).
- Changes to either writer's public API (ADR-0039 §1 + §2 correction
  box are locked) (`wave-decisions.md` D3).
- Extracting a shared test-helper module across `observe_otlp_flag.rs`,
  `observe_otlp_cinder_wiring.rs`, and the new
  `observe_otlp_read_flag.rs` (`wave-decisions.md` D6 — rule-of-three
  trigger arrives with this feature but the extraction is a separate
  follow-up).

## Learning hypothesis

Disproves the assumption that Lumen's `record_query` path needs
nothing more than the ingest-side recorder wiring template. The Lumen
writer already implements `record_query`
(`crates/self-observe/src/lumen_otlp_json.rs:205-207`), so the
expected outcome is that a copy-paste of the ingest-side wiring shape
"just works" on the read path. If it does not, the failure mode tells
DESIGN what the unobvious difference is at the call site
(ownership/lifetimes around `LogStore::query` vs `LogStore::ingest`,
unexpected trait-bound surprises around `FileBackedLogStore::open`,
etc.).

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `read_with_observe_otlp_emits_one_lumen_query_count_line`: pre-ingest
  N records for tenant `acme` (via a setup `ingest()` call without
  `--observe-otlp`, so the OTLP file is not polluted by ingest-side
  lines), then call `read()` with `otlp_log_path = Some(path)`. The
  file contains exactly 1 non-empty line. That line's
  `scopeMetrics[0].metrics[0].name == "lumen.query.count"`, its
  `scopeMetrics[0].scope.name == "kaleidoscope.lumen"`, its
  `resource.attributes[0].value.stringValue == "acme"`, and its
  `sum.dataPoints[0].asInt` equals the stringified count of records
  the query matched (which equals N for `TimeRange::all()`).
- `read_without_observe_otlp_creates_no_file_and_preserves_stdout`:
  pre-ingest N records, then call `read()` with `otlp_log_path = None`
  against a `Vec<u8>` stdout sink. The returned `count` equals N, the
  stdout bytes equal the pre-ingested records re-serialised as NDJSON
  (one record per line, trailing `\n`), and no file is created at any
  path the test would have specified for the flag-set case.
- `ingest_then_read_share_one_observe_otlp_file_in_one_session`:
  call `ingest()` with 6 records / batch_size 3 / `otlp_log_path =
  Some(path)`, then call `read()` with `otlp_log_path = Some(path)`
  (same path), then read back the file. Parse every non-empty line as
  `serde_json::Value`. Assert the metric-name multiset is
  `{ "lumen.ingest.count": 2, "cinder.place.count": 2,
  "lumen.query.count": 1 }`. Assert the file ends with `\n`.

## Dependencies

- Prior `--observe-otlp` Lumen wiring shipped (commit `3af7e82`).
- `cli-cinder-otlp-wiring-v0` shipped (Cinder side of the
  `--observe-otlp` file is already populated for the OK3 scenario).
- `LumenToOtlpJsonWriter` publicly re-exported from `self-observe`
  (already a `kaleidoscope-cli` dependency).
- `serde_json` already a dev-dependency on `kaleidoscope-cli`.

## Reference class

This is the fourth small feature in a row in the `self-observe` /
`kaleidoscope-cli` cluster (after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, and `cli-cinder-otlp-wiring-v0`).
Strictly smaller than the predecessor: only ONE writer participates,
so there is no cross-writer concurrency probe to write. The existing
`observe_otlp_flag.rs` test pattern is the exact harness shape; the
`LumenToOtlpJsonWriter::new(file)` construction site is the exact
mirror of the one at `crates/kaleidoscope-cli/src/lib.rs:164`.

## Effort estimate

Well under 1 day for the crafter. Breakdown: 30 minutes for the wiring
change inside `read` plus the `parse_observe_otlp` call in `main.rs`
plus the `print_usage` string update; 1-2 hours for the new acceptance
test (three scenarios — happy path, no-flag, ingest-then-read
symmetry); 30 minutes for the `Cargo.toml` `[[test]]` entry and a
local green run.

## Definition of Done for this slice

- All AC above green under `cargo test --package kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean (no new warnings).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli -- ingest
  acme /tmp/kdata --observe-otlp /tmp/kobs.ndjson < some_records.ndjson`,
  then `cargo run --bin kaleidoscope-cli -- read acme /tmp/kdata
  --observe-otlp /tmp/kobs.ndjson > /dev/null`, then `cat
  /tmp/kobs.ndjson | jq '.scopeMetrics[0].metrics[0].name' | sort |
  uniq -c` shows nonzero counts for each of `lumen.ingest.count`,
  `cinder.place.count`, and `lumen.query.count`.
