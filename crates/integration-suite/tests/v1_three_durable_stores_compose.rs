// Kaleidoscope integration suite — second-triad composition test
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

//! Cross-crate composition test for the second triad of durable v1
//! adapters.
//!
//! The first integration test
//! (`v1_three_adapters_compose_under_restart`) proved Cinder, Sluice
//! and Lumen compose under one tenant and survive a restart. This
//! test does the same for the other three storage pillars:
//!
//! - Pulse v1 (`FileBackedMetricStore`)
//! - Ray v1 (`FileBackedTraceStore`)
//! - Strata v1 (`FileBackedProfileStore`)
//!
//! With both triads proven, the platform's six storage pillars are
//! shown to compose and recover together under a single
//! `aegis::TenantId`, not merely to be six durable adapters that work
//! in isolation.
//!
//! ## Scenario
//!
//! A platform engineer ingests, for tenant `acme`, a stream of
//! metrics into Pulse, a trace into Ray, and a profile series into
//! Strata. A second tenant `globex` ingests its own parallel data
//! into all three. The process drops. On restart the test asserts:
//!
//! 1. Pulse recovers `acme`'s metric points in time order.
//! 2. Ray recovers `acme`'s trace, by trace id and by service.
//! 3. Strata recovers `acme`'s profiles in time order.
//! 4. None of `globex`'s state has leaked into `acme`'s, in any of
//!    the three adapters.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder as PulseRecorder, TimeRange as PulseRange,
};
use ray::{
    FileBackedTraceStore, NoopRecorder as RayRecorder, ServiceName as RayService, Span, SpanBatch,
    SpanId, SpanKind, SpanStatus, TimeRange as RayRange, TraceId, TraceStore,
};
use strata::{
    FileBackedProfileStore, NoopRecorder as StrataRecorder, Profile, ProfileBatch, ProfileStore,
    ServiceName as StrataService, TimeRange as StrataRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn temp_root(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("kal-integ-durable-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir root");
    path
}

fn cleanup(root: &std::path::Path) {
    let _ = fs::remove_dir_all(root);
}

// --- builders, mirroring each pillar's own v1 acceptance tests ---

fn metric_point(time: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano: time,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value,
    }
}

fn gauge(name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(name),
        description: String::new(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

fn span(trace_byte: u8, span_byte: u8, service: &str, name: &str, start: u64, end: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Span {
        trace_id: TraceId([trace_byte; 16]),
        span_id: SpanId([span_byte; 8]),
        parent_span_id: None,
        name: name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        status: SpanStatus::default(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

fn profile(time: u64, service: &str) -> Profile {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Profile {
        time_unix_nano: time,
        duration_nanos: 1_000_000,
        profile_type: "cpu".to_string(),
        sample_type: Vec::new(),
        samples: Vec::new(),
        locations: Vec::new(),
        functions: Vec::new(),
        mappings: Vec::new(),
        string_table: Vec::new(),
        resource_attributes: resource,
        attributes: BTreeMap::new(),
    }
}

#[test]
fn pulse_ray_strata_compose_under_shared_tenant_id_and_survive_restart() {
    let root = temp_root("compose_restart");
    let pulse_base = root.join("pulse-store");
    let ray_base = root.join("ray-store");
    let strata_base = root.join("strata-store");

    let acme = tenant("acme");
    let globex = tenant("globex");

    // --- Phase 1: ingest into all three, for two tenants; drop. ---
    {
        let pulse =
            FileBackedMetricStore::open(&pulse_base, Box::new(PulseRecorder)).expect("open pulse");
        let ray = FileBackedTraceStore::open(&ray_base, Box::new(RayRecorder)).expect("open ray");
        let strata = FileBackedProfileStore::open(&strata_base, Box::new(StrataRecorder))
            .expect("open strata");

        // Pulse: three points for acme, out of order to prove the
        // recovery sort, plus one isolated globex point.
        pulse
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "checkout.rps",
                    "checkout",
                    vec![metric_point(300, 3.0), metric_point(100, 1.0)],
                )]),
            )
            .expect("pulse ingest acme 1");
        pulse
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "checkout.rps",
                    "checkout",
                    vec![metric_point(200, 2.0)],
                )]),
            )
            .expect("pulse ingest acme 2");
        pulse
            .ingest(
                &globex,
                MetricBatch::with_metrics(vec![gauge(
                    "checkout.rps",
                    "billing",
                    vec![metric_point(150, 9.0)],
                )]),
            )
            .expect("pulse ingest globex");

        // Ray: a two-span trace for acme on service checkout, plus an
        // isolated globex trace.
        ray.ingest(
            &acme,
            SpanBatch::with_spans(vec![
                span(0xAA, 0x02, "checkout", "second", 200, 250),
                span(0xAA, 0x01, "checkout", "first", 100, 150),
            ]),
        )
        .expect("ray ingest acme");
        ray.ingest(
            &globex,
            SpanBatch::with_spans(vec![span(0xBB, 0x09, "billing", "globex", 150, 200)]),
        )
        .expect("ray ingest globex");

        // Strata: two profiles for acme on service checkout, plus an
        // isolated globex profile.
        strata
            .ingest(
                &acme,
                ProfileBatch::with_profiles(vec![
                    profile(200, "checkout"),
                    profile(100, "checkout"),
                ]),
            )
            .expect("strata ingest acme");
        strata
            .ingest(
                &globex,
                ProfileBatch::with_profiles(vec![profile(150, "billing")]),
            )
            .expect("strata ingest globex");

        // Stores drop at end of scope; each BufWriter flushes.
    }

    // --- Phase 2: reopen all three, verify recovery + isolation. ---
    let pulse2 =
        FileBackedMetricStore::open(&pulse_base, Box::new(PulseRecorder)).expect("reopen pulse");
    let ray2 = FileBackedTraceStore::open(&ray_base, Box::new(RayRecorder)).expect("reopen ray");
    let strata2 = FileBackedProfileStore::open(&strata_base, Box::new(StrataRecorder))
        .expect("reopen strata");

    // Pulse: acme has three points in ascending time order.
    let pts = pulse2
        .query(&acme, &MetricName::new("checkout.rps"), PulseRange::all())
        .expect("pulse query acme");
    assert_eq!(pts.len(), 3, "acme pulse points recovered");
    assert_eq!(pts[0].1.time_unix_nano, 100);
    assert_eq!(pts[1].1.time_unix_nano, 200);
    assert_eq!(pts[2].1.time_unix_nano, 300);
    // Pulse: globex's point did not leak into acme's series.
    assert!(pts.iter().all(|(_m, p)| p.value != 9.0));

    // Ray: acme's trace recovers by trace id, in start-time order.
    let trace = ray2
        .get_trace(&acme, &TraceId([0xAA; 16]))
        .expect("get_trace");
    assert_eq!(trace.len(), 2, "acme trace recovered");
    assert_eq!(trace[0].name, "first");
    assert_eq!(trace[1].name, "second");
    // Ray: acme's trace also recovers by service.
    let by_service = ray2
        .query(&acme, &RayService::new("checkout"), RayRange::all())
        .expect("ray query by service");
    assert_eq!(by_service.len(), 2, "acme service index recovered");
    // Ray: globex's trace is not visible under acme.
    assert!(ray2
        .get_trace(&acme, &TraceId([0xBB; 16]))
        .expect("get_trace globex under acme")
        .is_empty());

    // Strata: acme has two profiles in ascending time order.
    let profs = strata2
        .query(&acme, &StrataService::new("checkout"), StrataRange::all())
        .expect("strata query acme");
    assert_eq!(profs.len(), 2, "acme profiles recovered");
    assert_eq!(profs[0].time_unix_nano, 100);
    assert_eq!(profs[1].time_unix_nano, 200);

    // --- Isolation: globex recovers its own state, separately. ---
    let globex_pts = pulse2
        .query(&globex, &MetricName::new("checkout.rps"), PulseRange::all())
        .expect("pulse query globex");
    assert_eq!(globex_pts.len(), 1);
    assert_eq!(globex_pts[0].1.value, 9.0);

    let globex_trace = ray2
        .get_trace(&globex, &TraceId([0xBB; 16]))
        .expect("globex trace");
    assert_eq!(globex_trace.len(), 1);
    assert_eq!(globex_trace[0].name, "globex");

    let globex_profs = strata2
        .query(&globex, &StrataService::new("billing"), StrataRange::all())
        .expect("strata query globex");
    assert_eq!(globex_profs.len(), 1);
    assert_eq!(globex_profs[0].time_unix_nano, 150);

    // acme never sees globex's billing service in traces or profiles.
    assert!(ray2
        .query(&acme, &RayService::new("billing"), RayRange::all())
        .expect("ray acme billing")
        .is_empty());
    assert!(strata2
        .query(&acme, &StrataService::new("billing"), StrataRange::all())
        .expect("strata acme billing")
        .is_empty());

    cleanup(&root);
}

#[test]
fn tenant_id_is_the_cross_crate_identity_contract_for_the_second_triad() {
    // The same `&TenantId` reference passes to Pulse, Ray and Strata
    // with no conversion, no clone-per-call, no adapter-specific
    // tenant type. If aegis ever changes TenantId's shape, this test
    // breaks at compile time, alerting the maintainer that the
    // cross-crate contract has shifted, exactly as the first-triad
    // test does for Cinder, Sluice and Lumen.
    let root = temp_root("identity_contract");

    let one_tenant = tenant("shared");

    let pulse =
        FileBackedMetricStore::open(root.join("p"), Box::new(PulseRecorder)).expect("open pulse");
    let ray = FileBackedTraceStore::open(root.join("r"), Box::new(RayRecorder)).expect("open ray");
    let strata = FileBackedProfileStore::open(root.join("s"), Box::new(StrataRecorder))
        .expect("open strata");

    pulse
        .ingest(
            &one_tenant,
            MetricBatch::with_metrics(vec![gauge("svc.rps", "svc", vec![metric_point(100, 1.0)])]),
        )
        .expect("pulse ingest");
    ray.ingest(
        &one_tenant,
        SpanBatch::with_spans(vec![span(0x11, 0x01, "svc", "op", 100, 150)]),
    )
    .expect("ray ingest");
    strata
        .ingest(
            &one_tenant,
            ProfileBatch::with_profiles(vec![profile(100, "svc")]),
        )
        .expect("strata ingest");

    // All three are observable under the same tenant identity.
    assert_eq!(
        pulse
            .query(&one_tenant, &MetricName::new("svc.rps"), PulseRange::all())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ray.query(&one_tenant, &RayService::new("svc"), RayRange::all())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        strata
            .query(&one_tenant, &StrataService::new("svc"), StrataRange::all())
            .unwrap()
            .len(),
        1
    );

    cleanup(&root);
}
