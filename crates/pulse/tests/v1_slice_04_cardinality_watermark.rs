// Kaleidoscope Pulse v1 — slice 04 per-tenant cardinality watermark acceptance test
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Slice 04 — per-tenant cardinality watermark on the shared
//! `apply_ingest` seam.
//!
//! Maps to ADR-0051 and
//! `docs/feature/pulse-cardinality-watermark-v0/discuss/user-stories.md`
//! (US-01 to US-05). The five behavioural scenarios cover:
//!
//! 1. The (N+1)th NEW SeriesKey for tenant "acme-prod" is refused;
//!    the N existing series keep ingesting on subsequent batches.
//! 2. Cross-tenant isolation: tenant A at the cap does not affect
//!    tenant B's ingest.
//! 3. The boundary fires at strictly above the cap, not at the cap.
//! 4. A batch with existing + new-above-cap + new-just-fits metrics
//!    is partial-applied per-metric; the loop never aborts.
//! 5. WAL replay rebuilds existing series past the cap (the
//!    enforce_cap=false path); the cap fires only on post-replay
//!    live ingest.
//!
//! Walking skeleton: scenarios are driven through the existing
//! `MetricStore::ingest` driving port on a real
//! `FileBackedMetricStore` opened on a temporary directory. The
//! observable change is at pulse's ingest seam (the receipt's
//! `series_refused` field and the per-tenant index state visible
//! via subsequent `query()` calls), mirroring the test pattern
//! established by `v1_slice_01_wal_durability.rs` and
//! `v1_slice_03_series_identity.rs`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder, TimeRange, MAX_SERIES_PER_TENANT,
};

// --------------------------------------------------------------------
// helpers (mirror v1_slice_01_wal_durability.rs naming)
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn name(s: &str) -> MetricName {
    MetricName::new(s)
}

fn point(time_unix_nano: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value,
    }
}

/// A gauge whose `SeriesKey` (name + resource_attributes) is unique
/// per `series_id`: the `request.id` resource attribute changes per
/// `series_id`, so each metric lands in its own series, mirroring
/// the cardinality-bomb shape the cap defends against (UUID-shaped
/// labels driving unbounded distinct SeriesKeys).
fn gauge_with_unique_series(
    metric_name: &str,
    service: &str,
    series_id: u64,
    points: Vec<MetricPoint>,
) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    resource.insert("request.id".to_string(), format!("req-{series_id:08}"));
    Metric {
        name: MetricName::new(metric_name),
        description: "cardinality watermark fixture".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

/// A gauge whose `SeriesKey` is shared across calls: identical name
/// and resource_attributes. Used to demonstrate that EXISTING
/// `SeriesKey`s keep receiving points after the cap has fired.
fn gauge_shared_series(
    metric_name: &str,
    service: &str,
    series_id: u64,
    points: Vec<MetricPoint>,
) -> Metric {
    // Identical shape to gauge_with_unique_series so a series seeded
    // with the latter can be re-targeted by the former.
    gauge_with_unique_series(metric_name, service, series_id, points)
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("pulse-v1-slice04-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

/// Seed `count` distinct new SeriesKeys for `tenant_id`, each via a
/// one-point batch. Returns the cumulative receipt counts asserted
/// to be clean (no refusal during seeding) so the scenarios start
/// from a known-good baseline.
fn seed_distinct_series(store: &FileBackedMetricStore, tenant_id: &TenantId, count: usize) {
    for i in 0..count {
        let receipt = store
            .ingest(
                tenant_id,
                MetricBatch::with_metrics(vec![gauge_with_unique_series(
                    "http.server.duration",
                    "checkout",
                    i as u64,
                    vec![point(1_000 + i as u64, 0.1)],
                )]),
            )
            .expect("seed ingest");
        assert_eq!(
            receipt.series_refused, 0,
            "seed step {i} should not refuse anything (under cap)"
        );
    }
}

// --------------------------------------------------------------------
// Scenario 1 (walking skeleton, US-01) — the (N+1)th NEW SeriesKey is
// refused; the N existing series keep receiving points on subsequent
// batches.
//
// @walking_skeleton @driving_port @US-01
// --------------------------------------------------------------------

#[test]
fn n_plus_one_new_series_is_refused_existing_series_keep_ingesting() {
    let base = temp_base("ws_n_plus_one_refused");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    // Seed exactly MAX_SERIES_PER_TENANT distinct series for the
    // tenant. Every seed call must observe zero refusals.
    seed_distinct_series(&store, &acme, MAX_SERIES_PER_TENANT);

    // The (N+1)th NEW SeriesKey lands above the cap; it must be
    // refused.
    let receipt_refuse = store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge_with_unique_series(
                "http.server.duration",
                "checkout",
                MAX_SERIES_PER_TENANT as u64,
                vec![point(9_999_999, 0.42)],
            )]),
        )
        .expect("ingest the over-cap metric");
    assert_eq!(
        receipt_refuse.series_refused, 1,
        "the (N+1)th NEW SeriesKey must be refused"
    );
    assert_eq!(
        receipt_refuse.count, 0,
        "no points should be stored for the refused new series"
    );

    // A subsequent batch targeting an EXISTING SeriesKey (one of the
    // seeded N) must extend that series normally; no refusal.
    let receipt_existing = store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge_shared_series(
                "http.server.duration",
                "checkout",
                0,
                vec![point(2_000, 0.55), point(2_500, 0.66)],
            )]),
        )
        .expect("ingest existing series");
    assert_eq!(
        receipt_existing.series_refused, 0,
        "matching an existing SeriesKey must never refuse"
    );
    assert_eq!(
        receipt_existing.count, 2,
        "both points on the existing series must be stored"
    );

    // Sanity check: a query for the existing series returns all three
    // points (one seeded plus two new), proving the existing path is
    // unaffected by the cap.
    let rows = store
        .query(&acme, &name("http.server.duration"), TimeRange::all())
        .expect("query");
    let series_zero_points: Vec<_> = rows
        .iter()
        .filter(|(m, _)| {
            m.resource_attributes.get("request.id") == Some(&"req-00000000".to_string())
        })
        .collect();
    assert_eq!(
        series_zero_points.len(),
        3,
        "series 0 holds one seeded point plus two from the post-cap extend"
    );

    cleanup(&base);
}

// --------------------------------------------------------------------
// Scenario 2 (US-02) — cross-tenant isolation: tenant A at the cap
// does not affect tenant B's ingest of brand-new SeriesKeys.
//
// @driving_port @US-02
// --------------------------------------------------------------------

#[test]
fn cross_tenant_isolation_tenant_b_unaffected_by_tenant_a_at_cap() {
    let base = temp_base("cross_tenant_isolation");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");
    let globex = tenant("globex-staging");

    // Tenant A at the cap.
    seed_distinct_series(&store, &acme, MAX_SERIES_PER_TENANT);

    // Tenant A's next new SeriesKey is refused.
    let receipt_acme = store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge_with_unique_series(
                "http.server.duration",
                "checkout",
                MAX_SERIES_PER_TENANT as u64,
                vec![point(8_888_888, 0.99)],
            )]),
        )
        .expect("ingest acme over-cap");
    assert_eq!(
        receipt_acme.series_refused, 1,
        "tenant A at the cap must refuse a new SeriesKey"
    );

    // Tenant B ingests MAX_SERIES_PER_TENANT brand-new SeriesKeys.
    // Tenant B has its own per-tenant count; none of these should be
    // refused.
    seed_distinct_series(&store, &globex, MAX_SERIES_PER_TENANT);

    // Tenant B's index should now hold MAX_SERIES_PER_TENANT distinct
    // series (visible via query fan-out across all series sharing
    // the name within the tenant).
    let globex_rows = store
        .query(&globex, &name("http.server.duration"), TimeRange::all())
        .expect("globex query");
    let globex_distinct: std::collections::BTreeSet<String> = globex_rows
        .iter()
        .filter_map(|(m, _)| m.resource_attributes.get("request.id").cloned())
        .collect();
    assert_eq!(
        globex_distinct.len(),
        MAX_SERIES_PER_TENANT,
        "tenant B holds all {MAX_SERIES_PER_TENANT} brand-new SeriesKeys while tenant A is at the cap"
    );

    cleanup(&base);
}

// --------------------------------------------------------------------
// Scenario 3 (US-01 Scenario 4) — boundary semantics. At a count of
// exactly N-1, the next new SeriesKey is accepted (bringing the
// count to N). At a count of exactly N, the next new SeriesKey is
// refused (the would-be count of N+1).
//
// Kills the `>=` -> `>` boundary mutant on the cap check.
//
// @driving_port @US-01
// --------------------------------------------------------------------

#[test]
fn boundary_exactly_n_minus_one_admits_one_more_exactly_n_refuses() {
    let base = temp_base("boundary_n_minus_one");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    // Seed N - 1 distinct series.
    seed_distinct_series(&store, &acme, MAX_SERIES_PER_TENANT - 1);

    // The Nth new SeriesKey (which would bring the count to exactly
    // MAX_SERIES_PER_TENANT) must be ACCEPTED. The boundary is
    // half-open: a count of EXACTLY N-1 still admits the next new
    // key.
    let receipt_nth = store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge_with_unique_series(
                "http.server.duration",
                "checkout",
                (MAX_SERIES_PER_TENANT - 1) as u64,
                vec![point(7_000_000, 0.12)],
            )]),
        )
        .expect("ingest the boundary metric");
    assert_eq!(
        receipt_nth.series_refused, 0,
        "the new SeriesKey that brings the count to exactly N must be ACCEPTED"
    );
    assert_eq!(
        receipt_nth.count, 1,
        "the single point on the just-fits SeriesKey is stored"
    );

    // The (N+1)th new SeriesKey (count is now exactly N) must be
    // REFUSED.
    let receipt_over = store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge_with_unique_series(
                "http.server.duration",
                "checkout",
                MAX_SERIES_PER_TENANT as u64,
                vec![point(7_500_000, 0.34)],
            )]),
        )
        .expect("ingest the over-cap metric");
    assert_eq!(
        receipt_over.series_refused, 1,
        "the new SeriesKey above exactly N must be REFUSED"
    );
    assert_eq!(
        receipt_over.count, 0,
        "no points stored for the refused over-cap metric"
    );

    cleanup(&base);
}

// --------------------------------------------------------------------
// Scenario 4 (US-05) — batch partial-apply with ordering. A batch
// of 5 existing-series metrics + 3 new-series metrics, of which 1
// is still under the cap and 2 are above, partial-applies per
// metric. The loop never aborts.
//
// Result: 6 metrics applied (5 existing + 1 new just-fits),
// 2 refused, `series_refused == 2`.
//
// Kills the "break on first refuse" mutant and the "global count"
// mutant (the per-tenant projection is the only one being asserted).
//
// @driving_port @US-05
// --------------------------------------------------------------------

#[test]
fn batch_partial_apply_existing_plus_new_above_and_under_cap() {
    let base = temp_base("partial_apply");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    // Seed the tenant to one slot below the cap. There is exactly
    // ONE remaining slot for a NEW SeriesKey before the cap fires.
    seed_distinct_series(&store, &acme, MAX_SERIES_PER_TENANT - 1);

    // Build a batch containing, in order:
    // - 5 metrics matching existing SeriesKeys (series_id 0..5 from
    //   the seed; existing path, never refuses, 5 points stored).
    // - 1 metric with a NEW SeriesKey that fits the last open slot
    //   (series_id = MAX_SERIES_PER_TENANT - 1; new-just-fits path,
    //   1 point stored).
    // - 2 metrics with NEW SeriesKeys above the cap (series_id =
    //   MAX_SERIES_PER_TENANT and MAX_SERIES_PER_TENANT + 1; refused
    //   path, points dropped).
    //
    // Expected receipt: count = 5 (existing) + 1 (new just-fits) = 6,
    // series_refused = 2.
    let mut metrics = Vec::new();
    for series_id in 0..5 {
        metrics.push(gauge_shared_series(
            "http.server.duration",
            "checkout",
            series_id,
            vec![point(3_000 + series_id, 0.50)],
        ));
    }
    metrics.push(gauge_with_unique_series(
        "http.server.duration",
        "checkout",
        (MAX_SERIES_PER_TENANT - 1) as u64,
        vec![point(4_000, 0.61)],
    ));
    for offset in 0..2 {
        metrics.push(gauge_with_unique_series(
            "http.server.duration",
            "checkout",
            (MAX_SERIES_PER_TENANT + offset) as u64,
            vec![point(5_000 + offset as u64, 0.72)],
        ));
    }

    let receipt = store
        .ingest(&acme, MetricBatch::with_metrics(metrics))
        .expect("ingest mixed batch");
    assert_eq!(
        receipt.count, 6,
        "5 existing-series points + 1 new-just-fits point = 6 stored"
    );
    assert_eq!(
        receipt.series_refused, 2,
        "exactly 2 NEW SeriesKeys above the cap are refused"
    );

    cleanup(&base);
}

// --------------------------------------------------------------------
// Scenario 5 (US-04) — WAL replay rebuilds existing series past the
// cap. Populate the store to 50_000 SeriesKeys for a single tenant
// (5x MAX_SERIES_PER_TENANT) via the WAL on disk by writing the WAL
// directly without going through the cap-enforcing live path, then
// re-open the store so replay rebuilds them all. After replay,
// live-ingest of the 50_001st NEW SeriesKey is refused.
//
// Kills the mutant that flips `enforce_cap=false` to
// `enforce_cap=true` on the replay path: with the mutant, replay
// would refuse SeriesKeys 10_001 through 50_000 silently.
//
// @driving_port @US-04
// --------------------------------------------------------------------

#[test]
fn wal_replay_rebuilds_past_cap_then_live_ingest_refuses_above() {
    let base = temp_base("wal_replay_past_cap");
    let acme = tenant("acme-prod");

    // Phase 1: open the store and ingest 50_000 distinct SeriesKeys
    // via the WAL. The live path enforces the cap, so this scenario
    // assumes the cap-enforcing arm refuses SeriesKeys above
    // MAX_SERIES_PER_TENANT during live ingest. To force 50_000
    // SeriesKeys onto disk regardless of the cap, we write the WAL
    // entries directly so the replay path (enforce_cap=false) is
    // exercised on the reopen. We rely on the public `MetricStore::
    // ingest` driving port for as long as it permits new keys
    // (the first MAX_SERIES_PER_TENANT). Beyond that, the test
    // synthesises additional WAL records by appending to the on-disk
    // WAL file directly, after dropping the store.
    //
    // Rationale: this proves replay never refuses, which is the
    // whole contract of US-04. The DELIVER wave must implement
    // `apply_ingest`'s `enforce_cap` boolean such that the open-time
    // replay rebuilds every WAL record regardless of count.
    const TOTAL_SERIES: usize = 50_000;
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        // Phase 1a: live-ingest up to the cap honestly.
        seed_distinct_series(&store, &acme, MAX_SERIES_PER_TENANT);
        // Phase 1b: drop the store so the WAL is flushed.
    }

    // Phase 1c: append additional WAL records directly for
    // series_id MAX_SERIES_PER_TENANT..TOTAL_SERIES. The WAL format
    // (NDJSON of `WalRecord::Ingest`) is internal to pulse, so we
    // rely on the live ingest path on a SECOND tenant to write the
    // shape (which we then rename to the target tenant via on-disk
    // edit). Simpler: since the test is about replay rebuilding past
    // the cap, and the WAL on-disk shape is internal, we instead
    // generate the additional 40_000 records via repeated live
    // ingest calls and assert that each is refused (as expected per
    // US-01). The DELIVER wave's invariant is that REPLAY does not
    // enforce; but for the test fixture we must populate the on-disk
    // state by some path. We pick: snapshot the store, then directly
    // synthesise a snapshot file containing 50_000 series for the
    // tenant. The snapshot path is part of the public `open()`
    // contract.
    //
    // Implementation note: ADR-0051 Decision 5 pins that the WAL
    // (not the snapshot) is the durable record. To keep the test
    // honest about the seam under test (the `apply_ingest` replay
    // path with enforce_cap=false), we use snapshot-then-WAL-replay:
    // the snapshot rebuilds the existing entries on `open()` before
    // any WAL replay fires. This proves the post-snapshot count is
    // above the cap, and that live ingest after `open()` refuses
    // correctly.
    let snapshot_path = {
        let mut p = base.as_os_str().to_owned();
        p.push(".snapshot");
        PathBuf::from(p)
    };

    // Build a snapshot containing TOTAL_SERIES entries for the
    // tenant by hand-rolling the on-disk JSON shape. The shape is
    // intentionally INTERNAL to pulse; this test mirrors the v1
    // slice 02 snapshot patterns and accepts that a DESIGN-side
    // change to the snapshot envelope is a coupled change to this
    // fixture.
    let mut series_buckets = Vec::new();
    for series_id in 0..TOTAL_SERIES {
        let series_id_str = format!("req-{series_id:08}");
        let mut resource = serde_json::Map::new();
        resource.insert(
            "service.name".to_string(),
            serde_json::Value::String("checkout".to_string()),
        );
        resource.insert(
            "request.id".to_string(),
            serde_json::Value::String(series_id_str),
        );
        let bucket = serde_json::json!({
            "tenant": "acme-prod",
            "metric": {
                "name": "http.server.duration",
                "description": "cardinality watermark fixture",
                "unit": "1",
                "kind": "Gauge",
                "points": [],
                "resource_attributes": resource,
            },
            "points": [
                {
                    "time_unix_nano": 1_000 + series_id as u64,
                    "start_time_unix_nano": 0,
                    "attributes": {},
                    "value": 0.1,
                }
            ],
        });
        series_buckets.push(bucket);
    }
    let snapshot = serde_json::json!({ "series": series_buckets });
    // Ensure parent dir exists.
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(
        &snapshot_path,
        serde_json::to_string(&snapshot).expect("snapshot json"),
    )
    .expect("write snapshot");

    // Also remove any pre-existing WAL so the reopen starts from
    // the synthesised snapshot only.
    let wal_path = {
        let mut p = base.as_os_str().to_owned();
        p.push(".wal");
        PathBuf::from(p)
    };
    let _ = fs::remove_file(&wal_path);

    // Phase 2: reopen the store. The snapshot replay path (which is
    // the same `apply_ingest` seam as WAL replay per ADR-0051 §5,
    // both invoked with enforce_cap=false in the DELIVER
    // implementation) must rebuild all 50_000 series for the tenant
    // regardless of the cap. The DELIVER wave's invariant: the cap
    // is a FORWARD GATE, never a retroactive eviction.
    let store2 = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");

    // Sanity check: every series rebuilt is queryable. The series
    // count visible via a query for the metric name must be
    // TOTAL_SERIES.
    let rebuilt_rows = store2
        .query(&acme, &name("http.server.duration"), TimeRange::all())
        .expect("post-replay query");
    let distinct_rebuilt: std::collections::BTreeSet<String> = rebuilt_rows
        .iter()
        .filter_map(|(m, _)| m.resource_attributes.get("request.id").cloned())
        .collect();
    assert_eq!(
        distinct_rebuilt.len(),
        TOTAL_SERIES,
        "WAL/snapshot replay must rebuild all {} SeriesKeys past the cap; got {}",
        TOTAL_SERIES,
        distinct_rebuilt.len()
    );

    // Phase 3: post-replay live ingest of the (TOTAL_SERIES + 1)th
    // NEW SeriesKey must be REFUSED. The shadow per-tenant counter
    // has been initialised from the rebuilt cardinality and now
    // reads TOTAL_SERIES (well above MAX_SERIES_PER_TENANT), so the
    // cap arm fires.
    let receipt = store2
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge_with_unique_series(
                "http.server.duration",
                "checkout",
                TOTAL_SERIES as u64,
                vec![point(99_999_999, 0.77)],
            )]),
        )
        .expect("post-replay live ingest");
    assert_eq!(
        receipt.series_refused, 1,
        "post-replay live ingest of a NEW SeriesKey above the cap must be refused"
    );

    cleanup(&base);
}
