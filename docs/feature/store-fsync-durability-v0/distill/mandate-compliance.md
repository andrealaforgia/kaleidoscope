# Mandate Compliance — store-fsync-durability-v0 (DISTILL)

Evidence that the four acceptance-test design mandates hold, plus the RED
scaffold inventory DELIVER must replace.

## CM-A — Hexagonal boundary (driving ports only)

Every new test imports only the store's PUBLIC driving ports. Verified imports:

| Test | Imported entry points |
|------|-----------------------|
| lumen | `lumen::{FileBackedLogStore, LogBatch, LogRecord, LogStore, LyingFsyncBackend, NoopRecorder, SeverityNumber, TimeRange}` |
| ray | `ray::{FileBackedTraceStore, LyingFsyncBackend, NoopRecorder, Span, SpanBatch, SpanId, SpanKind, SpanStatus, TraceId, TraceStore}` |
| strata | `strata::{FileBackedProfileStore, …, LyingFsyncBackend, ProfileStore, ServiceName, TimeRange, …}` |
| cinder | `cinder::{FileBackedTieringStore, ItemId, LyingFsyncBackend, NoopRecorder, Tier, TieringStore}` |
| sluice | `sluice::{FileBackedQueue, LyingFsyncBackend, NoopRecorder, Queue}` |
| beacon | `beacon::{FileBackedRuleStateStore, LyingFsyncBackend, RuleState, RuleStateStore}` |
| pulse | `pulse::{FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore, NoopRecorder, TimeRange}` |

**Negative check (the brief's explicit prohibition)**: a grep for
`atomic_write_snapshot` / `fsync_probe(` calls across all seven new test files
returns ZERO call sites (the only match is a prose doc-comment). The driven
durability helpers are exercised INDIRECTLY through the store seams, never
entered as headline acceptance. Mechanism (b) enters through each store's
public `open_with_fsync_backend` inherent constructor — a driving port on the
concrete adapter, not the trait (C1 preserved).

## CM-B — Business language

Scenario function names and the per-scenario module/inline docs use domain
terms: "acked log survives a power loss", "the store opens cleanly after a
crash during a snapshot", "refuses to start on a substrate that lies about
fsync", "an in-flight item is recovered, not silently dropped". Persona Priya
(on-call SRE) frames the WS. Unavoidable durability vocabulary (`SIGKILL`,
`fsync`, `snapshot`, `WAL`) is the ubiquitous language of this domain and is
carried verbatim from the DISCUSS user stories and ADR-0060 — it IS the
business language of a crash-durability feature, not leaked implementation
jargon. No HTTP status codes, no JSON-shape assertions, no DB vocabulary
appear in scenario intent; the read path is named in business terms
("queryable after restart", "present in the recovered state/ledger/queue").

## CM-C — Complete user journeys with observable value

Every scenario is a complete journey: a trigger (the exporter acks a write /
the substrate lies), the system event (the crash / the discard), and an
observable outcome a stakeholder confirms (the acked record/span/profile/
migration/enqueue/transition is present after restart, OR the collector
refuses to start and exits non-zero). Walking skeleton (lumen) is demo-able:
"Priya restarts the collector and her acked log is still there, and it started
cleanly even though the crash hit during a snapshot." Observable-behaviour
(Dim 7): assertions check return values from driving-port calls (`query`,
`get_trace`, `get_tier`, `dequeue`, `load_all`) and process stderr/exit
status, never private fields or internal call counts.

## CM-D — Pure-function extraction / adapter parametrisation

The business logic under test (the tmp+fsync+rename+fsync-dir snapshot
sequence; the per-record `sync_all`) is extracted ONCE into
`wal_recovery::atomic_write_snapshot` and the `FsyncBackend` seam (ADR-0060
§4) — a single pure-ish helper behind a port, not seven copies. The acceptance
fixtures are NOT parametrised across environments; the only injected variant
is the thin adapter layer (`RealFsyncBackend` vs `LyingFsyncBackend::no_op()` /
`::truncating()`) threaded through the `open_with_fsync_backend` port. This is
exactly the "parametrise only the adapter layer" shape Mandate 4 prescribes.

## RED scaffold inventory (DELIVER replaces ALL of these)

Mandate 7 RED-ready scaffolds created in this wave. Each carries a
`// SCAFFOLD: true` marker and (for code bodies) a `panic!("__SCAFFOLD__ …")`.
**ZERO `// SCAFFOLD: true` markers tied to store-fsync-durability-v0 must
remain after DELIVER.** (Pre-existing markers in `crates/aperture/src/error.rs`
and `crates/query-http-common/src/lib.rs` are unrelated and untouched.)

### Shared seam (the ADR-0060 §4 home)

- `crates/wal-recovery/src/lib.rs` — the durability seam scaffold: `trait
  FsyncBackend`, `struct RealFsyncBackend`, `struct LyingFsyncBackend`
  (`no_op`/`truncating`), `enum FsyncProbeError` (+ `substrate_descriptor`),
  `fn fsync_probe`, `fn atomic_write_snapshot`. 8 panicking bodies. DELIVER
  MOVES the real pulse `FsyncBackend` family here verbatim and authors
  `atomic_write_snapshot` per ADR-0060 §2.

### Per-store seam (inherent constructor + re-export)

| Crate | `open_with_fsync_backend` scaffold | seam re-export |
|-------|-------------------------------------|----------------|
| lumen | `src/file_backed.rs` | `src/lib.rs` |
| ray | `src/file_backed.rs` | `src/lib.rs` |
| strata | `src/file_backed.rs` | `src/lib.rs` |
| cinder | `src/file_backed.rs` | `src/lib.rs` |
| sluice | `src/file_backed.rs` (carries the `cap` arg) | `src/lib.rs` |
| beacon | `src/state_store.rs` (no recorder arg) | `src/lib.rs` |

DELIVER replaces each panicking `open_with_fsync_backend` with the real wiring
and makes the public `open` delegate to it (C1: inherent method, trait surface
unchanged). pulse already has the real `open_with_fsync_backend` (ADR-0049) —
no scaffold there; DELIVER only re-points pulse's re-export to `wal_recovery`.

### Kill-target helper binaries (mechanism (a))

| Crate | bin source | `[[bin]]` name (sets `CARGO_BIN_EXE_*`) |
|-------|-----------|------------------------------------------|
| lumen | `src/bin/lumen_crash_target.rs` | `lumen-crash-target` |
| ray | `src/bin/ray_crash_target.rs` | `ray-crash-target` |
| strata | `src/bin/strata_crash_target.rs` | `strata-crash-target` |
| cinder | `src/bin/cinder_crash_target.rs` | `cinder-crash-target` |
| sluice | `src/bin/sluice_crash_target.rs` | `sluice-crash-target` |
| beacon | `src/bin/beacon_crash_target.rs` | `beacon-crash-target` |
| pulse | `src/bin/pulse_crash_target.rs` | `pulse-crash-target` |

Each is a `panic!("__SCAFFOLD__ …")` `fn main`. DELIVER implements the
`--seed-then-loop-snapshot` / `--probe-lying` (and strata `--open-then-idle` /
sluice `--seed-then-dequeue-inflight`) modes documented in each bin's
module-doc: open the store from `$KALEIDOSCOPE_CRASH_PILLAR_ROOT`, ack the
named records, print `CRASH_TARGET_READY`, then loop snapshotting so a kill
lands mid-snapshot; or drive the composition root with a `LyingFsyncBackend`
and emit `event=health.startup.refused substrate=<descriptor>` to stderr,
exiting non-zero. Writes ONLY under the parent-supplied tmp root.

### Cargo manifest additions

- `wal-recovery` dep added to `strata`, `sluice`, `beacon` (ray, cinder, pulse
  already had it). DELIVER keeps these edges (the stores route their
  `append_wal`/`snapshot` through the seam).
- `[[bin]]` + new `[[test]]` entries added to all seven store Cargo.toml.

## Why these scaffolds make RED for the RIGHT reason (No Fixture Theater)

The Given steps set up PRECONDITIONS (acked writes, a lying substrate, a tmp
pillar root), never the expected output. Today the seams panic, so every
enabled scenario would error; once DELIVER wires `sync_all` + the atomic
snapshot, the mechanism-(b) tests go green BECAUSE the write reached stable
storage, and the mechanism-(a) tests go green BECAUSE the snapshot is atomic.
A scenario cannot pass without the production change — exactly the
fixture-theatre guard.
