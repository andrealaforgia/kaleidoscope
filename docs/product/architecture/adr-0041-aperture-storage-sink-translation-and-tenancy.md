# ADR-0041 — Aperture storage sink: OTLP-to-pillar translation, tenancy, and unsupported-metric policy

- **Status**: Accepted
- **Date**: 2026-05-21
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture-storage-sink-v0`
- **Supersedes**: none (supersedes one DISCUSS acceptance criterion for US-03; see Decision 3)
- **Superseded by**: none

## Context

`aperture-storage-sink-v0` adds a third `OtlpSink` (sibling of `StubSink` and
`ForwardingSink`, ADR-0007) that persists accepted OTLP records into the durable
pillars: logs to `lumen`, traces to `ray`, metrics to `pulse`. It is the first
production consumer of ray and pulse and makes the platform run end to end.

Three decisions are non-obvious and will be read by anyone extending the sink, so
they are recorded here rather than left implicit in code:

1. The OTLP-proto-to-pillar-type translation contract (which proto field maps to
   which pillar field, and how mismatches are handled).
2. The tenant-resolution rule (OTLP has no native tenant).
3. The policy for OTLP metric data-point types that pulse v0 does not support
   (Histogram, ExponentialHistogram, Summary).

Two structural facts constrain the design and are decided in
`docs/feature/aperture-storage-sink-v0/design/wave-decisions.md` (DD1, DD2): the
sink is a new crate `aperture-storage-sink` (aperture must never depend on the
pillars), and it is wired through aperture's existing
`spawn(config, Arc<dyn OtlpSink>)` seam by a host composition binary, because the
shipped `aperture` binary's `run` path only knows `SinkKind::{Stub, Forwarding}`.
Those are recorded in the wave-decisions document; this ADR records the three
behavioural decisions above.

## Decision

### Decision 1 — Translation contract is field-by-field, all-or-nothing per accept

The full field-by-field mapping is specified in
`application-architecture.md` section 6 (logs, traces, metrics, the shared
attribute fold, and the `AnyValue`-to-`String` fold), read against the real
`opentelemetry-proto = "=0.27.0"` and pillar types. Two invariants govern it:

- **Byte-array identifiers are length-checked.** A `trace_id` must be exactly 16
  bytes and a `span_id`/`parent_span_id` exactly 8 bytes (empty `parent_span_id`
  and empty trace/span ids on logs map to `None`). A wrong-length id is a
  translation refusal naming the field.
- **Translation is atomic.** Translation runs to completion before any
  `ingest`. If any record is untranslatable, the whole `accept` is refused and
  nothing is persisted. Accepted implies fully translated implies persisted
  (KPI-5: "accepted => queryable; refused => writes nothing").

### Decision 2 — Tenant resolution: `tenant.id` -> `default_tenant` -> refuse

OTLP carries no tenant. The sink resolves a tenant once per `accept` from the
**first** resource's attributes, in this order:
1. resource attribute `tenant.id` (OTel-namespaced, consistent with aegis); else
2. the host binary's configured `default_tenant`; else
3. refuse with `SinkError::Internal { message }` naming the missing-tenant rule.

A record is filed under exactly one resolved `aegis::TenantId` or not at all;
never mis-filed. One tenant per export at v0; mixed-tenant batches deferred to v1.

### Decision 3 — Unsupported metric data-point types are SKIPPED, not fatal

pulse v0 supports gauge + sum number data points only. OTLP also carries
Histogram, ExponentialHistogram, and Summary. The sink **skips** unsupported
types and emits one `event=metric_point_type_skipped` (warn) per skipped metric
naming the type and the metric name. It does **not** refuse the record and does
**not** force a lossy mapping. A metrics payload containing only unsupported
types translates to an empty `MetricBatch`: still an accepted record (nothing to
persist), with the skip events making the loss observable.

This **supersedes** the US-03 DISCUSS acceptance criterion / Example 3, which said
a histogram metric is *refused* with an error. Skip-not-refuse is the brief's
explicit instruction and the OTel-collector-faithful behaviour (collectors drop
unsupported points rather than rejecting the whole batch). acceptance-designer
reconciles the AC wording in DISTILL.

### Earned-Trust (Principle 12)

The sink's only external dependency is the local filesystem via the three
`FileBacked*Store`s. `StorageSink` implements `Probe` (DD5) by ingesting an empty
batch under a reserved probe tenant into each store after open; a `pillar_root`
that opens but is not writable (read-only mount, full disk, overlayfs no-op
`fsync`) fails the probe and the host binary refuses to start with
`event=health.startup.refused`. Enforcement reuses ADR-0007's three orthogonal
layers (subtype at the composition root; structural via the extended `xtask` AST
walk; behavioural via a gold-test against a read-only `pillar_root`).

## Alternatives Considered

### Decision 1 alternatives

- **Lossy best-effort translation (drop bad fields, keep the record).** Rejected:
  silently dropping a wrong-length trace id would mis-relate spans and break
  KPI-5's "field-faithful" promise. Refusing and naming the field is honest.
- **Per-record partial persistence (ingest the good records, skip the bad).**
  Rejected: partial writes make "accepted => fully queryable" untrue and create
  half-persisted batches that are impossible to reason about after a restart.

### Decision 2 alternatives

- **`x-tenant-id` header / metadata instead of a resource attribute.** Rejected:
  the sink receives a typed `SinkRecord`, not transport metadata; aperture has
  already consumed the wire envelope. A resource attribute is the only
  tenant-carrying surface available at the port, and `tenant.id` matches aegis'
  and OTel's namespacing.
- **Fail-closed only (no `default_tenant`).** Rejected: a single-tenant operator
  (the common v0 case, e.g. Priya's "acme") would have to inject `tenant.id` into
  every exporter. `default_tenant` is the pragmatic single-tenant path; fail-closed
  remains the behaviour when neither source resolves.

### Decision 3 alternatives

- **Refuse the whole record on any unsupported type (the DISCUSS AC).** Rejected:
  a real exporter mixes gauges/sums with histograms; refusing the batch would
  reject the supported points too and break liveness. Not collector-faithful.
- **Force histograms into sums/gauges (e.g. store count or sum as a number
  point).** Rejected: silently misrepresents the data type; a histogram is not a
  gauge. Forcing would corrupt query semantics. Skip-with-event is honest loss.

## Consequences

### Positive
- The translation contract is explicit and testable; round-trip KPIs (KPI-1/2/3)
  assert field equality post-restart.
- Tenant isolation is never violated by mis-filing; refusal is observable.
- Histogram-heavy exporters keep working (their gauge/sum points persist) while
  the gap is visible via skip events, guiding the pulse v1 histogram work.
- aperture and the pillars are untouched; the new logic is isolated in one crate.

### Negative
- `AnyValue`-to-`String` and `i64`-to-`f64` are lossy at the pillar boundary
  (the pillars are string/`f64`-valued at v0). Documented as the v0 contract, not
  a defect; v1 may richen the pillar types.
- The DISCUSS AC for US-03 is superseded; the acceptance test wording must change
  (skip + empty persistence, not refusal). Flagged to acceptance-designer.
- Histogram/Summary data is dropped at v0. Acceptable: pulse v0 cannot store it;
  the skip event records the loss.

### Trade-off ATAM
- **Sensitivity point** for Functional Suitability (translation fidelity) and
  Reliability (atomic, no partial writes).
- **Trade-off point** for Functional Suitability vs Reliability in Decision 3:
  skip favours collector-faithful liveness over strict reject-on-unknown; biased
  to liveness at v0 with observability as the mitigation.
