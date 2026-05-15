# Sluice v1 — outcome KPIs

## KPI 1 — Enqueue latency

- **What**: `enqueue` p95 ≤ 300 µs on
  `FileBackedQueue`.
- **Why**: enqueue sits on every producer's hot path.
  WAL flush adds per-op cost that the v0 in-memory
  adapter doesn't pay.
- **Why 300 µs and not the v0 50 µs**: the WAL needs to
  flush before `enqueue` returns; that's a real syscall.
  The v0 50 µs was the in-memory-only ceiling. v1's
  300 µs is "WAL durability buys ~6× the per-op cost,
  which is the right price for a queue that survives
  restart". v2's binary format will tighten this.
- **Measured by**: `sluice::tests::v1_slice_01_wal_durability::
  enqueue_p95_latency_under_three_hundred_microseconds`.
  Open a fresh queue in a tempdir, warm up with 100
  enqueues, time 1 000 enqueues, read off p95.
- **Target**: 300 µs p95 over 1 000 trials.

## KPI 2 — Recovery time

- **What**: `open(path)` p95 ≤ 500 ms when recovering
  10 000 enqueued messages from snapshot + WAL in debug
  build.
- **Why**: recovery sits on the queue-process startup
  path. The 10 000-message corpus matches a realistic
  durable-queue load.
- **Why 500 ms not 50 ms** (initial guess pattern,
  same posture as Cinder v1): NDJSON parsing of 10 000
  entries in debug mode dominates. v2's binary format
  will collapse this. The KPI describes what v1 ships.
- **Measured by**: `sluice::tests::v1_slice_02_snapshot::
  recovery_p95_latency_under_five_hundred_milliseconds`.
  Enqueue 10 000 messages, snapshot, enqueue 100 more,
  drop. Time 20 reopens, read off p95.
- **Target**: 500 ms p95 over 20 trials in debug build.

## Out-of-scope (deliberate)

- **fsync semantics**. v1 uses `BufWriter::flush`. v2.
- **Atomic rename for snapshot**. v2.
- **File locking**. v2.
- **Binary WAL format**. v2.
- **Compaction inside `enqueue`**. v1's `snapshot()` is
  an explicit call.
- **Kafka / NATS / Redpanda adapters**. Still v2 (these
  always lived behind the same `Queue` trait; Sluice v0
  promised them and Sluice v1 keeps the door open
  without implementing them).
