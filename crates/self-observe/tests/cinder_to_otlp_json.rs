// Kaleidoscope self-observe — Cinder → OTLP-JSON acceptance test
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

//! Cross-process bridge: Cinder → OTLP-JSON NDJSON stream.
//!
//! Sibling of `lumen_to_otlp_json.rs` (the Lumen-side OTLP-JSON
//! writer) and `cinder_to_pulse.rs` (the in-process Pulse sink
//! for the same three Cinder events). The writer emits one line
//! of OTLP-JSON `ResourceMetrics` per Cinder `MetricsRecorder`
//! call: `record_place`, `record_migrate`, `record_evaluate`.
//!
//! Test seam (locked by ADR-0039 §3, DESIGN DD3): drive the
//! bridge through `cinder::InMemoryTieringStore`; capture the
//! emitted bytes through a `SharedBuf(Arc<Mutex<Vec<u8>>>)` sink
//! mirroring `tests/lumen_to_otlp_json.rs:54-64`; parse each line
//! as `serde_json::Value` and assert against the parsed tree.
//! Driving Cinder (rather than calling the writer directly) lets
//! Slice 03's dual-emission test exercise the
//! `InMemoryTieringStore::evaluate_at` cascade end-to-end.
//!
//! Per-event contract per ADR-0039 §2:
//!
//! - `record_place(tenant, tier)`     → metric `cinder.place.count`,
//!   `asInt = "1"`, point attrs `{tenant_id, tier}`
//! - `record_migrate(tenant, f, t)`   → metric `cinder.migrate.count`,
//!   `asInt = "1"`, point attrs `{tenant_id, from, to}`
//! - `record_evaluate(tenant, n)`     → metric `cinder.evaluate.migrated.count`,
//!   `asInt = n.to_string()`, point attrs `{tenant_id}`
//!
//! All scenarios run against a clean, in-memory environment per
//! `devops/environments.yaml` — no filesystem, no subprocess, no
//! network.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use aegis::TenantId;
use cinder::{InMemoryTieringStore, ItemId, MigrateError, Tier, TierPolicy, TieringStore};
use self_observe::CinderToOtlpJsonWriter;
use serde_json::Value;

// ---------- helpers (mirror lumen_to_otlp_json.rs + cinder_to_pulse.rs) ----

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn item(id: &str) -> ItemId {
    ItemId::new(id)
}

/// Shareable writer so the test can hold the buffer while
/// Cinder holds the recorder. `Arc<Mutex<Vec<u8>>>` implements
/// `Write` through this wrapper. Copied verbatim from
/// `tests/lumen_to_otlp_json.rs:54-64` per ADR-0039 §3 / DD3
/// (rule-of-three: extraction into `tests/common.rs` becomes
/// warranted when a third OTLP-JSON-writer test file lands).
#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn collect_lines(buf: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse otlp-json"))
        .collect()
}

/// Construct the standard test wiring: a shared in-memory byte
/// buffer (the sink), a `CinderToOtlpJsonWriter` wrapping that
/// buffer, and an `InMemoryTieringStore` whose recorder is the
/// writer. The buffer handle is returned for post-call inspection.
///
/// Mirrors the `wire()` helper in `tests/cinder_to_pulse.rs:64-69`,
/// adapted for the OTLP-JSON byte-sink seam instead of the Pulse
/// store seam.
fn wire() -> (Arc<Mutex<Vec<u8>>>, InMemoryTieringStore) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = CinderToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let cinder = InMemoryTieringStore::new(Box::new(writer));
    (buf, cinder)
}

/// Number of lines (in the captured NDJSON stream) whose
/// `scopeMetrics[0].metrics[0].name` matches the given metric.
fn count_with_metric_name(lines: &[Value], metric_name: &str) -> usize {
    lines
        .iter()
        .filter(|l| l["scopeMetrics"][0]["metrics"][0]["name"] == metric_name)
        .count()
}

/// Number of lines whose resource `tenant_id` matches the given
/// tenant AND whose `scopeMetrics[0].metrics[0].name` matches the
/// given metric. Asserts the per-tenant resource-attribute slot is
/// populated as ADR-0039 §2 specifies (one resource attribute:
/// `tenant_id`).
fn count_with_tenant_and_metric(lines: &[Value], tenant_id: &str, metric_name: &str) -> usize {
    lines
        .iter()
        .filter(|l| {
            l["resource"]["attributes"][0]["key"] == "tenant_id"
                && l["resource"]["attributes"][0]["value"]["stringValue"] == tenant_id
                && l["scopeMetrics"][0]["metrics"][0]["name"] == metric_name
        })
        .count()
}

/// Does any entry in the data point's `attributes` array match
/// the given key/string-value pair? OTLP-JSON allows the
/// attribute array order to vary; tests must assert on
/// set-containment, not on array index. Mirrors the same
/// looseness as Lumen's tests (which only have one attribute per
/// point, so the question does not arise there).
fn point_attrs_contain(line: &Value, key: &str, string_value: &str) -> bool {
    let dp = &line["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    let attrs = dp["attributes"].as_array().expect("attributes array");
    attrs
        .iter()
        .any(|a| a["key"] == key && a["value"]["stringValue"] == string_value)
}

// ======================================================================
//  Slice 01 — place events emit OTLP-JSON lines
//  Story: US-01  |  KPI: OK1 (+ OK4 guardrail, + OK5 guardrail)
//  Tag:  @infrastructure @US-01
// ======================================================================

#[test]
fn cinder_place_emits_one_otlp_resource_metrics_line_under_same_tenant() {
    // Given a freshly-wired CinderToOtlpJsonWriter capturing into
    //   an empty in-memory NDJSON sink, and tenant "acme" with no
    //   prior tier state
    let (buf, cinder) = wire();
    let acme = tenant("acme");

    // When cinder.place is called once for "acme"/Tier::Hot
    cinder
        .place(
            &acme,
            &item("trade-2026-05-18-001"),
            Tier::Hot,
            SystemTime::now(),
        )
        .expect("place");

    // Then exactly one OTLP-JSON line lands in the sink, with the
    //   full ResourceMetrics envelope shape from ADR-0039 §2.
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 1, "exactly one OTLP line emitted");
    let line = &lines[0];

    // Resource-level tenant attribute.
    assert_eq!(line["resource"]["attributes"][0]["key"], "tenant_id");
    assert_eq!(
        line["resource"]["attributes"][0]["value"]["stringValue"],
        "acme"
    );

    // Scope name (parallel to lumen_otlp_json.rs:138).
    assert_eq!(
        line["scopeMetrics"][0]["scope"]["name"],
        "kaleidoscope.cinder"
    );

    // Metric name (cross-bridge contract with cinder_bridge.rs:121).
    assert_eq!(
        line["scopeMetrics"][0]["metrics"][0]["name"],
        "cinder.place.count"
    );

    // Sum envelope (cumulative, monotonic — ADR-0039 §2).
    let sum = &line["scopeMetrics"][0]["metrics"][0]["sum"];
    assert_eq!(sum["isMonotonic"], true);
    assert_eq!(sum["aggregationTemporality"], 2);

    // Data point: asInt="1", tenant_id+tier point attributes,
    //   timeUnixNano parses as u64.
    let dp = &sum["dataPoints"][0];
    assert_eq!(dp["asInt"], "1");
    assert!(
        point_attrs_contain(line, "tenant_id", "acme"),
        "point attributes must contain tenant_id=acme"
    );
    assert!(
        point_attrs_contain(line, "tier", "hot"),
        "point attributes must contain tier=hot"
    );
    let time_str = dp["timeUnixNano"].as_str().expect("string");
    assert!(
        time_str.parse::<u64>().is_ok(),
        "timeUnixNano is uint64 string"
    );
}

#[test]
fn cinder_place_serialises_each_tier_as_lowercase_string() {
    // Given a freshly-wired writer and tenant "acme"
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let now = SystemTime::now();

    // When three places land for Hot, Warm, Cold (distinct items,
    //   same tenant)
    cinder
        .place(&acme, &item("trade-001"), Tier::Hot, now)
        .expect("place");
    cinder
        .place(&acme, &item("trade-002"), Tier::Warm, now)
        .expect("place");
    cinder
        .place(&acme, &item("trade-003"), Tier::Cold, now)
        .expect("place");

    // Then exactly three lines exist, all with metric
    //   "cinder.place.count", and the set of tier point-attribute
    //   values is exactly {"hot","warm","cold"} (lowercase). The
    //   set form is robust to line-order variation if Cinder ever
    //   batches; today it does not, but the contract is on the
    //   set, not the order.
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 3, "three place events recorded");
    assert_eq!(
        count_with_metric_name(&lines, "cinder.place.count"),
        3,
        "every line has metric cinder.place.count"
    );

    let observed_tiers: std::collections::BTreeSet<String> = lines
        .iter()
        .filter_map(|l| {
            l["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]["attributes"]
                .as_array()
                .and_then(|attrs| attrs.iter().find(|a| a["key"] == "tier"))
                .and_then(|a| a["value"]["stringValue"].as_str().map(String::from))
        })
        .collect();
    let expected_tiers: std::collections::BTreeSet<String> = ["hot", "warm", "cold"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(observed_tiers, expected_tiers);
}

#[test]
fn two_tenants_cinder_place_emit_distinct_otlp_resource_attributes() {
    // Given a freshly-wired writer
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let globex = tenant("globex");
    let now = SystemTime::now();

    // When one place lands for acme and two for globex
    cinder
        .place(&acme, &item("a1"), Tier::Hot, now)
        .expect("place");
    cinder
        .place(&globex, &item("g1"), Tier::Hot, now)
        .expect("place");
    cinder
        .place(&globex, &item("g2"), Tier::Hot, now)
        .expect("place");

    // Then exactly three lines exist, partitioned by tenant on the
    //   resource attribute: 1 for acme, 2 for globex. No
    //   cross-tenant leak.
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 3, "three place lines total");
    assert_eq!(
        count_with_tenant_and_metric(&lines, "acme", "cinder.place.count"),
        1,
        "exactly one line under acme"
    );
    assert_eq!(
        count_with_tenant_and_metric(&lines, "globex", "cinder.place.count"),
        2,
        "exactly two lines under globex"
    );
}

#[test]
fn no_cinder_event_means_zero_bytes_in_the_ndjson_sink() {
    // Given a freshly-wired writer
    let (buf, _cinder) = wire();

    // When nothing is called on Cinder

    // Then the sink contains zero bytes (and therefore zero lines).
    //   This is the cross-cutting quiescence assertion; it lives in
    //   Slice 01 by convention and covers all three metric names.
    assert!(
        buf.lock().unwrap().is_empty(),
        "no cinder call should yield zero bytes"
    );
    assert_eq!(collect_lines(&buf).len(), 0);
}

#[test]
fn output_is_ndjson_one_line_per_event_with_trailing_newline() {
    // OK5 — NDJSON-validity guardrail. The byte stream produced
    //   by the writer MUST satisfy three invariants per
    //   `devops/kpi-instrumentation.md` OK5:
    //
    //   1. every emitted line parses independently as JSON,
    //   2. the stream terminates with `\n`,
    //   3. per-line atomicity holds when the writer is invoked
    //      multiple times in succession (no interleaving, no
    //      truncated lines, no missing terminators).
    //
    //   This is the substrate-lie probe per ADR-0039 §3 / Earned
    //   Trust Principle 12c behavioural-check layer. A regression
    //   that drops the `write_all(b"\n")` call, removes the
    //   `flush()`, or breaks the `Mutex<W>` critical section
    //   surfaces here first.
    //
    //   Mirror of the Lumen-side `output_is_ndjson_one_record_per_line_with_trailing_newline`
    //   (tests/lumen_to_otlp_json.rs:192-219).
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let now = SystemTime::now();

    // When three place events fire in sequence
    cinder
        .place(&acme, &item("trade-001"), Tier::Hot, now)
        .expect("place");
    cinder
        .place(&acme, &item("trade-002"), Tier::Warm, now)
        .expect("place");
    cinder
        .place(&acme, &item("trade-003"), Tier::Cold, now)
        .expect("place");

    // Then the byte stream is well-formed NDJSON:
    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    assert!(s.ends_with('\n'), "stream ends with newline");
    let raw_lines: Vec<&str> = s.lines().collect();
    assert_eq!(raw_lines.len(), 3, "exactly three lines, one per event");
    for line in raw_lines {
        // Every line is independently parseable JSON.
        let _: Value = serde_json::from_str(line).expect("each line is JSON");
    }
}

// ----- Cross-cutting compile-time probe -------------------------------
// Lives in this test file but covers all slices. Mirrors
// `tests/lumen_to_pulse.rs` and `tests/cinder_to_pulse.rs`'s
// equivalents. Subtype-check layer of the Earned Trust contract
// (Principle 12c, ADR-0039 §3).

#[test]
fn the_writer_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>();
}

// ======================================================================
//  Slice 02 — migrate events emit OTLP-JSON lines with direction
//  Story: US-02  |  KPI: OK2 (+ OK4 guardrail)
//  Tag:  @infrastructure @US-02
// ======================================================================

#[test]
fn cinder_migrate_emits_line_with_from_and_to_attributes() {
    // Given tenant acme has placed "trade-001" in Hot
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let id = item("trade-2026-05-18-001");
    let t0 = SystemTime::now();
    let t1 = t0 + Duration::from_secs(60);

    cinder.place(&acme, &id, Tier::Hot, t0).expect("place");

    // When the item is migrated to Warm and Cinder reports Ok(())
    let result = cinder.migrate(&acme, &id, Tier::Warm, t1);
    assert!(result.is_ok(), "successful migrate returns Ok");

    // Then two lines exist (one place + one migrate); the migrate
    //   line has metric "cinder.migrate.count", asInt="1", and
    //   point attributes containing from=hot AND to=warm.
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 2, "one place line + one migrate line");

    let migrate_lines: Vec<&Value> = lines
        .iter()
        .filter(|l| l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.migrate.count")
        .collect();
    assert_eq!(migrate_lines.len(), 1, "exactly one migrate line");
    let m = migrate_lines[0];

    // Resource tenant.
    assert_eq!(
        m["resource"]["attributes"][0]["value"]["stringValue"],
        "acme"
    );

    // Value and direction attributes (set-containment, not
    //   array-index, per slice-02.md risk row).
    assert_eq!(
        m["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]["asInt"],
        "1"
    );
    assert!(
        point_attrs_contain(m, "from", "hot"),
        "migrate point attrs must contain from=hot"
    );
    assert!(
        point_attrs_contain(m, "to", "warm"),
        "migrate point attrs must contain to=warm"
    );
}

#[test]
fn failed_cinder_migrate_emits_no_otlp_line() {
    // Given tenant acme has placed nothing (so any migrate must
    //   fail with UnknownItem)
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let t1 = SystemTime::now();

    // When the operator attempts to migrate a ghost item
    let result = cinder.migrate(&acme, &item("ghost"), Tier::Warm, t1);

    // Then (OK4 guardrail) Cinder's user-facing API returns
    //   Err(UnknownItem) unchanged from its NoopRecorder behaviour
    assert!(
        matches!(result, Err(MigrateError::UnknownItem { .. })),
        "migrate on never-placed item must return UnknownItem"
    );

    // And (OK2 negative case) the sink contains zero
    //   "cinder.migrate.count" lines — because Cinder does not
    //   call the recorder on the failure path
    //   (crates/cinder/src/store.rs:174-188), the writer
    //   structurally cannot emit. This pins both the writer's
    //   contract AND the inherited Cinder cascade contract.
    let lines = collect_lines(&buf);
    assert_eq!(
        count_with_metric_name(&lines, "cinder.migrate.count"),
        0,
        "failed migrate must not emit a migrate line"
    );
}

#[test]
fn two_tenants_cinder_migrate_emit_isolated_otlp_lines() {
    // Given acme has placed "a1" in Hot AND globex has placed "g1"
    //   in Hot
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let globex = tenant("globex");
    let t0 = SystemTime::now();
    let t1 = t0 + Duration::from_secs(60);

    cinder
        .place(&acme, &item("a1"), Tier::Hot, t0)
        .expect("place");
    cinder
        .place(&globex, &item("g1"), Tier::Hot, t0)
        .expect("place");

    // When acme's item moves Hot->Warm AND globex's item moves
    //   Hot->Cold (both succeed)
    cinder
        .migrate(&acme, &item("a1"), Tier::Warm, t1)
        .expect("acme migrate ok");
    cinder
        .migrate(&globex, &item("g1"), Tier::Cold, t1)
        .expect("globex migrate ok");

    // Then exactly one migrate line per tenant, each with the
    //   correct direction attributes and no cross-tenant leak.
    let lines = collect_lines(&buf);
    assert_eq!(
        count_with_tenant_and_metric(&lines, "acme", "cinder.migrate.count"),
        1,
        "exactly one migrate line under acme"
    );
    assert_eq!(
        count_with_tenant_and_metric(&lines, "globex", "cinder.migrate.count"),
        1,
        "exactly one migrate line under globex"
    );

    // Per-tenant direction attributes correct.
    let acme_migrate = lines
        .iter()
        .find(|l| {
            l["resource"]["attributes"][0]["value"]["stringValue"] == "acme"
                && l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.migrate.count"
        })
        .expect("acme migrate line present");
    assert!(point_attrs_contain(acme_migrate, "from", "hot"));
    assert!(point_attrs_contain(acme_migrate, "to", "warm"));

    let globex_migrate = lines
        .iter()
        .find(|l| {
            l["resource"]["attributes"][0]["value"]["stringValue"] == "globex"
                && l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.migrate.count"
        })
        .expect("globex migrate line present");
    assert!(point_attrs_contain(globex_migrate, "from", "hot"));
    assert!(point_attrs_contain(globex_migrate, "to", "cold"));
}

// ======================================================================
//  Slice 03 — evaluate events emit OTLP-JSON lines w/ per-tenant counts
//  Story: US-03  |  KPI: OK3 (+ DISCUSS D8 dual-emission contract)
//  Tag:  @infrastructure @US-03
// ======================================================================

#[test]
fn cinder_evaluate_emits_dual_lines_n_migrate_plus_one_evaluate() {
    // The dual-emission contract from DISCUSS D8 / slice-03.md.
    //   The highest-information-density assertion in the suite:
    //   one `evaluate_at` call cascades through Cinder and must
    //   produce both N per-item `cinder.migrate.count` lines AND
    //   1 per-tenant `cinder.evaluate.migrated.count` line in the
    //   SAME captured byte stream.
    //
    //   A regression in EITHER the writer's `record_evaluate` OR
    //   Cinder's `InMemoryTieringStore::evaluate_at` cascade
    //   surfaces here. Diagnose Cinder's in-tree tests first if
    //   this fails — they would go red first on a Cinder
    //   regression.

    // Given 5 items placed for acme in Hot at t0 and a policy that
    //   migrates Hot->Warm after 24h
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let t0 = SystemTime::now();
    let policy = TierPolicy::age_based(
        Duration::from_secs(24 * 3600),
        Duration::from_secs(72 * 3600),
    );
    for n in 0..5 {
        cinder
            .place(&acme, &item(&format!("trade-{n}")), Tier::Hot, t0)
            .expect("place");
    }

    // When evaluate_at runs at t0 + 25h (so all 5 are eligible)
    let migrated = cinder
        .evaluate_at(t0 + Duration::from_secs(25 * 3600), &policy)
        .expect("evaluate");

    // Then (OK4 guardrail) Cinder returns the total migration
    //   count unchanged from its NoopRecorder behaviour
    assert_eq!(migrated, 5, "evaluate_at returns total migration count");

    // And the captured byte stream contains BOTH:
    //   - exactly 5 cinder.migrate.count lines under acme, each
    //     with from=hot, to=warm;
    //   - exactly 1 cinder.evaluate.migrated.count line under acme,
    //     with asInt="5" and the tenant_id point attribute.
    //
    //   The test asserts SUBSET shape (count of matching lines),
    //   not total line count, so it is robust to the additional 5
    //   cinder.place.count lines emitted at place time.
    let lines = collect_lines(&buf);

    let acme_migrate_lines: Vec<&Value> = lines
        .iter()
        .filter(|l| {
            l["resource"]["attributes"][0]["value"]["stringValue"] == "acme"
                && l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.migrate.count"
        })
        .collect();
    assert_eq!(
        acme_migrate_lines.len(),
        5,
        "five per-item migrate lines under acme"
    );
    for m in &acme_migrate_lines {
        assert!(point_attrs_contain(m, "from", "hot"));
        assert!(point_attrs_contain(m, "to", "warm"));
    }

    let acme_evaluate_lines: Vec<&Value> = lines
        .iter()
        .filter(|l| {
            l["resource"]["attributes"][0]["value"]["stringValue"] == "acme"
                && l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.evaluate.migrated.count"
        })
        .collect();
    assert_eq!(
        acme_evaluate_lines.len(),
        1,
        "exactly one per-tenant evaluate line under acme"
    );
    let e = acme_evaluate_lines[0];
    assert_eq!(
        e["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]["asInt"], "5",
        "evaluate asInt == migrated count rendered as string (DISCUSS D4)"
    );
    assert!(
        point_attrs_contain(e, "tenant_id", "acme"),
        "evaluate point attrs must contain tenant_id=acme"
    );
}

#[test]
fn cinder_evaluate_with_zero_eligible_items_emits_no_evaluate_line() {
    // Given 3 items placed for acme in Hot at t0 and a policy that
    //   migrates Hot->Warm only after 24h
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let t0 = SystemTime::now();
    let policy = TierPolicy::age_based(
        Duration::from_secs(24 * 3600),
        Duration::from_secs(72 * 3600),
    );
    for n in 0..3 {
        cinder
            .place(&acme, &item(&format!("trade-{n}")), Tier::Hot, t0)
            .expect("place");
    }

    // When evaluate_at runs at t0 + 1h (so nothing is eligible)
    let migrated = cinder
        .evaluate_at(t0 + Duration::from_secs(3600), &policy)
        .expect("evaluate");

    // Then (OK4 guardrail) Cinder returns 0 unchanged
    assert_eq!(migrated, 0, "nothing eligible for migration at +1h");

    // And the sink contains zero cinder.evaluate.migrated.count
    //   lines (Cinder does not call record_evaluate for tenants
    //   with zero migrations — store.rs lines 218-230) and zero
    //   cinder.migrate.count lines (no items migrated).
    let lines = collect_lines(&buf);
    assert_eq!(
        count_with_metric_name(&lines, "cinder.evaluate.migrated.count"),
        0,
        "zero-migration evaluate must not emit an evaluate line"
    );
    assert_eq!(
        count_with_metric_name(&lines, "cinder.migrate.count"),
        0,
        "no items migrated so no migrate lines either"
    );
}

#[test]
fn two_tenants_cinder_evaluate_emits_per_tenant_evaluate_lines() {
    // Given 5 items placed for acme in Hot at t0 and 2 items
    //   placed for globex in Hot at t0, plus a policy migrating
    //   Hot->Warm after 24h
    let (buf, cinder) = wire();
    let acme = tenant("acme");
    let globex = tenant("globex");
    let t0 = SystemTime::now();
    let policy = TierPolicy::age_based(
        Duration::from_secs(24 * 3600),
        Duration::from_secs(72 * 3600),
    );
    for n in 0..5 {
        cinder
            .place(&acme, &item(&format!("a-{n}")), Tier::Hot, t0)
            .expect("place");
    }
    for n in 0..2 {
        cinder
            .place(&globex, &item(&format!("g-{n}")), Tier::Hot, t0)
            .expect("place");
    }

    // When evaluate_at runs at t0 + 25h
    let migrated = cinder
        .evaluate_at(t0 + Duration::from_secs(25 * 3600), &policy)
        .expect("evaluate");

    // Then (OK4) total return is 5 + 2 = 7
    assert_eq!(migrated, 7, "5 acme + 2 globex");

    // And per-tenant evaluate lines have the per-tenant counts
    //   (asInt="5" for acme, asInt="2" for globex), AND per-tenant
    //   migrate counts (5 for acme, 2 for globex). No cross-tenant
    //   leak.
    let lines = collect_lines(&buf);
    assert_eq!(
        count_with_tenant_and_metric(&lines, "acme", "cinder.evaluate.migrated.count"),
        1,
        "one evaluate line for acme"
    );
    assert_eq!(
        count_with_tenant_and_metric(&lines, "globex", "cinder.evaluate.migrated.count"),
        1,
        "one evaluate line for globex"
    );
    assert_eq!(
        count_with_tenant_and_metric(&lines, "acme", "cinder.migrate.count"),
        5,
        "five migrate lines for acme"
    );
    assert_eq!(
        count_with_tenant_and_metric(&lines, "globex", "cinder.migrate.count"),
        2,
        "two migrate lines for globex"
    );

    // asInt strings exact for each tenant's evaluate line.
    let acme_eval = lines
        .iter()
        .find(|l| {
            l["resource"]["attributes"][0]["value"]["stringValue"] == "acme"
                && l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.evaluate.migrated.count"
        })
        .expect("acme evaluate line present");
    assert_eq!(
        acme_eval["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]["asInt"],
        "5"
    );

    let globex_eval = lines
        .iter()
        .find(|l| {
            l["resource"]["attributes"][0]["value"]["stringValue"] == "globex"
                && l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.evaluate.migrated.count"
        })
        .expect("globex evaluate line present");
    assert_eq!(
        globex_eval["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]["asInt"],
        "2"
    );
}
