# Pulse v1 — DISTILL wave decisions

Feature: `pulse-v1` (disk-backed `FileBackedMetricStore`: WAL durability + snapshot compaction).
Wave: DISTILL. Author: Quinn (acceptance-test-designer). Date: 2026-05-21.

These decisions record the acceptance-test design for the v1 disk-backed
adapter. Tests enter exclusively through the `MetricStore` driving port and
the `FileBackedMetricStore::open` / `snapshot` constructors. Production code
is the crafter's job in DELIVER.

## DWD-01 — `FileBackedMetricStore` is the new driving surface; compile failure is the RED gate

Both new test files import `pulse::FileBackedMetricStore`, which does not
exist in `src/` yet. The crate therefore fails to compile until DELIVER adds
the struct. That compile failure is the deliberate RED gate for this wave:
no test can pass, and none is meant to, before the adapter exists. The store
is exercised only through the `MetricStore` trait (`ingest`, `query`,
`query_with`) plus the `open` and `snapshot` lifecycle methods — never via
internal WAL or snapshot file handling. This keeps the hexagonal boundary
intact (CM-A).

## DWD-02 — byte-for-byte mirror of the Lumen v1 templates, adapted logs to metrics

`v1_slice_01_wal_durability.rs` and `v1_slice_02_snapshot.rs` mirror the
Lumen v1 slice tests structurally: same `temp_base` / `cleanup` /
`wal_size_bytes` / `snapshot_exists` helpers, same per-test layout, same
`.wal` / `.snapshot` sidecar-file conventions. The domain swaps from
`LogRecord` to `Metric` + `MetricPoint`. The one structural divergence is
forced by the Pulse trait: Lumen's `query(tenant, range)` becomes Pulse's
`query(tenant, metric_name, range)`, and results are `Vec<(Metric,
MetricPoint)>` tuples rather than bare records. Assertions therefore reach
through `out[i].1.time_unix_nano` / `.value` for points and `out[i].0` for
metric metadata. Recovery ordering is asserted on `time_unix_nano` ascending,
mirroring the v0 walking-skeleton idiom.

## DWD-03 — KPI budgets mirror the post-bump Lumen/Pulse v0 numbers, not the original ceilings

Slice 01 carries KPI 1 (`ingest_p95_latency_under_two_milliseconds`,
budget 2 ms = 2000 µs over 1000 timed 100-point ingests after 50 warm-up
ingests). Slice 02 carries KPI 2
(`recovery_p95_latency_under_two_and_a_half_seconds`, budget 2.5 s over 20
reopens of a 10 000-point store with a post-snapshot tail batch). Both budgets
match the 2026-05-19 CI-realism bump batch (Lumen v0/v1 KPI 1, Cinder KPI 2):
local baselines sit far under budget; the ceilings carry the GitHub Actions
ubuntu-latest margin. The CI-realism rationale is preserved verbatim-in-spirit
in each test's module comment. KPI 1 uses 2 ms (matching Pulse v0, since the
in-memory baseline differs from Lumen's log-record cost); KPI 2 uses 2.5 s.

## DWD-04 — error/edge coverage and the empty-batch no-op

Slice 01 includes the empty-batch no-op (`empty_batch_ingest_writes_nothing
_to_wal`) asserting the WAL sidecar stays zero-length — the durability
analogue of v0's "empty range returns Ok empty". Slice 02 covers the snapshot
failure-and-recovery edges: snapshot-then-replay composition, snapshot+WAL
equivalence to pure-WAL recovery, and snapshot idempotence under no
intervening writes. These are the structural edge cases the disk adapter must
honour; richer corruption paths (the Lumen `PersistenceFailed` case) are out
of scope here because Pulse's `MetricStoreError` is still an empty enum at v0
and the brief scopes these two slices only. DELIVER may widen the error enum;
if so a corruption test should be added then.

## DWD-05 — hard constraints honoured; no production or build changes

No Pulse v0 tests touched (`slice_01_walking_skeleton.rs`,
`slice_02_structured_query.rs` unchanged). No edits to `src/`, `Cargo.toml`,
or `ci.yml`. `cargo` was not run. Only the two new `tests/v1_slice_*.rs`
files and this decisions note were written. British English throughout, no
em dashes. Handoff to DELIVER: the crafter adds `FileBackedMetricStore` with
`open`, `snapshot`, and the `MetricStore` impl, driving the nine tests from
RED to GREEN one at a time.
