# Story Map: `cli-stats-subcommand-v0`

## User: Priya the platform operator

## Goal

When Priya runs `kaleidoscope-cli stats acme /tmp/data`, she sees on
stdout — in milliseconds — the Lumen record count for tenant `acme`
plus, when the tenant is populated, the earliest and latest record
timestamps as ISO 8601 UTC strings, on three plain-text key=value
lines, terminated by `\n`. The output pipes naturally through `grep`
/ `cut` / `awk` and answers the canonical post-ingest smoke-test
question ("did data land, and across what window?") without
materialising the record set through any pipeline. The empty-tenant
case prints exactly one line, `records=0`, with no timestamp lines,
unambiguously distinguishing the empty case from the populated case.

## Backbone

The journey has exactly one activity: the operator inspects a
tenant's data without dumping it. The activity is a thin end-to-end
slice: a single `kaleidoscope-cli stats <tenant> <data_dir>`
invocation calls `lumen.query(tenant, TimeRange::all())` exactly
once, takes the length for the count, and iterates the sorted result
for min/max — exactly the same single-call shape that `read` already
uses (`crates/kaleidoscope-cli/src/lib.rs:283-285`). The CLI
substrate, `LogStore` trait, `FileBackedLogStore` adapter,
quiescent `LumenToPulseRecorder` recorder pattern, and
`parse_positional` helper all already exist; this feature is a thin
extension that adds one subcommand and one library function on the
existing substrate.

| Activity 1: operator inspects a tenant's data without dumping it |
|---|
| `kaleidoscope-cli stats <tenant> <data_dir>` is dispatched by the binary's `main.rs` argument matcher, which calls the new library function `kaleidoscope_cli::stats(&tenant, &data_dir, stdout)`. The function opens the Lumen store, queries for the tenant, and writes plain-text key=value lines to stdout: `records=N` always; `earliest=<ISO 8601 UTC>` and `latest=<ISO 8601 UTC>` only when N > 0. The Lumen store is unchanged (read-only). No Cinder lookup. No OTLP file created. Exit code 0 in both populated and empty cases. |

## Walking Skeleton

Per `wave-decisions.md` (no explicit decision needed; the answer is
N/A), the walking-skeleton concept does not apply because:

- The CLI already exists, with two working subcommands (`ingest`,
  `read`).
- The `LogStore::query(tenant, TimeRange::all())` trait method
  already returns a `Vec<LogRecord>` sorted by
  `observed_time_unix_nano` ascending
  (`crates/lumen/src/store.rs:69-70, 84`).
- The `LogRecord::observed_time_unix_nano: u64` field is the
  canonical sort key (`crates/lumen/src/record.rs:48`).
- The binary's `parse_positional` helper already extracts
  `(tenant, data_dir)` from `args[2..]`
  (`crates/kaleidoscope-cli/src/main.rs:155-161`).
- The quiescent `LumenToPulseRecorder` pattern is already used by
  `read`'s no-flag arm (`crates/kaleidoscope-cli/src/lib.rs:275-279`).

Equivalent statement: **the smallest valuable change is to add a
new subcommand dispatch arm in `main.rs`, a new `run_stats(&args)`
helper that calls `parse_positional` and forwards to a new library
function `kaleidoscope_cli::stats`, and the library function itself
which opens `FileBackedLogStore`, calls
`lumen.query(tenant, TimeRange::all())`, computes count + min/max,
and writes the key=value lines to the supplied writer.** Slice 01
ships exactly that.

## Release Slices

### Slice 01 — `stats` subcommand emits record count and time range

- **Outcome**: An operator running
  `kaleidoscope-cli stats acme /tmp/data` sees three stdout lines
  (`records=N`, `earliest=<ISO 8601 UTC>`, `latest=<ISO 8601 UTC>`)
  for a populated tenant or one stdout line (`records=0`) for an
  empty tenant. The count is consistent with what `read` would
  return for the same `(tenant, data_dir)` (OK1); the time range is
  consistent with the min/max `observed_time_unix_nano` across the
  record set (OK2); the empty case is unambiguous (OK3).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that the
  existing `LogStore::query(tenant, TimeRange::all())` API is
  sufficient for stats without needing new methods. The current
  `LogStore` trait returns a `Vec<LogRecord>` for the full
  time-range query (`crates/lumen/src/store.rs:84`); the assumption
  is that materialising the full vector to compute count + min/max
  is acceptable at v0 record volumes (the FileBackedLogStore is the
  v1 adapter; the disk substrate is already exercised by
  `cli-read-observe-otlp-v0` for the same `Vec<LogRecord>` return
  shape). If the assumption holds, the slice ships with a single
  call to `query()` plus a `records.len()` plus a single iteration
  (or zero iterations if we use `records.first()` / `.last()`
  exploiting the documented ascending-by-observed-time sort). If
  the assumption fails — i.e. materialising the full
  `Vec<LogRecord>` is too expensive for the operator's actual
  tenant sizes — the failure mode tells DESIGN to propose a new
  `LogStore::stats(tenant) -> Result<(usize, Option<(u64, u64)>),
  LogStoreError>` trait method that the FileBackedLogStore can
  implement by streaming the WAL+snapshot once without
  materialising the full record vector. That follow-up feature is
  OUT OF SCOPE for v0 and is not pre-emptively designed here.
- **Production-data-equivalent AC**: an end-to-end test invokes
  `kaleidoscope_cli::stats` (the actual library function the binary
  calls — same entry point) with a `(tenant, data_dir, writer)`
  triple against a real temp `data_dir`, against a Lumen store
  pre-populated by setup `ingest()` calls, and reads back the
  captured stdout to assert the three key=value lines (populated
  scenario) or the single line (empty scenario). This is the same
  data path the operator's `kaleidoscope-cli stats acme /tmp/data`
  invocation will exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a
  terminal, runs `cargo run --bin kaleidoscope-cli -- ingest acme
  /tmp/kdata < some_records.ndjson`, then `cargo run --bin
  kaleidoscope-cli -- stats acme /tmp/kdata`. Stdout shows three
  lines. `cargo run --bin kaleidoscope-cli -- stats acme /tmp/kdata
  | grep ^records= | cut -d= -f2` returns the same integer that
  `cargo run --bin kaleidoscope-cli -- read acme /tmp/kdata | wc
  -l` returns. `cargo run --bin kaleidoscope-cli -- stats acmee
  /tmp/kdata` (typo of `acme`) shows exactly one line, `records=0`,
  with no timestamp lines. The three observations together are the
  dogfood gate for the slice.
- **Effort**: well under 1 day. The change inside the library is
  structurally a tighter version of `read()` (no recorder branch,
  no NDJSON serialisation loop, no writer.flush, just one
  `query()` + format + write); the new acceptance test mirrors the
  existing `observe_otlp_read_flag.rs` harness pattern; no
  concurrency probe, no OTLP wiring, no Cinder lookup.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the fifth consecutive small feature in the
`kaleidoscope-cli` cluster, after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`, and
`cli-read-observe-otlp-v0`, and strictly smaller than all four
because no OTLP wiring of any kind is needed) means there is no
benefit from further splitting:

- Slice 01 carries the wiring change (one new subcommand arm in
  `main.rs`, one new `run_stats` helper, one new library function
  `kaleidoscope_cli::stats`, one updated `print_usage` block), the
  OK1 record-count test, the OK2 time-range test, and the OK3
  empty-tenant test all together. Splitting any one of the three
  KPIs into a separate slice would force a second PR for trivially
  the same wiring — net negative for the reviewer.
- The principal KPI (OK1) is the count correctness; OK2 is the
  time-range enrichment that turns the subcommand from "cheap `wc
  -l` replacement" into "cheap `wc -l + head -1 + tail -1`
  replacement"; OK3 is the empty-case unambiguity that distinguishes
  the empty case from the populated case in a grep-friendly way.
  Shipping any without the others is meaningless: OK1 alone is just
  a record counter that loses to `wc -l`; OK2 alone has nothing to
  contextualise the timestamps against; OK3 alone is structurally
  impossible without OK1 because the empty-case format derives from
  the populated-case format by omitting two lines.

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the wiring change is one new
subcommand arm in the dispatcher plus one new free function in
`lib.rs`. There is no sub-slice worth shipping in isolation.

The choice between the two empty-case encodings (`records=0` with
omitted timestamp lines, vs `records=0\nearliest=<none>\nlatest=<none>\n`)
is locked in `wave-decisions.md` D5: omit the timestamp lines
entirely. The rationale (no sentinel value to misparse, grep-friendly
disambiguation, mathematically defensible "undefined min/max of an
empty set") favours the chosen option. The rejected option is
recorded in the slice brief for traceability.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture of
`cli-read-observe-otlp-v0/discuss/story-map.md` and inherits its
persona (Priya), Lumen-store substrate
(`FileBackedLogStore::open(lumen_base(data_dir), recorder)`), and
quiescent-recorder convention. The principal contractual difference
is that this feature does NOT touch OTLP at all: no
`--observe-otlp` flag, no `LumenToOtlpJsonWriter`, no NDJSON metric
emission, no cross-process observability question. The OTLP-related
KPIs in the four reference features have no counterpart in this
feature.

The cross-feature contract this feature DOES inherit is the
positional argument convention (`<tenant_id> <data_dir>`) and the
tenant-isolation invariant from `lumen::LogStore`'s per-tenant
isolation guarantee (`crates/lumen/src/store.rs:69-70`). The new
`stats` subcommand reuses the existing `parse_positional` helper
unchanged.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the wiring change is
  one new subcommand arm in `main.rs`, one new `run_stats` helper,
  one new free function in `lib.rs`, and the new acceptance test
  lives in one new file).
- 2 modified files (`crates/kaleidoscope-cli/src/lib.rs`,
  `crates/kaleidoscope-cli/src/main.rs`), 1 new file
  (`crates/kaleidoscope-cli/tests/stats_subcommand.rs`), 1
  line-level modification (`crates/kaleidoscope-cli/Cargo.toml` for
  the new `[[test]]` entry).
- 1 integration point (the new free function
  `kaleidoscope_cli::stats` calling the existing
  `lumen::LogStore::query(tenant, TimeRange::all())` trait method).
- Estimated effort: well under 1 day for the crafter. No OTLP
  wiring, no concurrency test, no shared-handle ownership puzzle,
  no Cinder store construction. Strictly smaller than the four
  reference features in this cluster.

The feature is right-sized. No splitting required, no thinning
possible.
