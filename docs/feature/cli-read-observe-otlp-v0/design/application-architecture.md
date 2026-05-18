# Application Architecture — `cli-read-observe-otlp-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

The architectural question this feature must answer:

> The `read()` function constructs exactly one writer (the Lumen
> recorder) and emits exactly one OTLP-JSON line per invocation
> (one `lumen.query.count` event). How does the wiring open the
> operator-supplied `--observe-otlp <path>` file for that single
> writer, and does the open mechanism reuse the `try_clone`
> machinery from ADR-0039 §8?

The decision is **single-handle `OpenOptions::append`, no `try_clone`**:
open the path exactly once with
`std::fs::OpenOptions::new().create(true).append(true).open(path)`
and pass the resulting `File` directly into
`LumenToOtlpJsonWriter::new(file)`. The `try_clone` step in §8 was
specifically motivated by the two-writer ingest case (Lumen + Cinder
over one shared file description); with one writer there is no second
owner to produce a clone for, so the call is elided. Cross-invocation
append safety (the OK3 ingest-then-read shell-session symmetry) is
inherited for free from POSIX `O_APPEND` semantics, which seek to
end-of-file on every `write(2)` regardless of which process opened the
file first. Full rationale, rejected alternatives, and the Reuse
Analysis in `design/wave-decisions.md > DD1, DD2, DD4`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-read-observe-otlp v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli read; tails the --observe-otlp file via a sidecar that forwards to an OTLP/HTTP collector.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. read subcommand now routes the Lumen query event into the same --observe-otlp file the ingest subcommand already writes to. AGPL-3.0-or-later.")
  System_Ext(sidecar, "Operator sidecar", "Tails the --observe-otlp NDJSON file, wraps each line in a MetricsData envelope, POSTs to a real OTLP/HTTP collector. Out of scope for this feature.")
  System_Ext(collector, "OTLP/HTTP collector", "Org-supplied. Now receives kaleidoscope.lumen / lumen.query.count lines alongside the existing lumen.ingest.count and cinder.place.count lines.")
  System_Ext(dashboard, "Operator dashboard", "Renders a new lumen.query.count panel for per-tenant query throughput and expensive-tenant detection.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts the --observe-otlp <path> file with O_APPEND semantics; survives between shell invocations of ingest and read.")

  Rel(operator, cli, "Invokes `read <tenant> <data_dir> --observe-otlp <path>` against, capturing stdout NDJSON of matched records elsewhere")
  Rel(cli, filesystem, "Appends exactly one OTLP-JSON `lumen.query.count` line per `read` invocation to the --observe-otlp path through")
  Rel(sidecar, filesystem, "Tails the --observe-otlp file from")
  Rel(sidecar, collector, "Wraps each NDJSON line in a MetricsData envelope and POSTs to")
  Rel(collector, dashboard, "Surfaces ingested metrics to")
  Rel(operator, dashboard, "Reads `kaleidoscope.lumen / lumen.query.count` row on")
```

The system context view shows the operator-visible value chain. The
change this feature ships is confined to the `kaleidoscope-cli` node:
the `read` subcommand joins `ingest` at the `--observe-otlp <path>`
file boundary. Everything downstream (the sidecar, the collector, the
dashboard) is unchanged — that is the operational value the feature
delivers. Priya's existing chain gains the read-side metric type
without any configuration change.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-read-observe-otlp v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "run_read now calls parse_observe_otlp(args); forwards Option<PathBuf> into read() as as_deref. print_usage gains one mention of --observe-otlp on the read line.")
    Container(read_fn, "read function", "Rust, src/lib.rs:252-269", "Signature gains otlp_log_path: Option<&Path> (4th positional param). Matches on it: Some(path) opens the file once with OpenOptions::create(true).append(true) and wraps in LumenToOtlpJsonWriter; None preserves today's LumenToPulseRecorder wiring byte-equivalently.")
    Container(lumen_writer, "LumenToOtlpJsonWriter<File>", "Rust, self-observe::lumen_otlp_json", "Owns Mutex<File>. On record_query, emits one `lumen.query.count` OTLP-JSON line via write_all(body_with_trailing_newline) + flush inside the Mutex guard. Public API locked.")
    Container(pulse_recorder, "LumenToPulseRecorder", "Rust, self-observe::lumen_bridge", "Today's behaviour for the None arm. Wraps an in-process InMemoryMetricStore over PulseRecorder. Emits nothing observable to any file; dies at end of call.")
  }
  Container_Boundary(stores, "Storage adapter") {
    Container(lumen_store, "FileBackedLogStore", "Rust, lumen crate", "Wires the chosen recorder as its MetricsRecorder. read() calls lumen.query(tenant, TimeRange::all()) exactly once; the recorder receives one record_query event per invocation.")
  }
  ContainerDb(otlp_file, "--observe-otlp <path>", "POSIX file, O_APPEND", "Single NDJSON file shared with the ingest subcommand. Receives one new lumen.query.count line per read invocation. Pre-existing lumen.ingest.count and cinder.place.count lines from prior ingest invocations are preserved (no truncation; O_APPEND seeks to end-of-file).")
  System_Ext(sidecar, "Operator sidecar", "Tails NDJSON; forwards to OTLP/HTTP collector.")

  Rel(operator, main, "Invokes with --observe-otlp <path>")
  Rel(main, read_fn, "Dispatches to (otlp_log_path = Some(path))")
  Rel(read_fn, otlp_file, "Opens once with OpenOptions::create(true).append(true) through (Some arm only)")
  Rel(read_fn, lumen_writer, "Constructs LumenToOtlpJsonWriter::new(file) from the opened handle (Some arm)")
  Rel(read_fn, pulse_recorder, "Constructs LumenToPulseRecorder over fresh Pulse (None arm; byte-equivalent to today)")
  Rel(read_fn, lumen_store, "Wires the chosen recorder into via FileBackedLogStore::open")
  Rel(lumen_store, lumen_writer, "Calls record_query exactly once per read invocation (Some arm)")
  Rel(lumen_store, pulse_recorder, "Calls record_query exactly once per read invocation (None arm; dropped at end of call)")
  Rel(lumen_writer, otlp_file, "write_all(body_with_trailing_newline) + flush via Mutex<File> guard to")
  Rel(sidecar, otlp_file, "Tails NDJSON lines from")
```

The container view shows the single writer feeding one file. Unlike
the ingest container view (where two writers shared one file
description via `try_clone`), here only the Lumen writer participates
— the `read()` function does not construct a Cinder store at all
(DISCUSS D2). The within-writer NDJSON-validity guarantee (ADR-0039
§2: single coalesced `write_all` of body + `\n` inside the
`Mutex<File>` guard) is inherited unchanged. No cross-writer atomicity
question arises in this feature because no second in-process writer
exists. The OK3 ingest-then-read symmetry is a SEQUENTIAL-process
property: the prior `ingest` invocation's lines are already on disk
when `read` opens the file; `O_APPEND` ensures the new
`lumen.query.count` line lands after them without disturbing the
existing content.

## C4 — Component View (Level 3)

**Not produced.** The change inside `read()` is one match expression
over `otlp_log_path` (two arms, four lines per arm), and one positional
parameter added to the function signature. The change inside `main.rs`
is one `parse_observe_otlp(args)?` call and one `print_usage` line
edit. The new acceptance test is one file mirroring
`observe_otlp_flag.rs`. Per the SA principle ("Component (L3) only
for complex subsystems"), L3 is **explicitly skipped** for this
feature. Reification conditions: L3 would become appropriate if (a) a
shared `open_observe_otlp_file` helper were extracted (which DD2
rejected on rule-of-three grounds), (b) a second internal call site
emerged inside `read` itself (it does not — the recorder construction
is the only file-touching site), or (c) the recorder match grew a
third arm (e.g. a `read --observe-pulse` variant). None apply at v0.
