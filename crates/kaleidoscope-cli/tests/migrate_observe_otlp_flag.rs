// Kaleidoscope CLI — `migrate --observe-otlp` flag acceptance test
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

//! # Acceptance tests — `migrate --observe-otlp <path>` flag
//!
//! Feature: `cli-migrate-observe-otlp-v0`. The `kaleidoscope_cli::migrate`
//! library function gains an optional `otlp_log_path: Option<&Path>`
//! parameter in last position (DESIGN DD1). When `Some(path)`, the
//! Cinder recorder constructed at `FileBackedTieringStore::open` time
//! is `CinderToOtlpJsonWriter::new(file)`, where `file` is opened with
//! `OpenOptions::new().create(true).append(true).open(path)` exactly
//! once on the success arm of the parse — i.e. AFTER `parse_tier`
//! returns Ok (DESIGN DD2). When `None`, behaviour is byte-equivalent
//! to today: `cinder::NoopRecorder` is constructed and no file is
//! created at any path (DISCUSS D-RecorderConstruction / OK2).
//!
//! These tests drive the four outcome KPIs of this feature:
//!
//! - **OK1 — wire shape per successful migrate (principal / North
//!   Star)**: library-direct happy path. Seeds one item in Hot for
//!   tenant `acme`, calls `migrate(..., Some(&otlp_path))`, reads the
//!   sink file, asserts exactly one non-empty line whose
//!   `scopeMetrics[0].metrics[0].name == "cinder.migrate.count"`, with
//!   `resource.attributes[0].value.stringValue == "acme"`, point
//!   attributes carrying `from == "hot"` and `to == "cold"`, and
//!   `asInt == "1"`. Stdout is byte-equivalent to the no-flag
//!   pre-feature behaviour (one `migrated …\n` line).
//! - **OK2 — no-flag byte-equivalence guardrail**: library-direct
//!   call with `otlp_log_path = None`. Asserts no file is created at
//!   the candidate sink path (i.e. the path that WOULD have been used
//!   had the flag been set) and stdout matches the pre-feature
//!   behaviour exactly. The locked `migrate_subcommand.rs` test file
//!   is the byte-equivalence probe for the broader OK2 contract; this
//!   scenario adds the file-absence assertion that the locked file
//!   does not cover (the locked file pre-dates the flag).
//! - **OK3 — UnknownItem leaves no emission**: subprocess invocation
//!   of the binary with a typo'd item id. Asserts non-zero exit, and
//!   that the sink file (if it exists at all — `OpenOptions::create`
//!   may have created an empty file before the pre-flight `get_entry`
//!   short-circuited) contains zero non-empty lines whose metric name
//!   is `cinder.migrate.count`.
//! - **OK4 — InvalidTier creates no file**: subprocess invocation with
//!   an invalid tier value. Asserts non-zero exit and that the sink
//!   path does NOT exist after the call, because `parse_tier(...)`
//!   short-circuits BEFORE the `OpenOptions::open(path)` call (DESIGN
//!   DD2; pins the parse-before-open contract).
//!
//! ## Library-direct vs subprocess split (DISTILL DWD-04)
//!
//! OK1 and OK2 are library-direct calls into a `Vec<u8>` stdout buffer
//! plus a real on-disk OTLP sink path: they pin the LIBRARY contract
//! (wire shape of the emitted line; no file on the `None` arm). OK3
//! and OK4 are subprocess calls against the actual binary
//! (`CARGO_BIN_EXE_kaleidoscope-cli`): they exercise the BINARY
//! boundary — argv parsing for `--observe-otlp`, dispatcher arm,
//! `main.rs` thread-through, exit-code propagation, stderr substring
//! composition. Both shapes serve the same KPIs but at different
//! boundaries. This mirrors the split established in
//! `migrate_subcommand.rs` (library-direct happy / idempotent /
//! tenant-isolation / library-direct unknown-item; subprocess
//! unknown-item / invalid-tier).
//!
//! ## RED state at v0
//!
//! At authoring time the `migrate(...)` library function takes five
//! arguments (no `otlp_log_path` parameter). Every call in this file
//! passes a sixth argument — either `Some(&otlp_path)` or an explicit
//! `None`. The file will not compile against the current crate; that
//! compile failure IS the RED gate for outside-in TDD (DELIVER wave /
//! Crafty adds the parameter and the internal `match` arm).
//!
//! ## Harness duplication
//!
//! The `tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`,
//! `bin` helpers are duplicated inline per DISCUSS D5 (rule-of-three
//! deferral). This is the NINTH inline duplication in the
//! `kaleidoscope-cli/tests/` cluster; extraction to a
//! `tests/common/mod.rs` is overdue but is a deliberate cross-file
//! refactor, not this feature's job (DEVOPS forward-compat note).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::migrate;
use serde_json::Value;

// --------------------------------------------------------------------
// Helpers — mirror the shape used by `migrate_subcommand.rs` and the
// `kaleidoscope-cli/tests/` cluster (rule-of-three deferral per
// DISCUSS D5 / DEVOPS forward-compat note).
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn temp_root(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("kal-cli-migrate-otlp-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// Returns `cinder_base(data_dir)` — kept in lock-step with the
/// private helper at `crates/kaleidoscope-cli/src/lib.rs:130-132`.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Places one item under `tenant` in `tier` against
/// `cinder_base(data_dir)`. The store is dropped immediately so the
/// WAL is flushed before `migrate` reopens it.
fn place_item(data_dir: &Path, tenant: &TenantId, item_id: &str, tier: Tier) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    cinder
        .place(tenant, &ItemId::new(item_id), tier, SystemTime::now())
        .expect("place");
    drop(cinder);
}

/// Returns the absolute path of the binary under test
/// (`CARGO_BIN_EXE_kaleidoscope-cli`). Cargo guarantees the binary is
/// built before tests run when the crate has both `[lib]` and `[[bin]]`
/// targets — same pattern as `migrate_subcommand.rs::bin()`.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Test #1 — OK1: happy path Hot → Cold emits one `cinder.migrate.count`
// line on the OTLP sink with the correct wire shape, and stdout is
// byte-equivalent to the no-flag transition report.
//
// Library-direct invocation. Seeds one item `acme/batch-00042` in Hot
// for tenant `acme`, calls `migrate(..., "cold", &mut buf,
// Some(&otlp_path))`, then reads back the sink file. Asserts:
//
// - returned Result is Ok(())
// - stdout buffer is exactly `migrated tenant=acme item=acme/batch-00042
//   from=hot to=cold\n` (no change vs. the no-flag path, OK2 byte-
//   equivalence on stdout)
// - the sink file exists
// - the sink file ends with `\n` (NDJSON line termination)
// - exactly ONE non-empty line in the sink has metric name
//   `cinder.migrate.count` (the principal OK1 cardinality invariant)
// - that line parses as `serde_json::Value`
// - `scopeMetrics[0].scope.name == "kaleidoscope.cinder"` (locked by
//   ADR-0039 §2 / `CinderToOtlpJsonWriter::scope_name` at
//   `crates/self-observe/src/cinder_otlp_json.rs:178`)
// - `resource.attributes[0].value.stringValue == "acme"` (the
//   tenant_id resource attribute)
// - `sum.dataPoints[0].asInt == "1"` (one migrate event)
// - point attributes contain `{"key":"from","value":{"stringValue":"hot"}}`
// - point attributes contain `{"key":"to","value":{"stringValue":"cold"}}`
// --------------------------------------------------------------------

#[test]
fn migrate_with_observe_otlp_emits_one_cinder_migrate_count_line() {
    // Given Priya has placed item `acme/batch-00042` under tenant
    // `acme` in tier Hot at `<data>/cinder.*`, and the OTLP sink path
    // does not yet exist.
    let root = temp_root("ok1_happy_hot_to_cold");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let otlp_path = root.join("audit.ndjson");
    assert!(
        !otlp_path.exists(),
        "pre-call: sink file does not yet exist"
    );
    let acme = tenant("acme");
    place_item(&data, &acme, "acme/batch-00042", Tier::Hot);

    // When Priya calls migrate with Some(&otlp_path) and target tier
    // `cold`.
    let mut buf = Vec::<u8>::new();
    let result = migrate(
        &acme,
        &data,
        "acme/batch-00042",
        "cold",
        &mut buf,
        Some(&otlp_path),
    );

    // Then the call returns Ok(()).
    assert!(
        result.is_ok(),
        "migrate hot→cold with --observe-otlp returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is exactly the one-line transition report — byte-
    // equivalent to the no-flag path (OK2 stdout invariant). The flag
    // adds the sink line; it does NOT alter stdout.
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n",
        "stdout is byte-equivalent to the no-flag transition report"
    );

    // And the sink file exists.
    assert!(
        otlp_path.exists(),
        "sink file was created by the Some(path) arm of the construction"
    );

    // And the sink file content ends with `\n` (NDJSON line
    // termination — ADR-0039 §8 / `CinderToOtlpJsonWriter::emit`
    // appends `\n` then flushes inside the Mutex critical section).
    let content = fs::read_to_string(&otlp_path).expect("read sink file");
    assert!(
        content.ends_with('\n'),
        "sink file ends with \\n (got tail: {:?})",
        content.chars().rev().take(8).collect::<String>()
    );

    // And there is exactly ONE non-empty line in the sink whose metric
    // name is `cinder.migrate.count` (the OK1 cardinality invariant:
    // one line per successful migrate, no more, no less).
    let migrate_lines: Vec<Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("parse OTLP-JSON line"))
        .filter(|v| v["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.migrate.count")
        .collect();
    assert_eq!(
        migrate_lines.len(),
        1,
        "exactly one cinder.migrate.count line per successful migrate \
         (got {} lines in sink: {:?})",
        migrate_lines.len(),
        content
    );

    let line = &migrate_lines[0];

    // And the scope name is `kaleidoscope.cinder` (the Cinder scope —
    // pinned by `CinderToOtlpJsonWriter::scope_name`).
    assert_eq!(
        line["scopeMetrics"][0]["scope"]["name"], "kaleidoscope.cinder",
        "scope.name is kaleidoscope.cinder"
    );

    // And the resource attribute carries `tenant_id == "acme"`.
    assert_eq!(
        line["resource"]["attributes"][0]["key"], "tenant_id",
        "resource attribute key is tenant_id"
    );
    assert_eq!(
        line["resource"]["attributes"][0]["value"]["stringValue"], "acme",
        "resource attribute stringValue is the tenant id passed on the command line"
    );

    // And `sum.dataPoints[0].asInt == "1"` (one migrate event encoded
    // as an OTLP-JSON int — OTLP-JSON encodes uint64 as a string).
    let dp = &line["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    assert_eq!(dp["asInt"], "1", "asInt is \"1\" (one migrate event)");

    // And the point attributes contain `{from: "hot"}` and
    // `{to: "cold"}` — the migrate-specific attribute pair locked by
    // ADR-0039 §2 / DD1 (the order of from vs to is NOT pinned at the
    // CLI level; the library writer's order is pinned at the library
    // level).
    let point_attrs = dp["attributes"]
        .as_array()
        .expect("point attributes is an array");
    let has_attr = |key: &str, value: &str| {
        point_attrs
            .iter()
            .any(|a| a["key"] == key && a["value"]["stringValue"] == value)
    };
    assert!(
        has_attr("from", "hot"),
        "point attributes contain from=hot (got: {point_attrs:?})"
    );
    assert!(
        has_attr("to", "cold"),
        "point attributes contain to=cold (got: {point_attrs:?})"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK2: no-flag byte-equivalence (file-absence half).
//
// Library-direct invocation with `otlp_log_path = None`. The locked
// `migrate_subcommand.rs` test file already covers the stdout byte-
// equivalence half of OK2 (it asserts the exact stdout line on Hot→Cold,
// idempotent same-tier, cross-tenant isolation, and unknown-item
// paths). This scenario adds the file-absence assertion: when the
// flag is absent, no file is created at the candidate sink path —
// the Cinder recorder is `cinder::NoopRecorder` and no
// `OpenOptions::create(true)` call is reached.
//
// We use a stable candidate path under `<data>/audit.ndjson` and
// assert it does NOT exist after the call. This pins the contract
// that the `None` arm does NOT open ANY file as a side effect.
// --------------------------------------------------------------------

#[test]
fn migrate_without_observe_otlp_creates_no_file_at_candidate_path() {
    // Given Priya has placed item `acme/batch-00007` under tenant
    // `acme` in tier Hot, and a stable candidate sink path that does
    // not yet exist.
    let root = temp_root("ok2_no_flag_no_file");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let candidate_path = root.join("audit.ndjson");
    assert!(
        !candidate_path.exists(),
        "pre-call: candidate sink path does not exist"
    );
    let acme = tenant("acme");
    place_item(&data, &acme, "acme/batch-00007", Tier::Hot);

    // When Priya calls migrate with otlp_log_path = None and target
    // tier `cold` (changing tier so the call exercises the same path
    // as the OK1 happy path — only the recorder differs).
    let mut buf = Vec::<u8>::new();
    let result = migrate(&acme, &data, "acme/batch-00007", "cold", &mut buf, None);

    // Then the call returns Ok(()) (the migrate succeeds in the same
    // shape it does on the locked OK1 path of `migrate_subcommand.rs`).
    assert!(
        result.is_ok(),
        "migrate with None returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is byte-equivalent to the pre-feature transition
    // report (the no-flag OK2 stdout invariant — locked file pre-
    // dates the flag and asserts this exact string; we re-assert it
    // here so the OK2 contract is also testable from this file).
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "migrated tenant=acme item=acme/batch-00007 from=hot to=cold\n",
        "stdout is byte-equivalent to the no-flag transition report"
    );

    // And the candidate sink path does NOT exist (the `None` arm
    // constructs `cinder::NoopRecorder` — no file is opened, no
    // `OpenOptions::create(true)` runs against any path).
    assert!(
        !candidate_path.exists(),
        "no file is created at the candidate sink path on the None arm \
         (the recorder is NoopRecorder; no OpenOptions call is reached)"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK3: UnknownItem leaves no `cinder.migrate.count` line
// (subprocess).
//
// Spawns the actual binary with args
// `["migrate", "acme", <data>, "ghost-item", "warm",
//   "--observe-otlp", <otlp_path>]` against a Cinder directory with
// NO placement for `ghost-item`. The pre-flight `get_entry` returns
// `None` and short-circuits BEFORE `cinder.migrate(...)` is invoked
// (locked by `cli-migrate-subcommand-v0` DESIGN DD2 / DD6 and
// re-asserted byte-equivalently by `migrate_subcommand.rs`).
// Therefore the `record_migrate` writer method is never called and
// no `cinder.migrate.count` line is emitted.
//
// The sink file MAY exist (the `Some(path)` arm of the recorder
// construction runs `OpenOptions::create(true).append(true).open(path)`
// BEFORE the pre-flight `get_entry` runs, since the writer must be
// passed to `FileBackedTieringStore::open` BEFORE `get_entry` can be
// called on the store). The contract asserts only that the sink
// contains zero non-empty lines whose metric name is
// `cinder.migrate.count` — not that the file is absent. This is the
// emission-absence guarantee, not a file-absence guarantee.
// --------------------------------------------------------------------

#[test]
fn migrate_subcommand_unknown_item_with_observe_otlp_leaves_no_emission() {
    // Given a fresh data_dir with NO placement for
    // (tenant=acme, item=ghost-item), and an OTLP sink path that may
    // or may not exist after the call (the contract permits the file
    // to be created empty by `OpenOptions::create(true)`).
    let root = temp_root("ok3_unknown_item_no_emission");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let otlp_path = root.join("audit.ndjson");

    // When Priya invokes the binary's `migrate` subcommand with the
    // ghost item id AND `--observe-otlp <otlp_path>`.
    let output = Command::new(bin())
        .arg("migrate")
        .arg("acme")
        .arg(&data)
        .arg("ghost-item")
        .arg("warm")
        .arg("--observe-otlp")
        .arg(&otlp_path)
        .output()
        .expect("spawn kaleidoscope-cli migrate");

    // Then exit code is non-zero (the fail-fast invariant inherited
    // from `cli-migrate-subcommand-v0`).
    assert!(
        !output.status.success(),
        "unknown-item invocation with --observe-otlp exits non-zero \
         (status: {:?})",
        output.status
    );

    // And stdout is empty (the success-path transition line is only
    // written when migrate returns Ok).
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.is_empty(),
        "stdout is empty on the fail-fast path (got: {stdout:?})"
    );

    // And the sink either does NOT exist or, if it exists (because
    // `OpenOptions::create(true)` ran before `get_entry` short-
    // circuited), contains zero non-empty lines whose metric name is
    // `cinder.migrate.count`. The OK3 emission-absence guarantee
    // ignores the file's mere existence — it pins the absence of the
    // migrate line, not the absence of the file.
    if otlp_path.exists() {
        let content = fs::read_to_string(&otlp_path).expect("read sink");
        let migrate_lines: Vec<Value> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Value>(l).expect("parse OTLP-JSON line"))
            .filter(|v| v["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.migrate.count")
            .collect();
        assert!(
            migrate_lines.is_empty(),
            "sink contains zero cinder.migrate.count lines on the \
             UnknownItem path (got {} line(s): {:?})",
            migrate_lines.len(),
            content
        );
    }

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK4: InvalidTier creates no sink file (subprocess).
//
// Spawns the actual binary with args
// `["migrate", "acme", <data>, "item", "LUKEWARM",
//   "--observe-otlp", <otlp_path>]`. The `parse_tier(...)` call at
// `crates/kaleidoscope-cli/src/lib.rs:431` short-circuits BEFORE the
// internal `match otlp_log_path` block runs (DESIGN DD2 pins the
// match block strictly BETWEEN `parse_tier?` and
// `FileBackedTieringStore::open`). Therefore no `OpenOptions::open(path)`
// call is ever reached on this path and the sink file is NEVER
// created.
//
// Unlike OK3 (where the file may exist empty), OK4 pins file-absence:
// after the call, `otlp_path.exists()` MUST be false. This is the
// load-bearing assertion for the parse-before-open contract;
// mutating the order (opening the file before parsing the tier)
// would be detected here.
// --------------------------------------------------------------------

#[test]
fn migrate_subcommand_invalid_tier_with_observe_otlp_creates_no_sink_file() {
    // Given a fresh data_dir, no Cinder placement at all, and a
    // sink path that does NOT exist before the call.
    let root = temp_root("ok4_invalid_tier_no_file");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let otlp_path = root.join("audit.ndjson");
    assert!(!otlp_path.exists(), "pre-call: sink path does not exist");

    // When Priya invokes the binary with an invalid tier value AND
    // `--observe-otlp <otlp_path>`.
    let output = Command::new(bin())
        .arg("migrate")
        .arg("acme")
        .arg(&data)
        .arg("item")
        .arg("LUKEWARM")
        .arg("--observe-otlp")
        .arg(&otlp_path)
        .output()
        .expect("spawn kaleidoscope-cli migrate");

    // Then exit code is non-zero (fail-fast invariant inherited from
    // `cli-migrate-subcommand-v0`).
    assert!(
        !output.status.success(),
        "invalid-tier invocation with --observe-otlp exits non-zero \
         (status: {:?})",
        output.status
    );

    // And stdout is empty.
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.is_empty(),
        "stdout is empty on the fail-fast path (got: {stdout:?})"
    );

    // And the sink file does NOT exist after the call. This is the
    // OK4 load-bearing assertion: `parse_tier(...)` short-circuits
    // BEFORE the internal `match otlp_log_path` block, so no
    // `OpenOptions::create(true).append(true).open(path)` call is
    // ever reached. A future refactor that moves the open call
    // BEFORE `parse_tier?` would fail this assertion loudly.
    assert!(
        !otlp_path.exists(),
        "sink file does NOT exist after InvalidTier invocation \
         (parse-before-open contract: parse_tier? runs BEFORE \
         OpenOptions::open(path))"
    );

    cleanup(&root);
}
