# Sluice v1 — user stories

Two LeanUX user stories with mandatory Elevator Pitches.
This is the **second v1 feature anywhere in the platform
plane**, after Cinder v1. The point of repeating the
pattern on a different crate is to validate that the
v0→v1 carry-forward is not a Cinder-specific accident —
it is the way Kaleidoscope ships durable adapters behind
v0 traits.

The principal user is **Sasha, a platform engineer** who
saw Cinder v1 ship and now wants the queue to survive
restart as well. Sluice v0's "at-least-once delivery" was
true only across in-process restarts; a real process
crash lost every pending message. v1 closes that gap with
a WAL.

The secondary user is **Riley, an SRE** running a
recovery drill. Riley wants to assert "I kill the queue
process at any point and pending plus in-flight messages
recover correctly on restart". v1 ships that property.

System constraints (apply to every story):

1. Same crate (`sluice`). v1 ships a new adapter
   alongside `InMemoryQueue`, not a new crate.
2. **Same v0 trait**. `FileBackedQueue` implements the
   existing `Queue` trait verbatim. The v0 acceptance
   tests stay green.
3. **NDJSON WAL**. One operation per line:
   `Enqueue`, `Dequeue`, `Ack`, `Nack`. Same shape as
   Cinder v1 by design.
4. **Hex-encoded payloads**. `Vec<u8>` payloads need
   string-safe encoding for JSON. Hex is chosen over
   base64 to avoid a new dependency; the 100% size
   overhead is acceptable at v1 (binary payload format
   is a v2 concern).
5. **Trait error enum extension**. `EnqueueError` gains
   `PersistenceFailed { reason: String }`, mirroring
   Cinder v1's `MigrateError::PersistenceFailed`. Same
   additive change, same one-line cost for exhaustive
   callers.
6. **Recovery preserves nack-to-head ordering**. v0
   semantics say a nacked message goes back to the head
   of its tenant's queue. Replay applies operations in
   WAL order; the resulting in-memory state preserves
   nack ordering by construction.
7. **`BufWriter::flush` semantics**. Same as Cinder v1.
   Graceful drop flushes; `kill -9` may lose the last
   record. v2 adds fsync.
8. **No file locking at v1**. One process per WAL path.
9. **`MessageId` counter recovery**. The monotonic
   counter resumes from `max(id_in_wal) + 1` so new
   enqueues never collide with replayed ones.
10. **AGPL-3.0-or-later**.

---

## US-SLV1-01 — WAL durability: enqueue + dequeue + ack + nack survive restart

### Elevator Pitch

- **Before**: Sluice v0 holds every pending and
  in-flight message in memory. A process crash loses
  every queued message.
- **After**: run `cargo test -p sluice --test v1_slice_01_wal_durability`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test enqueues several messages across
  tenants, dequeues some, acks one, nacks one, drops
  the queue, opens a new queue at the same path, and
  asserts the recovered state matches the state before
  drop.
- **Decision enabled**: Sasha removes the "v0 is
  in-memory only, restart loses data" qualifier from
  Sluice's story. Two v0 traits have now been validated
  to carry forward to durable adapters.

### Acceptance criteria

- AC-1.1 — `FileBackedQueue::open(path, cap, recorder)`
  opens or creates the WAL at `path` and replays
  existing records into in-memory state.
- AC-1.2 — `enqueue(tenant, payload)` appends an
  `Enqueue` record to the WAL and updates in-memory
  state. Returns the assigned `MessageId`.
- AC-1.3 — `dequeue(tenant)` appends a `Dequeue`
  record, moves the message from pending to in-flight,
  and returns the message.
- AC-1.4 — `ack(id)` appends an `Ack` record and
  removes the message from in-flight.
- AC-1.5 — `nack(id)` appends a `Nack` record and
  returns the message to the head of its tenant's
  queue.
- AC-1.6 — A fresh `FileBackedQueue::open` on the same
  path after `drop` recovers every prior message:
  pending and in-flight populations are identical, and
  FIFO ordering within each tenant is preserved.
- AC-1.7 — Recovery preserves the v0 nack-to-head
  invariant.
- AC-1.8 — `MessageId` counter resumes from
  `max(id) + 1` so new enqueues never collide.
- AC-1.9 — Enqueue beyond capacity returns
  `EnqueueError::Full` without writing to the WAL.
- AC-1.10 — I/O failures surface as
  `EnqueueError::PersistenceFailed { reason: String }`.
- AC-1.11 — Tenant isolation preserved across restart.

### KPI anchor

- KPI 1 (Enqueue latency): `enqueue` p95 ≤ 300 µs on
  the file-backed adapter. Higher than the v0 50 µs
  ceiling because WAL flush is on the hot path; lower
  than Cinder v1's 200 µs is not realistic.

---

## US-SLV1-02 — Snapshot compaction for bounded recovery time

### Elevator Pitch

- **Before**: the WAL grows unbounded as messages flow
  through the queue. Recovery time grows linearly. Riley
  wants bounded recovery for the queue, the same way
  Cinder v1 bounds it for tier metadata.
- **After**: run `cargo test -p sluice --test v1_slice_02_snapshot`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test enqueues many messages, calls
  `snapshot()`, enqueues more, restarts, and asserts the
  recovered state is complete and the WAL on disk has
  been truncated.
- **Decision enabled**: Sasha sets a snapshot cadence in
  the operator binary.

### Acceptance criteria

- AC-2.1 — `snapshot()` writes the current in-memory
  state (pending queues + in-flight ledger + next_id) to
  a snapshot file then truncates the WAL.
- AC-2.2 — `open(path)` after a snapshot loads the
  snapshot first then replays only the remaining WAL
  records.
- AC-2.3 — Recovery from snapshot + partial WAL produces
  the same in-memory state as recovery from full WAL.
- AC-2.4 — Snapshot is idempotent: a second consecutive
  snapshot call succeeds and leaves a valid snapshot.
- AC-2.5 — Pending and in-flight populations survive
  snapshot+restart correctly.

### KPI anchor

- KPI 2 (Recovery time): `open` p95 ≤ 500 ms when
  recovering 10 000 enqueued messages from snapshot +
  WAL in debug build. Same calibration philosophy as
  Cinder v1's KPI 2: NDJSON parsing dominates in debug,
  v2's binary format will collapse this.
