# Application Architecture — `cli-place-subcommand-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

**Question**: how does `place <tenant> <data_dir> <item_id> <tier>
[--observe-otlp <path>]` join `kaleidoscope-cli` as a seventh
subcommand, faithful to `TieringStore::place`'s overwrite-semantics,
with optional Cinder-side OTLP-JSON emission, and no Lumen-side
touch?

**Decision**: new `pub fn place(...)` free function (DD1) mirroring
`migrate()`'s six-parameter shape; recorder construction copied
byte-for-byte from `migrate()`'s `match otlp_log_path` arms (DD2);
NO new `Error` variant (the trait method returns `()`; DD3);
existing `parse_tier`, `tier_lowercase`, `Error::InvalidTier`,
`Error::CinderOpen`, `Error::Io` reused without modification (DD4).
Full rationale in `design/wave-decisions.md > DD1..DD5`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-place-subcommand v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>] to bootstrap a single Cinder item outside the ingest flow, set up a controlled test scenario, or recover a Cinder catalog from a manifest.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. Gains a seventh subcommand `place` calling TieringStore::place on one item, optionally emitting one cinder.place.count OTLP-JSON line per call. AGPL-3.0-or-later.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts Cinder WAL+snapshot under <data_dir>/cinder.*. The Lumen WAL+snapshot under <data_dir>/lumen.* is NOT opened by this subcommand (D-NoLumenTouch). May host an optional <otlp_path> file appended to when --observe-otlp is set.")
  System_Ext(unix_tools, "Unix text tools (grep, cut, awk, tail)", "Consume the one-line stdout report `placed tenant=<t> item=<i> tier=<x>` via shell pipelines; `tail -f <otlp_path>` watches the OTLP-JSON sidecar.")

  Rel(operator, cli, "Invokes `place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>]` at, capturing one stdout line on success from")
  Rel(cli, filesystem, "Opens Cinder WAL+snapshot; calls place() once on; optionally appends one OTLP-JSON line on")
  Rel(operator, unix_tools, "Pipes stdout `placed ...` line through `grep` / `cut` / `awk` on; `tail -f`s the optional OTLP-JSON sidecar on")
  Rel(unix_tools, operator, "Returns extracted fields to")
```

The change is confined to the `kaleidoscope-cli` node. The
filesystem container gains one new write access pattern
(`<data_dir>/cinder.*` mutation via the `place` trait method, plus
an optional append to `<otlp_path>` when `--observe-otlp` is set).
The Lumen container is unchanged — `<data_dir>/lumen.*` is byte-
equivalent before and after every invocation including the
failure paths.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-place-subcommand v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "Existing dispatcher gains Some('place') => run_place(&args). run_place delegates to run_place_with<O: Write>(args, stdout), parsing argv[2]=tenant, argv[3]=data_dir via parse_positional, argv[4]=item_id, argv[5]=tier inline, plus --observe-otlp via the shared parse_observe_otlp helper. write_usage gains one new paragraph documenting the place subcommand. ~25 new lines.")
    Container(place_fn, "place function (new)", "Rust, src/lib.rs (new, ~20 lines)", "Signature: pub fn place(tenant: &TenantId, data_dir: &Path, item_id: &str, tier_arg: &str, mut writer: impl Write, otlp_log_path: Option<&Path>) -> Result<(), Error>. Body: parse_tier(tier_arg) -> InvalidTier on fail; match otlp_log_path to construct recorder (Some -> CinderToOtlpJsonWriter on a fresh OpenOptions::create(true).append(true) handle; None -> Box::new(CinderRecorder)); open FileBackedTieringStore; call cinder.place(tenant, &ItemId::new(item_id), tier, SystemTime::now()) — returns (); writeln!('placed tenant=.. item=.. tier=..').")
    Container(parse_tier, "parse_tier (existing private)", "Rust, src/lib.rs:505-512", "Inverse of tier_lowercase. Three literal-match arms: 'hot' => Tier::Hot, 'warm' => Tier::Warm, 'cold' => Tier::Cold, _ => Err(()). No trim, no case-fold. Reused unchanged (fourth call site).")
    Container(tier_lower, "tier_lowercase (existing private)", "Rust, src/lib.rs:519-525", "Existing renderer. Reused unchanged for the tier= field of the stdout line.")
    Container(error_enum, "Error enum (unchanged)", "Rust, src/lib.rs:72-88", "No new variant. Error::InvalidTier (parse fail), Error::CinderOpen (store-open fail), and Error::Io (OTLP file-open or writeln! fail) all reused. The trait method TieringStore::place returns () — no place-side failure variant needed.")
    Container(noop_cinder, "cinder::NoopRecorder (alias CinderRecorder)", "Rust, cinder crate", "Quiescent recorder for the None arm. Same construction pattern as migrate()'s no-flag arm and ingest()'s no-flag arm.")
    Container(otlp_writer, "self_observe::CinderToOtlpJsonWriter", "Rust, self-observe crate (already imported)", "OTLP-JSON sink for the Some(path) arm. Emits exactly one cinder.place.count NDJSON line per place() call (mirrors the cinder.migrate.count emission shape from cli-migrate-observe-otlp-v0).")
  }
  Container_Boundary(stores, "Storage adapters") {
    Container(cinder_store, "FileBackedTieringStore", "Rust, cinder crate", "Honours TieringStore port: per-tenant isolation. place(tenant, item, tier, placed_at) returns () with overwrite-semantics (unconditional insert; any prior entry for the same (tenant, item) key is replaced).")
  }
  ContainerDb(cinder_files, "<data_dir>/cinder.*", "POSIX files, read+write", "Cinder v1 WAL + snapshot. The place trait call appends to the WAL (one entry) and updates the in-memory snapshot. Invalid-tier failure leaves both unchanged (parse short-circuit before open).")
  ContainerDb(lumen_files, "<data_dir>/lumen.*", "POSIX files, NOT opened", "Byte-equivalent before and after every place invocation including failure paths (D-NoLumenTouch).")
  ContainerDb(otlp_file, "<otlp_path> (optional)", "POSIX file, append-only", "OPTIONAL. Only opened when --observe-otlp is set AND parse_tier succeeded. One NDJSON line per place() call. Empty/absent on invalid-tier path (OK3 invariant).")
  System_Ext(unix_pipeline, "Unix text-tool pipeline", "grep / cut / awk on the stdout report; tail -f on the OTLP-JSON sidecar.")

  Rel(operator, main, "Invokes `place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>]` at")
  Rel(main, place_fn, "Dispatches to with io::stdout().lock() as writer; on Err prints `kaleidoscope-cli: {e}` to stderr and exits non-zero")
  Rel(place_fn, parse_tier, "Calls once with tier_arg; lifts Err(()) into Error::InvalidTier { value }")
  Rel(place_fn, error_enum, "Materialises Error::InvalidTier on parse fail; Error::CinderOpen on store-open fail; Error::Io on OTLP file-open or writeln! fail")
  Rel(place_fn, noop_cinder, "Constructs quiescent recorder via Box::new(CinderRecorder) on None arm")
  Rel(place_fn, otlp_writer, "Constructs Box::new(CinderToOtlpJsonWriter::new(file)) on Some(path) arm against a freshly-opened append-handle on")
  Rel(place_fn, cinder_store, "Opens via FileBackedTieringStore::open(cinder_base(data_dir), recorder); calls place(tenant, item, tier, SystemTime::now()) once on")
  Rel(place_fn, tier_lower, "Calls once on success (for the tier= field) on")
  Rel(cinder_store, cinder_files, "Appends to")
  Rel(otlp_writer, otlp_file, "Appends one NDJSON line per place() call to")
  Rel(place_fn, operator, "Writes one line `placed tenant=.. item=.. tier=..` to writer back to (via stdout)")
  Rel(operator, unix_pipeline, "Pipes stdout line through; tail -fs OTLP sidecar via")
```

`place()` is the seventh sibling of `ingest`, `read`, `stats`,
`stats_with_tiers`, `migrate`, `list_items`. It composes the Cinder
store-open pattern from `migrate()`'s shape (including the
`Some(path) => CinderToOtlpJsonWriter / None => CinderRecorder`
recorder match) with a simpler body (no `get_entry` pre-flight; no
`Result` lift on the trait call). The Lumen container is absent
from this diagram by construction (D-NoLumenTouch).

## C4 — Component View (Level 3)

**Not produced.** Four-step linear flow (parse → construct
recorder → open store → place → render) with no branch fan-out
beyond the three reused error variants and the two recorder arms.
Simpler than `migrate()`'s flow (which has the additional
`get_entry` pre-flight branch). **Reification conditions**: (a)
`--placed-at` flag reversal (D-Timestamp); (b) bulk placement
(D-OutOfScope-Bulk reversal); (c) `--no-overwrite` flag
introduction (D-Overwrite reversal) which would add a pre-flight
`get_entry` branch and a new `Error::AlreadyPlaced { item }`
variant; (d) recorder-construction helper extraction at the third
single-writer site (rule of three; today only `migrate()` and
`place()` share the shape).
