# Sluice v0 — user stories

Two LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`.

The principal user is **Sasha, a platform engineer** who wants
Sieve's filtered OTLP batches to survive a brief downstream
unavailability without losing data. Sasha's job is to interpose a
queue port between Sieve (the filter) and whatever consumer is
downstream — at v0 a test sink; at v1 a real storage engine.

The secondary user is **Riley, an SRE** investigating a "logs went
missing during a Prometheus blip" incident. Riley needs to verify
that Sluice's in-memory queue absorbed the gap and replayed cleanly
when the consumer recovered.

System constraints (apply to every story):

1. Library at v0. Sluice ships as a Rust crate (`sluice`) exposing
   the queue port trait and an in-memory adapter. The Kafka /
   NATS / Redpanda adapters live in `crates/sluice-*` at v1; v0
   ships the trait + one in-memory adapter.
2. AGPL-3.0-or-later. Same licensing posture as every platform
   component.
3. **OTLP-shaped enqueue at v0.** The queue's payload type is
   `Vec<u8>` (encoded OTLP). Sieve's job is to fill the bytes;
   Sluice is byte-agnostic. The OTLP-decode-and-route concern
   lives in storage-engine consumers (v1+).
4. **At-least-once delivery semantics.** A consumer that processes
   a message and then crashes before acknowledging will see the
   message again on restart. Sluice prefers duplicates over loss.
5. **Per-tenant queues.** A single Sluice instance hosts many
   per-tenant queues keyed by `TenantId` (re-exported from Aegis).
   No cross-tenant data flow at v0.
6. **Bounded queue per tenant.** Each tenant's queue caps at a
   configurable size; enqueue beyond cap is operator-visible as
   `EnqueueError::Full`. Sluice prefers backpressure over silent
   data loss.
7. **In-memory only at v0.** No persistence; a process restart
   loses queued messages. This is acceptable because Sluice v0 is
   a *port* with one adapter; durable adapters (Kafka, Redpanda)
   land at v1.
8. **No telemetry-on-telemetry.** Sluice itself emits OTLP
   telemetry to the operator's Aperture; depth gauges and enqueue
   counts via metric instruments.
9. **AGPL-3.0-or-later.** Same posture as every platform component.

---

## US-SL-01 — Walking skeleton: enqueue + dequeue per tenant

### Elevator Pitch

- **Before**: Sieve emits filtered OTLP batches directly to a
  forwarder. If the forwarder is briefly unavailable (Aperture
  restart, network blip), the batches drop on the floor.
- **After**: Sieve enqueues each filtered batch into a Sluice
  queue keyed by `TenantId`. The downstream consumer (a test
  sink at v0, a storage engine at v1) dequeues at its own pace.
  A 5-second forwarder blip is absorbed; the consumer drains the
  backlog on recovery.
- **Decision enabled**: Sasha removes the direct-coupling between
  Sieve and the forwarder; downstream evolution can proceed
  independently of Sieve's release cycle.

### Acceptance criteria

- AC-1.1 — `Queue::enqueue(tenant, payload)` returns
  `Ok(MessageId)` on success.
- AC-1.2 — `Queue::dequeue(tenant)` returns the next pending
  message for that tenant as `Some(Message)` or `None` if empty.
- AC-1.3 — Messages are FIFO within a tenant.
- AC-1.4 — Two tenants' messages are isolated — dequeue on tenant
  A never returns tenant B's messages.
- AC-1.5 — `Queue::ack(message_id)` removes a dequeued message
  permanently. Until acked, the message can be re-delivered.
- AC-1.6 — `Queue::nack(message_id)` returns the message to its
  tenant's queue (at-least-once on consumer crash).
- AC-1.7 — Enqueue beyond the per-tenant cap returns
  `Err(EnqueueError::Full { tenant })`.

### KPI anchor

- KPI 1 (Enqueue + dequeue latency): p95 ≤ 50 µs per operation
  on an in-memory queue. Sluice sits on the hot path; cannot be
  a bottleneck.

---

## US-SL-02 — Depth observability

### Elevator Pitch

- **Before**: an operator cannot tell whether Sluice is empty,
  building up a backlog, or about to hit its cap. The first
  symptom of saturation is `EnqueueError::Full` on the producer
  side — too late to react.
- **After**: `Queue::depth(tenant)` returns the current count for
  that tenant; `Queue::total_depth()` returns the sum across all
  tenants. Both are O(1). Sluice also emits OTLP gauge metrics
  named `sluice.queue.depth{tenant=...}` and
  `sluice.queue.enqueued_total{tenant=...,result=...}` for
  Prometheus scraping.
- **Decision enabled**: Riley sets a Prometheus alert at 80% of
  the cap; the team replaces the static cap with a tunable based
  on real backlog patterns.

### Acceptance criteria

- AC-2.1 — `Queue::depth(tenant) -> usize` returns the current
  pending-count for that tenant.
- AC-2.2 — `Queue::total_depth() -> usize` returns the sum across
  every tenant.
- AC-2.3 — Depth is consistent with enqueue/ack/nack semantics:
  enqueue +1, ack -1, nack 0 (the message returns to depth).
- AC-2.4 — A `MetricsRecorder` trait abstracts the emission of
  the depth gauge and enqueue counters. v0 ships a no-op recorder
  and a test-capturing recorder; the OTLP-binding recorder lives
  in `beacon-server`-style wiring at v1.

### KPI anchor

- KPI 2 (Depth lookup cost): O(1) per call (verified by
  `criterion`-style benchmark or by-construction trait shape).
