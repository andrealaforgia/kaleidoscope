# Ray v1 — DISTILL wave decisions

Feature: `ray-v1` (disk-backed `FileBackedTraceStore`: WAL durability + snapshot compaction).
Wave: DISTILL. Author: Quinn (acceptance-test-designer). Date: 2026-05-21.

These decisions record the acceptance-test design for the v1 disk-backed
trace adapter. Tests enter exclusively through the `TraceStore` driving port
and the `FileBackedTraceStore::open` / `snapshot` lifecycle methods.
Production code is the crafter's job in DELIVER.

## DWD-01 — `FileBackedTraceStore` is the new driving surface; compile failure is the RED gate

Both new test files import `ray::FileBackedTraceStore`, which does not exist
in `src/` yet. The crate therefore fails to compile until DELIVER adds the
struct. That compile failure is the deliberate RED gate for this wave: no
test can pass, and none is meant to, before the adapter exists. The store is
exercised only through the `TraceStore` trait (`ingest`, `get_trace`,
`query`) plus `open` and `snapshot` — never via internal WAL or snapshot file
handling. This keeps the hexagonal boundary intact (CM-A).

## DWD-02 — byte-for-byte mirror of the Pulse v1 templates, adapted metrics to traces

`v1_slice_01_wal_durability.rs` and `v1_slice_02_snapshot.rs` mirror the Pulse
v1 slice tests structurally: same `temp_base` / `cleanup` / `wal_size_bytes` /
`snapshot_exists` helpers, same per-test layout, same `.wal` / `.snapshot`
sidecar-file conventions. The domain swaps from `Metric` + `MetricPoint` to
`Span` + `SpanBatch`. The forced divergences come from the trait shape: Pulse's
`query(tenant, metric_name, range)` returning `Vec<(Metric, MetricPoint)>`
splits into Ray's TWO recovery-query surfaces — `get_trace(tenant, trace_id)`
returning `Vec<Span>` and `query(tenant, service, range)` returning `Vec<Span>`.
Spans are built with the v0 walking-skeleton `span(...)` helper (per-byte
`TraceId`/`SpanId`, `service.name` in resource attributes). Recovery ordering
is asserted on `start_time_unix_nano` ascending.

## DWD-03 — KPI budgets mirror the post-bump Ray v0 / Pulse v1 numbers

Slice 01 carries KPI 1 (`ingest_p95_latency_under_two_milliseconds`, budget
2 ms = 2000 µs over 1000 timed 100-span ingests after 50 warm-up ingests;
batches span 10 traces and 4 services, matching the v0 KPI 1 shape). Slice 02
carries KPI 2 (`recovery_p95_latency_under_two_and_a_half_seconds`, budget
2.5 s over 20 reopens of a 10 000-span store with a post-snapshot tail batch).
Both budgets match the 2026-05-19 CI-realism bump batch. Ray's 2 ms ingest
ceiling reflects the dual `by_trace` + `by_service` sort-after-extend plus the
new disk costs; the 2.5 s recovery ceiling additionally covers rebuilding the
derived `by_service` index. CI-realism rationale is preserved in each test's
module comment.

## DWD-04 — the by-service-rebuild-on-recovery test is the principal new coverage vs Pulse v1

DESIGN (`application-architecture.md`, DD3/DD4 no-drift surface) fixes that the
snapshot persists the `by_trace` buckets ONLY; `by_service` is derived and
rebuilt from the recovered spans on `open`. Pulse v1 has a single index, so
this risk does not exist there. Ray's principal new failure mode is therefore:
recover the trace index but forget to rebuild the service index. Two tests pin
it. Slice 01's `restart_recovers_both_trace_and_service_indices` checks dual
recovery from a pure WAL. Slice 02's CRITICAL
`by_service_index_is_rebuilt_after_reopen_from_snapshot` is the sharp one: it
seeds one trace fanning out across THREE distinct services (gateway, checkout,
billing), snapshots so the WAL is truncated to zero, drops, reopens, then runs
a by-service query per service. With an empty WAL, recovery can only come from
the snapshot, so this test fails if and only if the crafter omits the
`by_service` rebuild. Slice 02's equivalence test
(`snapshot_plus_wal_recovery_matches_pure_wal_recovery`) further asserts BOTH
indices match between a pure-WAL store and a snapshot+tail store, using
distinct per-batch services.

## DWD-05 — hard constraints honoured; no production or build changes

No Ray v0 tests touched (`slice_01_walking_skeleton.rs`,
`slice_02_structured_query.rs` unchanged). No edits to `src/`, `Cargo.toml`,
or `ci.yml`. `cargo` was not run. Only the two new `tests/v1_slice_*.rs` files
and this decisions note were written. Error/edge coverage matches the Pulse v1
scope: empty-batch no-op (slice 01) and the snapshot recovery edges
(snap-then-replay, snapshot+WAL equivalence, idempotence, service-index
rebuild) in slice 02; richer corruption paths stay out of scope because
`TraceStoreError` is still an empty enum and the brief scopes these two slices
only. British English throughout, no em dashes. Handoff to DELIVER: the crafter
adds `FileBackedTraceStore` with `open`, `snapshot`, and the `TraceStore` impl,
driving the eleven tests from RED to GREEN one at a time.
