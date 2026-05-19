# Slice 01 — `stats` subcommand emits record count and time range

**Story**: US-01
**Outcome KPIs**: OK1-CLI-stats-record-count (principal),
OK2-CLI-stats-time-range, OK3-CLI-stats-empty-tenant
**Tag**: operator-visible (not `@infrastructure` — the CLI surface is
the real user-invocable entry point)
**Estimated effort**: well under 1 day

## Goal

Add a third subcommand `stats` to `kaleidoscope-cli` invoked as
`kaleidoscope-cli stats <tenant_id> <data_dir>` that prints to stdout
the Lumen record count for the tenant plus, when the tenant is
populated, the earliest and latest record timestamps as ISO 8601 UTC
strings. The result: a single `kaleidoscope-cli stats acme
/tmp/k-data` invocation prints three plain-text key=value lines
(populated case) or one (empty case) on stdout, terminated by `\n`,
without materialising the record set through any pipeline.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | NEW free function `stats(tenant: &TenantId, data_dir: &Path, writer: impl Write) -> Result<(usize, Option<(SystemTime, SystemTime)>), Error>` (exact signature is DESIGN-locked per `wave-decisions.md` D8; the shape above is the most-likely-correct candidate). The function opens `FileBackedLogStore::open(lumen_base(data_dir), recorder)` with a quiescent `LumenToPulseRecorder` over an in-process Pulse sink (same pattern as `read`'s no-flag arm at `crates/kaleidoscope-cli/src/lib.rs:275-279`), calls `lumen.query(tenant, TimeRange::all())` exactly once, computes count + (when N > 0) min/max `observed_time_unix_nano`, and writes the key=value lines to the supplied writer. Populated case: 3 lines `records=N\nearliest=<ISO 8601 UTC>\nlatest=<ISO 8601 UTC>\n`. Empty case: 1 line `records=0\n`. |
| `crates/kaleidoscope-cli/src/main.rs` | `match args.get(1).map(String::as_str)` (lines 48-60) gains a `Some("stats") => run_stats(&args)` arm. NEW helper `run_stats(args: &[String]) -> Result<(), Box<dyn std::error::Error>>` mirroring the shape of `run_read` (lines 134-153), without the OTLP flag parsing — calls `parse_positional(args)?` and then `stats(&tenant, &data_dir, io::stdout().lock())`. `print_usage` (lines 71-97) gains a `stats` section: `kaleidoscope-cli stats <tenant_id> <data_dir>` with a one-line description of the stdout key=value output shape (3 lines for populated, 1 for empty). |
| `crates/kaleidoscope-cli/tests/stats_subcommand.rs` | NEW. Mirrors the harness pattern in `observe_otlp_read_flag.rs`. Hosts the acceptance tests below (OK1 populated-tenant + tenant-isolation, OK2 time-range + single-record, OK3 empty-tenant). |
| `crates/kaleidoscope-cli/Cargo.toml` | New `[[test]] name = "stats_subcommand", path = "tests/stats_subcommand.rs"`. Potentially one new dev-dependency for ISO 8601 timestamp formatting (`chrono` or `time`) — DESIGN locks the choice per `wave-decisions.md` D6. If the workspace already pulls in such a crate transitively that can be elevated to a direct dev-dependency, that is preferred. |

## IN scope

- The `stats` subcommand only.
- Just Lumen: one `lumen.query(tenant, TimeRange::all())` call per
  invocation. No Cinder lookup of any kind
  (`wave-decisions.md` D2).
- Three keys exactly: `records`, `earliest`, `latest`. No other
  keys, no header, no JSON, no CSV, no Unicode box drawing, no
  colour.
- Empty-tenant behaviour: `records=0` only, no `earliest=`, no
  `latest=` (`wave-decisions.md` D5).
- ISO 8601 UTC timestamp rendering with `Z` suffix, target
  nanosecond precision (`wave-decisions.md` D6).
- Stdout (NOT stderr — the stats are the principal output).
- Exit code 0 in both populated and empty cases.

## OUT of scope

- Cinder stats (`wave-decisions.md` D2 — no per-tier counts, no
  hot/warm distribution, no Cinder-place lookup).
- `--observe-otlp` wiring on `stats` (`wave-decisions.md` D3 — the
  flag is NOT accepted by this subcommand in v0).
- JSON output / CSV output / `--format=...` (`wave-decisions.md` D4).
- Sorting (`--sort-by-time`), filtering (`--severity-min=`,
  `--since=`, `--until=`), multi-tenant aggregates (`stats
  <data_dir>` without a tenant), per-day/per-hour histograms
  (`wave-decisions.md` D7).
- Per-tenant aggregate other than count + earliest + latest (no
  average, no median, no percentile, no severity breakdown, no
  attribute-value cardinality, no resource-attribute aggregation).
- Multi-process scenarios (two CLI processes querying the same path
  simultaneously). Same posture as the four reference features.
- Extracting a shared test-helper module across the four
  `tests/*.rs` files that now use the same harness pattern
  (`wave-decisions.md` D9 — the rule-of-three trigger arrived at
  the predecessor feature and the extraction remains a separate
  follow-up).

## Rejected alternatives

- **`records=0` plus `earliest=<none>` plus `latest=<none>` (the
  sentinel encoding for the empty case)**: rejected in
  `wave-decisions.md` D5. Operators may parse the sentinel `<none>`
  as a real timestamp string and silently get the wrong answer; the
  chosen encoding (omit the lines entirely) is robust to that
  failure mode and is grep-friendly (`grep ^records=0` is the
  unambiguous empty-tenant check).
- **`-o json` / `--format=json`**: rejected in `wave-decisions.md`
  D4. The v0 contract is plain-text key=value. JSON output is a
  reasonable v1 once the v0 output shape proves it is the right
  thing to make machine-parseable.
- **A new `LogStore::stats(tenant) -> Result<(usize, Option<(u64,
  u64)>), LogStoreError>` trait method**: not introduced in v0. The
  existing `query(tenant, TimeRange::all())` call returning
  `Vec<LogRecord>` is sufficient at v0 record volumes. If
  materialising the full vector proves operationally expensive at
  the operator's actual tenant sizes, a follow-up feature can
  introduce a streaming/aggregate trait method without breaking
  the v0 `stats` subcommand's stdout contract.

## Learning hypothesis

Disproves the assumption that the existing `LogStore` query API is
sufficient for stats without needing new methods. The current
`LogStore::query(tenant, TimeRange::all())` returns
`Vec<LogRecord>` sorted by `observed_time_unix_nano` ascending
(`crates/lumen/src/store.rs:69-70, 84`), which is sufficient to
compute count (`records.len()`) and min/max (`records.first()` and
`records.last()` exploiting the sort order, or a single iteration
if DESIGN prefers explicit iteration). If the assumption holds, the
slice ships with one `query()` call plus a constant-time count and
min/max derivation. If the assumption fails — materialising the
full vector is too expensive — the failure mode tells DESIGN to
propose a streaming/aggregate trait method as a follow-up feature
(NOT pre-emptively designed here).

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `stats_populated_tenant_emits_three_lines_in_order`: pre-ingest 7
  records for tenant `acme` (via a setup `ingest()` call) with
  deterministic `observed_time_unix_nano` values spanning a known
  window (e.g. 2026-05-18T00:00:00Z earliest, 2026-05-19T00:00:00Z
  latest), then call `stats()` with a captured stdout sink. Assert
  the captured stdout contains exactly 3 non-empty lines, in order:
  line 1 equals `records=7`; line 2 begins with `earliest=` and
  parses to the seeded earliest nanos converted to ISO 8601 UTC;
  line 3 begins with `latest=` and parses to the seeded latest
  nanos converted to ISO 8601 UTC. Assert stdout ends with `\n`.
- `stats_empty_tenant_emits_records_zero_and_no_timestamps`:
  open a fresh `data_dir` (or pre-ingest records for tenant `acme`
  and query a different never-ingested tenant `acmee`), then call
  `stats()` with a captured stdout sink. Assert the captured
  stdout contains exactly 1 non-empty line equal to `records=0`;
  assert no line begins with `earliest=`; assert no line begins
  with `latest=`; assert stdout ends with `\n`. Assert exit code
  equivalent / `Result` is `Ok`.
- `stats_single_record_tenant_emits_identical_earliest_and_latest`:
  pre-ingest exactly 1 record for tenant `globex` with
  `observed_time_unix_nano` corresponding to 2026-05-11T00:00:00Z.
  Call `stats()` with a captured stdout sink. Assert the captured
  stdout contains exactly 3 non-empty lines; assert `records=1`;
  assert the `earliest=...` line and the `latest=...` line have
  the same timestamp value (after the `earliest=` and `latest=`
  key strip).
- `stats_for_acme_does_not_count_globex_records_in_same_data_dir`:
  pre-ingest 7 records for tenant `acme` and 3 records for tenant
  `globex` into the same `data_dir`. Call `stats(&acme, ...)` with
  a captured stdout sink. Assert `records=7` (NOT 10). Assert the
  `earliest=`/`latest=` lines reflect only `acme`'s records.
- `stats_count_matches_read_count_for_same_tenant_and_data_dir`:
  pre-ingest N records for tenant `acme`. Call `read(&acme,
  &data_dir, captured_stdout, None)` and capture its returned
  `count`. Then call `stats(&acme, &data_dir, separate_stdout)`.
  Assert the `records=` line in the stats output equals
  `read`'s returned `count` (consistency check between the two
  subcommands for the same `(tenant, data_dir)`).

## Dependencies

- `lumen::LogStore::query(tenant, TimeRange::all())` already
  returns `Result<Vec<LogRecord>, LogStoreError>` with the records
  sorted ascending by `observed_time_unix_nano`
  (`crates/lumen/src/store.rs:69-70, 84`).
- `lumen::LogRecord::observed_time_unix_nano: u64` is the
  canonical sort key (`crates/lumen/src/record.rs:48`).
- `lumen::FileBackedLogStore` already implements `LogStore` and is
  already constructed by `read()`
  (`crates/kaleidoscope-cli/src/lib.rs:281-282`).
- `self_observe::LumenToPulseRecorder` is the quiescent recorder
  used by `read()`'s no-flag arm
  (`crates/kaleidoscope-cli/src/lib.rs:275-279`); already a
  `kaleidoscope-cli` dependency.
- `aegis::TenantId` already a dependency.
- An ISO 8601 timestamp formatter — `chrono` or `time` — is the
  only potential new external dependency. DESIGN locks the choice
  per `wave-decisions.md` D6.

## Reference class

This is the fifth small feature in a row in the `kaleidoscope-cli`
cluster (after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`, and
`cli-read-observe-otlp-v0`). Strictly smaller than all four: no
OTLP wiring of any kind (no `--observe-otlp` flag, no
`LumenToOtlpJsonWriter`, no NDJSON metric emission, no
cross-process observability question), no Cinder store
construction, no concurrency probe, no shared-handle ownership
puzzle. The new free function on `kaleidoscope_cli` is a tighter
shape than the existing `read()`: no recorder branch, no NDJSON
serialisation loop.

## Effort estimate

Well under 1 day for the crafter. Breakdown: 30 minutes for the
library function `stats()` (open store, query, count, min/max,
format three lines); 30 minutes for the binary's new dispatch arm
plus `run_stats` helper plus updated `print_usage`; 1-2 hours for
the new acceptance test (five scenarios — populated, empty,
single-record, tenant-isolation, consistency-with-read); 30
minutes for the `Cargo.toml` `[[test]]` entry, any new dev-dep
declaration for the ISO 8601 formatter, and a local green run.

## Definition of Done for this slice

- All AC above green under `cargo test --package kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean (no new
  warnings).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  ingest acme /tmp/kdata < some_records.ndjson`, then `cargo run
  --bin kaleidoscope-cli -- stats acme /tmp/kdata` shows three
  lines; `cargo run --bin kaleidoscope-cli -- stats acme /tmp/kdata
  | grep ^records= | cut -d= -f2` returns the same integer that
  `cargo run --bin kaleidoscope-cli -- read acme /tmp/kdata | wc
  -l` returns; `cargo run --bin kaleidoscope-cli -- stats acmee
  /tmp/kdata` (typo) shows exactly one line `records=0` with no
  timestamp lines.
- The three prior `tests/observe_otlp_*` test files continue to
  pass green (non-regression on the four reference features).
