# DISCUSS Decisions — sluice-v0

## Key decisions

- **[D1] Library + trait, one adapter at v0.** Sluice ships the
  `Queue` trait + an `InMemoryQueue` adapter. Kafka / NATS /
  Redpanda adapters are separate crates at v1, behind the same
  trait. (See: user-stories.md system constraint 1.)
- **[D2] Payload is `Vec<u8>`.** OTLP-encode is upstream (Sieve);
  OTLP-decode is downstream (storage engine). Sluice is byte-
  agnostic. (See: system constraint 3.)
- **[D3] At-least-once delivery.** Crash-after-dequeue-before-ack
  redelivers. Idempotency is the consumer's responsibility — the
  storage engine deduplicates on its own keys. (See: system
  constraint 4.)
- **[D4] Per-tenant queues, bounded.** TenantId from `aegis::TenantId`.
  Enqueue beyond cap returns `EnqueueError::Full` for the
  operator to react. (See: system constraints 5, 6.)
- **[D5] In-memory only at v0.** No persistence; restart loses
  queued messages. The durable adapters (Kafka, Redpanda) ship
  at v1 behind the same trait. (See: system constraint 7.)
- **[D6] Metrics via injected recorder.** `MetricsRecorder` trait
  abstracts depth-gauge and enqueue-counter emission; v0 ships
  no-op + test-capturing implementations. The OTLP recorder
  lives in a v1 binary wrapper. (See: US-SL-02 AC-2.4.)
- **[D7] Two stories at v0.** Walking skeleton + observability.
  Retrofitting Sluice into Sieve is its own slice in v1.
- **[D8] AGPL-3.0-or-later.**
- **[D9] No telemetry-on-telemetry.** Sluice emits its own depth
  metrics into the operator's Aperture per architecture doc §A.2.
  No third-party endpoints.
- **[D10] No retrofit into Sieve at v0.** Sieve keeps its current
  direct-forwarding path; the Sieve-Sluice integration is a v1
  slice once a durable adapter (Kafka) is available.

## Requirements summary

- Primary user need: a byte-agnostic, per-tenant, bounded queue
  port that Sieve can use to decouple from downstream consumers.
- Walking skeleton scope: enqueue + dequeue + ack + nack on a
  per-tenant in-memory queue.
- Feature type: backend (library, no UI).

## Constraints established

- Sluice v0 cannot depend on Sieve or any storage engine.
- The queue port trait is the contract; future adapters
  (Kafka, NATS, Redpanda) implement it.
- In-memory adapter is sufficient for v0 because Sluice is the
  abstraction; durability arrives with the adapter swap.

## Upstream changes

None. Architecture doc names Sluice's role; ADR-0035 (Aegis
TenantId) carries forward as Sluice's keying type.
