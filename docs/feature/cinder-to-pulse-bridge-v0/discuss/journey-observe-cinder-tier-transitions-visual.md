# Journey: Operator observes Cinder tier transitions

> Feature: `cinder-to-pulse-bridge-v0`
> Persona: **Priya the platform operator** — she runs the multi-tenant
> Kaleidoscope deployment for a fintech and is responsible for keeping the
> tiering invariants honest. She already queries Pulse for `lumen.ingest.count`
> and `lumen.query.count` per tenant.
> Job: "I want to see Cinder's tier movements with the same query API I
> already use for Lumen, so I can answer tier-policy questions without
> learning a second toolchain."

## Why this journey exists (the void Priya sees today)

Cinder ships with `NoopRecorder` as its default `MetricsRecorder`. Every
`place`, `migrate`, and `evaluate_at` call fires events into a function that
swallows them. Priya can run Cinder in production for a month and have **zero
evidence** of:

- How many items per tenant currently live in Hot vs Warm vs Cold
- How often `acme` migrates Hot to Warm in a given hour
- Whether `globex`'s evaluate runs are actually moving items or noop-ing

Today Priya's only recourse is to add `println!` to Cinder source and rebuild,
or to write a one-off CapturingRecorder and dump it to disk on shutdown. Both
are operationally hostile.

## The emotional arc

Three states, mirroring the Lumen bridge arc almost line-for-line:

```
   anxious           focused            confident
      |                 |                   |
  "I can't see       "Wire the bridge,    "Same query API
   what Cinder        run the workload,    I already use.
   is doing."         metrics start         I can answer
                      flowing."             my own questions."
```

- **Entry**: anxious. Priya knows Cinder is doing something but has no eyes
  on it. The `NoopRecorder` default is an operational lie of omission.
- **Middle**: focused. She wires `CinderToPulseRecorder::new(pulse_store)`
  into the Cinder constructor. The wiring is one line. She runs a workload
  that places some items, migrates a few, and calls `evaluate_at`.
- **Exit**: confident. She queries `pulse_store.query(&tenant,
  &MetricName::new("cinder.migrate.count"), TimeRange::all())` and sees her
  expected points, with `from`/`to` attributes. The mental model from Lumen
  transfers without translation.

Cross-feature invariant inherited from `incident-response.yaml`:
**no-stale-data-on-error** still applies — best-effort emission must not
mask Cinder's actual behaviour by pretending success when Pulse refused
ingest. The bridge's `let _ = pulse.ingest(...)` is acceptable today only
because `MetricStoreError` is uninhabitable at v0. The risk register
(see `wave-decisions.md` D5) records that this is a forward-compatibility
deferral.

## Journey flow (ASCII)

```
+-----------------------------------------------------------------+
| Step 1: Wire the bridge once at construction                    |
|                                                                 |
|   let pulse: Arc<dyn MetricStore + Send + Sync> = ...;          |
|   let bridge = CinderToPulseRecorder::new(pulse.clone());       |
|   let cinder = InMemoryTieringStore::new(Box::new(bridge));     |
|                                                                 |
|   Emotion: focused. One line, type-checked, no surprises.       |
+-----------------------------------------------------------------+
                                |
                                v
+-----------------------------------------------------------------+
| Step 2: Cinder runs its normal API                              |
|                                                                 |
|   cinder.place(&acme, &item("trade-2026-05-18"), Tier::Hot, t); |
|   cinder.migrate(&acme, &item("trade-2026-05-18"),              |
|                  Tier::Warm, t+24h)?;                           |
|   cinder.evaluate_at(t+30d, &policy);                           |
|                                                                 |
|   Emotion: focused -> confident. Cinder calls feel unchanged    |
|   from her existing usage; the bridge is invisible at the call  |
|   site.                                                         |
+-----------------------------------------------------------------+
                                |
                                v
+-----------------------------------------------------------------+
| Step 3: Priya queries Pulse with her existing idiom             |
|                                                                 |
|   let migrate_points = pulse.query(                             |
|     &acme,                                                      |
|     &MetricName::new("cinder.migrate.count"),                   |
|     TimeRange::all(),                                           |
|   )?;                                                           |
|   // -> Vec<(Metric, MetricPoint)> with attrs from/to per point |
|                                                                 |
|   Emotion: confident. Zero new query semantics. The bridge      |
|   "just" turned Cinder's events into the same shape Lumen       |
|   already produces.                                             |
+-----------------------------------------------------------------+
```

## Per-step detail (with mockup output)

### Step 1 — Wire the bridge

```rust
use std::sync::Arc;
use cinder::InMemoryTieringStore;
use pulse::{InMemoryMetricStore, MetricStore, NoopRecorder as PulseNoop};
use self_observe::CinderToPulseRecorder;

let pulse: Arc<dyn MetricStore + Send + Sync> =
    Arc::new(InMemoryMetricStore::new(Box::new(PulseNoop)));
let bridge = CinderToPulseRecorder::new(pulse.clone());
let cinder = InMemoryTieringStore::new(Box::new(bridge));
```

Shared artefacts at this step:
- `pulse` — the `MetricStore` instance. Owned by the operator binary at
  runtime; reused as both the recorder sink (via `bridge`) AND the query
  surface in Step 3.
- `bridge` — moved into Cinder. The operator does not retain a handle.

### Step 2 — Workload runs against Cinder

No new code from Priya's perspective. Cinder's existing `place`,
`migrate`, `evaluate_at` API is unchanged. Internally each call fans out
to the `MetricsRecorder` trait, which the bridge implements.

Inside the bridge (sketch, the DESIGN wave owns the final shape):

```rust
impl cinder::MetricsRecorder for CinderToPulseRecorder {
    fn record_place(&self, tenant: &TenantId, tier: Tier) {
        // emit cinder.place.count with attr tier=<lowercase tier>, value=1
    }
    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier) {
        // emit cinder.migrate.count with attrs from=..., to=..., value=1
    }
    fn record_evaluate(&self, tenant: &TenantId, migrated: usize) {
        // emit cinder.evaluate.migrated.count, value=migrated as f64
    }
}
```

### Step 3 — Priya queries Pulse

What she sees when she queries `cinder.migrate.count` for tenant `acme`
after a workload that migrated three items (Hot->Warm twice, Warm->Cold
once):

```
+-- Query result: cinder.migrate.count for acme -------------------+
|                                                                  |
|   Metric { name: "cinder.migrate.count", kind: Sum, unit: "1" }  |
|                                                                  |
|   Point[0]  time=<t1>   value=1.0   attrs { from=hot,  to=warm } |
|   Point[1]  time=<t2>   value=1.0   attrs { from=hot,  to=warm } |
|   Point[2]  time=<t3>   value=1.0   attrs { from=warm, to=cold } |
|                                                                  |
+------------------------------------------------------------------+
```

Cross-checked against `cinder.evaluate.migrated.count` for `acme` after an
`evaluate_at` that migrated 5 items in one call:

```
+-- Query result: cinder.evaluate.migrated.count for acme ---------+
|                                                                  |
|   Metric { name: "cinder.evaluate.migrated.count",               |
|            kind: Sum, unit: "1" }                                |
|                                                                  |
|   Point[0]  time=<t_eval>   value=5.0   attrs {}                 |
|                                                                  |
+------------------------------------------------------------------+
```

She also sees 5 corresponding points on `cinder.migrate.count` (one per
item migrated during the evaluate), since `InMemoryTieringStore::evaluate_at`
emits both signals. The journey makes this explicit so it does not look
like a double-counting bug.

## Failure modes acknowledged

| What could go wrong | What the bridge does today | DISTILL test |
|--------------------|----------------------------|--------------|
| Pulse `ingest` returns `Err` | Bridge ignores it. `MetricStoreError` empty at v0 so unreachable in practice. | None at v0 (uninhabitable). Slice 03 docs the deferral. |
| Cinder is constructed with `CinderToPulseRecorder` but never used | No points emitted. Pulse query returns empty Vec. | `no_cinder_event_means_no_pulse_metric_point` (Slice 01) |
| Tenant isolation leak: `acme`'s migration shows under `globex` query | Bridge MUST pass `tenant` through unchanged. | `two_tenants_cinder_events_land_in_isolated_pulse_buckets` (Slice 02) |
| Concurrent Cinder calls from multiple threads | `MetricsRecorder` is `Send + Sync`; `Arc<dyn MetricStore + Send + Sync>` is shareable. | `the_bridge_is_send_and_sync` compile-time check (all slices) |
| `evaluate_at` with 0 migrations for a tenant | Cinder does NOT call `record_evaluate` for tenants with 0 migrations (see store.rs:228 — only tenants in `per_tenant` map). | `evaluate_with_no_eligible_items_emits_no_evaluate_point` (Slice 03) |

## Integration checkpoints

After Slice 01: `cinder.place.count` points queryable per tenant with `tier`
attribute. Verifies the emission path end-to-end with a single event type.

After Slice 02: `cinder.migrate.count` points queryable per tenant with
`from`/`to` attributes. Verifies multi-attribute emission and tenant isolation
across both event types so far.

After Slice 03: `cinder.evaluate.migrated.count` points queryable per
tenant with `value=migrated_count`. Verifies that `evaluate_at`'s double
emission (per-item migrate + per-tenant evaluate) lands correctly and that
the zero-migration tenant case produces no evaluate point.

After Slice 03 the journey is complete: every Cinder event type has an
exact-mapping Pulse query, and the operator's mental model from Lumen
transfers without surprise.
