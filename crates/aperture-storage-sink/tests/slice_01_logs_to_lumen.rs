// Kaleidoscope aperture-storage-sink — slice 01 acceptance test
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

//! Slice 01 — logs persist to lumen end to end.
//!
//! Maps to `docs/feature/aperture-storage-sink-v0/slices/slice-01-logs-to-lumen.md`.
//! Story: US-01. Decisions: DD1 (third OtlpSink + Probe), DD3 (tenant
//! resolution tenant.id -> default_tenant -> refuse), DD4 (sink holds
//! Arc<FileBackedLogStore>), DD5 (probe is an active write check), DD7
//! (atomic translation, accepted => persisted, refused => writes
//! nothing). ADR-0041 Decisions 1 and 2.
//!
//! These tests enter through the real aperture driving port
//! `OtlpSink::accept` and the `Probe::probe` contract. The observable
//! outcome is what an operator can later query out of lumen
//! (`LogStore::query`). Nothing internal to the translator is invoked
//! directly: the OTLP `ExportLogsServiceRequest` goes in at the port,
//! and the persisted `lumen::LogRecord`s come out at the store.
//!
//! ## RED-gate boundary
//!
//! The `aperture-storage-sink` crate and its `StorageSink` /
//! `StorageSinkConfig` do not exist yet. This file imports those
//! not-yet-existing symbols deliberately: the compile error against
//! `aperture_storage_sink::{StorageSink, StorageSinkConfig}` is the
//! RED state for the classic Rust outside-in loop. DELIVER creates the
//! implementation and these tests turn GREEN, committed atomic with the
//! slice.
//!
//! ## Assumed StorageSink surface (DELIVER must match)
//!
//! The DESIGN (DD4) pins that the sink holds `Arc<FileBacked*Store>`
//! handles plus a config carrying `default_tenant`, but leaves the
//! exact constructor name open. This slice is logs-only, so the tests
//! assume the smallest honest constructor:
//!
//! - `StorageSinkConfig { default_tenant: Option<String> }` with a
//!   convenience builder `StorageSinkConfig::with_default_tenant(s)`
//!   and `StorageSinkConfig::no_default_tenant()`.
//! - `StorageSink::with_log_store(Arc<FileBackedLogStore>, StorageSinkConfig)`
//!   constructs a logs-only sink. DELIVER adds `with_trace_store` /
//!   `with_metric_store` (or a builder) for slices 02 / 03; the logs
//!   path must work with just the log store wired.
//!
//! If DELIVER chooses a different shape (e.g. a single builder taking
//! all three handles), it must keep an equivalent logs-only entry so
//! this slice stays independently shippable.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use lumen::{FileBackedLogStore, LogStore, NoopRecorder, TimeRange};

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;

use aperture::ports::{OtlpSink, Probe, SinkRecord};

use aperture_storage_sink::{StorageSink, StorageSinkConfig};

// =========================================================================
// Tempdir helper — mirrors the lumen v1 test shape (temp_base + cleanup),
// re-prefixed for this crate.
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
    path.push("lumen");
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

fn open_log_store(base: &Path) -> Arc<FileBackedLogStore> {
    Arc::new(FileBackedLogStore::open(base, Box::new(NoopRecorder)).expect("open lumen store"))
}

// =========================================================================
// OTLP ExportLogsServiceRequest builder — hand-crafted from the real
// upstream opentelemetry-proto types, matching the shape an OTel SDK
// emits (one ResourceLogs, one ScopeLogs, N LogRecords). The body is a
// String AnyValue; the resource carries service.name and optionally
// tenant.id.
// =========================================================================

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

/// One proto `LogRecord` with a string body and a string attribute.
fn proto_log_record(
    observed: u64,
    severity_number: i32,
    severity_text: &str,
    body: &str,
    attr_key: &str,
    attr_value: &str,
) -> LogRecord {
    LogRecord {
        time_unix_nano: observed,
        observed_time_unix_nano: observed,
        severity_number,
        severity_text: severity_text.to_string(),
        body: Some(AnyValue {
            value: Some(any_value::Value::StringValue(body.to_string())),
        }),
        attributes: vec![string_kv(attr_key, attr_value)],
        dropped_attributes_count: 0,
        flags: 0,
        trace_id: vec![],
        span_id: vec![],
    }
}

/// Build an `ExportLogsServiceRequest` for one service, with the given
/// resource attributes folded in and the supplied proto log records.
fn logs_request(
    service_name: &str,
    extra_resource_attrs: Vec<KeyValue>,
    records: Vec<LogRecord>,
) -> ExportLogsServiceRequest {
    let mut resource_attrs = vec![string_kv("service.name", service_name)];
    resource_attrs.extend(extra_resource_attrs);

    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "aperture-storage-sink.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// The canonical slice-01 payload: the "order 1001 placed" INFO log
/// for checkout-api, observed at 1716240000000000000 ns. severity INFO
/// is SeverityNumber(9) on the wire (per US-01 domain example 1).
fn checkout_api_info_log(extra_resource_attrs: Vec<KeyValue>) -> ExportLogsServiceRequest {
    logs_request(
        "checkout-api",
        extra_resource_attrs,
        vec![proto_log_record(
            1_716_240_000_000_000_000,
            9,
            "INFO",
            "order 1001 placed",
            "order.id",
            "1001",
        )],
    )
}

// =========================================================================
// Walking skeleton — the operator sends a log and later finds it in lumen
// =========================================================================

// @walking_skeleton @driving_port @US-01
//
// Strategy: real local filesystem adapter (FileBackedLogStore over a
// tmp dir). If the real adapter were deleted this skeleton could not
// pass, so it proves wiring, not an in-memory double. The user goal:
// Priya exports a log and later queries it back, field-faithful.
#[tokio::test]
async fn operator_exports_a_log_and_finds_it_in_lumen() {
    let base = temp_base("ws_export_and_find");
    let store = open_log_store(&base);

    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = checkout_api_info_log(vec![]);
    sink.accept(SinkRecord::Logs(req))
        .await
        .expect("the gateway accepts the log");

    let found = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query lumen for acme");

    assert_eq!(found.len(), 1, "exactly the one exported log is queryable");
    assert_eq!(found[0].body, "order 1001 placed");
    assert_eq!(
        found[0]
            .resource_attributes
            .get("service.name")
            .map(String::as_str),
        Some("checkout-api"),
    );

    cleanup(&base);
}

// =========================================================================
// Faithful translation — every mapped field round-trips through accept
// =========================================================================

// @driving_port @US-01
//
// Asserts the field-by-field translation contract (arch section 6.1):
// observed time, severity number + text, body (AnyValue String ->
// String), record attributes fold, and resource service.name fold.
#[tokio::test]
async fn persisted_log_faithfully_reflects_the_translated_fields() {
    let base = temp_base("faithful_translation");
    let store = open_log_store(&base);
    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = checkout_api_info_log(vec![]);
    sink.accept(SinkRecord::Logs(req))
        .await
        .expect("accept the checkout-api log");

    let found = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 1);
    let record = &found[0];

    assert_eq!(record.observed_time_unix_nano, 1_716_240_000_000_000_000);
    assert_eq!(record.severity_number.0, 9, "INFO is severity number 9");
    assert_eq!(record.severity_text, "INFO");
    assert_eq!(record.body, "order 1001 placed");
    assert_eq!(
        record.attributes.get("order.id").map(String::as_str),
        Some("1001"),
        "record-level attribute is folded through",
    );
    assert_eq!(
        record
            .resource_attributes
            .get("service.name")
            .map(String::as_str),
        Some("checkout-api"),
        "resource service.name is folded through",
    );

    cleanup(&base);
}

// @driving_port @US-01
//
// Two records on one ScopeLogs both persist, preserving their distinct
// bodies, severities and timestamps. Guards against a translator that
// only ever maps the first record.
#[tokio::test]
async fn a_batch_of_two_logs_persists_both_records() {
    let base = temp_base("two_records");
    let store = open_log_store(&base);
    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = logs_request(
        "checkout-api",
        vec![],
        vec![
            proto_log_record(100, 9, "INFO", "order 1001 placed", "order.id", "1001"),
            proto_log_record(200, 17, "ERROR", "payment declined", "order.id", "1002"),
        ],
    );
    sink.accept(SinkRecord::Logs(req))
        .await
        .expect("accept two-record batch");

    let found = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 2, "both records persist");
    // Stores return ascending observed-time order.
    assert_eq!(found[0].body, "order 1001 placed");
    assert_eq!(found[0].severity_text, "INFO");
    assert_eq!(found[1].body, "payment declined");
    assert_eq!(found[1].severity_text, "ERROR");
    assert_eq!(found[1].severity_number.0, 17);

    cleanup(&base);
}

// =========================================================================
// Durability — persisted logs survive a gateway restart
// =========================================================================

// @real-io @adapter-integration @US-01
//
// Accept through the sink, drop the store, reopen the FileBackedLogStore
// at the same pillar_root, and the log is still queryable, identical.
// This is the KPI-1 durability promise: 100% of accepted records
// queryable post-restart.
#[tokio::test]
async fn persisted_logs_survive_a_gateway_restart() {
    let base = temp_base("durability_restart");

    {
        let store = open_log_store(&base);
        let sink = StorageSink::with_log_store(
            Arc::clone(&store),
            StorageSinkConfig::with_default_tenant("acme"),
        );
        sink.accept(SinkRecord::Logs(checkout_api_info_log(vec![])))
            .await
            .expect("accept before restart");
        // sink and store dropped here, simulating process exit.
    }

    // Reopen against the same pillar_root, as a restarted process would.
    let reopened = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let found = reopened
        .query(&tenant("acme"), TimeRange::all())
        .expect("query after restart");

    assert_eq!(found.len(), 1, "the log survived the restart");
    assert_eq!(found[0].body, "order 1001 placed");
    assert_eq!(found[0].severity_text, "INFO");
    assert_eq!(
        found[0]
            .resource_attributes
            .get("service.name")
            .map(String::as_str),
        Some("checkout-api"),
    );

    cleanup(&base);
}

// =========================================================================
// Tenant resolution (DD3 / ADR-0041 Decision 2)
// =========================================================================

// @driving_port @US-01
//
// (a) An explicit tenant.id resource attribute wins over default_tenant.
// The record files under globex; acme returns nothing.
#[tokio::test]
async fn explicit_tenant_id_attribute_overrides_the_default_tenant() {
    let base = temp_base("tenant_explicit");
    let store = open_log_store(&base);
    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = logs_request(
        "billing-worker",
        vec![string_kv("tenant.id", "globex")],
        vec![proto_log_record(
            100,
            9,
            "INFO",
            "invoice issued",
            "invoice.id",
            "77",
        )],
    );
    sink.accept(SinkRecord::Logs(req))
        .await
        .expect("accept with explicit tenant");

    let globex = store
        .query(&tenant("globex"), TimeRange::all())
        .expect("query globex");
    let acme = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query acme");

    assert_eq!(globex.len(), 1, "filed under the explicit tenant.id");
    assert_eq!(globex[0].body, "invoice issued");
    assert!(acme.is_empty(), "nothing leaks into the default tenant");

    cleanup(&base);
}

// @driving_port @US-01
//
// (b) No tenant.id, but the sink is configured with a default_tenant:
// the record files under the default.
#[tokio::test]
async fn missing_tenant_id_falls_back_to_the_configured_default_tenant() {
    let base = temp_base("tenant_default");
    let store = open_log_store(&base);
    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    // checkout_api_info_log carries no tenant.id attribute.
    sink.accept(SinkRecord::Logs(checkout_api_info_log(vec![])))
        .await
        .expect("accept under default tenant");

    let found = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 1, "filed under the configured default tenant");
    assert_eq!(found[0].body, "order 1001 placed");

    cleanup(&base);
}

// @driving_port @US-01
//
// (c) No tenant.id AND no default_tenant configured: the record is
// refused (Err) and NOTHING is written. This is the KPI-5 guardrail —
// refused implies writes nothing, never mis-filed. We probe a couple of
// plausible tenant ids to assert the store is genuinely empty.
#[tokio::test]
async fn a_log_with_no_resolvable_tenant_is_refused_and_writes_nothing() {
    let base = temp_base("tenant_unresolvable");
    let store = open_log_store(&base);
    let sink =
        StorageSink::with_log_store(Arc::clone(&store), StorageSinkConfig::no_default_tenant());

    let result = sink
        .accept(SinkRecord::Logs(checkout_api_info_log(vec![])))
        .await;

    assert!(
        result.is_err(),
        "an unresolvable tenant must be refused, not silently dropped",
    );

    // Nothing was written under any plausible tenant.
    for candidate in ["acme", "checkout-api", "default", ""] {
        let leaked = store
            .query(&tenant(candidate), TimeRange::all())
            .expect("query candidate tenant");
        assert!(
            leaked.is_empty(),
            "refused record must not be filed under tenant {candidate:?}",
        );
    }

    cleanup(&base);
}

// =========================================================================
// Atomic translation (DD7 / ADR-0041 Decision 1) — a malformed
// byte-array identifier refuses the whole accept and writes nothing.
// =========================================================================

// @driving_port @US-01
//
// A log record with a non-16-byte trace_id is untranslatable. Translation
// is all-or-nothing per accept, so even the otherwise-valid sibling
// record in the same batch must not be persisted. Accepted => fully
// translated => persisted; otherwise nothing.
#[tokio::test]
async fn a_log_with_a_malformed_trace_id_refuses_the_whole_batch() {
    let base = temp_base("malformed_trace_id");
    let store = open_log_store(&base);
    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let mut good = proto_log_record(100, 9, "INFO", "order 1001 placed", "order.id", "1001");
    let mut bad = proto_log_record(200, 9, "INFO", "order 1002 placed", "order.id", "1002");
    good.trace_id = vec![]; // empty -> None, valid
    bad.trace_id = vec![0x11; 7]; // seven bytes -> not a valid trace id

    let req = logs_request("checkout-api", vec![], vec![good, bad]);
    let result = sink.accept(SinkRecord::Logs(req)).await;

    assert!(
        result.is_err(),
        "a wrong-length trace id refuses the accept"
    );

    let found = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    assert!(
        found.is_empty(),
        "atomic translation: no record from a refused batch is persisted",
    );

    cleanup(&base);
}

// @driving_port @US-01
//
// A valid 16-byte trace_id and 8-byte span_id translate to Some(..) and
// round-trip. Guards the happy side of the length check.
#[tokio::test]
async fn a_log_with_well_formed_trace_and_span_ids_persists_them() {
    let base = temp_base("well_formed_ids");
    let store = open_log_store(&base);
    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let mut record = proto_log_record(100, 9, "INFO", "order 1001 placed", "order.id", "1001");
    record.trace_id = vec![0xAB; 16];
    record.span_id = vec![0xCD; 8];

    let req = logs_request("checkout-api", vec![], vec![record]);
    sink.accept(SinkRecord::Logs(req))
        .await
        .expect("accept well-formed ids");

    let found = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].trace_id, Some([0xAB; 16]));
    assert_eq!(found[0].span_id, Some([0xCD; 8]));

    cleanup(&base);
}

// =========================================================================
// Probe (DD5 / Earned-Trust) — startup health check
// =========================================================================

// @driving_port @adapter-integration @US-01
//
// Against a writable pillar_root the probe returns Ok: the store opened
// and an empty probe-ingest succeeds.
#[tokio::test]
async fn probe_returns_ok_when_the_pillar_root_is_writable() {
    let base = temp_base("probe_ok");
    let store = open_log_store(&base);
    let sink = StorageSink::with_log_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    sink.probe()
        .await
        .expect("probe Ok against a writable store");

    cleanup(&base);
}

// @infrastructure-failure @real-io @adapter-integration @US-01
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
    // Open once while writable so the snapshot/WAL exist, then drop.
    drop(open_log_store(&base));

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
    match FileBackedLogStore::open(&base, Box::new(NoopRecorder)) {
        Ok(store) => {
            let sink = StorageSink::with_log_store(
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
