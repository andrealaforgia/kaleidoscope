# Application Architecture — `cli-list-items-subcommand-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

**Question**: how does `list-items <tenant> <data_dir> <tier>`
join `kaleidoscope-cli` as a sixth subcommand, faithful to the
existing read-only `TieringStore::list_by_tier` port, with no
Lumen-side touch and no Cinder mutation?

**Decision**: new `pub fn list_items(...)` free function (DD1)
reusing the existing `parse_tier` helper (visibility promoted
to `pub(crate)`, DD4); `sort_unstable` lexicographic boundary
sort (DD2); no stderr summary (DD3); existing `Error` variants
reused verbatim (DD5). Full rationale in
`design/wave-decisions.md > DD1..DD5`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-list-items-subcommand v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli list-items <tenant> <data_dir> <tier> to enumerate every item this tenant currently has in one tier, typically as the input to a scripted xargs pipeline.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. Gains a sixth subcommand list-items calling TieringStore::list_by_tier as a pure read. AGPL-3.0-or-later.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts Cinder WAL+snapshot under <data_dir>/cinder.*. Read-only access on this path (D-ReadOnly). The Lumen WAL+snapshot under <data_dir>/lumen.* is NOT opened (D-NoLumenTouch).")
  System_Ext(unix_pipeline, "Unix text-tool pipeline", "Consumes the one-bare-id-per-line stdout via xargs / sort / wc -l / diff. Empty stdout for N=0 is the natural shell signal for nothing to iterate.")

  Rel(operator, cli, "Invokes list-items <tenant> <data_dir> <tier> capturing N lines of stdout from")
  Rel(cli, filesystem, "Opens Cinder WAL+snapshot read-only; calls list_by_tier() once on")
  Rel(operator, unix_pipeline, "Pipes stdout through xargs / sort / wc -l on")
  Rel(unix_pipeline, operator, "Returns reduced lines / counts / diffs to")
```

The change is confined to the `kaleidoscope-cli` node. The
filesystem container gains zero new write access patterns
(D-ReadOnly: Cinder WAL+snapshot is byte-equivalent across all
paths). The Lumen container is unchanged (D-NoLumenTouch).

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-list-items-subcommand v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "Existing dispatcher gains Some('list-items') => run_list_items(&args). run_list_items parses argv[2]=tenant, argv[3]=data_dir via parse_positional, argv[4]=tier inline, then calls list_items(..) with io::stdout().lock() as writer. write_usage gains one new paragraph. ~10 new lines.")
    Container(list_items_fn, "list_items function (new)", "Rust, src/lib.rs (new, ~15 lines)", "Signature: pub fn list_items(tenant: &TenantId, data_dir: &Path, tier_arg: &str, mut writer: impl Write) -> Result<(), Error>. Body: parse_tier(tier_arg) -> InvalidTier on fail; open FileBackedTieringStore with CinderRecorder; call list_by_tier(tenant, tier); sort_unstable; writeln loop one bare id per line.")
    Container(parse_tier, "parse_tier helper (visibility promoted)", "Rust, src/lib.rs:475-482 (existing)", "DD4: visibility raised from private to pub(crate). Body unchanged. Three literal-match arms: 'hot' => Tier::Hot, 'warm' => Tier::Warm, 'cold' => Tier::Cold, _ => Err(()).")
    Container(error_enum, "Error enum (UNCHANGED)", "Rust, src/lib.rs:72-88", "No new variants. list_items reuses InvalidTier (DD5: existing Display verbatim), CinderOpen (store-open failure), and Io (writeln failure via existing From<io::Error>).")
    Container(noop_cinder, "cinder::NoopRecorder", "Rust, cinder crate (aliased as CinderRecorder)", "Quiescent recorder. Same construction pattern as migrate()'s no-flag arm. No OTLP file (D-OutOfScope-Observe).")
  }
  Container_Boundary(stores, "Storage adapters") {
    Container(cinder_store, "FileBackedTieringStore", "Rust, cinder crate", "Honours TieringStore port. list_by_tier(&tenant, tier) is a pure read: returns Vec<ItemId> with HashMap-iteration-order randomness (boundary sort happens in list_items, DD2).")
  }
  ContainerDb(cinder_files, "<data_dir>/cinder.*", "POSIX files, READ-ONLY in this flow", "Cinder v1 WAL + snapshot. Byte-equivalent before and after every list-items invocation including failure paths (D-ReadOnly).")
  ContainerDb(lumen_files, "<data_dir>/lumen.*", "POSIX files, NOT opened", "Byte-equivalent before and after every list-items invocation including failure paths (D-NoLumenTouch).")
  System_Ext(unix_pipeline, "Unix text-tool pipeline", "xargs / sort / wc -l on the one-bare-id-per-line stdout.")

  Rel(operator, main, "Invokes list-items <tenant> <data_dir> <tier> at")
  Rel(main, list_items_fn, "Dispatches to with io::stdout().lock() as writer; on Err prints kaleidoscope-cli: {e} to stderr and exits non-zero (DD3: no stderr summary on success)")
  Rel(list_items_fn, parse_tier, "Calls once with tier_arg; lifts Err(()) into Error::InvalidTier{value} before opening the store (DD5)")
  Rel(list_items_fn, error_enum, "Materialises Error::InvalidTier on parse fail; Error::CinderOpen on store-open fail; Error::Io on writeln fail")
  Rel(list_items_fn, noop_cinder, "Constructs quiescent recorder via Box::new(CinderRecorder)")
  Rel(list_items_fn, cinder_store, "Opens via FileBackedTieringStore::open(cinder_base(data_dir), recorder); calls list_by_tier(tenant, tier) once; sort_unstable then writeln loop on")
  Rel(cinder_store, cinder_files, "Reads WAL+snapshot from")
  Rel(list_items_fn, operator, "Writes N bare item-id lines to writer back to (via stdout)")
  Rel(operator, unix_pipeline, "Pipes stdout through")
```

`list_items()` is the sixth sibling of `ingest`, `read`,
`stats`, `stats_with_tiers`, `migrate`. It composes the
Cinder store-open pattern from `migrate()`'s no-flag arm
with one existing port call (`list_by_tier`) and one
boundary sort. The Lumen container is absent by
construction (D-NoLumenTouch).

## C4 — Component View (Level 3)

**Not produced.** Four-step linear flow (parse → open →
list_by_tier → sort → writeln loop) with no branch fan-out
beyond the three reused error variants. **Reification
conditions**: (a) cross-tenant aggregate (D-OutOfScope-
CrossTenant reversal) introducing a `list_tenants()` step;
(b) pagination (D-OutOfScope-Pagination reversal)
introducing a windowing component; (c) `--at <timestamp>`
historical reconstruction (D-OutOfScope-Historical
reversal). None expected in v0.
