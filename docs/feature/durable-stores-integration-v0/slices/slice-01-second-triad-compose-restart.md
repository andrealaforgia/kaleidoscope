# Slice 01: Second-triad compose-and-restart

Story: US-01 (Second triad recovers identically across a platform restart)
KPI: KPI-1 (durability completeness of the composed triad — north star)
Priority: P1
Status: Ready (pending DISCUSS reviewer approval)

## Outcome

Running
`cargo test -p integration-suite --test v1_three_durable_stores_compose`
reports `test result: ok` for the test
`pulse_ray_strata_compose_under_shared_tenant_id_and_survive_restart`, proving
that pulse + ray + strata durable adapters compose under one shared
`aegis::TenantId`, recover identically after a drop-and-reopen, and never leak
one tenant's data into another's view.

## Shippable end-to-end?

Yes. Self-contained: one test, exercising all three backbone activities. On its
own it closes the trust gap for the second triad. No downstream slice is needed
for it to deliver value.

## Carpaccio taste tests

- **Demonstrable in one session**: yes — one `cargo test` invocation.
- **Thin vertical, not a layer**: yes — write -> drop -> reopen -> assert,
  through all three pillars, in one test body.
- **Independently valuable**: yes — delivers KPI-1 alone.
- **Right-sized**: ~0.4 day; one test function; mirrors first-triad test 1.

## Work (DELIVER wave, authored by @nw-software-crafter)

1. In `crates/integration-suite/Cargo.toml`: add `ray` and `strata` to
   `[dev-dependencies]` (pulse, aegis already present); add a `[[test]]` block
   named `v1_three_durable_stores_compose` pointing at the new file.
2. Create `crates/integration-suite/tests/v1_three_durable_stores_compose.rs`
   with the AGPL header and the first-triad-style helpers (`temp_root`,
   `cleanup`, `tenant`) plus small builders for one metric / span / profile.
3. Write `pulse_ray_strata_compose_under_shared_tenant_id_and_survive_restart`:
   - Phase 1: open the three FileBacked stores at distinct sub-paths of one
     temp root; ingest `acme` metrics+spans+profile and parallel `globex` data;
     let the stores drop (scope exit flushes BufWriters).
   - Phase 2: reopen all three from the same paths; assert `acme` recovery in
     each pillar (counts + order), and `globex` isolation (no leakage into
     `acme`); `cleanup(root)`.

## Real API anchors (confirmed by reading the crates)

- `pulse::FileBackedMetricStore::open(&base, Box::new(pulse::NoopRecorder))?`
  then `ingest(&acme, MetricBatch::with_metrics(vec![..]))`,
  `query(&acme, &MetricName::new("process.cpu.utilization"), TimeRange::all())`.
- `ray::FileBackedTraceStore::open(&base, Box::new(ray::NoopRecorder))?`
  then `ingest(&acme, SpanBatch::with_spans(vec![..]))`,
  `get_trace(&acme, &trace_id)`,
  `query(&acme, &ServiceName::new("checkout"), TimeRange::all())`.
- `strata::FileBackedProfileStore::open(&base, Box::new(strata::NoopRecorder))?`
  then `ingest(&acme, ProfileBatch::with_profiles(vec![..]))`,
  `query(&acme, &ServiceName::new("checkout"), TimeRange::all())`.

## Acceptance criteria (from US-01)

- [ ] pulse returns `acme`'s metric points after reopen, ascending by time.
- [ ] ray returns `acme`'s full trace by id and spans by service after reopen.
- [ ] strata returns `acme`'s profile for `(acme, "checkout")` after reopen.
- [ ] No `globex` data is visible under `acme` in any pillar.
- [ ] A short recovered count makes an assertion fail with a clear mismatch.

## False-PASS guard

The reopen path MUST equal the write path for each store. A mismatched path
would reopen an empty store and produce a green-but-meaningless run. Reuse the
exact `PathBuf` from Phase 1 in Phase 2.
</content>
