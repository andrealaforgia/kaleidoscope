# Application Architecture — `cli-stats-cinder-tier-distribution-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

**The architectural question**: the `kaleidoscope-cli` `stats`
subcommand today emits Lumen-side lines only (`records=N`,
`earliest=`, `latest=`). How does the Cinder tier distribution
(`hot=H`, `warm=W`, `cold=C`) join the stdout shape WITHOUT modifying
the locked `tests/stats_subcommand.rs` file (DISCUSS D10) and WITHOUT
breaking the predecessor's byte-equivalent contract for tenants with
zero Cinder placements (OK4)?

**The decision**: add a new sibling free function `stats_with_tiers`
that reuses `stats()`'s Lumen body and appends a Cinder-side loop
over `[Tier::Hot, Tier::Warm, Tier::Cold]` emitting one line per
non-zero tier; repoint `main.rs::run_stats` from `stats` to
`stats_with_tiers`; leave `stats()` itself untouched (DD1). The
quiescent recorder pattern from `ingest()`'s no-flag arm is reused
for the Cinder side (DD3). Full rationale, alternatives, and the
Reuse Analysis in `design/wave-decisions.md > DD1, DD2, DD3, DD5`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-stats-cinder-tier-distribution v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli stats <tenant> <data_dir> as the post-ingest tier-distribution probe: are migrations happening, is hot ballooning, what is read-latency expectation.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. The existing stats subcommand gains up to three Cinder tier lines (hot=H / warm=W / cold=C) appended after the existing Lumen lines. AGPL-3.0-or-later.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts Lumen WAL+snapshot under <data_dir>/lumen.* AND Cinder WAL+snapshot under <data_dir>/cinder.*. stats_with_tiers is a pure read on both: no WAL writes, no snapshot updates, no Cinder placements.")
  System_Ext(unix_tools, "Unix text tools (grep, cut, awk)", "Consume stdout key=value lines via shell pipelines. `grep ^hot= | cut -d= -f2` returns the hot-tier integer or empty when hot is zero (Option B selective emission).")

  Rel(operator, cli, "Invokes `stats <tenant> <data_dir>` at, capturing 1 to 6 lines of stdout from")
  Rel(cli, filesystem, "Opens Lumen WAL+snapshot read-only AND Cinder WAL+snapshot read-only; calls query() once and list_by_tier() three times on")
  Rel(operator, unix_tools, "Pipes stats stdout through `grep -E '^(hot|warm|cold)=' | cut -d= -f2` on")
  Rel(unix_tools, operator, "Returns per-tier integers (or empty lines for zero-tier suppressed cases) to")
```

The change is confined to the `kaleidoscope-cli` node. Before this
feature, Priya answered the tier-distribution question via a one-off
Rust harness around `list_by_tier(tenant, ..)`. After it, the same
answer falls out of the existing `stats` invocation appended to the
Lumen lines in `grep`-friendly key=value shape. The filesystem
container gains one new read access pattern (`<data_dir>/cinder.*`);
no Cinder WAL writes occur.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-stats-cinder-tier-distribution v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "Existing dispatcher arm Some('stats') => run_stats(&args) is repointed: run_stats now calls stats_with_tiers(..) instead of stats(..). parse_positional and the write_usage block for stats are unchanged. Single-line code change.")
    Container(stats_legacy, "stats function (legacy)", "Rust, src/lib.rs (unchanged)", "Pre-existing Lumen-only stats. Still reachable. Still exercised by the locked tests/stats_subcommand.rs file as the byte-level oracle for OK4. NOT called by main.rs after this feature; called only from the locked test file and from any future library caller wanting Lumen-only stats.")
    Container(stats_with_tiers, "stats_with_tiers function (new)", "Rust, src/lib.rs (new, ~25 lines)", "Signature: pub fn stats_with_tiers(tenant: &TenantId, data_dir: &Path, mut writer: impl Write) -> Result<usize, Error>. Body: inherits stats()'s Lumen block verbatim (quiescent LumenToPulseRecorder, FileBackedLogStore::open, query, records=/earliest=/latest= writelns); then opens FileBackedTieringStore with NoopRecorder; then iterates [Tier::Hot, Tier::Warm, Tier::Cold] calling list_by_tier(tenant, tier).len() per tier; emits one `key=count` line per non-zero count (Option B); returns the Lumen record count.")
    Container(format_iso, "format_iso8601_utc_nanos (private)", "Rust, src/lib.rs (unchanged)", "Hand-rolled formatter inherited from cli-stats-subcommand-v0 DD1. Reused for the Lumen earliest/latest lines. Zero external deps.")
    Container(pulse_recorder, "LumenToPulseRecorder", "Rust, self-observe::lumen_bridge", "Quiescent recorder over fresh InMemoryMetricStore. Lumen-side. Identical to stats()'s usage. Dies at end of stats_with_tiers() call.")
    Container(noop_cinder, "cinder::NoopRecorder", "Rust, cinder crate (aliased as CinderRecorder)", "Quiescent recorder for the Cinder side. Same construction pattern as ingest()'s no-flag arm. No OTLP file is created, no metric is emitted.")
  }
  Container_Boundary(stores, "Storage adapters") {
    Container(lumen_store, "FileBackedLogStore", "Rust, lumen crate", "Honours LogStore port: per-tenant isolation, observed-time ascending order. query(tenant, TimeRange::all()) returns the full ascending vector; stats_with_tiers takes .first()/.last() for the time-range bounds.")
    Container(cinder_store, "FileBackedTieringStore", "Rust, cinder crate", "Honours TieringStore port: per-tenant isolation. list_by_tier(tenant, tier) returns Vec<ItemId>; stats_with_tiers calls .len() and drops the vector (no per-item rendering — DISCUSS D4).")
  }
  ContainerDb(lumen_files, "<data_dir>/lumen.*", "POSIX files, read-only access", "Lumen v1 WAL + snapshot. Opened read-only.")
  ContainerDb(cinder_files, "<data_dir>/cinder.*", "POSIX files, read-only access", "Cinder v1 WAL + snapshot. Opened read-only by stats_with_tiers — no place(), no migrate(), no evaluate_at(). New access pattern introduced by this feature.")
  System_Ext(unix_pipeline, "Unix text-tool pipeline", "grep / cut / awk on stdout key=value lines.")

  Rel(operator, main, "Invokes `stats <tenant> <data_dir>` at")
  Rel(main, stats_with_tiers, "Dispatches to with io::stdout().lock() as writer")
  Rel(stats_with_tiers, pulse_recorder, "Constructs quiescent Lumen recorder via Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)))")
  Rel(stats_with_tiers, noop_cinder, "Constructs quiescent Cinder recorder via Box::new(CinderRecorder)")
  Rel(stats_with_tiers, lumen_store, "Opens via FileBackedLogStore::open(lumen_base(data_dir), recorder) and calls query(tenant, TimeRange::all()) once on")
  Rel(stats_with_tiers, cinder_store, "Opens via FileBackedTieringStore::open(cinder_base(data_dir), recorder) and calls list_by_tier(tenant, tier) three times on")
  Rel(lumen_store, lumen_files, "Reads WAL + snapshot from")
  Rel(cinder_store, cinder_files, "Reads WAL + snapshot from")
  Rel(stats_with_tiers, format_iso, "Calls twice per populated Lumen invocation on")
  Rel(stats_with_tiers, operator, "Writes 1 to 6 key=value lines to the supplied writer back to (via stdout)")
  Rel(operator, unix_pipeline, "Pipes stdout key=value lines through")
```

`stats_with_tiers()` is the fourth sibling of `ingest`, `read`,
`stats`. It composes `stats()`'s Lumen body verbatim with the Cinder
store-open pattern from `ingest()`'s no-flag arm. The legacy `stats`
is retained as the byte-level test oracle for OK4 (DD1). The Cinder
container appears here as this feature's single new outbound dep.

## C4 — Component View (Level 3)

**Not produced.** The change inside `stats_with_tiers()` is one
inherited `(first, last)` match plus a three-iteration `for` loop
with an `if count > 0` guard and a `match tier` to key string. L3 is
explicitly skipped per the SA principle. **Reification conditions**:
(a) the quiescent recorder construction is extracted into a shared
helper (rule of three passed; extraction deferred); (b) a future
`--json` flag (DD5 reversal) introduces a `TierCounts` public struct;
(c) `Tier::all()` lands on `cinder` (DD2 reversal) and the loop
switches from the hardcoded array.
