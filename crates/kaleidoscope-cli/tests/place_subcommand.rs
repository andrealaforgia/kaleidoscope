// Kaleidoscope CLI — `place` subcommand acceptance test
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

//! # Acceptance tests — `place` subcommand
//!
//! When Priya the platform operator invokes
//! `kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier>
//!  [--observe-otlp <path>]`,
//! the dispatcher in `main.rs` calls a new library free function
//! `kaleidoscope_cli::place(...)` (DESIGN DD1) which:
//!
//! 1. Parses `<tier>` via the existing private `parse_tier(s)` helper
//!    at `crates/kaleidoscope-cli/src/lib.rs:505-512`. Only the
//!    literal lower-case strings `hot` / `warm` / `cold` are accepted
//!    (DISCUSS D-LowerCase). Any other spelling materialises as
//!    `Error::InvalidTier { value }` carrying the verbatim invalid
//!    input (DESIGN DD3 / DD4).
//! 2. Constructs the Cinder recorder via the byte-for-byte mirror of
//!    `migrate()`'s `match otlp_log_path { Some(p) =>
//!    CinderToOtlpJsonWriter::new(file), None => CinderRecorder }`
//!    pattern (DESIGN DD2). On the `Some(path)` arm the file is
//!    opened once with `OpenOptions::create(true).append(true)` per
//!    ADR-0039 §8.
//! 3. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
//!    recorder)` — Cinder ONLY, the Lumen store is NEVER opened
//!    (DISCUSS D-NoLumenTouch).
//! 4. Calls `cinder.place(tenant, &ItemId::new(item_id), tier,
//!    SystemTime::now())` exactly once (DISCUSS D-Timestamp /
//!    D-Overwrite). The trait method returns `()` — overwrite-
//!    semantics, no failure modes at the trait level.
//! 5. Writes exactly one line to the writer:
//!    `placed tenant=<tenant> item=<item_id> tier=<tier>\n`,
//!    where `<tier>` renders via the existing `tier_lowercase` helper
//!    as `hot` / `warm` / `cold`.
//!
//! These tests drive the four outcome KPIs of the feature:
//!
//! - **US-01 / OK1 (principal — place-success correctness)**: stdout
//!   reports the placement exactly; post-call `get_entry().tier ==
//!   requested_tier`.
//! - **US-01 / OK2 (guardrail — overwrite-semantics fidelity)**:
//!   placing over an existing item updates the entry to the new tier;
//!   no error, no special case (DISCUSS D-Overwrite; DESIGN DD1
//!   rationale 2).
//! - **US-01 / OK3 (invalid-tier fail-fast — subprocess)**: subprocess
//!   exit code non-zero, stderr substring contains the verbatim
//!   invalid value; the Cinder store is unchanged.
//! - **US-01 / OK4 (`--observe-otlp` emission)**: one
//!   `cinder.place.count` OTLP-JSON line per place call when the flag
//!   is supplied; no on-disk OTLP file when the flag is absent.
//!
//! Note on subprocess vs library-direct split (DISTILL DWD-04): the
//! happy-path, overwrite-semantics, and `--observe-otlp` (both
//! present and absent) scenarios are library-direct calls into a
//! `Vec<u8>` writer — they assert exact stdout bytes and exercise the
//! parse + open + place + writeln composition without subprocess fork
//! overhead. The single subprocess test (#3 invalid-tier) spawns the
//! actual binary built by Cargo (`CARGO_BIN_EXE_kaleidoscope-cli`)
//! and asserts on exit code + stderr substrings; this is the only
//! path that exercises the binary boundary (dispatcher arm, `main.rs`
//! glue, exit-code propagation, stderr `kaleidoscope-cli: {e}`
//! prefix). The library-direct and subprocess shapes serve different
//! KPIs at different boundaries.
//!
//! Note on RED state at v0: every test below calls either
//! `kaleidoscope_cli::place(...)` or spawns the binary with a `place`
//! subcommand. Neither path exists yet on `lib.rs` / `main.rs`. The
//! file will not compile against the current crate — that compile
//! failure IS the RED gate for outside-in TDD (DELIVER wave / Crafty
//! adds the free function and the dispatch arm).
//!
//! Note on the harness pattern: the `tenant`, `temp_root`, `cleanup`,
//! `cinder_base`, `place_item`, `read_entry`, `bin` helpers are
//! duplicated inline per DISCUSS D-NewTestFile (rule-of-three
//! extraction deferred — this is the ELEVENTH test file in the
//! `kaleidoscope-cli/tests/` cluster using the same harness shape).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::place;

// --------------------------------------------------------------------
// Helpers (mirror the migrate_subcommand.rs harness shape; rule-of-
// three deferral per DISCUSS D-NewTestFile / DEVOPS forward-compat).
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
    p.push(format!("kal-cli-place-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// Returns the `cinder_base(data_dir)` path — kept in lock-step with
/// the private helper at `crates/kaleidoscope-cli/src/lib.rs:130-132`.
/// `place` opens Cinder against exactly this path, so tests MUST seed
/// (and read back) against the same join.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Places one item under `tenant` in `tier` against
/// `cinder_base(data_dir)`. The store is dropped immediately so the
/// WAL is flushed before `place` reopens it.
fn place_item(data_dir: &Path, tenant: &TenantId, item_id: &str, tier: Tier) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    cinder
        .place(tenant, &ItemId::new(item_id), tier, SystemTime::now())
        .expect("place");
    drop(cinder);
}

/// Reopens Cinder and returns the entry for `(tenant, item)`. Used as
/// the post-call OK1 / OK2 oracle: after a successful place the
/// returned entry MUST have `tier == requested_tier`.
fn read_entry(data_dir: &Path, tenant: &TenantId, item_id: &str) -> Option<cinder::TierEntry> {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("reopen cinder for read");
    cinder.get_entry(tenant, &ItemId::new(item_id))
}

/// Returns the absolute path of the binary under test
/// (`CARGO_BIN_EXE_kaleidoscope-cli`). Cargo guarantees the binary is
/// built before tests run when the crate has both `[lib]` and
/// `[[bin]]` targets — same pattern as `migrate_subcommand.rs::bin()`.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Test #1 — OK1 happy path: fresh placement in Hot.
//
// Library-direct invocation. Against a fresh `data_dir` with NO prior
// placement, calls `place(..., "new-item", "hot", &mut buf, None)`.
// Asserts the returned Result is Ok, the exact stdout line, and that
// a reopen of the Cinder store shows the item in Hot (the principal
// OK1 oracle: stdout report matches the request AND
// `get_entry().tier == requested_tier`).
// --------------------------------------------------------------------

#[test]
fn place_fresh_item_in_hot_emits_placement_line_and_persists_tier() {
    // Given Priya has a fresh `data_dir` with NO prior placement for
    // (tenant=acme, item="new-item").
    let root = temp_root("ok1_fresh_hot");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    assert_eq!(
        read_entry(&data, &acme, "new-item"),
        None,
        "pre-call state: no placement exists"
    );

    // When Priya calls place(..., "new-item", "hot", &mut buf, None).
    let mut buf = Vec::<u8>::new();
    let result = place(&acme, &data, "new-item", "hot", &mut buf, None);

    // Then the call returns Ok(()).
    assert!(
        result.is_ok(),
        "place hot returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is exactly the one-line placement report.
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "placed tenant=acme item=new-item tier=hot\n",
        "stdout is the exact one-line placement report"
    );

    // And reopening Cinder shows the item now in Hot (the OK1 post-
    // condition that pins `parse_tier`'s `"hot" => Tier::Hot` arm
    // against arm-swap mutants, and pins the `tier` argument passed
    // to `TieringStore::place` against a hard-coded substitution).
    assert_eq!(
        read_entry(&data, &acme, "new-item").map(|e| e.tier),
        Some(Tier::Hot),
        "post-call state: item is in Hot"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK2 overwrite-semantics fidelity: Hot → Cold via a second
// `place` call overwrites the first faithfully (no special case in
// the CLI per DISCUSS D-Overwrite / DESIGN DD1 rationale 2; the
// underlying `TieringStore::place` is overwrite-semantics per
// `crates/cinder/src/store.rs:78-81`).
//
// Two consecutive library-direct calls. The first places the item in
// Hot; the second places the SAME item in Cold. The second call must
// return Ok, emit the placement line reflecting the NEW tier (Cold,
// not Hot), and the post-call `get_entry().tier` must be Cold. This
// kills a "guard against existing entry" mutant (an
// `if get_entry(...).is_some() { return Err(...) }` introduction
// before the place call) and a "stdout reports the old tier" mutant.
// --------------------------------------------------------------------

#[test]
fn place_over_existing_item_overwrites_to_new_tier() {
    // Given Priya has placed item `overwrite-item` under tenant `acme`
    // in tier Hot by calling `place(...)` once (the first call is
    // exercised through the production library function — not a
    // seeding helper — so the OK1 happy-path shape is double-covered
    // alongside the OK2 overwrite shape).
    let root = temp_root("ok2_overwrite_hot_to_cold");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");

    let mut first_buf = Vec::<u8>::new();
    let first = place(&acme, &data, "overwrite-item", "hot", &mut first_buf, None);
    assert!(
        first.is_ok(),
        "first place hot returns Ok (got Err: {:?})",
        first.err()
    );
    assert_eq!(
        std::str::from_utf8(&first_buf).expect("stdout is UTF-8"),
        "placed tenant=acme item=overwrite-item tier=hot\n",
        "first call: stdout reports tier=hot"
    );
    assert_eq!(
        read_entry(&data, &acme, "overwrite-item").map(|e| e.tier),
        Some(Tier::Hot),
        "after first call: item is in Hot"
    );

    // When Priya re-places the SAME item with target tier `cold`.
    let mut second_buf = Vec::<u8>::new();
    let second = place(
        &acme,
        &data,
        "overwrite-item",
        "cold",
        &mut second_buf,
        None,
    );

    // Then the second call returns Ok (no special-case rejection;
    // faithful to the underlying overwrite-semantics).
    assert!(
        second.is_ok(),
        "second place cold returns Ok (got Err: {:?})",
        second.err()
    );

    // And the stdout line reflects the NEW tier (Cold), not the old
    // tier (Hot) — pinning the rendering against a "reports the old
    // tier" mutant.
    let out = std::str::from_utf8(&second_buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "placed tenant=acme item=overwrite-item tier=cold\n",
        "second call: stdout reports the new tier (cold), not the old (hot)"
    );

    // And reopening Cinder shows the item now in Cold (the OK2 post-
    // condition: overwrite is faithful at the storage level).
    assert_eq!(
        read_entry(&data, &acme, "overwrite-item").map(|e| e.tier),
        Some(Tier::Cold),
        "post-call state: item is in Cold (the new tier, not Hot)"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK3 invalid-tier fail-fast (subprocess). Spawns the actual
// binary with args ["place", "acme", <data_dir>, "item", "LUKEWARM"].
// Asserts non-zero exit, stderr substring contains the verbatim
// invalid value `LUKEWARM`, and that the Cinder store was NOT mutated
// (the parse fails BEFORE any `place` call is issued, per DESIGN DD1
// step 1 and DD2 short-circuit ordering).
//
// We pre-seed an item for tenant `acme` in Hot so the no-mutation
// invariant can be verified against a real Cinder snapshot
// (byte-equivalent before and after the call).
// --------------------------------------------------------------------

#[test]
fn place_subcommand_invalid_tier_exits_nonzero_with_stderr_naming_value() {
    // Given a fresh `data_dir` with one pre-existing placement for
    // tenant `acme` in tier Hot (the no-mutation oracle).
    let root = temp_root("ok3_subprocess_invalid_tier");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "seed-item", Tier::Hot);
    assert_eq!(
        read_entry(&data, &acme, "seed-item").map(|e| e.tier),
        Some(Tier::Hot),
        "pre-call: the seed item is in Hot"
    );

    // When Priya invokes the binary's `place` subcommand with an
    // invalid tier value.
    let output = Command::new(bin())
        .arg("place")
        .arg("acme")
        .arg(&data)
        .arg("item")
        .arg("LUKEWARM")
        .output()
        .expect("spawn kaleidoscope-cli place");

    // Then exit code is non-zero (fail-fast invariant).
    assert!(
        !output.status.success(),
        "invalid-tier invocation exits non-zero (status: {:?})",
        output.status
    );

    // And stderr contains the verbatim invalid value `LUKEWARM`
    // (substring invariant per DISCUSS D-StderrWording / DESIGN DD5;
    // the inherited `Error::InvalidTier` Display impl at
    // `crates/kaleidoscope-cli/src/lib.rs:98-100` renders the value
    // verbatim).
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("LUKEWARM"),
        "stderr contains the verbatim invalid tier value (got: {stderr:?})"
    );

    // And stdout is empty (the placement line is only written on the
    // success path).
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.is_empty(),
        "stdout is empty on the fail-fast path (got: {stdout:?})"
    );

    // And the seed item's tier is byte-equivalent (no mutation —
    // `parse_tier(...)?` short-circuits BEFORE any `place` call is
    // issued; per DESIGN DD1 step 1 and DD2 short-circuit ordering,
    // reordering the parse to run AFTER the store-open or the place
    // call would fail this assertion).
    assert_eq!(
        read_entry(&data, &acme, "seed-item").map(|e| e.tier),
        Some(Tier::Hot),
        "post-call: seed item is still in Hot (no mutation on invalid-tier path)"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK4 `--observe-otlp` emission (library-direct). Calls
// `place(..., "hot", &mut buf, Some(&otlp_path))` against a fresh
// `data_dir`. Asserts the sink file exists, contains exactly one
// non-empty line, and that line contains the substrings
// `cinder.place.count` (metric name), `acme` (tenant_id resource
// attribute), and `hot` (tier point attribute). This is the
// emission-shape contract for OK4 — byte-identical to the
// `cinder.place.count` lines `ingest` already emits via the same
// `CinderToOtlpJsonWriter`.
//
// We intentionally assert via substring containment rather than full
// JSON-shape parsing: the OK4 contract from `outcome-kpis.md` is
// "the line contains the substrings cinder.place.count, acme, and
// hot". The byte-level wire shape of the OTLP-JSON envelope is
// already pinned by the locked `observe_otlp_cinder_wiring.rs`
// acceptance test for `ingest`'s emission, which uses the same
// `CinderToOtlpJsonWriter` adapter and the same `record_place(...)`
// fan-in — so the wire shape does not need re-pinning here.
// --------------------------------------------------------------------

#[test]
fn place_with_observe_otlp_emits_one_cinder_place_count_line() {
    // Given Priya has a fresh `data_dir` and the OTLP sink path does
    // not yet exist.
    let root = temp_root("ok4_observe_otlp_hot");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let otlp_path = root.join("audit.ndjson");
    assert!(
        !otlp_path.exists(),
        "pre-call: sink file does not yet exist"
    );
    let acme = tenant("acme");

    // When Priya calls place with Some(&otlp_path) and target tier
    // `hot`.
    let mut buf = Vec::<u8>::new();
    let result = place(
        &acme,
        &data,
        "observe-item",
        "hot",
        &mut buf,
        Some(&otlp_path),
    );

    // Then the call returns Ok(()).
    assert!(
        result.is_ok(),
        "place hot with --observe-otlp returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is byte-equivalent to the no-flag path (the flag
    // adds the sink line; it does NOT alter stdout).
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "placed tenant=acme item=observe-item tier=hot\n",
        "stdout is byte-equivalent to the no-flag placement report"
    );

    // And the sink file exists.
    assert!(
        otlp_path.exists(),
        "sink file was created by the Some(path) arm of the construction"
    );

    // And the sink contains exactly ONE non-empty line whose content
    // carries the metric name `cinder.place.count`, the tenant id
    // `acme`, and the tier `hot` as substrings (the OK4 cardinality
    // and content invariant — one place call yields one place line).
    let content = fs::read_to_string(&otlp_path).expect("read sink file");
    let non_empty_lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        non_empty_lines.len(),
        1,
        "exactly one non-empty line per successful place call (got {} lines in sink: {:?})",
        non_empty_lines.len(),
        content
    );

    let line = non_empty_lines[0];
    assert!(
        line.contains("cinder.place.count"),
        "sink line contains metric name cinder.place.count (got: {line:?})"
    );
    assert!(
        line.contains("acme"),
        "sink line contains tenant id acme (got: {line:?})"
    );
    assert!(
        line.contains("hot"),
        "sink line contains tier hot (got: {line:?})"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — OK4 (inverse) `--observe-otlp` absent creates no file
// (library-direct). Calls `place(..., "hot", &mut buf, None)` against
// a fresh `data_dir` and a stable candidate sink path. Asserts the
// call returns Ok, the placement line is emitted to stdout, and the
// candidate sink path does NOT exist after the call. This pins the
// contract that the `None` arm does NOT open ANY file as a side
// effect: the recorder is the quiescent `CinderRecorder`, no
// `OpenOptions::create(true)` call is reached.
//
// Together with Test #4 this kills the "swap Some(path) and None arms
// in the recorder match" mutant (Test #4 would lose its sink file;
// Test #5 would gain an unexpected file).
// --------------------------------------------------------------------

#[test]
fn place_without_observe_otlp_creates_no_file_at_candidate_path() {
    // Given Priya has a fresh `data_dir` and a stable candidate sink
    // path that does not yet exist.
    let root = temp_root("ok4_inverse_no_flag_no_file");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let candidate_path = root.join("audit.ndjson");
    assert!(
        !candidate_path.exists(),
        "pre-call: candidate sink path does not exist"
    );
    let acme = tenant("acme");

    // When Priya calls place with otlp_log_path = None.
    let mut buf = Vec::<u8>::new();
    let result = place(&acme, &data, "no-observe-item", "hot", &mut buf, None);

    // Then the call returns Ok(()) (the place succeeds identically
    // to the OK1 happy path; only the recorder differs).
    assert!(
        result.is_ok(),
        "place with None returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is byte-equivalent to the OK1 happy-path output.
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "placed tenant=acme item=no-observe-item tier=hot\n",
        "stdout is byte-equivalent to the no-flag placement report"
    );

    // And the candidate sink path does NOT exist (the None arm
    // constructs `CinderRecorder` — no file is opened, no
    // `OpenOptions::create(true)` runs against any path).
    assert!(
        !candidate_path.exists(),
        "no file is created at the candidate sink path on the None arm \
         (the recorder is CinderRecorder; no OpenOptions call is reached)"
    );

    cleanup(&root);
}
