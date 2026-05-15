# Sluice v1 — DISCUSS wave decisions

## Key decisions

- **[D1] Second v1 anywhere in the platform plane.**
  Cinder v1 was the first; one is a one-off, two is a
  pattern. Sluice v1 validates that the v0→v1 carry-
  forward is repeatable across independent crates.

- **[D2] Same crate, new adapter**. `FileBackedQueue`
  joins `InMemoryQueue` behind the same `Queue` trait.
  v0 acceptance tests stay green.

- **[D3] NDJSON WAL format**, same as Cinder v1. One
  operation per line: `Enqueue`, `Dequeue`, `Ack`,
  `Nack`. Recovery applies in WAL order.

- **[D4] Hex-encoded payloads**. `Vec<u8>` payloads
  need string-safe encoding for JSON. Hand-rolled hex
  is chosen over base64 to avoid a new dependency at
  v1. The 100% size overhead is acceptable; v2's binary
  WAL format collapses this.

- **[D5] `EnqueueError` extends additively**.
  `PersistenceFailed { reason: String }` is added,
  mirroring Cinder v1's `MigrateError::PersistenceFailed`.
  Same rationale, same v0 caller cost (one wildcard arm
  if previously matched exhaustively).

- **[D6] Operation log, not state diff**. Same posture
  as Cinder v1. Recovery replays operations.

- **[D7] Nack-to-head invariant preserved by replay
  order**. The v0 nack semantics says a nacked message
  returns to the head of its tenant's queue.
  `VecDeque::push_front` on nack vs `VecDeque::push_back`
  on enqueue handles this correctly during replay
  because operations are applied in WAL order.

- **[D8] `MessageId` counter resumes from
  `max(id_in_wal) + 1`**. Replay scans for the highest
  id and resumes the monotonic counter above it. New
  enqueues never collide with replayed ids.

- **[D9] `BufWriter::flush` semantics**, same as Cinder
  v1. v2 adds fsync.

- **[D10] Explicit `snapshot()` call** for compaction.
  Operator binary owns the cadence.

- **[D11] No file locking at v1**. One process per WAL
  path.

- **[D12] AGPL-3.0-or-later**.

- **[D13] Two carpaccio slices in one implementation
  commit** per established precedent.

## Slicing

- **Slice 01 — WAL durability** (US-SLV1-01).
  `FileBackedQueue::open` + `enqueue` + `dequeue` +
  `ack` + `nack` + replay-on-restart.
- **Slice 02 — snapshot compaction** (US-SLV1-02).
  `snapshot()` + truncate-WAL.

## Constraints established

- v0 `InMemoryQueue` acceptance tests continue to pass.
- The `Queue` trait shape does not change. Only the
  `EnqueueError` enum extends additively.
- Sluice depends on `aegis`, `serde`, `serde_json`. No
  new third-party deps beyond workspace deps.

## DESIGN handoff

DESIGN collapses into the implementation commit.
