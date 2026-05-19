# Application Architecture — `cli-migrate-subcommand-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

**Question**: how does `migrate <tenant> <data_dir> <item_id> <to_tier>`
join `kaleidoscope-cli` as a fifth subcommand, faithful to the
underlying idempotent API, with no Lumen-side touch?

**Decision**: new `pub fn migrate(...)` free function (DD1) with
private `parse_tier` (DD3, inverse of `tier_lowercase`); pre-flight
`get_entry` discovers `from` tier (DD2); two new `Error` variants —
`InvalidTier { value }` and `CinderMigrate(_)` (DD4). Full rationale
in `design/wave-decisions.md > DD1..DD6`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-migrate-subcommand v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli migrate <tenant> <data_dir> <item_id> <to_tier> to manually rebalance, compensate auto-tiering, or test lifecycle on one Cinder item.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. Gains a fifth subcommand `migrate` calling TieringStore::migrate on one item. AGPL-3.0-or-later.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts Cinder WAL+snapshot under <data_dir>/cinder.*. The Lumen WAL+snapshot under <data_dir>/lumen.* is NOT opened by this subcommand (D-NoLumenTouch).")
  System_Ext(unix_tools, "Unix text tools (grep, cut, awk)", "Consume the one-line stdout report `migrated tenant=<t> item=<i> from=<f> to=<x>` via shell pipelines. `grep -o 'from=[a-z]*'` extracts the from-tier name.")

  Rel(operator, cli, "Invokes `migrate <tenant> <data_dir> <item_id> <to_tier>` at, capturing one stdout line on success from")
  Rel(cli, filesystem, "Opens Cinder WAL+snapshot; calls get_entry() once then migrate() once on")
  Rel(operator, unix_tools, "Pipes stdout `migrated ...` line through `grep` / `cut` / `awk` on")
  Rel(unix_tools, operator, "Returns extracted fields to")
```

The change is confined to the `kaleidoscope-cli` node. The
filesystem container gains one new write access pattern
(`<data_dir>/cinder.*` mutation via the `migrate` trait method).
The Lumen container is unchanged — `<data_dir>/lumen.*` is byte-
equivalent before and after every invocation including the
failure paths.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-migrate-subcommand v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "Existing dispatcher gains Some('migrate') => run_migrate(&args). run_migrate parses argv[2]=tenant, argv[3]=data_dir via parse_positional, argv[4]=item_id and argv[5]=to_tier inline, then calls migrate(..) with io::stdout().lock() as writer. write_usage gains one new paragraph documenting the migrate subcommand. ~15 new lines.")
    Container(migrate_fn, "migrate function (new)", "Rust, src/lib.rs (new, ~25 lines)", "Signature: pub fn migrate(tenant: &TenantId, data_dir: &Path, item_id: &str, to_tier_arg: &str, mut writer: impl Write) -> Result<(), Error>. Body: parse_tier(to_tier_arg) -> InvalidTier on fail; open FileBackedTieringStore with NoopRecorder; get_entry to discover from-tier (None -> CinderMigrate::UnknownItem); call migrate(..., SystemTime::now()); writeln!('migrated tenant=.. item=.. from=.. to=..').")
    Container(parse_tier, "parse_tier helper (new private)", "Rust, src/lib.rs (new, ~8 lines)", "Inverse of tier_lowercase. Three literal-match arms: 'hot' => Tier::Hot, 'warm' => Tier::Warm, 'cold' => Tier::Cold, _ => Err(()). No trim, no case-fold, no normalisation.")
    Container(tier_lower, "tier_lowercase (private, unchanged)", "Rust, src/lib.rs:389-395", "Existing renderer. Reused for the from= and to= fields of the stdout line. Forms a symmetric pair with parse_tier.")
    Container(error_enum, "Error enum (extended)", "Rust, src/lib.rs:72-84", "Gains two variants: InvalidTier { value: String } (parse fail; Display: `<to_tier> {value:?}: expected one of hot, warm, cold`) and CinderMigrate(MigrateError) (store fail or get_entry None; Display: `cinder migrate: {e}`).")
    Container(noop_cinder, "cinder::NoopRecorder", "Rust, cinder crate (aliased as CinderRecorder)", "Quiescent recorder. Same construction pattern as ingest()'s no-flag arm and stats_with_tiers(). No OTLP file, no metric emission.")
  }
  Container_Boundary(stores, "Storage adapters") {
    Container(cinder_store, "FileBackedTieringStore", "Rust, cinder crate", "Honours TieringStore port: per-tenant isolation, manual migrate any-direction. get_entry returns Option<TierEntry>; migrate returns Result<(), MigrateError>; UnknownItem path leaves state untouched (no silent insert).")
  }
  ContainerDb(cinder_files, "<data_dir>/cinder.*", "POSIX files, read+write", "Cinder v1 WAL + snapshot. The migrate trait call appends to the WAL (one entry) and updates the in-memory snapshot. Failure paths leave both unchanged.")
  ContainerDb(lumen_files, "<data_dir>/lumen.*", "POSIX files, NOT opened", "Byte-equivalent before and after every migrate invocation including failure paths (D-NoLumenTouch).")
  System_Ext(unix_pipeline, "Unix text-tool pipeline", "grep / cut / awk on the one-line stdout report.")

  Rel(operator, main, "Invokes `migrate <tenant> <data_dir> <item_id> <to_tier>` at")
  Rel(main, migrate_fn, "Dispatches to with io::stdout().lock() as writer; on Err prints `kaleidoscope-cli: {e}` to stderr and exits non-zero")
  Rel(migrate_fn, parse_tier, "Calls once with to_tier_arg; lifts Err(()) into Error::InvalidTier")
  Rel(migrate_fn, error_enum, "Materialises Error::InvalidTier on parse fail; Error::CinderOpen on store-open fail; Error::CinderMigrate on get_entry None or migrate Err")
  Rel(migrate_fn, noop_cinder, "Constructs quiescent recorder via Box::new(CinderRecorder)")
  Rel(migrate_fn, cinder_store, "Opens via FileBackedTieringStore::open(cinder_base(data_dir), recorder); calls get_entry(tenant, item) once then migrate(tenant, item, to_tier, SystemTime::now()) once on")
  Rel(migrate_fn, tier_lower, "Calls twice on success (once for from, once for to) on")
  Rel(cinder_store, cinder_files, "Reads then appends to")
  Rel(migrate_fn, operator, "Writes one line `migrated tenant=.. item=.. from=.. to=..` to writer back to (via stdout)")
  Rel(operator, unix_pipeline, "Pipes stdout line through")
```

`migrate()` is the fifth sibling of `ingest`, `read`, `stats`,
`stats_with_tiers`. It composes the Cinder store-open pattern
from `ingest()`'s no-flag arm with the two new constructs
`parse_tier` and the extended `Error` enum. The Lumen container
is absent from this diagram by construction (D-NoLumenTouch).

## C4 — Component View (Level 3)

**Not produced.** Four-step linear flow (parse → open →
get_entry → migrate → render) with no branch fan-out beyond the
four error variants. **Reification conditions**: (a) `--dry-run`
flag reversal (DD7); (b) bulk migration (D-OutOfScope-Bulk
reversal); (c) quiescent-recorder helper extraction (rule of
three passes with `migrate`; deferred).
