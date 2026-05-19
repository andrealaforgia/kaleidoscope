// Kaleidoscope CLI — `migrate` subcommand acceptance test
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

//! # Acceptance tests — `migrate` subcommand
//!
//! When the operator invokes
//! `kaleidoscope-cli migrate <tenant> <data_dir> <item_id> <to_tier>`,
//! the dispatcher in `main.rs` calls a new library function
//! `kaleidoscope_cli::migrate(...)` (DESIGN DD1) which:
//!
//! 1. Parses `<to_tier>` via the private `parse_tier(s)` helper
//!    (DESIGN DD3); only `hot`/`warm`/`cold` literal lower-case
//!    strings are accepted — no trim, no case-fold (DD3 rationales
//!    1-2). Any other spelling materialises as
//!    `Error::InvalidTier { value }` (DESIGN DD4) carrying the
//!    verbatim invalid input.
//! 2. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
//!    Box::new(NoopRecorder))` — quiescent recorder, no OTLP file
//!    (DISCUSS D-OutOfScope-Observe). The Lumen store is NEVER
//!    opened (DISCUSS D-NoLumenTouch).
//! 3. Calls `cinder.get_entry(tenant, &ItemId::new(item_id))` to
//!    discover the `from` tier. `None` → returns
//!    `Err(Error::CinderMigrate(MigrateError::UnknownItem { ... }))`
//!    WITHOUT issuing a `migrate` call (no silent insert).
//! 4. Calls `cinder.migrate(tenant, &item, to_tier,
//!    SystemTime::now())` (DESIGN DD2; DISCUSS D-Timestamp — no
//!    `--at` flag).
//! 5. Writes exactly one line to the writer:
//!    `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`
//!    where `<from>` and `<to>` render via the existing
//!    `tier_lowercase` helper as `hot` / `warm` / `cold`.
//!
//! These tests drive the user-visible outcomes of feature
//! `cli-migrate-subcommand-v0`:
//!
//! - **US-01 / OK1 (principal — migrate-success correctness)**:
//!   stdout reports the from→to transition exactly; post-call
//!   `get_entry().tier == to_tier`.
//! - **US-01 / OK2 (leading — unknown-item fail-fast)**: subprocess
//!   exit code non-zero, stderr substring contains the verbatim
//!   item id; Cinder store unchanged.
//! - **US-01 / OK3 (leading — invalid-tier fail-fast)**: subprocess
//!   exit code non-zero, stderr substring contains the verbatim
//!   invalid value; Cinder store unchanged (parse fails BEFORE the
//!   store is opened — DESIGN DD1 step 1).
//! - **US-01 / OK4 (guardrail — idempotent same-tier)**: stdout
//!   reports `from=X to=X` faithfully, exit 0, no special case
//!   (DISCUSS D-Idempotent; DESIGN DD2).
//! - **US-01 (tenant isolation)**: per-tenant isolation invariant
//!   inherited from `cinder::TieringStore`'s per-tenant key.
//!
//! Note on subprocess vs library-direct split (DISTILL DWD-04): the
//! happy-path, idempotent, tenant-isolation, and library-direct
//! unknown-item scenarios are library-direct calls into a `Vec<u8>`
//! writer — they assert exact stdout bytes and exercise the
//! get_entry+migrate composition without subprocess fork overhead.
//! The subprocess tests (#3 and #4) spawn the actual binary built
//! by Cargo (`CARGO_BIN_EXE_kaleidoscope-cli`) and assert on exit
//! code + stderr substrings; this is the only path that exercises
//! the binary boundary (dispatcher arm, `main.rs` glue, exit-code
//! propagation, stderr `kaleidoscope-cli: {e}` prefix). Both
//! library-direct and subprocess shapes serve the same KPIs (OK2
//! and OK3 substring contracts) but at different boundaries.
//!
//! Note on RED state at v0: every test below calls either
//! `kaleidoscope_cli::migrate(...)` or spawns the binary with a
//! `migrate` subcommand. Neither path exists yet on `lib.rs` /
//! `main.rs`. The file will not compile against the current crate
//! — that compile failure IS the RED gate for outside-in TDD
//! (DELIVER wave / Crafty adds the function and the dispatch arm).
//!
//! Note on the harness pattern: the `tenant`, `temp_root`,
//! `cleanup`, `cinder_base`, `lumen_base` helpers are duplicated
//! inline per DISCUSS D-NewTestFile (rule-of-three extraction
//! deferred — this is the EIGHTH test file in the cluster using the
//! same shape, after the seven siblings under `tests/`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::migrate;

// --------------------------------------------------------------------
// Helpers (mirror stats_cinder_tier_distribution.rs harness shape;
// rule-of-three deferral per DWD-02, DISCUSS D-NewTestFile).
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
    p.push(format!("kal-cli-migrate-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// Returns the `cinder_base(data_dir)` path — kept in lock-step with
/// the private helper at `crates/kaleidoscope-cli/src/lib.rs:122-124`.
/// `migrate` will open Cinder against exactly this path, so tests
/// MUST seed against the same join.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Places one item under `tenant` in `tier` against
/// `cinder_base(data_dir)`. The store is dropped immediately so the
/// WAL is flushed to disk before `migrate` reopens it.
fn place_item(data_dir: &Path, tenant: &TenantId, item_id: &str, tier: Tier) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    cinder.place(tenant, &ItemId::new(item_id), tier, SystemTime::now());
    drop(cinder);
}

/// Reopens Cinder and returns the entry for `(tenant, item)`.
/// Used as the post-call OK1 oracle: after a successful migrate the
/// returned entry MUST have `tier == to_tier`.
fn read_entry(data_dir: &Path, tenant: &TenantId, item_id: &str) -> Option<cinder::TierEntry> {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("reopen cinder for read");
    cinder.get_entry(tenant, &ItemId::new(item_id))
}

/// Returns the absolute path of the binary under test
/// (`CARGO_BIN_EXE_kaleidoscope-cli`). Cargo guarantees the binary
/// is built before tests are run when the crate has both `[lib]` and
/// `[[bin]]` targets — same pattern as `tests/cli_binary_smoke.rs`.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Test #1 — OK1 happy path: hot → cold transition.
//
// Library-direct invocation. Seeds one item in Hot for tenant `acme`,
// calls `migrate(..., "cold", &mut buf)`, asserts the exact stdout
// line, that the returned Result is Ok, and that a reopen of the
// Cinder store shows the item now in Cold (the principal OK1 oracle).
// --------------------------------------------------------------------

#[test]
fn migrate_hot_to_cold_emits_transition_line_and_persists_new_tier() {
    // Given Priya has placed item `acme/batch-00042` under tenant
    // `acme` in tier Hot at `<data>/cinder.*`.
    let root = temp_root("ok1_hot_to_cold");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "acme/batch-00042", Tier::Hot);
    assert_eq!(
        read_entry(&data, &acme, "acme/batch-00042").map(|e| e.tier),
        Some(Tier::Hot),
        "pre-call state: item is in Hot"
    );

    // When Priya calls migrate(..., "cold", &mut buf).
    let mut buf = Vec::<u8>::new();
    let result = migrate(&acme, &data, "acme/batch-00042", "cold", &mut buf);

    // Then the call returns Ok(()).
    assert!(
        result.is_ok(),
        "migrate hot→cold returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is exactly the one-line transition report.
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n",
        "stdout is the exact one-line transition report"
    );

    // And reopening Cinder shows the item now in Cold (the OK1
    // post-condition that pins `parse_tier`'s `\"cold\" => Tier::Cold`
    // arm against arm-swap mutants).
    assert_eq!(
        read_entry(&data, &acme, "acme/batch-00042").map(|e| e.tier),
        Some(Tier::Cold),
        "post-call state: item is in Cold"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK4 idempotent same-tier: hot → hot succeeds and the
// stdout line faithfully reports `from=hot to=hot`. No special case
// in the CLI per DISCUSS D-Idempotent / DESIGN DD2.
// --------------------------------------------------------------------

#[test]
fn migrate_same_tier_is_idempotent_and_emits_from_equals_to_line() {
    // Given Priya has placed item `acme/batch-00007` under tenant
    // `acme` in tier Hot.
    let root = temp_root("ok4_idempotent_same_tier");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "acme/batch-00007", Tier::Hot);

    // When Priya re-issues the current tier as the target.
    let mut buf = Vec::<u8>::new();
    let result = migrate(&acme, &data, "acme/batch-00007", "hot", &mut buf);

    // Then the call returns Ok (idempotent same-tier per the
    // underlying TieringStore::migrate API at
    // crates/cinder/src/store.rs:167-188; the CLI faithfully reports
    // the call's outcome and does NOT short-circuit).
    assert!(
        result.is_ok(),
        "migrate hot→hot returns Ok (got Err: {:?})",
        result.err()
    );

    // And the stdout line shows `from=hot to=hot` honestly.
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "migrated tenant=acme item=acme/batch-00007 from=hot to=hot\n",
        "stdout reports from=hot to=hot faithfully (no short-circuit)"
    );

    // And the post-call entry tier is still Hot (the underlying
    // migrate overwrites tier with the same value; reopening the
    // store yields a valid Hot entry).
    assert_eq!(
        read_entry(&data, &acme, "acme/batch-00007").map(|e| e.tier),
        Some(Tier::Hot),
        "post-call state: item remains in Hot"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK2 unknown-item fail-fast (subprocess). Spawns the
// actual binary with args ["migrate", "acme", <data_dir>,
// "ghost-item", "warm"] against a Cinder directory that has NO
// placement for `ghost-item`. Asserts non-zero exit code and that
// stderr contains BOTH the verbatim item id `ghost-item` AND the
// canonical MigrateError::UnknownItem Display fragment ("unknown
// item") — the substring invariant from DISCUSS D-StderrWording /
// DESIGN DD5.
// --------------------------------------------------------------------

#[test]
fn migrate_subcommand_unknown_item_exits_nonzero_with_stderr_naming_item() {
    // Given a fresh data_dir with NO placement for
    // (tenant=acme, item=ghost-item). The Cinder directory may not
    // even exist on disk yet; `migrate` opens cleanly on the empty
    // case (FileBackedTieringStore::open creates the directory).
    let root = temp_root("ok2_subprocess_unknown_item");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya invokes the binary's `migrate` subcommand with the
    // ghost item id.
    let output = Command::new(bin())
        .arg("migrate")
        .arg("acme")
        .arg(&data)
        .arg("ghost-item")
        .arg("warm")
        .output()
        .expect("spawn kaleidoscope-cli migrate");

    // Then exit code is non-zero (the fail-fast invariant).
    assert!(
        !output.status.success(),
        "unknown-item invocation exits non-zero (status: {:?})",
        output.status
    );

    // And stderr contains the verbatim item id `ghost-item`
    // (substring invariant per DISCUSS D-StderrWording — the
    // operator's diagnostic anchor).
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("ghost-item"),
        "stderr contains the verbatim item id (got: {stderr:?})"
    );

    // And stderr contains the canonical `MigrateError::UnknownItem`
    // Display fragment ("unknown item") per
    // crates/cinder/src/store.rs:55-58. Composed with the
    // `Error::CinderMigrate` Display prefix per DESIGN DD4/DD5, the
    // line reads `kaleidoscope-cli: cinder migrate: cannot migrate
    // unknown item "ghost-item" for tenant acme`.
    assert!(
        stderr.contains("unknown item"),
        "stderr contains the canonical MigrateError::UnknownItem \
         Display fragment 'unknown item' (got: {stderr:?})"
    );

    // And stdout is empty (the report line is only written on the
    // success path).
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.is_empty(),
        "stdout is empty on the fail-fast path (got: {stdout:?})"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK3 invalid-tier fail-fast (subprocess). Spawns the
// actual binary with args ["migrate", "acme", <data_dir>, "item_id",
// "LUKEWARM"]. Asserts non-zero exit, stderr substring contains the
// verbatim invalid value `LUKEWARM`, and that the Cinder store was
// NOT opened (the parse fails BEFORE any store-open, per DESIGN DD1
// step 1).
//
// We do NOT pre-seed any Cinder item: the parse path short-circuits
// before `FileBackedTieringStore::open`, so the test does not need a
// placed item to drive the OK3 invariant. Asserting that the Cinder
// store directory does NOT exist after the call would over-specify
// (DESIGN may legitimately mkdir before the parse for defence-in-
// depth); instead we assert the wire-observable invariants only
// (exit code, stderr content, empty stdout).
// --------------------------------------------------------------------

#[test]
fn migrate_subcommand_invalid_tier_exits_nonzero_with_stderr_naming_value() {
    // Given a fresh data_dir; no Cinder placement at all (the parse
    // should fail BEFORE any store is opened).
    let root = temp_root("ok3_subprocess_invalid_tier");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya invokes the binary with an invalid tier value.
    let output = Command::new(bin())
        .arg("migrate")
        .arg("acme")
        .arg(&data)
        .arg("item_id")
        .arg("LUKEWARM")
        .output()
        .expect("spawn kaleidoscope-cli migrate");

    // Then exit code is non-zero (fail-fast invariant).
    assert!(
        !output.status.success(),
        "invalid-tier invocation exits non-zero (status: {:?})",
        output.status
    );

    // And stderr contains the verbatim invalid value `LUKEWARM`
    // (substring invariant per DISCUSS D-StderrWording / DESIGN DD5;
    // the Error::InvalidTier Display impl renders it as
    // `<to_tier> "LUKEWARM": expected one of hot, warm, cold`).
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("LUKEWARM"),
        "stderr contains the verbatim invalid tier value (got: {stderr:?})"
    );

    // And stdout is empty.
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.is_empty(),
        "stdout is empty on the fail-fast path (got: {stdout:?})"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — tenant isolation. Places one item under tenant A
// (`acme`) in Hot, plus the same-named item under tenant B
// (`globex`) in Warm. Calls `migrate` for tenant B with that same
// item id but a target tier (`cold`) different from B's current
// tier (Warm). The call MUST succeed (B has its own placement,
// independent from A's) AND the resulting transition line MUST
// report `from=warm to=cold` (NOT `from=hot to=cold` — proving the
// pre-flight get_entry honoured tenant B's view).
//
// This combines two invariants in one scenario:
// - Tenant isolation on the read side (`get_entry(globex, item)`
//   sees `globex`'s Warm placement, not `acme`'s Hot placement)
// - Tenant isolation on the mutate side (the post-call entry for
//   `acme/item` remains Hot — unchanged)
// --------------------------------------------------------------------

#[test]
fn migrate_for_one_tenant_does_not_affect_other_tenants_same_item_id() {
    // Given Priya has placed item `acme/batch-00042` under tenant
    // `acme` in Hot AND the same item id under tenant `globex` in
    // Warm (cross-tenant confounder: same ItemId, different tenants,
    // same data_dir).
    let root = temp_root("tenant_isolation");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    let globex = tenant("globex");
    place_item(&data, &acme, "acme/batch-00042", Tier::Hot);
    place_item(&data, &globex, "acme/batch-00042", Tier::Warm);
    assert_eq!(
        read_entry(&data, &acme, "acme/batch-00042").map(|e| e.tier),
        Some(Tier::Hot),
        "pre-call: acme's item is in Hot"
    );
    assert_eq!(
        read_entry(&data, &globex, "acme/batch-00042").map(|e| e.tier),
        Some(Tier::Warm),
        "pre-call: globex's same-named item is in Warm"
    );

    // When Priya invokes migrate for tenant `globex` (NOT acme).
    let mut buf = Vec::<u8>::new();
    let result = migrate(&globex, &data, "acme/batch-00042", "cold", &mut buf);

    // Then the call returns Ok (globex has its own placement).
    assert!(
        result.is_ok(),
        "migrate for globex returns Ok (got Err: {:?})",
        result.err()
    );

    // And the stdout line reports `from=warm to=cold` (proving the
    // pre-flight get_entry honoured globex's view — NOT acme's Hot
    // placement).
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "migrated tenant=globex item=acme/batch-00042 from=warm to=cold\n",
        "stdout reports globex's transition (from=warm, NOT from=hot)"
    );

    // And acme's same-named item is byte-equivalent (still Hot —
    // the OK1 tenant-isolation post-condition).
    assert_eq!(
        read_entry(&data, &acme, "acme/batch-00042").map(|e| e.tier),
        Some(Tier::Hot),
        "post-call: acme's item is still in Hot (unchanged)"
    );

    // And globex's item is now in Cold.
    assert_eq!(
        read_entry(&data, &globex, "acme/batch-00042").map(|e| e.tier),
        Some(Tier::Cold),
        "post-call: globex's item is in Cold"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #6 — library-direct fail-fast on unknown item. Mirrors test
// #3 but invokes the library function directly so the inline
// assertion shape is easier to inspect: the test asserts the
// returned `Result` is `Err`, that the writer is empty (no stdout
// bytes written on the fail-fast path), and that the Cinder store
// remains in its pre-call state (no silent insert; the OK2
// no-mutation invariant).
//
// This is the library-direct OK2 companion to subprocess test #3.
// Both serve the same KPI but at different boundaries: this one
// pins the library function's behaviour (no writeln! on the error
// path; pre-flight get_entry short-circuits before any migrate call
// is issued); test #3 pins the binary's behaviour (non-zero exit,
// stderr substring composed via Error::CinderMigrate Display).
// --------------------------------------------------------------------

#[test]
fn migrate_library_direct_unknown_item_returns_err_without_mutating_store() {
    // Given a fresh data_dir with NO placement for
    // (tenant=acme, item=ghost-item). We do place a DIFFERENT item
    // for the same tenant so the Cinder store exists on disk and
    // we can assert the no-mutation invariant against a real
    // snapshot (rather than a non-existent directory).
    let root = temp_root("ok2_library_direct_unknown_item");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "acme/real-item", Tier::Hot);
    assert_eq!(
        read_entry(&data, &acme, "acme/real-item").map(|e| e.tier),
        Some(Tier::Hot),
        "pre-call: the placeholder item is in Hot"
    );

    // When Priya calls migrate with a ghost item id (never placed).
    let mut buf = Vec::<u8>::new();
    let result = migrate(&acme, &data, "ghost-item", "warm", &mut buf);

    // Then the call returns Err.
    assert!(
        result.is_err(),
        "migrate on unknown item returns Err (got Ok)"
    );

    // And the writer is empty (no stdout bytes — the report line is
    // only written on the success path; the pre-flight get_entry
    // short-circuits before writeln!).
    assert!(
        buf.is_empty(),
        "writer is empty on the fail-fast path (got: {buf:?})"
    );

    // And the Display of the error contains the verbatim item id
    // `ghost-item` (the OK2 substring invariant at the Error
    // boundary, before the binary wraps it with the
    // `kaleidoscope-cli: ` prefix).
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("ghost-item"),
        "Err Display contains the verbatim item id (got: {err_msg:?})"
    );

    // And the placeholder item's tier is byte-equivalent (no
    // silent insert; no mutation; the pre-flight get_entry's None
    // arm short-circuits before any migrate call is issued —
    // DESIGN DD2 / DD6).
    assert_eq!(
        read_entry(&data, &acme, "acme/real-item").map(|e| e.tier),
        Some(Tier::Hot),
        "post-call: placeholder item is still in Hot (no mutation)"
    );

    // And the ghost item is STILL not placed (no silent insert).
    assert_eq!(
        read_entry(&data, &acme, "ghost-item"),
        None,
        "post-call: ghost item is still unplaced (no silent insert)"
    );

    cleanup(&root);
}
