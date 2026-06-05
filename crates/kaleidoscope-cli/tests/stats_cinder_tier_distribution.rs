// Kaleidoscope CLI — `stats_with_tiers` subcommand acceptance test
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

//! # Acceptance tests — `stats_with_tiers` subcommand
//!
//! When the operator invokes `kaleidoscope-cli stats <tenant> <data_dir>`,
//! the dispatcher in `main.rs` now calls the library function
//! `kaleidoscope_cli::stats_with_tiers(...)` (DESIGN DD1), which queries
//! Lumen once via `LogStore::query(tenant, TimeRange::all())` AND opens
//! Cinder once via `FileBackedTieringStore::open(cinder_base(data_dir),
//! NoopRecorder)` and calls `TieringStore::list_by_tier(tenant, tier)`
//! for each of `Tier::Hot`, `Tier::Warm`, `Tier::Cold` (DESIGN DD2 /
//! DD3). Stdout is plain-text `key=value\n` lines in the fixed order:
//!
//!   records=N
//!   earliest=<ISO 8601 UTC>
//!   latest=<ISO 8601 UTC>
//!   hot=H        (only when H > 0 — Option B per DESIGN DD4)
//!   warm=W       (only when W > 0)
//!   cold=C       (only when C > 0)
//!
//! These tests drive the user-visible outcomes of feature
//! `cli-stats-cinder-tier-distribution-v0`:
//!
//! - **US-01 / OK1 (principal — tier count correctness)**: each
//!   non-zero `hot=` / `warm=` / `cold=` line equals
//!   `list_by_tier(tenant, tier).len()` for the corresponding tier; a
//!   zero count produces no line at all (Option B).
//! - **US-01 / OK2 (leading — tenant isolation)**: counts reflect ONLY
//!   the queried tenant's Cinder placements, never the cross-tenant
//!   union.
//! - **US-01 / OK3 (guardrail — orphan tier metadata)**: an empty-Lumen
//!   tenant with non-zero Cinder placements emits `records=0` followed
//!   by the selectively-emitted non-zero Cinder lines, with no
//!   `earliest=` / `latest=` and no zero-count tier lines.
//! - **US-01 / OK4 (guardrail — backwards compatibility)**: a tenant
//!   with positive Lumen records and zero Cinder placements emits
//!   stdout byte-equivalent to the predecessor (`cli-stats-subcommand-v0`)
//!   — exactly three lines, no Cinder lines. The locked
//!   `tests/stats_subcommand.rs` is the supplementary byte-level oracle
//!   for OK4.
//!
//! Note on Cinder seeding strategy (DISTILL DWD-02): every test below
//! seeds Cinder via direct `TieringStore::place` calls on a
//! `FileBackedTieringStore` opened against the same `cinder_base(data_dir)`
//! the system under test will reopen. We deliberately do NOT route Cinder
//! placement through `kaleidoscope_cli::ingest()` (which would place one
//! Hot item per batch as a side effect of its `flush()` —
//! `crates/kaleidoscope-cli/src/lib.rs:243-244`), because that would make
//! the tier counts emergent from batch size rather than from the
//! test's explicit intent. For Lumen seeding, tests #1, #2, and #5 use
//! `kaleidoscope_cli::ingest()` (which exercises the same Lumen ingest
//! path operators use and therefore couples the records= count to a
//! real ingest run), then manually correct the Cinder side. Test #4
//! uses the Lumen API directly to side-step `ingest()`'s automatic Hot
//! placement entirely — that is the only way to assert byte-equivalence
//! with the predecessor's three-line output (OK4) without seeding Cinder
//! by accident.
//!
//! Note on RED state at v0: every test below calls
//! `kaleidoscope_cli::stats_with_tiers(...)`. That function does not
//! yet exist on `lib.rs`. The file will not compile against the current
//! crate — that compile failure IS the RED gate for outside-in TDD
//! (DELIVER wave / Crafty adds the function).
//!
//! Note on the harness pattern: the `tenant`, `record`, `temp_root`,
//! `cleanup`, and `ndjson` helpers are duplicated inline at v0 per
//! DISTILL DWD-02 (rule-of-three extraction deferred — this is the
//! sixth test file in the cluster using the same shape, after the
//! four `tests/observe_otlp_*.rs` siblings and the locked
//! `tests/stats_subcommand.rs`).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::{ingest, stats_with_tiers, DEFAULT_BATCH_SIZE};
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, NoopRecorder as LumenNoopRecorder,
    SeverityNumber, TimeRange,
};

// --------------------------------------------------------------------
// Helpers (mirror stats_subcommand.rs + observe_otlp_*.rs harness
// shape; rule-of-three deferral per DWD-02).
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn temp_root(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("kal-cli-stats-tiers-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

fn ndjson(records: &[LogRecord]) -> String {
    records
        .iter()
        .map(|r| serde_json::to_string(r).expect("serialise"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns the `cinder_base(data_dir)` path — kept in lock-step with
/// the private helper at `crates/kaleidoscope-cli/src/lib.rs:122-124`.
/// `stats_with_tiers` will reopen Cinder against exactly this path, so
/// tests MUST seed against the same join.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Returns the `lumen_base(data_dir)` path — kept in lock-step with the
/// private helper at `crates/kaleidoscope-cli/src/lib.rs:118-120`. Used
/// only by Test #4 which seeds Lumen directly (without going through
/// `kaleidoscope_cli::ingest()`).
fn lumen_base(data_dir: &Path) -> PathBuf {
    data_dir.join("lumen")
}

/// Opens a `FileBackedTieringStore` against `cinder_base(data_dir)` with
/// a quiescent `NoopRecorder` and calls `place` once per `(item_id,
/// tier)` triple. The store is then dropped, which forces any in-memory
/// WAL flush to disk so the system under test sees the placements when
/// it reopens the store. `placed_at` is `SystemTime::now()` for every
/// placement — this feature does not depend on the timestamp value
/// (stats only reads tier counts via `list_by_tier`, never `evaluate_at`).
fn seed_cinder(data_dir: &Path, tenant: &TenantId, hot: usize, warm: usize, cold: usize) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    let mut seq = 0usize;
    for _ in 0..hot {
        let item = ItemId::new(format!("{}/seed-{:05}", tenant.0, seq));
        cinder
            .place(tenant, &item, Tier::Hot, SystemTime::now())
            .expect("place");
        seq += 1;
    }
    for _ in 0..warm {
        let item = ItemId::new(format!("{}/seed-{:05}", tenant.0, seq));
        cinder
            .place(tenant, &item, Tier::Warm, SystemTime::now())
            .expect("place");
        seq += 1;
    }
    for _ in 0..cold {
        let item = ItemId::new(format!("{}/seed-{:05}", tenant.0, seq));
        cinder
            .place(tenant, &item, Tier::Cold, SystemTime::now())
            .expect("place");
        seq += 1;
    }
    // Drop forces the FileBackedTieringStore to release any open file
    // handles; persistence is per-`place` call on this adapter, so by
    // the time we return the WAL is durable for the reopen below.
    drop(cinder);
}

/// Returns the count of Cinder items currently placed under `tier` for
/// `tenant` in `data_dir`. Used by Test #1's read-only invariant check
/// (the SUT must not mutate Cinder; the post-call counts must equal
/// the pre-call counts).
fn cinder_count(data_dir: &Path, tenant: &TenantId, tier: Tier) -> usize {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("reopen cinder for count");
    cinder.list_by_tier(tenant, tier).len()
}

// --------------------------------------------------------------------
// Deterministic timestamp seeds (mirror stats_subcommand.rs so that any
// shared expectations on the earliest/latest ISO 8601 byte shape remain
// consistent across the two test files).
// --------------------------------------------------------------------

const SEED_EARLIEST_NS: u64 = 1_779_062_400_000_000_000; // 2026-05-18T00:00:00Z
const SEEDED_STEP_NS: u64 = 14_400_000_000_000; // 4 hours

// --------------------------------------------------------------------
// Test #1 — OK1 happy path: populated tenant with all three tiers
// non-zero emits exactly six lines in the order records, earliest,
// latest, hot, warm, cold.
//
// Seeds 7 Lumen records via `kaleidoscope_cli::ingest()` (which as a
// side effect places one Hot Cinder item — see lib.rs:243-244), then
// directly resets Cinder's placement counts to the test's intended
// 5/12/47 distribution by seeding 5 Hot + 12 Warm + 47 Cold items on
// top of the inherited Hot item. To make the assertion deterministic
// we open a fresh tmp dir, ingest into Lumen, then track Cinder's
// post-ingest Hot count and seed (5 - hot_after_ingest) additional Hot
// items so the final Hot total is exactly 5 (which is the value the
// scenario specifies).
// --------------------------------------------------------------------

#[test]
fn stats_with_tiers_populated_multi_tier_emits_six_lines_in_order() {
    // Given Priya has pre-ingested 7 records for tenant `acme`.
    let root = temp_root("ok1_populated_multi_tier");
    let data = root.join("data");
    let records: Vec<LogRecord> = (0..7u64)
        .map(|i| record(SEED_EARLIEST_NS + i * SEEDED_STEP_NS, "x"))
        .collect();

    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("setup ingest");

    // And Cinder has been populated such that list_by_tier(acme, Hot) ==
    // 5, list_by_tier(acme, Warm) == 12, list_by_tier(acme, Cold) == 47.
    // `ingest()` already placed 1 Hot item (one per batch; 7 records ≤
    // DEFAULT_BATCH_SIZE so a single batch flushed). We seed an
    // additional 4 Hot + 12 Warm + 47 Cold to bring the totals to the
    // scenario's 5/12/47.
    let hot_after_ingest = cinder_count(&data, &acme, Tier::Hot);
    assert_eq!(
        hot_after_ingest, 1,
        "ingest() places exactly one Hot Cinder item per batch \
         (lib.rs:243-244); 7 records ≤ DEFAULT_BATCH_SIZE → 1 batch"
    );
    seed_cinder(&data, &acme, 5 - hot_after_ingest, 12, 47);
    assert_eq!(cinder_count(&data, &acme, Tier::Hot), 5);
    assert_eq!(cinder_count(&data, &acme, Tier::Warm), 12);
    assert_eq!(cinder_count(&data, &acme, Tier::Cold), 47);

    // When Priya invokes `stats_with_tiers` with tenant `acme`, the
    // data_dir, and a captured stdout sink.
    let mut stdout = Vec::<u8>::new();
    let count =
        stats_with_tiers(&acme, &data, &mut stdout, TimeRange::all()).expect("stats_with_tiers");

    // Then the returned count equals 7 (mirrors stats()/read() shape).
    assert_eq!(
        count, 7,
        "stats_with_tiers() returns the Lumen record count"
    );

    // And the captured stdout contains exactly 6 non-empty lines, in
    // order: records=7, earliest=<ISO>, latest=<ISO>, hot=5, warm=12,
    // cold=47.
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        6,
        "exactly six non-empty lines on stdout (got: {out:?})"
    );
    assert_eq!(lines[0], "records=7", "line 1 is the records= line");
    assert!(
        lines[1].starts_with("earliest="),
        "line 2 is the earliest= line (got {:?})",
        lines[1]
    );
    assert!(
        lines[2].starts_with("latest="),
        "line 3 is the latest= line (got {:?})",
        lines[2]
    );
    assert_eq!(
        lines[3], "hot=5",
        "line 4 is hot=5 — exactly the seeded Hot count"
    );
    assert_eq!(
        lines[4], "warm=12",
        "line 5 is warm=12 — exactly the seeded Warm count"
    );
    assert_eq!(
        lines[5], "cold=47",
        "line 6 is cold=47 — exactly the seeded Cold count"
    );

    // And the stdout ends with `\n` (output-shape contract from
    // user-stories.md System Constraints).
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    // And the Cinder store is unchanged after the call (read-only
    // invariant from the user-stories.md Read-only contract).
    assert_eq!(
        cinder_count(&data, &acme, Tier::Hot),
        5,
        "stats_with_tiers does not mutate Cinder Hot count"
    );
    assert_eq!(
        cinder_count(&data, &acme, Tier::Warm),
        12,
        "stats_with_tiers does not mutate Cinder Warm count"
    );
    assert_eq!(
        cinder_count(&data, &acme, Tier::Cold),
        47,
        "stats_with_tiers does not mutate Cinder Cold count"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK1 + Option B selective emission: a tenant with only Hot
// placements (Warm == 0 and Cold == 0) emits four lines — no warm= and
// no cold= lines.
// --------------------------------------------------------------------

#[test]
fn stats_with_tiers_hot_only_omits_warm_and_cold_lines() {
    // Given Priya has pre-ingested 3 records for tenant `acme`.
    let root = temp_root("ok1_hot_only");
    let data = root.join("data");
    let records: Vec<LogRecord> = (0..3u64)
        .map(|i| record(SEED_EARLIEST_NS + i, "h"))
        .collect();

    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("setup ingest");

    // And Cinder has been populated for `acme` such that
    // list_by_tier(acme, Hot) == 3, Warm == 0, Cold == 0. `ingest()`
    // already placed 1 Hot item (one batch); we seed 2 more Hot items
    // and zero Warm/Cold to bring the Hot total to 3.
    let hot_after_ingest = cinder_count(&data, &acme, Tier::Hot);
    assert_eq!(hot_after_ingest, 1, "single-batch ingest places 1 Hot item");
    seed_cinder(&data, &acme, 3 - hot_after_ingest, 0, 0);
    assert_eq!(cinder_count(&data, &acme, Tier::Hot), 3);
    assert_eq!(cinder_count(&data, &acme, Tier::Warm), 0);
    assert_eq!(cinder_count(&data, &acme, Tier::Cold), 0);

    // When Priya invokes `stats_with_tiers`.
    let mut stdout = Vec::<u8>::new();
    let count =
        stats_with_tiers(&acme, &data, &mut stdout, TimeRange::all()).expect("stats_with_tiers");

    // Then the returned count equals 3.
    assert_eq!(
        count, 3,
        "stats_with_tiers() returns the Lumen record count"
    );

    // And the captured stdout contains exactly 4 non-empty lines:
    // records=3, earliest=<ISO>, latest=<ISO>, hot=3 — and crucially
    // no warm= line and no cold= line (Option B per DESIGN DD4).
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        4,
        "exactly four non-empty lines on stdout (got: {out:?})"
    );
    assert_eq!(lines[0], "records=3", "line 1 is the records= line");
    assert!(
        lines[1].starts_with("earliest="),
        "line 2 is the earliest= line"
    );
    assert!(
        lines[2].starts_with("latest="),
        "line 3 is the latest= line"
    );
    assert_eq!(lines[3], "hot=3", "line 4 is hot=3");
    assert!(
        !out.contains("warm="),
        "no warm= line appears when Warm count is zero (Option B)"
    );
    assert!(
        !out.contains("cold="),
        "no cold= line appears when Cold count is zero (Option B)"
    );
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK3 orphan tier metadata: an empty-Lumen tenant with
// non-zero Cinder placements emits records=0 plus the selectively-
// emitted non-zero tier lines, with no earliest= / latest= / warm=.
//
// No Lumen ingest at all — the data_dir is fresh and only the Cinder
// side is seeded. This proves stats_with_tiers handles the "Lumen
// store directory does not yet exist" case AND the "non-zero Cinder
// placements visible despite zero Lumen records" case (the operator's
// orphan-tier-metadata detection signal).
// --------------------------------------------------------------------

#[test]
fn stats_with_tiers_empty_lumen_with_populated_cinder_surfaces_orphan_metadata() {
    // Given the Lumen store at the data_dir contains zero records for
    // tenant `acme` (no ingest at all — fresh tmp dir).
    let root = temp_root("ok3_orphan_tier_metadata");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // And Cinder has been populated for `acme` such that
    // list_by_tier(acme, Hot) == 2, Warm == 0, Cold == 1.
    let acme = tenant("acme");
    seed_cinder(&data, &acme, 2, 0, 1);
    assert_eq!(cinder_count(&data, &acme, Tier::Hot), 2);
    assert_eq!(cinder_count(&data, &acme, Tier::Warm), 0);
    assert_eq!(cinder_count(&data, &acme, Tier::Cold), 1);

    // When Priya invokes `stats_with_tiers`.
    let mut stdout = Vec::<u8>::new();
    let count =
        stats_with_tiers(&acme, &data, &mut stdout, TimeRange::all()).expect("stats_with_tiers");

    // Then the returned count is 0 (empty-Lumen tenant is a valid
    // query result, not an error).
    assert_eq!(
        count, 0,
        "stats_with_tiers() returns 0 for empty-Lumen tenant"
    );

    // And the captured stdout contains exactly 3 non-empty lines:
    // records=0, hot=2, cold=1. No earliest=, no latest= (inherited
    // OK3 from predecessor — see stats() body: the (first, last) match
    // only fires when records is non-empty). No warm= (Option B).
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        3,
        "exactly three non-empty lines on stdout (got: {out:?})"
    );
    assert_eq!(lines[0], "records=0", "line 1 is records=0");
    assert_eq!(lines[1], "hot=2", "line 2 is hot=2 (Lumen lines absent)");
    assert_eq!(
        lines[2], "cold=1",
        "line 3 is cold=1 (warm= absent — Option B)"
    );
    assert!(
        !out.contains("earliest="),
        "no earliest= line for empty-Lumen tenant"
    );
    assert!(
        !out.contains("latest="),
        "no latest= line for empty-Lumen tenant"
    );
    assert!(
        !out.contains("warm="),
        "no warm= line when Warm count is zero (Option B)"
    );
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK4 backwards compatibility: a tenant with positive Lumen
// records and zero Cinder placements emits stdout byte-equivalent to
// the predecessor's three-line output — no hot=, no warm=, no cold=.
//
// To seed Lumen WITHOUT also placing Cinder Hot items as a side effect
// (which `kaleidoscope_cli::ingest()`'s flush does at lib.rs:243-244),
// this test uses the Lumen API directly: `FileBackedLogStore::open` +
// `LogStore::ingest`. No Cinder placements occur at all, so when
// stats_with_tiers reopens Cinder the placement counts are all zero
// and (per Option B) no Cinder lines are emitted. The result is
// byte-equivalent to the predecessor's `records=N\nearliest=...\nlatest=...\n`
// for the same (tenant, data_dir) pair — the OK4 invariant.
// --------------------------------------------------------------------

#[test]
fn stats_with_tiers_populated_lumen_with_zero_cinder_is_byte_equivalent_to_predecessor() {
    // Given the Lumen store at the data_dir has been populated for
    // tenant `legacy_acme` with 4 records via the Lumen API directly,
    // bypassing `kaleidoscope_cli::ingest()` so that NO Cinder
    // placements occur. (This is the only path that produces a
    // populated-Lumen + empty-Cinder state — `ingest()` always places
    // one Hot per batch.)
    let root = temp_root("ok4_backwards_compat");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    let legacy = tenant("legacy_acme");
    let records: Vec<LogRecord> = (0..4u64)
        .map(|i| record(SEED_EARLIEST_NS + i, "L"))
        .collect();
    let lumen = FileBackedLogStore::open(lumen_base(&data), Box::new(LumenNoopRecorder))
        .expect("open lumen");
    let receipt = lumen
        .ingest(&legacy, LogBatch::with_records(records))
        .expect("lumen ingest");
    assert_eq!(receipt.count, 4, "Lumen ingested 4 records directly");
    drop(lumen);

    // And the Cinder store has zero placements for `legacy_acme`
    // (we never called `place` for this tenant — the Cinder directory
    // may not even exist on disk yet; stats_with_tiers must handle
    // open-on-empty cleanly).
    // (No assertion of pre-state needed — Cinder is empty by
    // construction since we never opened it.)

    // When Priya invokes `stats_with_tiers`.
    let mut stdout = Vec::<u8>::new();
    let count =
        stats_with_tiers(&legacy, &data, &mut stdout, TimeRange::all()).expect("stats_with_tiers");

    // Then the returned count is 4.
    assert_eq!(
        count, 4,
        "stats_with_tiers() returns the Lumen record count"
    );

    // And the captured stdout contains exactly 3 non-empty lines —
    // byte-equivalent to the predecessor (`cli-stats-subcommand-v0`)'s
    // output for the same (tenant, data_dir) pair. The locked
    // `tests/stats_subcommand.rs` Test #1 asserts exactly this shape;
    // OK4 binds the new function to produce the same bytes for
    // zero-Cinder tenants.
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        3,
        "exactly three non-empty lines on stdout — byte-equivalent to predecessor (got: {out:?})"
    );
    assert_eq!(lines[0], "records=4", "line 1 is records=4");
    assert!(
        lines[1].starts_with("earliest="),
        "line 2 is the earliest= line"
    );
    assert!(
        lines[2].starts_with("latest="),
        "line 3 is the latest= line"
    );
    assert!(
        !out.contains("hot="),
        "no hot= line when Hot count is zero (Option B / OK4)"
    );
    assert!(
        !out.contains("warm="),
        "no warm= line when Warm count is zero (Option B / OK4)"
    );
    assert!(
        !out.contains("cold="),
        "no cold= line when Cold count is zero (Option B / OK4)"
    );
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — OK2 tenant isolation: Cinder tier counts for `acme` do NOT
// count `globex`'s placements that coexist in the same data_dir.
//
// Seeds 5 Hot items for tenant `acme` and 9 Hot items for tenant
// `globex` in the same Cinder store. Also ingests a few Lumen records
// for `acme` so the records= line is non-zero (the isolation point is
// about the hot= line, not about records=). Asserts the hot= line
// reports 5 — NOT 14 (cross-tenant union) and NOT 9 (globex's count).
// --------------------------------------------------------------------

#[test]
fn stats_with_tiers_for_acme_does_not_count_globex_placements() {
    // Given Priya has pre-ingested records for tenant `acme` (the
    // ingest is incidental — it grounds the records= line at a
    // non-zero value so the isolation assertion targets the hot=
    // line specifically).
    let root = temp_root("ok2_tenant_isolation");
    let data = root.join("data");
    let acme_records: Vec<LogRecord> = (0..2u64)
        .map(|i| record(SEED_EARLIEST_NS + i, "a"))
        .collect();

    let acme = tenant("acme");
    let globex = tenant("globex");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&acme_records).into_bytes()),
        None,
    )
    .expect("setup ingest acme");

    // And Cinder has been populated such that
    // list_by_tier(acme, Hot) == 5 and list_by_tier(globex, Hot) == 9.
    // `ingest()` already placed 1 Hot item for `acme` (1 batch); we
    // seed 4 more Hot for `acme` to bring it to 5, then 9 Hot for
    // `globex` in the same data_dir to set up the cross-tenant
    // confounder.
    let acme_hot_after_ingest = cinder_count(&data, &acme, Tier::Hot);
    assert_eq!(acme_hot_after_ingest, 1, "single-batch ingest places 1 Hot");
    seed_cinder(&data, &acme, 5 - acme_hot_after_ingest, 0, 0);
    seed_cinder(&data, &globex, 9, 0, 0);
    assert_eq!(
        cinder_count(&data, &acme, Tier::Hot),
        5,
        "acme has 5 Hot items"
    );
    assert_eq!(
        cinder_count(&data, &globex, Tier::Hot),
        9,
        "globex has 9 Hot items in the SAME data_dir"
    );

    // When Priya invokes `stats_with_tiers` against tenant `acme`.
    let mut stdout = Vec::<u8>::new();
    let count =
        stats_with_tiers(&acme, &data, &mut stdout, TimeRange::all()).expect("stats_with_tiers");

    // Then the returned count equals 2 (only acme's Lumen records).
    assert_eq!(
        count, 2,
        "stats_with_tiers() counts ONLY acme's Lumen records"
    );

    // And the hot= line on stdout shows the count 5 — NOT 14 (cross-
    // tenant union) and NOT 9 (globex's count). This is the OK2
    // tenant-isolation invariant inherited from cinder's
    // TieringStore::list_by_tier per-tenant semantic.
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    let hot_line = lines
        .iter()
        .find(|l| l.starts_with("hot="))
        .expect("hot= line is present");
    assert_eq!(
        *hot_line, "hot=5",
        "hot= line reports acme's count (5), NOT 14 (union) NOT 9 (globex's count)"
    );

    // And the records= line shows acme's 2 records (NOT 2 + globex's
    // ingested record count, which would surface a cross-tenant leak
    // on the Lumen side). Lumen-side isolation is already covered by
    // the locked predecessor test file's tenant-isolation case; the
    // assertion here is a defence-in-depth check on the new function.
    assert_eq!(
        lines[0], "records=2",
        "records= reports acme's count (2), not the cross-tenant union"
    );

    cleanup(&root);
}
