# Slice 01 — Walking skeleton (US-SL-01)

## Goal

`Queue` trait + `InMemoryQueue` adapter supporting enqueue / dequeue
/ ack / nack with per-tenant FIFO ordering and bounded capacity.

## IN scope

- Public types: `MessageId(u64)`, `Message`, `EnqueueError`
- `Queue` trait with the four methods
- `InMemoryQueue` adapter: HashMap<TenantId, VecDeque<Message>>
- Per-tenant cap configurable at construction
- Acceptance test exercising every method + KPI 1 latency

## OUT scope

- Depth observability (slice 02)
- Kafka/NATS/Redpanda adapters (v1)
- Durability (v1)
- Sieve integration (v1)

## Learning hypothesis

Disproves "the queue port trait shape is workable for both
in-memory and Kafka/Redpanda adapters". Risk: at-least-once
semantics around ack/nack may not translate cleanly to all
backends. Low risk at v0; only one adapter.
