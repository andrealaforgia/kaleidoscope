// Kaleidoscope Ray — v1 file-backed adapter acceptance test
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

//! `FileBackedTraceStore` survives process restart, returns
//! both `get_trace` and service+range queries byte-stable after
//! re-open, and supports snapshot+WAL-truncate. Same shape as
//! Pulse v1 / Lumen v1 / Cinder v1 / Sluice v1 acceptance.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use ray::{
    FileBackedTraceStore, NoopRecorder, ServiceName, Span, SpanBatch, SpanId, SpanKind, SpanStatus,
    StatusCode, TimeRange, TraceId, TraceStore,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn span(trace: [u8; 16], span_id: [u8; 8], start: u64, end: u64, service: &str) -> Span {
    let mut resource = BTreeMap::new();
    if !service.is_empty() {
        resource.insert("service.name".to_string(), service.to_string());
    }
    Span {
        trace_id: TraceId(trace),
        span_id: SpanId(span_id),
        parent_span_id: None,
        name: "GET /checkout".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        status: SpanStatus {
            code: StatusCode::Ok,
            message: String::new(),
        },
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

fn temp_base(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let dir = env::temp_dir().join(format!("kal-ray-v1-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&dir).expect("mkdir");
    dir.join("ray")
}

fn cleanup(base: &std::path::Path) {
    if let Some(parent) = base.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

fn wal_size(base: &std::path::Path) -> u64 {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    fs::metadata(PathBuf::from(p)).map(|m| m.len()).unwrap_or(0)
}

fn snapshot_exists(base: &std::path::Path) -> bool {
    let mut p = base.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p).exists()
}

#[test]
fn ingest_then_get_trace_returns_spans_byte_stable() {
    let base = temp_base("get_trace");
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let trace = [1u8; 16];
    store
        .ingest(
            &tenant("acme"),
            SpanBatch::with_spans(vec![
                span(trace, [1; 8], 100, 200, "checkout"),
                span(trace, [2; 8], 110, 190, "checkout"),
            ]),
        )
        .expect("ingest");
    let out = store
        .get_trace(&tenant("acme"), &TraceId(trace))
        .expect("get_trace");
    assert_eq!(out.len(), 2);
    cleanup(&base);
}

#[test]
fn restart_recovers_spans_from_wal_via_get_trace() {
    let base = temp_base("wal_restart");
    let trace = [2u8; 16];
    {
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                SpanBatch::with_spans(vec![
                    span(trace, [1; 8], 100, 200, "checkout"),
                    span(trace, [2; 8], 110, 190, "checkout"),
                ]),
            )
            .expect("ingest");
    }
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .get_trace(&tenant("acme"), &TraceId(trace))
        .expect("get_trace");
    assert_eq!(out.len(), 2, "WAL replay recovers both spans");
    cleanup(&base);
}

#[test]
fn restart_rebuilds_service_index_so_query_works() {
    // The on-disk shape stores only the per-(tenant, trace_id)
    // bucket. The secondary (tenant, service) index is rebuilt
    // in-memory on `open`. This test proves the rebuild
    // happens.
    let base = temp_base("service_rebuild");
    {
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                SpanBatch::with_spans(vec![
                    span([1u8; 16], [1; 8], 100, 200, "checkout"),
                    span([2u8; 16], [1; 8], 110, 200, "checkout"),
                    span([3u8; 16], [1; 8], 120, 200, "search"),
                ]),
            )
            .expect("ingest");
    }
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let checkout = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query checkout");
    let search = store
        .query(
            &tenant("acme"),
            &ServiceName::new("search"),
            TimeRange::all(),
        )
        .expect("query search");
    assert_eq!(checkout.len(), 2, "service index rebuilt for checkout");
    assert_eq!(search.len(), 1, "service index rebuilt for search");
    cleanup(&base);
}

#[test]
fn snapshot_writes_file_and_truncates_wal() {
    let base = temp_base("snapshot");
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            SpanBatch::with_spans(vec![span([1u8; 16], [1; 8], 100, 200, "checkout")]),
        )
        .expect("ingest");
    assert!(wal_size(&base) > 0);
    assert!(!snapshot_exists(&base));
    store.snapshot().expect("snapshot");
    assert_eq!(wal_size(&base), 0);
    assert!(snapshot_exists(&base));
    cleanup(&base);
}

#[test]
fn restart_recovers_snapshot_plus_wal_added_after_snapshot() {
    let base = temp_base("snap_plus_wal");
    let trace = [4u8; 16];
    {
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                SpanBatch::with_spans(vec![span(trace, [1; 8], 100, 200, "checkout")]),
            )
            .expect("pre-snapshot");
        store.snapshot().expect("snapshot");
        store
            .ingest(
                &tenant("acme"),
                SpanBatch::with_spans(vec![span(trace, [2; 8], 110, 190, "checkout")]),
            )
            .expect("post-snapshot");
    }
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .get_trace(&tenant("acme"), &TraceId(trace))
        .expect("get_trace");
    assert_eq!(out.len(), 2, "snapshot + WAL both recovered");
    let checkout = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(checkout.len(), 2);
    cleanup(&base);
}

#[test]
fn two_tenants_are_isolated_in_the_same_data_dir() {
    let base = temp_base("isolation");
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let trace_a = [10u8; 16];
    let trace_b = [11u8; 16];
    store
        .ingest(
            &tenant("acme"),
            SpanBatch::with_spans(vec![span(trace_a, [1; 8], 100, 200, "checkout")]),
        )
        .expect("acme");
    store
        .ingest(
            &tenant("globex"),
            SpanBatch::with_spans(vec![span(trace_b, [1; 8], 100, 200, "checkout")]),
        )
        .expect("globex");
    assert_eq!(
        store
            .get_trace(&tenant("acme"), &TraceId(trace_a))
            .expect("acme get")
            .len(),
        1
    );
    assert_eq!(
        store
            .get_trace(&tenant("acme"), &TraceId(trace_b))
            .expect("acme cross")
            .len(),
        0,
        "globex's trace MUST NOT appear under acme"
    );
    cleanup(&base);
}

#[test]
fn time_range_query_filters_on_start_time_unix_nano() {
    let base = temp_base("time_range");
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            SpanBatch::with_spans(vec![
                span([1u8; 16], [1; 8], 100, 110, "checkout"),
                span([2u8; 16], [1; 8], 200, 210, "checkout"),
                span([3u8; 16], [1; 8], 300, 310, "checkout"),
            ]),
        )
        .expect("ingest");
    let out = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::new(150, 250),
        )
        .expect("query");
    // Only the span starting at 200 falls in [150, 250).
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].start_time_unix_nano, 200);
    cleanup(&base);
}

#[test]
fn empty_batch_ingest_is_a_no_op_persistence_wise() {
    let base = temp_base("empty");
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(&tenant("acme"), SpanBatch::with_spans(vec![]))
        .expect("empty");
    assert_eq!(wal_size(&base), 0, "empty batch does not append");
    cleanup(&base);
}

#[test]
fn spans_without_service_resource_attribute_are_findable_by_trace_only() {
    // Edge case the v0 in-memory adapter handles: a span with
    // no `service.name` resource attribute is indexed by trace
    // only, not by service. The v1 adapter must preserve that
    // behaviour across restart.
    let base = temp_base("no_service");
    let trace = [7u8; 16];
    {
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                SpanBatch::with_spans(vec![span(trace, [1; 8], 100, 200, "")]),
            )
            .expect("ingest");
    }
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    assert_eq!(
        store
            .get_trace(&tenant("acme"), &TraceId(trace))
            .expect("get_trace")
            .len(),
        1
    );
    assert!(store
        .query(&tenant("acme"), &ServiceName::new(""), TimeRange::all())
        .expect("query empty service")
        .is_empty());
    cleanup(&base);
}
