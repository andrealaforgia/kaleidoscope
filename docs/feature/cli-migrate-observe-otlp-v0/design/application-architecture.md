# Application Architecture — `cli-migrate-observe-otlp-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

Architectural question: how does `kaleidoscope-cli migrate` emit a
`cinder.migrate.count` OTLP-JSON line into the operator's
`--observe-otlp <path>` file when set, while preserving byte-for-byte
stdout and locked-test outcomes when absent? Answer (DD1+DD2): grow
`migrate(...)` by one trailing `Option<&Path>` parameter; dispatch
internally on `match otlp_log_path` that yields
`CinderToOtlpJsonWriter::new(file)` in `Some` and `CinderRecorder` in
`None`, both passed to `FileBackedTieringStore::open(...)`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-migrate-observe-otlp v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli migrate; tails the --observe-otlp file via a sidecar that forwards to an OTLP/HTTP collector.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. migrate subcommand emits one cinder.migrate.count OTLP-JSON line per successful manual tier transition when --observe-otlp is set. AGPL-3.0-or-later.")
  System_Ext(sidecar, "Operator sidecar", "Tails the --observe-otlp NDJSON file, wraps each line in a MetricsData envelope, POSTs to a real OTLP/HTTP collector. Unchanged by this feature.")
  System_Ext(collector, "OTLP/HTTP collector", "Org-supplied. Already ingests kaleidoscope.cinder scoped metrics from the sidecar (cinder.place.count via cli-cinder-otlp-wiring-v0).")
  System_Ext(dashboard, "Operator dashboard", "Gains a cinder.migrate.count panel alongside the existing cinder.place.count panel — same scope, sibling metric.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts the --observe-otlp <path> file with O_APPEND atomicity up to PIPE_BUF.")

  Rel(operator, cli, "Invokes `migrate <tenant> <data_dir> <item_id> <to_tier> [--observe-otlp <path>]`")
  Rel(cli, filesystem, "Appends one cinder.migrate.count OTLP-JSON line per successful migrate to the --observe-otlp path")
  Rel(sidecar, filesystem, "Tails the --observe-otlp file from")
  Rel(sidecar, collector, "Wraps each NDJSON line in a MetricsData envelope and POSTs to")
  Rel(collector, dashboard, "Surfaces ingested metrics to")
  Rel(operator, dashboard, "Reads `kaleidoscope.cinder / cinder.migrate.count` row on")
```

The system context shows the migrate event joining the same value
chain ingest-side place events already use. Operationally Priya
gains a queryable audit trail of state-mutating manual migrations
with zero configuration change to the sidecar or collector.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-migrate-observe-otlp v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "Parses argv; run_migrate calls parse_observe_otlp; forwards Option<&Path> to library migrate().")
    Container(migrate_fn, "migrate function", "Rust, src/lib.rs:424-456", "Six-arg: parse_tier (parse-before-open); match otlp_log_path -> recorder box; FileBackedTieringStore::open(base, recorder); get_entry pre-flight; cinder.migrate(...); writeln stdout transition line.")
    Container(cinder_writer, "CinderToOtlpJsonWriter<File>", "Rust, self-observe::cinder_otlp_json", "Owns Mutex<File>. record_migrate(tenant, from, to) emits one cinder.migrate.count OTLP-JSON line. Public API locked by ADR-0039 §1; constructor reused unchanged.")
    Container(noop, "cinder::NoopRecorder", "Rust, cinder::metrics", "Quiescent recorder used on the no-flag path; byte-identical to today.")
  }
  Container_Boundary(stores, "Storage adapter") {
    Container(cinder_store, "FileBackedTieringStore", "Rust, cinder crate", "Receives the recorder box at open() time; calls recorder.record_migrate on each successful tier transition.")
  }
  ContainerDb(otlp_file, "--observe-otlp <path>", "POSIX file, O_APPEND", "NDJSON sink. Receives one cinder.migrate.count line per successful migrate. Created lazily inside the Some(path) arm AFTER parse_tier succeeds (OK4: invalid-tier never reaches the open).")
  System_Ext(sidecar, "Operator sidecar", "Tails NDJSON; forwards to OTLP/HTTP collector.")

  Rel(operator, main, "Invokes with [--observe-otlp <path>]")
  Rel(main, migrate_fn, "Calls migrate(..., otlp_log_path = otlp_path.as_deref())")
  Rel(migrate_fn, otlp_file, "Opens lazily with OpenOptions::create(true).append(true) inside the Some(path) arm AFTER parse_tier")
  Rel(migrate_fn, cinder_writer, "Constructs CinderToOtlpJsonWriter::new(file) in the Some(path) arm and boxes as the recorder")
  Rel(migrate_fn, noop, "Constructs Box::new(CinderRecorder) in the None arm — byte-identical to pre-feature behaviour")
  Rel(migrate_fn, cinder_store, "Wires recorder into via FileBackedTieringStore::open")
  Rel(cinder_store, cinder_writer, "Calls record_migrate(tenant, from, to) on successful migrate (Some path only)")
  Rel(cinder_writer, otlp_file, "write_all(line) + flush via Mutex<File> guard to")
  Rel(sidecar, otlp_file, "Tails NDJSON lines from")
```

The container view shows the single-writer shape (contrast with the
two-writer + `try_clone` shape in `cli-cinder-otlp-wiring-v0`'s
ingest path). Only the Cinder store is opened; Lumen is never
touched on the migrate path (D-NoLumenTouch inherited from
`cli-migrate-subcommand-v0`). The within-writer NDJSON-validity
guarantee from ADR-0039 §2 covers the single-writer case
trivially — no cross-writer composition is needed because there is
no second writer.

## C4 — Component View (Level 3)

**Not produced.** The change inside `migrate()` is one match-arm
insertion (the recorder construction at
`crates/kaleidoscope-cli/src/lib.rs:434` becomes a `match
otlp_log_path { ... }` over two arms) plus one signature parameter
addition. The acceptance test is one new file mirroring
`migrate_subcommand.rs`. Per the SA principle "L3 only for complex
subsystems (5+ components)", L3 is explicitly skipped. Reification
conditions: L3 would become appropriate only if (a) a sink-fanout
abstraction were introduced (rejected by DD2 of this wave and DD1
of `cli-cinder-otlp-wiring-v0`), (b) the `migrate()` body grew
sub-components for the writer construction (it does not — the
construction is five lines of std-lib calls), or (c) a third
writer landed on the same path (out of scope).
