// Kaleidoscope aperture-storage-sink — slice 02 acceptance test
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

//! Slice 02 — traces persist to ray end to end.
//!
//! Maps to `docs/feature/aperture-storage-sink-v0/slices/slice-02-traces-to-ray.md`.
//! Story: US-02. Decisions: DD3 (tenant resolution tenant.id ->
//! default_tenant -> refuse), DD4 (sink holds `Arc<FileBacked*Store>`),
//! DD5 (probe is an active write check), DD7 (atomic translation,
//! accepted => persisted, refused => writes nothing). ADR-0041
//! Decisions 1 and 2. Outcome KPIs: KPI-2 (round-trip fidelity +
//! durability), KPI-4, KPI-5.
//!
//! These tests enter through the real aperture driving port
//! `OtlpSink::accept` and the `Probe::probe` contract. The observable
//! outcome is what an operator can later query out of ray
//! (`TraceStore::get_trace` / `TraceStore::query`). Nothing internal to
//! the translator is invoked directly: the OTLP
//! `ExportTraceServiceRequest` goes in at the port, and the persisted
//! `ray::Span`s come out at the store.
//!
//! ## RED-gate boundary
//!
//! Slice 01 delivered `StorageSink` / `StorageSinkConfig` with a
//! logs-only `with_log_store` constructor and an honest no-op traces
//! arm. This slice imports a not-yet-existing traces-only constructor
//! `StorageSink::with_trace_store`: the compile error against that
//! symbol is the RED state for the classic Rust outside-in loop.
//! DELIVER adds the `trace_store: Option<Arc<FileBackedTraceStore>>`
//! field plus this constructor and turns the `SinkRecord::Traces` arm
//! from a no-op into real translation + ingest; these tests then go
//! GREEN, committed atomic with the slice.
//!
//! ## Assumed StorageSink traces surface (DELIVER must match)
//!
//! Mirroring the slice-01 logs-only constructor (DD4), the smallest
//! honest slice-02 shape is a traces-only constructor:
//!
//! - `StorageSink::with_trace_store(Arc<ray::FileBackedTraceStore>, StorageSinkConfig)`
//!   constructs a traces-only sink. DELIVER adds the
//!   `trace_store: Option<Arc<...>>` field non-breakingly; the logs
//!   path (slice 01) keeps working with just the log store wired, and
//!   the traces path works with just the trace store wired.
//! - `StorageSinkConfig` is unchanged from slice 01
//!   (`with_default_tenant` / `no_default_tenant`).
//!
//! If DELIVER chooses a combined builder taking both handles, it must
//! keep an equivalent traces-only entry so this slice stays
//! independently shippable.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use ray::{
    FileBackedTraceStore, NoopRecorder, ServiceName, SpanKind, StatusCode, TimeRange, TraceId,
    TraceStore,
};

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{
    span as proto_span, status as proto_status, ResourceSpans, ScopeSpans, Span, Status,
};

use aperture::ports::{OtlpSink, Probe, SinkRecord, TenantScoped};

use aperture_storage_sink::{StorageSink, StorageSinkConfig};

/// Wrap a payload with a fixed test tenant for the post-ADR-0068
/// `TenantScoped` `SinkRecord` shape (aegis-ingest-auth-v0).
fn scoped<T>(inner: T) -> TenantScoped<T> {
    TenantScoped::new(TenantId("acme-prod".to_string()), inner)
}

// =========================================================================
// Tempdir helper — mirrors the slice-01 shape (temp_base + cleanup),
// pointing at a "ray" pillar root rather than "lumen".
// =========================================================================

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("aperture-storage-sink-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("ray");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn open_trace_store(base: &Path) -> Arc<FileBackedTraceStore> {
    Arc::new(FileBackedTraceStore::open(base, Box::new(NoopRecorder)).expect("open ray store"))
}

// =========================================================================
// OTLP ExportTraceServiceRequest builder — hand-crafted from the real
// upstream opentelemetry-proto types, matching the shape an OTel SDK
// emits (one ResourceSpans, one ScopeSpans, N Spans). The resource
// carries service.name and optionally tenant.id.
//
// Wire shapes the translator must handle (asserted across the
// scenarios below):
//   - trace_id is a 16-byte vec, span_id / parent_span_id are 8-byte
//     vecs. An empty parent_span_id means "trace root" (-> None).
//   - kind is the OTLP SpanKind enum as i32 (Server = 2, Client = 3).
//   - status is an Option<Status> carrying a StatusCode enum as i32
//     (Unset = 0, Ok = 1, Error = 2) plus a message string.
//   - events / links are repeated nested messages with their own
//     byte-array identifiers and attribute lists.
// =========================================================================

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

/// A status with the supplied code and message, as an SDK emits it.
fn status(code: proto_status::StatusCode, message: &str) -> Status {
    Status {
        message: message.to_string(),
        code: code as i32,
    }
}

/// One proto `Span` with explicit fields. The single builder the
/// scenarios use; defaults (trace_state, flags, dropped counts) are
/// fixed at the empty / zero values an SDK emits for a simple span.
#[allow(clippy::too_many_arguments)]
fn proto_span(
    trace_id: Vec<u8>,
    span_id: Vec<u8>,
    parent_span_id: Vec<u8>,
    name: &str,
    kind: proto_span::SpanKind,
    start: u64,
    end: u64,
    status: Option<Status>,
    attributes: Vec<KeyValue>,
    events: Vec<proto_span::Event>,
    links: Vec<proto_span::Link>,
) -> Span {
    Span {
        trace_id,
        span_id,
        trace_state: String::new(),
        parent_span_id,
        flags: 0,
        name: name.to_string(),
        kind: kind as i32,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        attributes,
        dropped_attributes_count: 0,
        events,
        dropped_events_count: 0,
        links,
        dropped_links_count: 0,
        status,
    }
}

/// Build an `ExportTraceServiceRequest` for one service, with the given
/// resource attributes folded in and the supplied proto spans on one
/// ScopeSpans.
fn traces_request(
    service_name: &str,
    extra_resource_attrs: Vec<KeyValue>,
    spans: Vec<Span>,
) -> ExportTraceServiceRequest {
    let mut resource_attrs = vec![string_kv("service.name", service_name)];
    resource_attrs.extend(extra_resource_attrs);

    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "aperture-storage-sink.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

// Stable identifier bytes reused across scenarios. A 16-byte trace id
// and 8-byte span ids, exactly the on-the-wire widths.
const TRACE: [u8; 16] = [0xAB; 16];
const ROOT_SPAN: [u8; 8] = [0x01; 8];
const CHILD_SPAN: [u8; 8] = [0x02; 8];

/// The canonical slice-02 payload: a root server span "POST /orders"
/// (no parent, status Ok) at 1716240000000000000 ns for checkout-api,
/// carrying one span attribute http.route="/orders" (US-02 domain
/// example 1, root half).
fn checkout_root_span() -> Span {
    proto_span(
        TRACE.to_vec(),
        ROOT_SPAN.to_vec(),
        vec![], // root: empty parent -> None
        "POST /orders",
        proto_span::SpanKind::Server,
        1_716_240_000_000_000_000,
        1_716_240_000_500_000_000,
        Some(status(proto_status::StatusCode::Ok, "")),
        vec![string_kv("http.route", "/orders")],
        vec![],
        vec![],
    )
}

/// The child client span "charge-card" whose parent is the root span
/// id (US-02 domain example 1, child half).
fn checkout_child_span() -> Span {
    proto_span(
        TRACE.to_vec(),
        CHILD_SPAN.to_vec(),
        ROOT_SPAN.to_vec(),
        "charge-card",
        proto_span::SpanKind::Client,
        1_716_240_000_100_000_000,
        1_716_240_000_400_000_000,
        Some(status(proto_status::StatusCode::Ok, "")),
        vec![],
        vec![],
        vec![],
    )
}

// =========================================================================
// Walking skeleton — the operator sends a trace and later finds it in ray
// =========================================================================

// @walking_skeleton @driving_port @real-io @adapter-integration @US-02
//
// Strategy: real local filesystem adapter (FileBackedTraceStore over a
// tmp dir). If the real adapter were deleted this skeleton could not
// pass, so it proves wiring, not an in-memory double. The user goal:
// Priya exports a root span and later queries the trace back, with the
// mapped fields faithful.
#[tokio::test]
async fn operator_exports_a_trace_and_finds_it_in_ray() {
    let base = temp_base("ws_export_and_find");
    let store = open_trace_store(&base);

    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = traces_request("checkout-api", vec![], vec![checkout_root_span()]);
    sink.accept(SinkRecord::Traces(scoped(req)))
        .await
        .expect("the gateway accepts the trace");

    let found = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get the trace out of ray for acme");

    assert_eq!(found.len(), 1, "exactly the one exported span is queryable");
    assert_eq!(found[0].name, "POST /orders");
    assert_eq!(found[0].service_name(), "checkout-api");

    cleanup(&base);
}

// =========================================================================
// Faithful translation — every mapped field round-trips through accept
// =========================================================================

// @driving_port @US-02
//
// Asserts the field-by-field translation contract: trace id, span id,
// parent (None for a root), name, kind (OTLP Server -> ray::Server),
// start / end times, status (OTLP Ok -> ray::StatusCode::Ok), span
// attribute fold and resource service.name fold.
#[tokio::test]
async fn persisted_span_faithfully_reflects_the_translated_fields() {
    let base = temp_base("faithful_translation");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = traces_request("checkout-api", vec![], vec![checkout_root_span()]);
    sink.accept(SinkRecord::Traces(scoped(req)))
        .await
        .expect("accept the checkout-api root span");

    let found = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace");
    assert_eq!(found.len(), 1);
    let span = &found[0];

    assert_eq!(span.trace_id, TraceId(TRACE));
    assert_eq!(span.span_id.0, ROOT_SPAN);
    assert_eq!(span.parent_span_id, None, "an empty parent maps to a root");
    assert_eq!(span.name, "POST /orders");
    assert_eq!(
        span.kind,
        SpanKind::Server,
        "OTLP Server maps to ray Server"
    );
    assert_eq!(span.start_time_unix_nano, 1_716_240_000_000_000_000);
    assert_eq!(span.end_time_unix_nano, 1_716_240_000_500_000_000);
    assert_eq!(span.status.code, StatusCode::Ok, "OTLP Ok maps to ray Ok");
    assert_eq!(
        span.attributes.get("http.route").map(String::as_str),
        Some("/orders"),
        "span-level attribute is folded through",
    );
    assert_eq!(
        span.resource_attributes
            .get("service.name")
            .map(String::as_str),
        Some("checkout-api"),
        "resource service.name is folded through",
    );

    cleanup(&base);
}

// @driving_port @US-02
//
// The two-span trace (root server + child client) persists both spans,
// preserving the parent relationship and the distinct kinds. Guards
// against a translator that only ever maps the first span and against
// dropping the parent link. (US-02 domain example 1.)
#[tokio::test]
async fn a_two_span_trace_persists_both_spans_with_the_parent_intact() {
    let base = temp_base("two_span_trace");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = traces_request(
        "checkout-api",
        vec![],
        vec![checkout_root_span(), checkout_child_span()],
    );
    sink.accept(SinkRecord::Traces(scoped(req)))
        .await
        .expect("accept the two-span trace");

    let found = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace");
    assert_eq!(found.len(), 2, "both spans of the trace persist");
    // Stores return ascending start-time order: root (000) then child (100).
    let root = &found[0];
    let child = &found[1];

    assert_eq!(root.name, "POST /orders");
    assert_eq!(root.kind, SpanKind::Server);
    assert_eq!(root.parent_span_id, None);

    assert_eq!(child.name, "charge-card");
    assert_eq!(child.kind, SpanKind::Client);
    assert_eq!(
        child.parent_span_id.map(|id| id.0),
        Some(ROOT_SPAN),
        "the child's parent is the root span id",
    );

    cleanup(&base);
}

// @driving_port @US-02
//
// A "process-payment" span carrying one event "retry-attempted" and one
// link to a span in another trace round-trips with the event
// name / timestamp / attribute and the link trace id / span id faithful.
// This is the richest part of the mapping (US-02 domain example 2).
#[tokio::test]
async fn span_events_and_links_are_persisted_faithfully() {
    let base = temp_base("events_and_links");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let linked_trace = [0xCD; 16];
    let linked_span = [0xEF; 8];

    let event = proto_span::Event {
        time_unix_nano: 1_716_240_000_250_000_000,
        name: "retry-attempted".to_string(),
        attributes: vec![string_kv("attempt", "2")],
        dropped_attributes_count: 0,
    };
    let link = proto_span::Link {
        trace_id: linked_trace.to_vec(),
        span_id: linked_span.to_vec(),
        trace_state: String::new(),
        attributes: vec![string_kv("link.kind", "follows_from")],
        dropped_attributes_count: 0,
        flags: 0,
    };

    let span = proto_span(
        TRACE.to_vec(),
        ROOT_SPAN.to_vec(),
        vec![],
        "process-payment",
        proto_span::SpanKind::Internal,
        1_716_240_000_000_000_000,
        1_716_240_000_500_000_000,
        Some(status(proto_status::StatusCode::Ok, "")),
        vec![],
        vec![event],
        vec![link],
    );

    let req = traces_request("billing-worker", vec![], vec![span]);
    sink.accept(SinkRecord::Traces(scoped(req)))
        .await
        .expect("accept the span with an event and a link");

    let found = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace");
    assert_eq!(found.len(), 1);
    let span = &found[0];

    assert_eq!(span.events.len(), 1, "the event persists");
    assert_eq!(span.events[0].name, "retry-attempted");
    assert_eq!(span.events[0].time_unix_nano, 1_716_240_000_250_000_000);
    assert_eq!(
        span.events[0].attributes.get("attempt").map(String::as_str),
        Some("2"),
    );

    assert_eq!(span.links.len(), 1, "the link persists");
    assert_eq!(
        span.links[0].trace_id,
        TraceId(linked_trace),
        "the link carries the linked trace id",
    );
    assert_eq!(
        span.links[0].span_id.0, linked_span,
        "the link carries the linked span id",
    );

    cleanup(&base);
}

// =========================================================================
// Durability — persisted spans survive a gateway restart (KPI-2)
// =========================================================================

// @real-io @adapter-integration @US-02
//
// Accept through the sink, drop the store, reopen the
// FileBackedTraceStore at the same pillar_root, and the trace is still
// queryable, identical. KPI-2 durability promise: 100% of accepted
// spans queryable post-restart with faithful structure.
#[tokio::test]
async fn persisted_traces_survive_a_gateway_restart() {
    let base = temp_base("durability_restart");

    {
        let store = open_trace_store(&base);
        let sink = StorageSink::with_trace_store(
            Arc::clone(&store),
            StorageSinkConfig::with_default_tenant("acme"),
        );
        let req = traces_request(
            "checkout-api",
            vec![],
            vec![checkout_root_span(), checkout_child_span()],
        );
        sink.accept(SinkRecord::Traces(scoped(req)))
            .await
            .expect("accept before restart");
        // sink and store dropped here, simulating process exit.
    }

    // Reopen against the same pillar_root, as a restarted process would.
    let reopened = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let found = reopened
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace after restart");

    assert_eq!(found.len(), 2, "both spans survived the restart");
    assert_eq!(found[0].name, "POST /orders");
    assert_eq!(found[1].name, "charge-card");
    assert_eq!(
        found[1].parent_span_id.map(|id| id.0),
        Some(ROOT_SPAN),
        "the parent relationship survives the restart",
    );

    cleanup(&base);
}

// =========================================================================
// Tenant resolution (DD3 / ADR-0041 Decision 2)
// =========================================================================

// @driving_port @US-02
//
// (a) An explicit tenant.id resource attribute wins over default_tenant.
// The span files under globex; acme returns nothing.
#[tokio::test]
async fn explicit_tenant_id_attribute_overrides_the_default_tenant() {
    let base = temp_base("tenant_explicit");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = traces_request(
        "billing-worker",
        vec![string_kv("tenant.id", "globex")],
        vec![checkout_root_span()],
    );
    sink.accept(SinkRecord::Traces(scoped(req)))
        .await
        .expect("accept with explicit tenant");

    let globex = store
        .get_trace(&tenant("globex"), &TraceId(TRACE))
        .expect("get_trace globex");
    let acme = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace acme");

    assert_eq!(globex.len(), 1, "filed under the explicit tenant.id");
    assert_eq!(globex[0].name, "POST /orders");
    assert!(acme.is_empty(), "nothing leaks into the default tenant");

    cleanup(&base);
}

// @driving_port @US-02
//
// (b) No tenant.id, but the sink is configured with a default_tenant:
// the span files under the default.
#[tokio::test]
async fn missing_tenant_id_falls_back_to_the_configured_default_tenant() {
    let base = temp_base("tenant_default");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    // checkout_root_span carries no tenant.id attribute.
    let req = traces_request("checkout-api", vec![], vec![checkout_root_span()]);
    sink.accept(SinkRecord::Traces(scoped(req)))
        .await
        .expect("accept under default tenant");

    let found = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace");
    assert_eq!(found.len(), 1, "filed under the configured default tenant");
    assert_eq!(found[0].name, "POST /orders");

    cleanup(&base);
}

// @driving_port @US-02
//
// (c) No tenant.id AND no default_tenant configured: the span is refused
// (Err) and NOTHING is written. KPI-5 guardrail — refused implies writes
// nothing, never mis-filed. We probe a couple of plausible tenants to
// assert the store is genuinely empty.
#[tokio::test]
async fn a_trace_with_no_resolvable_tenant_is_refused_and_writes_nothing() {
    let base = temp_base("tenant_unresolvable");
    let store = open_trace_store(&base);
    let sink =
        StorageSink::with_trace_store(Arc::clone(&store), StorageSinkConfig::no_default_tenant());

    let req = traces_request("checkout-api", vec![], vec![checkout_root_span()]);
    let result = sink.accept(SinkRecord::Traces(scoped(req))).await;

    assert!(
        result.is_err(),
        "an unresolvable tenant must be refused, not silently dropped",
    );

    // Nothing was written under any plausible tenant.
    for candidate in ["acme", "checkout-api", "default", ""] {
        let leaked = store
            .get_trace(&tenant(candidate), &TraceId(TRACE))
            .expect("get_trace candidate tenant");
        assert!(
            leaked.is_empty(),
            "refused span must not be filed under tenant {candidate:?}",
        );
    }

    cleanup(&base);
}

// =========================================================================
// Atomic translation (DD7 / ADR-0041 Decision 1) — a malformed
// byte-array identifier refuses the whole accept and writes nothing.
// =========================================================================

// @driving_port @US-02
//
// A span with a non-16-byte trace_id is untranslatable. Translation is
// all-or-nothing per accept, so even the otherwise-valid sibling span in
// the same batch must not be persisted. Accepted => fully translated =>
// persisted; otherwise nothing. (US-02 domain example 3.)
#[tokio::test]
async fn a_span_with_a_malformed_trace_id_refuses_the_whole_batch() {
    let base = temp_base("malformed_trace_id");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let good = checkout_root_span(); // valid 16-byte trace id
    let bad = proto_span(
        vec![0x11; 7], // seven bytes -> not a valid trace id
        CHILD_SPAN.to_vec(),
        vec![],
        "charge-card",
        proto_span::SpanKind::Client,
        200,
        300,
        Some(status(proto_status::StatusCode::Ok, "")),
        vec![],
        vec![],
        vec![],
    );

    let req = traces_request("checkout-api", vec![], vec![good, bad]);
    let result = sink.accept(SinkRecord::Traces(scoped(req))).await;

    assert!(
        result.is_err(),
        "a wrong-length trace id refuses the accept"
    );

    let found = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace");
    assert!(
        found.is_empty(),
        "atomic translation: no span from a refused batch is persisted",
    );

    cleanup(&base);
}

// @driving_port @US-02
//
// A non-8-byte span_id is equally untranslatable and refuses the whole
// accept, writing nothing. Guards the span-id half of the length check
// (the trace-id case above guards only the trace-id half).
#[tokio::test]
async fn a_span_with_a_malformed_span_id_refuses_the_whole_batch() {
    let base = temp_base("malformed_span_id");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let good = checkout_root_span();
    let bad = proto_span(
        TRACE.to_vec(),
        vec![0x22; 5], // five bytes -> not a valid span id
        vec![],
        "charge-card",
        proto_span::SpanKind::Client,
        200,
        300,
        Some(status(proto_status::StatusCode::Ok, "")),
        vec![],
        vec![],
        vec![],
    );

    let req = traces_request("checkout-api", vec![], vec![good, bad]);
    let result = sink.accept(SinkRecord::Traces(scoped(req))).await;

    assert!(result.is_err(), "a wrong-length span id refuses the accept");

    let found = store
        .get_trace(&tenant("acme"), &TraceId(TRACE))
        .expect("get_trace");
    assert!(
        found.is_empty(),
        "atomic translation: no span from a refused batch is persisted",
    );

    cleanup(&base);
}

// @driving_port @US-02
//
// A valid 16-byte trace id and 8-byte span ids translate and round-trip
// queryable by service name as well as by trace id. Guards the happy
// side of the length check and the by-service index path.
#[tokio::test]
async fn a_well_formed_trace_is_queryable_by_service_name() {
    let base = temp_base("queryable_by_service");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = traces_request("checkout-api", vec![], vec![checkout_root_span()]);
    sink.accept(SinkRecord::Traces(scoped(req)))
        .await
        .expect("accept well-formed trace");

    let by_service = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout-api"),
            TimeRange::all(),
        )
        .expect("query by service");
    assert_eq!(by_service.len(), 1, "the span is queryable by service name");
    assert_eq!(by_service[0].name, "POST /orders");

    cleanup(&base);
}

// =========================================================================
// Probe (DD5 / Earned-Trust) — startup health check
// =========================================================================

// @driving_port @adapter-integration @US-02
//
// Against a writable pillar_root the probe returns Ok: the trace store
// opened and an active write check succeeds.
#[tokio::test]
async fn probe_returns_ok_when_the_pillar_root_is_writable() {
    let base = temp_base("probe_ok");
    let store = open_trace_store(&base);
    let sink = StorageSink::with_trace_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    sink.probe()
        .await
        .expect("probe Ok against a writable trace store");

    cleanup(&base);
}

// @infrastructure-failure @real-io @adapter-integration @US-02
//
// Against a read-only pillar_root the probe must return Err: the
// catalogued substrate lie is "the path opens but is not writable". The
// host binary refuses to start in that case (wire then probe then use).
// Skipped on platforms where the chmod-based read-only setup does not
// take effect (e.g. running as root, or filesystems that ignore the
// permission bits) so the test stays meaningful rather than flaky.
#[cfg(unix)]
#[tokio::test]
async fn probe_returns_err_when_the_pillar_root_is_not_writable() {
    use std::os::unix::fs::PermissionsExt;

    let base = temp_base("probe_readonly");
    // Open once while writable so the snapshot / WAL exist, then drop.
    drop(open_trace_store(&base));

    // Make the containing directory read-only so a probe write fails.
    let parent = base.parent().expect("base has a parent dir").to_path_buf();
    let mut perms = fs::metadata(&parent).expect("metadata").permissions();
    let original_mode = perms.mode();
    perms.set_mode(0o500); // r-x------ : not writable
    fs::set_permissions(&parent, perms).expect("set read-only");

    // If we can still create a file, the read-only bit did not take
    // (running as root or a permissive fs); restore and skip the assertion.
    let writable_anyway = fs::write(parent.join(".probe-write-check"), b"x").is_ok();
    if writable_anyway {
        let _ = fs::remove_file(parent.join(".probe-write-check"));
        let mut restore = fs::metadata(&parent).expect("metadata").permissions();
        restore.set_mode(original_mode);
        let _ = fs::set_permissions(&parent, restore);
        cleanup(&base);
        eprintln!("skipping read-only probe assertion: directory remained writable");
        return;
    }

    // Reopening against the read-only path either fails to open or the
    // sink's probe fails the active write check. Either way the operator
    // cannot trust this pillar_root, which is what we assert.
    match FileBackedTraceStore::open(&base, Box::new(NoopRecorder)) {
        Ok(store) => {
            let sink = StorageSink::with_trace_store(
                Arc::new(store),
                StorageSinkConfig::with_default_tenant("acme"),
            );
            assert!(
                sink.probe().await.is_err(),
                "probe must refuse a non-writable pillar_root",
            );
        }
        Err(_) => {
            // Open itself refused the unwritable path: also an acceptable
            // wire-then-probe-then-use refusal.
        }
    }

    // Restore permissions so cleanup can remove the tree.
    let mut restore = fs::metadata(&parent).expect("metadata").permissions();
    restore.set_mode(original_mode);
    let _ = fs::set_permissions(&parent, restore);
    cleanup(&base);
}
