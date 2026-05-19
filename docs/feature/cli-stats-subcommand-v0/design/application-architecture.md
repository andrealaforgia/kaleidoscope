# Application Architecture — `cli-stats-subcommand-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

The architectural question:

> The `kaleidoscope-cli` binary today dispatches two subcommands
> (`ingest`, `read`), each a free function that opens
> `FileBackedLogStore`, performs one Lumen operation, and writes to a
> supplied writer. How does a third subcommand `stats` join the
> shape: which constructs does it reuse, how is its single rendering
> concern (ISO 8601 formatting) realised, and what is the minimal
> new surface?

The decision is **third subcommand arm, mirrors `read()`'s shape,
hand-rolled ISO 8601 formatter, no new external dependency**.
`stats()` constructs the same quiescent `LumenToPulseRecorder` that
`read()`'s no-flag arm constructs
(`crates/kaleidoscope-cli/src/lib.rs:275-279`); opens
`FileBackedLogStore` via the existing `lumen_base(data_dir)` helper
(line 118-120); calls `lumen.query(tenant, TimeRange::all())` once,
relying on the `LogStore` port's documented ascending-order invariant
(`crates/lumen/src/store.rs:67-75`) to take `records.first()` and
`records.last()` in O(1); writes three (or one) key=value lines via a
private formatter; returns `Result<usize, Error>`. Full rationale,
rejected alternatives, and the Reuse Analysis in
`design/wave-decisions.md > DD1, DD2, DD3, DD4`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-stats-subcommand v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli stats <tenant> <data_dir> as the post-ingest smoke-test, the capacity-planning probe, and the audit/compliance first-question answer.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. Gains a third subcommand `stats` alongside `ingest` and `read`. AGPL-3.0-or-later.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts the Lumen WAL+snapshot under <data_dir>/lumen.*. `stats` is a pure read; no WAL writes, no snapshot updates.")
  System_Ext(unix_tools, "Unix text tools (grep, cut, awk)", "Consume stdout key=value lines via shell pipelines. `grep ^records=` for the count; `grep -E '^(earliest|latest)='` for the time range.")

  Rel(operator, cli, "Invokes `stats <tenant> <data_dir>` at, capturing 3 lines (populated) or 1 line (empty) of stdout from")
  Rel(cli, filesystem, "Opens Lumen WAL+snapshot read-only via FileBackedLogStore::open against, calls query(tenant, TimeRange::all()) exactly once on")
  Rel(operator, unix_tools, "Pipes stats stdout through `grep ^records= | cut -d= -f2` on")
  Rel(unix_tools, operator, "Returns the count integer, or the ISO 8601 timestamp string, to")
```

The system context view shows the operator-visible value chain.
Before this feature, Priya answered "is there data for this tenant,
across what window?" via four invocations of `kaleidoscope-cli read`
piped through `wc -l`, `head -1`, `tail -1`, plus manual `jq` and
manual nanos-to-ISO-8601 conversion; each invocation materialised the
full record set just to throw it away. After this feature, one
`stats` invocation prints exactly the information the pipeline was
reducing to, without materialising the records anywhere. The change
is confined to the `kaleidoscope-cli` node.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-stats-subcommand v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "Dispatcher gains a third arm: Some('stats') => run_stats(&args). New run_stats helper: parse_positional then call stats(&tenant, &data_dir, io::stdout().lock()). write_usage gains a `stats` block; trailing footer updated so it does not contradict `stats`'s stdout posture.")
    Container(stats_fn, "stats function", "Rust, src/lib.rs (new)", "Signature: pub fn stats(tenant: &TenantId, data_dir: &Path, mut writer: impl Write) -> Result<usize, Error>. Constructs quiescent LumenToPulseRecorder; opens FileBackedLogStore via lumen_base(data_dir); calls query(tenant, TimeRange::all()) once; writes records=N (always) plus earliest/latest (when N>0) to writer; returns N.")
    Container(format_iso, "format_iso8601_utc_nanos (private)", "Rust, src/lib.rs (new, ~30 lines)", "Hand-rolled formatter: ns -> (year, month, day, hour, minute, second, nanos) via civil_from_days arithmetic, then write! the format string `{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z` to the supplied writer. Zero external deps.")
    Container(pulse_recorder, "LumenToPulseRecorder", "Rust, self-observe::lumen_bridge", "Quiescent recorder over fresh InMemoryMetricStore. Emits nothing observable; dies at end of stats() call. Same pattern as read()'s no-flag arm.")
  }
  Container_Boundary(stores, "Storage adapter") {
    Container(lumen_store, "FileBackedLogStore", "Rust, lumen crate", "Honours LogStore port invariants: per-tenant isolation, observed-time ascending order. query(tenant, TimeRange::all()) returns the full ascending-sorted vector; stats() takes .first() and .last() for time-range bounds.")
  }
  ContainerDb(lumen_files, "<data_dir>/lumen.*", "POSIX files, read-only access", "Lumen v1 WAL + snapshot. stats() opens these read-only. No WAL writes, no snapshot updates. Cinder files under <data_dir>/cinder.* are untouched (D2).")
  System_Ext(unix_pipeline, "Unix text-tool pipeline", "grep / cut / awk on stdout key=value lines.")

  Rel(operator, main, "Invokes `stats <tenant> <data_dir>` at")
  Rel(main, stats_fn, "Dispatches to with io::stdout().lock() as writer")
  Rel(stats_fn, pulse_recorder, "Constructs quiescent recorder via Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)))")
  Rel(stats_fn, lumen_store, "Opens via FileBackedLogStore::open(lumen_base(data_dir), recorder) and calls query(tenant, TimeRange::all()) once on")
  Rel(lumen_store, lumen_files, "Reads WAL + snapshot from")
  Rel(stats_fn, format_iso, "Calls twice per populated invocation (earliest and latest) on")
  Rel(stats_fn, operator, "Writes 3 key=value lines (populated) or 1 line (empty) to the supplied writer back to (via stdout)")
  Rel(operator, unix_pipeline, "Pipes stdout key=value lines through")
```

`stats()` is the third sibling of `ingest()` and `read()`, sharing
the recorder construction pattern with `read()`'s no-flag arm. The
hand-rolled ISO 8601 formatter is a private helper visible only
within `lib.rs`; it has no public surface and is exercised end-to-end
through the acceptance test's deterministic seed. The Lumen storage
container is reused unchanged. The Cinder container is **absent from
the diagram on purpose**: `stats()` does not construct
`FileBackedTieringStore` and never touches `<data_dir>/cinder.*`
(DISCUSS D2).

## C4 — Component View (Level 3)

**Not produced.** The change inside `stats()` is one match on
`(records.first(), records.last())` plus a private formatter call per
populated timestamp. The change inside `main.rs` is one new
`run_stats` helper (parse, call, return) and one extended
`print_usage` block. The new test file mirrors
`observe_otlp_read_flag.rs`'s harness shape (DISCUSS D9 keeps it
inline-duplicated).

Per the SA principle ("Component (L3) only for complex subsystems"),
L3 is **explicitly skipped**.

**Reification conditions** — L3 becomes appropriate if any of:

(a) The hand-rolled ISO 8601 formatter is extracted into a shared
helper consumed by more than one subcommand.

(b) The quiescent recorder construction is extracted into a shared
helper — rule of three triggers here (`stats()` is the third site
after `ingest`'s and `read`'s no-flag arms); DISCUSS does not
mandate the extraction at v0, and this wave does not propose it.

(c) A future `--json` / `--csv` flag (DISCUSS D4 reversal) introduces
a `StatsSummary` public struct (DD2 reversal); the formatter then
sits on the boundary between data shape and rendering shape.

None apply at v0. L3 stays unproduced.
