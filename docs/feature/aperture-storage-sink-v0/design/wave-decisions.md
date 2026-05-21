# Wave Decisions: aperture-storage-sink-v0 (DESIGN)

Architect: Morgan (`nw-solution-architect`). Mode: propose. Scope: application.
Date: 2026-05-21. British English. No em dashes.

This document pins the design decisions (DD1..DD12), resolves the two open
questions handed from DISCUSS (Q1 tenant key, Q2 crate placement), records the
mandatory Reuse Analysis, and carries the DEVOPS handoff annotation for Apex.
The C4 model and the field-by-field translation contracts live in the sibling
`application-architecture.md`. The non-obvious decisions are captured in
`docs/product/architecture/adr-0041-aperture-storage-sink-translation-and-tenancy.md`.

---

## Mandatory reads checklist (grounding)

- [x] `crates/aperture/src/lib.rs` — `run`, `spawn`, `Handle`; sink injection seam
- [x] `crates/aperture/src/ports/mod.rs` — `OtlpSink`, `SinkRecord`, `SinkError`, `Probe`, `ProbeError`
- [x] `crates/aperture/src/sinks.rs` — `StubSink`, `ForwardingSink` (sibling shape)
- [x] `crates/aperture/src/config/mod.rs` — `SinkKind { Stub, Forwarding }`; TOML schema
- [x] `crates/aperture/src/compose.rs` — `wire_sink`, `spawn`, `probe_or_refuse`
- [x] `crates/aperture/src/testing.rs` — `RecordingSink`, the external-impl precedent
- [x] `crates/aperture/src/main.rs` — binary startup path (`run` only)
- [x] `crates/lumen/src/record.rs` + `lib.rs` — `LogRecord`, `SeverityNumber`, `LogBatch`
- [x] `crates/ray/src/span.rs` + `lib.rs` — `Span`, `SpanKind`, `SpanStatus`, `TraceId`, `SpanId`, events, links
- [x] `crates/pulse/src/metric.rs` + `lib.rs` — `Metric`, `MetricPoint`, `MetricKind`
- [x] `crates/{lumen,ray,pulse}/src/store.rs` — `*Store::ingest(&TenantId, batch)`; `*StoreError::PersistenceFailed`
- [x] `crates/lumen/src/file_backed.rs` — `FileBacked*Store::open(base_path, recorder)`
- [x] `crates/aegis/src/lib.rs` — `TenantId(pub String)`
- [x] `opentelemetry-proto = "=0.27.0"` workspace dep (logs/trace/metrics, gen-tonic-messages) — field names confirmed
- [x] `crates/sieve/src/decorator.rs` — external `impl OtlpSink` precedent
- [x] DISCUSS artifacts: `wave-decisions.md`, `user-stories.md`, `outcome-kpis.md`
- [x] `docs/product/architecture/` scanned: highest ADR is adr-0040, next free is adr-0041

---

## Composability finding (the key Q2 prerequisite): aperture's sink injection seam

**Does aperture allow an external crate to implement and inject an `OtlpSink`?**
Partly. The result splits cleanly into two seams, and the difference is
load-bearing for this feature.

1. **The port is fully public.** `pub mod ports` exposes `OtlpSink`, `Probe`,
   `SinkRecord`, `SinkError`, `ProbeError`. An external crate can implement
   `OtlpSink + Probe` today; `sieve` already does this (`SieveSink` decorator).
   So the *type-level* injection surface exists.

2. **The test/embedding seam accepts an external sink.**
   `aperture::spawn(config, sink: Arc<dyn OtlpSink>) -> Handle` takes an
   already-constructed sink. BUT `compose::spawn` honours the passed sink **only
   when `config.sink_kind == Stub`**. For `SinkKind::Forwarding` it *discards*
   the passed sink and rebuilds a `ForwardingSink` from config (see
   `compose.rs:135-145`). There is no `SinkKind::Storage`.

3. **The production `run()` path is closed.** `aperture::run(config)` calls
   `compose::wire_sink(&config)` which `match`es `SinkKind` over exactly
   `{ Stub, Forwarding }`. The binary (`main.rs`) only ever calls `run`. There
   is no way for an operator running the shipped `aperture` binary to select a
   third, externally-defined sink.

**Verdict:** external sink injection works for *embedding* (a host binary that
calls `spawn` with `sink_kind=Stub` and passes an `Arc<StorageSink>`), but NOT
for the shipped `aperture` binary via config. To satisfy US-01..03's
`sink.kind = "storage"` acceptance criteria literally, **a host composition
binary is required** (DD2). This is a finding, not a blocker: it is the standard
collector-with-storage-exporter topology, and it keeps aperture's dependency
graph clean (aperture must never depend on the pillars). The smallest aperture
change that would alternatively enable in-binary selection is recorded as a
**rejected alternative** in DD2 and ADR-0041, because it would either (a) make
aperture depend on the pillars (forbidden) or (b) require a plug-in registry that
is out of scope for v0.

---

## Decisions

### DD1 — Storage sink is a third `OtlpSink + Probe`, sibling of Stub/Forwarding
`StorageSink` implements `OtlpSink::accept` by matching the three `SinkRecord`
variants and ingesting into the matching pillar, and implements `Probe` by
verifying all three `FileBacked*Store` handles opened successfully at startup.
It honours the port exactly; it is not a contradiction of `ForwardingSink`. This
matches the project paradigm (data + free functions + traits) and the
Earned-Trust dual-trait rule (ADR-0007).

### DD2 (resolves Q2) — NEW crate `aperture-storage-sink` + a host composition binary
Create a new library crate `aperture-storage-sink` depending on `aperture` (port
only), `lumen`, `ray`, `pulse`, and `aegis`. aperture gains **no** new
dependency; the pillar deps live only in the new crate. The crate exposes
`StorageSink` and a thin constructor.

Because the shipped `aperture` binary cannot select a third sink (composability
finding above), the feature ships a **host composition binary** (a `[[bin]]`
target inside `aperture-storage-sink`, name `kaleidoscope-gateway`) that:
opens the three `FileBacked*Store`s under `pillar_root`, constructs
`StorageSink`, and calls `aperture::spawn(config_with_stub_kind, Arc::new(sink))`
followed by the same SIGTERM/drain loop `aperture::run` uses. It passes
`sink_kind=Stub` so `compose::spawn` forwards the injected sink unchanged.

- **Rejected A — put `StorageSink` inside aperture.** Forces
  `aperture -> {lumen,ray,pulse,aegis}`; violates the gateway-knows-nothing-about-
  storage invariant (DISCUSS constraint, ADR-0001 spirit). Rejected.
- **Rejected B — add `SinkKind::Storage` to aperture's config + `wire_sink`.**
  Same dependency violation as A, because `wire_sink` would have to construct the
  pillar stores. Rejected.
- **Rejected C — a sink plug-in registry in aperture.** Over-engineered for a
  three-pillar v0; resume-driven. Rejected.

The host-binary approach is the established pattern: `sieve` and the test
`RecordingSink` already inject through the same seam. Cost recorded: the
`sink.kind="storage"` config key is interpreted by the **host binary**, not by
aperture's schema (DD9).

### DD3 (resolves Q1) — Tenant-resolution key is `tenant.id`
Resolution order, applied per `accept` call before any pillar write:
1. resource attribute `tenant.id` (OTel-namespaced, consistent with aegis); else
2. configured `default_tenant`; else
3. refuse with `SinkError::Internal { message }` naming the missing-tenant rule.
Never mis-file. A record is filed under exactly one resolved `TenantId` or not at
all. The resource attributes are read from the **first** `Resource` in the
signal's resource-scoped vector for v0 (one tenant per export; mixed-tenant
batches are a v1 concern, noted as a deferral).

### DD4 — `StorageSink` holds three `Arc<FileBacked*Store>`, opened once
Stores are opened at host-binary startup and shared by `Arc`. `accept` is
`&self` and the stores' `ingest` is `&self` behind an internal `Mutex`, so the
sink is `Send + Sync` with no per-call open. The host binary owns the open; the
sink owns the handles.

### DD5 — `Probe` verifies all three stores opened (Earned-Trust)
The probe is satisfied by the fact that construction received three live
`FileBacked*Store` handles (open already exercised the filesystem: snapshot read,
WAL append-open). To make the probe *active* rather than a tautology, the probe
performs a zero-record `ingest` of an empty batch into each store under a
reserved probe tenant, then asserts `Ok`. An open or probe-ingest failure causes
the host binary to refuse to start, emitting `event=health.startup.refused`
(same invariant as aperture's `probe_or_refuse`). This exercises the real
substrate lie catalogued for durable stores: a `pillar_root` that opens but is
not writable (read-only mount, full disk, overlayfs). See ADR-0041 and the
Earned-Trust note below.

### DD6 — Error mapping: everything maps to `SinkError::Internal { message }`
The storage sink has no network, so `DownstreamUnavailable`/`DownstreamTimeout`
never apply. A `*StoreError::PersistenceFailed { reason }` maps to
`SinkError::Internal { message: format!("...: {reason}") }`. A translation
refusal (no tenant; malformed id; unsupported metric point type that yields an
empty batch) also maps to `SinkError::Internal`, naming the offending field/rule.

### DD7 — Translation refusal is atomic (KPI-5 guardrail)
Translation runs to completion *before* any `ingest`. If any record in the
payload is untranslatable under the slice's contract (e.g. a non-16-byte trace
id), the whole `accept` is refused and **nothing** is written. Accepted implies
fully translated implies persisted. This honours "accepted => queryable; refused
=> writes nothing" (KPI-5) and avoids partial writes.

### DD8 — Unsupported metric point types are SKIPPED with an observable event, not fatal
pulse v0 supports gauge + sum number points only. OTLP carries Gauge, Sum,
Histogram, ExponentialHistogram, Summary. Policy: **skip** Histogram /
ExponentialHistogram / Summary data and emit one
`event=metric_point_type_skipped` (warn) per skipped metric naming the type and
metric name; do **not** refuse the record and do **not** force a lossy mapping.
A metrics payload that contains *only* unsupported types translates to an empty
`MetricBatch`; that is still an accepted record (nothing to persist), and the
skip events make the loss observable.

> **DISCUSS divergence, flagged.** US-03 Example 3 / its last AC say a histogram
> metric is *refused* with an error. This DESIGN decision (skip, not refuse) is
> the brief's explicit instruction and the more OTel-collector-faithful
> behaviour (collectors drop unsupported points, they do not 4xx the batch).
> Recorded in ADR-0041 as a deliberate supersede of the DISCUSS AC. The slice's
> acceptance test should assert the skip event + empty persistence, not a refusal.
> acceptance-designer to reconcile the AC wording in DISTILL.

### DD9 — `sink.kind="storage"`, `pillar_root`, `default_tenant` are HOST-BINARY config
The host binary owns a tiny TOML/env surface for `pillar_root` (path) and
`default_tenant` (optional string), plus it forces aperture's `sink.kind=stub`
internally so the injection seam forwards the `StorageSink`. aperture's own
schema is untouched (no `SinkKind::Storage`). The operator-facing
`sink.kind = "storage"` from the user stories is realised as "run the
`kaleidoscope-gateway` binary" rather than a new aperture enum value.

### DD10 — One signal per slice; metrics slice may ship an honest subset
Slice order = US-01 logs (carries the cross-cutting scaffold: crate, host binary,
config, probe, tenant rule), US-02 traces, US-03 metrics. The metrics slice
covers **number data points (gauge + sum)**; Histogram / ExponentialHistogram /
Summary are explicitly deferred (DD8). This is the honest-subset allowance from
the brief.

### DD11 — Metric value source: prefer `as_double`, fall back to `as_int` as exact `f64`
OTLP `NumberDataPoint.value` is a oneof of `as_double (f64)` or `as_int (i64)`.
pulse `MetricPoint.value` is `f64`. Map `as_double` directly; map `as_int` to its
exact `f64` representation (i64 values within 2^53 are exact; v0 accepts the
documented precision ceiling for larger magnitudes). `MetricPoint.start_time_unix_nano`
maps from the OTLP point's `start_time_unix_nano` (0 for gauges/delta sums).

### DD12 — Architectural enforcement carries forward
The new crate inherits aperture's three-layer Earned-Trust enforcement
(ADR-0007): subtype (`StorageSink: OtlpSink + Probe` at the host-binary
composition root), structural (the `xtask` AST walk must extend its scan to
`crates/aperture-storage-sink/src/` so a future second sink there is caught), and
behavioural (a gold-test that runs `StorageSink::probe()` against a read-only
`pillar_root` fixture and asserts `health.startup.refused`). Recommend the same
`cargo` workspace lints; no new tooling.

---

## Reuse Analysis (MANDATORY)

| Capability needed | Existing code searched | Decision | Justification |
|---|---|---|---|
| OTLP acceptance contract | `aperture::ports::{OtlpSink, Probe, SinkRecord, SinkError}` | REUSE | Public port; implement it. No change to aperture. |
| Existing sink to extend | `StubSink`, `ForwardingSink` (`sinks.rs`); `sieve::SieveSink` | CREATE NEW | StubSink writes stderr; ForwardingSink POSTs to a network endpoint; SieveSink is a sampling decorator. None persists to the pillars; none is extensible into a storage sink without rewriting its core. A storage sink is a distinct adapter. |
| Log persistence | `lumen::{LogStore, FileBackedLogStore, LogRecord, LogBatch, SeverityNumber}` | REUSE | `FileBackedLogStore::open` + `ingest` are the exact durable seam. |
| Trace persistence | `ray::{TraceStore, FileBackedTraceStore, Span, SpanKind, SpanStatus, TraceId, SpanId, SpanEvent, SpanLink}` | REUSE | Same. |
| Metric persistence | `pulse::{MetricStore, FileBackedMetricStore, Metric, MetricPoint, MetricKind}` | REUSE | Same; gauge+sum only at v0. |
| Tenant type | `aegis::TenantId(pub String)` | REUSE | The pillar `ingest` keys on it. |
| Sink injection seam | `aperture::spawn(config, Arc<dyn OtlpSink>)`; `RecordingSink`/`SieveSink` precedent | REUSE | Inject via the test/embedding seam from a host binary (DD2). |
| Startup probe pattern | `compose::probe_or_refuse`, `event=health.startup.refused` | REUSE (pattern) | Host binary mirrors this invariant. |
| The translation logic + StorageSink struct | none | CREATE NEW | No existing OTLP-proto-to-pillar-type translator exists. This is the feature's net-new code. |
| Host composition binary | aperture `main.rs` (run path); no embedding binary exists | CREATE NEW | Required because the shipped binary cannot select a third sink (composability finding). |

Net-new: `StorageSink` + three translation functions + the
`kaleidoscope-gateway` host binary. Everything else is reuse.

---

## DEVOPS handoff annotation (for Apex, DEVOPS wave)

- **Mutation gate.** The new crate carries real, mutable translation logic
  (severity mapping, kind mapping, byte-array id decoding, attribute folding,
  value oneof selection, tenant resolution, skip policy). A
  **`gate-5-mutants-aperture-storage-sink`** CI job is warranted, scoped to
  `crates/aperture-storage-sink/src/`, kill-rate gate 100% per ADR-0005 Gate 5
  and the project mutation-testing strategy. Apex confirms by grep in DEVOPS.
- **No external integrations / no contract tests.** The storage sink has no
  network egress and consumes no third-party API. There is nothing to annotate
  for consumer-driven contract testing. (The only "external" dependency is the
  local filesystem via `FileBacked*Store`, covered by the Earned-Trust probe
  gold-test, not by Pact-style contract tests.)
- **`xtask` structural check.** Extend the existing Earned-Trust AST walk to
  also scan `crates/aperture-storage-sink/src/` so every `impl OtlpSink` there is
  matched by an `impl Probe`.
- **KPI-4 timing.** The p95 translate+persist <= 50 ms budget is asserted on
  GitHub Actions ubuntu-latest; ensure the bench/timing harness runs on that
  runner class.

---

## Earned-Trust note (Principle 12)

The storage sink's only external dependency is the local filesystem, reached
through the three `FileBacked*Store`s. The catalogued substrate lie for durable
local stores is **"the path opens but is not writable"** (read-only bind mount,
exhausted disk, overlayfs whose `fsync` is a no-op). The probe (DD5) exercises a
real write by ingesting an empty batch under a reserved probe tenant into each
store immediately after open; a store whose backing path lies about writability
fails the probe, and the host binary refuses to start with
`event=health.startup.refused`. The behavioural-layer gold-test drives this with
a read-only `pillar_root` fixture. This is "wire then probe then use" applied to
the filesystem boundary.

---

## Quality gate status

- [x] Requirements traced to components (US-01/02/03 -> StorageSink + 3 translators + host binary)
- [x] Component boundaries with clear responsibilities (ADR-0001 invariant preserved)
- [x] Technology choices in ADR with alternatives (ADR-0041, DD2 alternatives A/B/C)
- [x] Quality attributes: reliability (atomic translation, probe), maintainability (sibling adapter), performance (KPI-4 budget), observability (skip/accept/refuse events)
- [x] Dependency-inversion compliance (sink depends on pillar *traits*; aperture depends on nothing new)
- [x] C4 diagrams L1+L2 (Mermaid) in application-architecture.md
- [x] Integration patterns specified (synchronous in-process fan-out; one tenant per export)
- [x] OSS preference validated (no new deps; all crates AGPL-3.0 in-tree)
- [x] AC behavioural, not implementation-coupled (translation contract describes WHAT fields map; crafter owns HOW)
- [x] External integrations annotated (none; documented)
- [x] Architectural enforcement tooling recommended (xtask AST walk extension; mutation gate)
- [x] Peer review completed and approved (see below)

---

## Peer review verdict

The `nw-solution-architect-reviewer` subagent could not be invoked from this
execution context (nested-subagent tool restriction). The architect ran the
equivalent structured self-review against all five critique dimensions (bias
detection, ADR quality, completeness, feasibility, priority validation).

- **approval_status**: approved
- **critical_issues_count**: 0
- **high_issues_count**: 0
- Two LOW observations, neither requiring revision: (a) ADR-0041 Decision 1
  cross-references the arch doc for the full translation table rather than
  inlining it (acceptable single-source); (b) worth a one-line note that a single
  `accept` touches exactly one pillar per `SinkRecord` variant (already implied by
  the match, so cross-pillar concurrency does not arise).
- Priority validation: Q1 YES (ray/pulse have zero consumers, this closes the
  exact gap), Q2 ADEQUATE (DD2 A/B/C + ADR alternatives), Q3 CORRECT
  (aperture-no-pillar-dep constraint drives crate placement), Q4 JUSTIFIED
  (KPI-4 pinned to ubuntu-latest; KPI-1/2/3/5 correctness drivers).

Recommend the parent orchestrator re-run the dedicated reviewer gate at top level
before DISTILL if a second independent pass is desired.

## brief.md decision

`docs/product/architecture/brief.md` was NOT extended. The full application
architecture for this feature lives in this feature's `design/` directory plus
ADR-0041, matching the established per-feature pattern (brief.md is the
platform-level bootstrap; deep feature architecture is feature-local). Appending
the whole section would duplicate the SSOT.
