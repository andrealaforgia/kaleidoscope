# Application Architecture — `cli-stats-time-range-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

**The architectural question**: the `stats` subcommand today always
queries `lumen.query(tenant, TimeRange::all())` inside
`stats_with_tiers` at `crates/kaleidoscope-cli/src/lib.rs:359-361`.
How does the operator drive an arbitrary `TimeRange::new(s, e)` into
that call site via two optional flags `--since` / `--until`, WITHOUT
breaking the locked OK4 tests, WITHOUT touching the Cinder loop at
lines 375-380 (D-CinderScope), reusing every parsing construct
already shipped by `cli-read-time-range-v0`?

**The decision**: extend `stats_with_tiers()` from 3 args to 4 by
appending `range: TimeRange` (DD1); thread it ONLY into the Lumen
call — option (a), one function, Cinder branch ignores the param
(DD2); confirm the existing empty-tenant arm handles the empty-window
case naturally (DD3); mechanically update the five
`stats_with_tiers(...)` call sites in
`tests/stats_cinder_tier_distribution.rs` with `TimeRange::all()`
as the new 4th arg, leaving `tests/stats_subcommand.rs` untouched
because it exercises only the legacy 3-arg `stats()` (DD4). Reuses
every parser construct from the predecessor unchanged; no new
library function, no new helper, no new type, no new external crate
(DD5). Full rationale in `design/wave-decisions.md`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-stats-time-range v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli stats with --since and --until to count a tenant's records in a specific time window and bracket the windowed earliest/latest, without piping read through jq.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. stats subcommand gains --since and --until ISO 8601 UTC flags; values parsed to u64 nanos and threaded into lumen::TimeRange::new(s, e). Half-open [since, until). Cinder hot/warm/cold lines remain state-snapshot (D-CinderScope). AGPL-3.0-or-later.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts Lumen WAL+snapshot under <data_dir>/lumen.* and Cinder tier metadata under <data_dir>/cinder.*. stats opens both read-only.")

  Rel(operator, cli, "Invokes `stats <tenant> <data_dir> [--since X] [--until Y]` at, capturing records= / earliest= / latest= / hot= / warm= / cold= lines on stdout from")
  Rel(cli, filesystem, "Opens Lumen read-only and calls query(tenant, TimeRange::new(s, e)); ALSO opens Cinder read-only and calls list_by_tier(tenant, tier) per tier (state-snapshot, NOT range-filtered) on")
```

The change is confined to the `kaleidoscope-cli` node. The Cinder
branch's behaviour is INVARIANT to the new flags — that invariance
is the principal new contract this feature introduces (D-CinderScope)
and is empirically probed by OK3.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-stats-time-range v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "run_stats_with gains one new line `let range = parse_time_range(args)?` before invoking stats_with_tiers(..); the parsed TimeRange is threaded into the 4th parameter. write_usage gains --since / --until docs on the stats block, including D-CinderScope and D-EmptyWindow notes.")
    Container(parse_time_range, "parse_time_range (unchanged)", "Rust, src/main.rs:188-195", "REUSED from cli-read-time-range-v0. Subcommand-neutral argv scan (skip(2) past bin name and subcommand). Defaults absent --since to 0 and absent --until to u64::MAX. Produces stderr error naming the offending flag.")
    Container(stats_fn, "stats_with_tiers (extended)", "Rust, src/lib.rs:349-383", "Signature gains 4th param: range: TimeRange. Token swap at line 360: lumen.query(tenant, TimeRange::all()) → lumen.query(tenant, range). The Cinder loop at lines 375-380 is UNCHANGED — range applies to Lumen only (D-CinderScope). The existing empty-tenant arm at lines 364-369 handles the empty-window case automatically (D-EmptyWindow).")
    Container(parser, "parse_iso8601_utc_nanos (unchanged)", "Rust, src/lib.rs:528-647", "REUSED. Calendar-validated hand-rolled inverse of format_iso8601_utc_nanos. No new IsoParseError variant.")
    Container(stats_legacy, "stats (legacy, unchanged)", "Rust, src/lib.rs:312-331", "3-arg legacy function. NOT modified. Byte-level OK4 oracle for cli-stats-subcommand-v0.")
  }
  Container_Boundary(stores, "Storage adapters") {
    Container(lumen_store, "FileBackedLogStore", "Rust, lumen crate", "query(tenant, TimeRange::new(s, e)) returns ascending vector filtered by the half-open [s, e). TimeRange::contains at lumen/src/record.rs:116-119 IS the half-open semantics.")
    Container(cinder_store, "FileBackedTieringStore", "Rust, cinder crate", "list_by_tier(tenant, tier) returns the CURRENT placements — state-snapshot, NOT filtered by any TimeRange.")
  }
  ContainerDb(lumen_files, "<data_dir>/lumen.*", "POSIX files, read-only", "Lumen v1 WAL + snapshot.")
  ContainerDb(cinder_files, "<data_dir>/cinder.*", "POSIX files, read-only", "Cinder v1 tier metadata.")

  Rel(operator, main, "Invokes `stats <tenant> <data_dir> [--since X] [--until Y]` at")
  Rel(main, parse_time_range, "Calls before invoking stats_with_tiers(), passing argv slice")
  Rel(parse_time_range, parser, "Calls twice (one per supplied flag) on")
  Rel(main, stats_fn, "Threads parsed TimeRange (or TimeRange::all() when no flags) into 4th parameter of")
  Rel(stats_fn, lumen_store, "Calls query(tenant, range) once on — range applies HERE")
  Rel(stats_fn, cinder_store, "Calls list_by_tier(tenant, tier) per tier on — range does NOT apply HERE (D-CinderScope)")
  Rel(lumen_store, lumen_files, "Reads WAL + snapshot from")
  Rel(cinder_store, cinder_files, "Reads tier metadata from")
  Rel(stats_fn, operator, "Writes records= / earliest= / latest= / hot= / warm= / cold= lines back to (via stdout)")
```

The asymmetry on the two storage adapters is the principal
architectural contract this feature introduces: `range` flows into
the Lumen call but is structurally absent from the Cinder loop.
This is the source-level encoding of D-CinderScope. OK3 probes the
asymmetry empirically (two invocations with two different
`TimeRange` values; assert Cinder lines byte-identical, Lumen lines
differ).

## C4 — Component View (Level 3)

**Not produced.** Sub-component scale: one new positional parameter;
one token swap; one new line in `run_stats_with`. L3 reification
conditions: (a) Cinder time-bound queries become a real feature; (b)
`stats_with_tiers` grows additional Lumen filters warranting a
`StatsOptions` builder; (c) the test harness rule-of-three refactor
lands.
