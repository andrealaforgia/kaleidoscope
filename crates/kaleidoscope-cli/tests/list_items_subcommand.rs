// Kaleidoscope CLI — `list-items` subcommand acceptance test
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

//! # Acceptance tests — `list-items` subcommand
//!
//! When the operator invokes
//! `kaleidoscope-cli list-items <tenant> <data_dir> <tier>`,
//! the dispatcher in `main.rs` calls a new library function
//! `kaleidoscope_cli::list_items(...)` (DESIGN DD1) which:
//!
//! 1. Parses `<tier>` via the existing `parse_tier(s)` helper
//!    (DESIGN DD4 — promoted to `pub(crate)`); only `hot`/`warm`/
//!    `cold` literal lower-case strings are accepted — no trim,
//!    no case-fold (DISCUSS D-LowerCase). Any other spelling
//!    materialises as `Error::InvalidTier { value }` carrying
//!    the verbatim invalid input (DD5 reuses the existing
//!    `Display` impl verbatim).
//! 2. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
//!    Box::new(NoopRecorder))` — quiescent recorder, no OTLP
//!    file (DISCUSS D-OutOfScope-Observe). The Lumen store is
//!    NEVER opened (DISCUSS D-NoLumenTouch).
//! 3. Calls `cinder.list_by_tier(tenant, tier)` to obtain a
//!    `Vec<ItemId>` of every item this tenant currently has in
//!    the given tier.
//! 4. Sorts the returned vector lexicographically via
//!    `Vec::sort_unstable()` (DESIGN DD2). `ItemId` derives
//!    `Ord` over its inner `String`.
//! 5. Writes one bare item id per line to the supplied writer:
//!    `writeln!(writer, "{}", id.0)` per entry in the sorted
//!    vec. For an empty vec, nothing is written.
//!
//! These tests drive the user-visible outcomes of feature
//! `cli-list-items-subcommand-v0`:
//!
//! - **US-01 / OK1 (principal — list-items correctness)**:
//!   stdout is exactly N lines, each carrying one item id, in
//!   lexicographic byte-order; N equals
//!   `cinder.list_by_tier(tenant, tier).len()` at call time.
//! - **US-01 / OK1 (N=0 case)**: when the queried tier has zero
//!   entries for the tenant, stdout is empty (no header, no
//!   placeholder line, no trailing newline).
//! - **US-01 / OK2 (tenant isolation)**: items belonging to
//!   other tenants never appear on stdout — the per-tenant key
//!   in `cinder::TieringStore` filters them out by construction.
//! - **US-01 / OK3 (invalid-tier fail-fast)**: subprocess exit
//!   code non-zero, stderr substring contains the verbatim
//!   invalid value; the Cinder store is NOT opened (the parse
//!   error fires BEFORE `FileBackedTieringStore::open`).
//! - **US-01 / D-Sort (determinism via lex sort)**: items placed
//!   in non-lex insertion order surface on stdout in
//!   lexicographic order, masking the `HashMap` iteration order
//!   randomness in `cinder::InMemoryTieringStore::list_by_tier`.
//!
//! Note on subprocess vs library-direct split (DISTILL DWD-04):
//! the happy-path, empty-tier, and sort-determinism scenarios
//! are library-direct calls into a `Vec<u8>` writer — they
//! assert exact stdout bytes and exercise the
//! list_by_tier + sort + writeln composition without subprocess
//! fork overhead. The invalid-tier test (#3) spawns the actual
//! binary built by Cargo (`CARGO_BIN_EXE_kaleidoscope-cli`) and
//! asserts on exit code + stderr substring + empty stdout; this
//! is the only path that exercises the binary boundary
//! (dispatcher arm, `main.rs` glue, exit-code propagation,
//! stderr `kaleidoscope-cli: {e}` prefix). The OK3 contract is
//! defined at the binary boundary (it speaks of exit codes and
//! stderr lines), so the subprocess shape is the natural test
//! shape for it.
//!
//! Note on RED state at v0: every test below calls either
//! `kaleidoscope_cli::list_items(...)` or spawns the binary
//! with a `list-items` subcommand. Neither path exists yet on
//! `lib.rs` / `main.rs`. The file will not compile against the
//! current crate — that compile failure IS the RED gate for
//! outside-in TDD (DELIVER wave / Crafty adds the function and
//! the dispatch arm).
//!
//! Note on the harness pattern: the `tenant`, `temp_root`,
//! `cleanup`, `cinder_base`, `place_item`, `bin` helpers are
//! duplicated inline per DEVOPS A2 / DISCUSS D-NewTestFile
//! (rule-of-three extraction deferred — this is the TENTH test
//! file in the cluster using the same shape).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::list_items;

// --------------------------------------------------------------------
// Helpers (mirror migrate_subcommand.rs harness shape; rule-of-three
// extraction deferred per DEVOPS A2 / DISCUSS D-NewTestFile — TENTH
// inline duplication in the cluster).
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
    p.push(format!("kal-cli-list-items-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// Returns the `cinder_base(data_dir)` path — kept in lock-step with
/// the private helper at `crates/kaleidoscope-cli/src/lib.rs:130-132`.
/// `list_items` will open Cinder against exactly this path, so tests
/// MUST seed against the same join.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Places one item under `tenant` in `tier` against
/// `cinder_base(data_dir)`. The store is dropped immediately so the
/// WAL is flushed to disk before `list_items` reopens it.
fn place_item(data_dir: &Path, tenant: &TenantId, item_id: &str, tier: Tier) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    cinder
        .place(tenant, &ItemId::new(item_id), tier, SystemTime::now())
        .expect("place");
    drop(cinder);
}

/// Reopens Cinder and returns `list_by_tier(tenant, tier)`. Used as
/// the OK3 no-mutation oracle: the count under the seeded tier MUST
/// match before and after a failing invocation.
fn list_by_tier(data_dir: &Path, tenant: &TenantId, tier: Tier) -> Vec<ItemId> {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("reopen cinder for read");
    cinder.list_by_tier(tenant, tier)
}

/// Returns the absolute path of the binary under test
/// (`CARGO_BIN_EXE_kaleidoscope-cli`). Cargo guarantees the binary
/// is built before tests are run when the crate has both `[lib]` and
/// `[[bin]]` targets — same pattern as `tests/migrate_subcommand.rs`.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Test #1 — OK1 happy path (with tenant-isolation OK2 in-scenario):
// five Hot items for `acme` placed in non-lex insertion order to
// exercise the lex-sort boundary step, PLUS three Warm items for
// `globex` (a different tenant) which MUST NOT appear in the output.
//
// Library-direct invocation. Asserts:
// - the call returns Ok(())
// - stdout has exactly 5 lines (one per acme Hot item)
// - the 5 lines are in lexicographic byte-order
// - all 5 lines are acme items (the four expected items plus the
//   fifth — see the placement sequence below)
// - NO globex item id appears anywhere in stdout
// --------------------------------------------------------------------

#[test]
fn list_items_hot_for_acme_emits_five_sorted_lines_and_excludes_globex() {
    // Given Priya has placed five items under tenant `acme` in tier
    // Hot at `<data>/cinder.*`, intentionally NOT in lexicographic
    // order so the boundary sort step is exercised.
    let root = temp_root("ok1_happy_path_five_hot_items");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    let globex = tenant("globex");

    // Insertion order is deliberately scrambled: the lex sort at the
    // CLI boundary must surface them as 00007, 00019, 00041, 00050,
    // 00099 regardless of insertion or `HashMap` iteration order.
    place_item(&data, &acme, "acme/batch-00099", Tier::Hot);
    place_item(&data, &acme, "acme/batch-00007", Tier::Hot);
    place_item(&data, &acme, "acme/batch-00050", Tier::Hot);
    place_item(&data, &acme, "acme/batch-00019", Tier::Hot);
    place_item(&data, &acme, "acme/batch-00041", Tier::Hot);

    // And Priya has placed three items under tenant `globex` in tier
    // Warm — a different tenant AND a different tier. They MUST NOT
    // appear in acme's Hot listing.
    place_item(&data, &globex, "globex/batch-aaa", Tier::Warm);
    place_item(&data, &globex, "globex/batch-bbb", Tier::Warm);
    place_item(&data, &globex, "globex/batch-ccc", Tier::Warm);

    // When Priya calls list_items(..., "hot", &mut buf) for tenant
    // acme.
    let mut buf = Vec::<u8>::new();
    let result = list_items(&acme, &data, "hot", &mut buf);

    // Then the call returns Ok(()).
    assert!(
        result.is_ok(),
        "list_items hot for acme returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is exactly the five-line lex-sorted byte sequence.
    // This single assertion pins three OK1 sub-properties at once:
    // (a) exactly N lines (N == 5), (b) lexicographic byte-order,
    // (c) one bare item id per line terminated by `\n`.
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out,
        "acme/batch-00007\n\
         acme/batch-00019\n\
         acme/batch-00041\n\
         acme/batch-00050\n\
         acme/batch-00099\n",
        "stdout is the five-line lex-sorted byte sequence"
    );

    // And stdout has exactly 5 lines (a redundant cardinality check
    // that survives any future change to the exact item id strings).
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5, "stdout has exactly five lines");

    // And the five lines are in lexicographic byte-order (a redundant
    // sortedness check that catches a regression where the sort step
    // is removed but the items happen to land in lex order by
    // coincidence of `HashMap` iteration).
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(
        lines, sorted,
        "the five lines are in lexicographic byte-order"
    );

    // And every line is an acme item.
    for line in &lines {
        assert!(
            line.starts_with("acme/"),
            "every emitted line is an acme item (got: {line:?})"
        );
    }

    // And no globex item id appears anywhere in stdout (the OK2
    // tenant-isolation invariant: the per-tenant key in
    // `cinder::TieringStore` filters globex out at the list_by_tier
    // call site, before we even reach the sort step).
    assert!(
        !out.contains("globex"),
        "no globex item appears in stdout (got: {out:?})"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK1 empty-tier case (N=0): the tenant has items in Hot
// but ZERO items in Cold. Stdout is empty (no header, no placeholder
// line, no trailing newline).
//
// Library-direct invocation. The empty-stdout signal is the natural
// shell-pipeline behaviour for "nothing to iterate": downstream
// `xargs -I {} ...` is a no-op, `wc -l` reports `0`, `grep ...`
// exits non-zero (per `grep` semantics).
// --------------------------------------------------------------------

#[test]
fn list_items_for_empty_tier_emits_no_lines() {
    // Given Priya has placed one item under tenant `acme` in tier
    // Hot (so the Cinder store opens cleanly with at least one
    // entry, but the Cold tier remains empty for this tenant).
    let root = temp_root("ok1_empty_tier_cold");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "acme/batch-00042", Tier::Hot);

    // When Priya calls list_items(..., "cold", &mut buf) — a tier
    // for which acme has no placements.
    let mut buf = Vec::<u8>::new();
    let result = list_items(&acme, &data, "cold", &mut buf);

    // Then the call returns Ok(()).
    assert!(
        result.is_ok(),
        "list_items cold for acme returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is empty (zero bytes — no header, no placeholder,
    // no trailing newline). The absence of bytes IS the result.
    assert!(
        buf.is_empty(),
        "stdout is empty when the queried tier has zero items \
         (got: {buf:?})"
    );

    // And the Hot tier is untouched (a guardrail check on D-ReadOnly:
    // a list query against an empty tier MUST NOT mutate any other
    // tier's contents).
    let hot_after = list_by_tier(&data, &acme, Tier::Hot);
    assert_eq!(
        hot_after.len(),
        1,
        "Hot tier is unchanged after the empty-Cold list query"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK3 invalid-tier fail-fast (subprocess). Spawns the
// actual binary with args ["list-items", "acme", <data_dir>,
// "LUKEWARM"]. Asserts:
// - non-zero exit code (the fail-fast invariant)
// - stderr contains the verbatim invalid value `LUKEWARM`
// - stdout is empty
// - no Cinder side-effect: the pre-seeded Hot entry remains intact,
//   demonstrating the parse error short-circuited BEFORE the store
//   was opened (or, if the store was opened, that no mutation was
//   issued — either way the post-call list_by_tier count matches
//   the pre-call count exactly).
//
// This is the only test that exercises the binary boundary
// (dispatcher arm, `main.rs` glue, exit-code propagation, stderr
// `kaleidoscope-cli: {e}` prefix). The OK3 contract is defined at
// that boundary; the subprocess shape is the natural test shape.
// --------------------------------------------------------------------

#[test]
fn list_items_subcommand_invalid_tier_exits_nonzero_with_stderr_naming_value() {
    // Given a data_dir with one pre-seeded Hot entry for tenant
    // `acme` (so the Cinder store has content on disk and the
    // no-mutation invariant can be checked against a real snapshot,
    // not a non-existent directory).
    let root = temp_root("ok3_subprocess_invalid_tier");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "acme/batch-00042", Tier::Hot);
    let hot_before = list_by_tier(&data, &acme, Tier::Hot);
    assert_eq!(
        hot_before.len(),
        1,
        "pre-call snapshot: Hot tier has exactly one entry"
    );

    // When Priya invokes the binary's `list-items` subcommand with
    // an invalid tier value `LUKEWARM` (neither lower-case
    // hot/warm/cold nor any other accepted spelling).
    let output = Command::new(bin())
        .arg("list-items")
        .arg("acme")
        .arg(&data)
        .arg("LUKEWARM")
        .output()
        .expect("spawn kaleidoscope-cli list-items");

    // Then exit code is non-zero (the fail-fast invariant).
    assert!(
        !output.status.success(),
        "invalid-tier invocation exits non-zero (status: {:?})",
        output.status
    );

    // And stderr contains the verbatim invalid value `LUKEWARM`
    // (substring invariant per DISCUSS D-StderrWording / DESIGN DD5;
    // the Error::InvalidTier Display impl renders it as
    // `invalid tier "LUKEWARM": expected one of hot, warm, cold`,
    // prefixed by `kaleidoscope-cli: ` by main.rs).
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("LUKEWARM"),
        "stderr contains the verbatim invalid tier value (got: {stderr:?})"
    );

    // And stdout is empty (the listing is only written on the
    // success path).
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.is_empty(),
        "stdout is empty on the fail-fast path (got: {stdout:?})"
    );

    // And the Cinder store side is unchanged: the Hot tier still
    // contains exactly the one pre-seeded entry, demonstrating that
    // the invalid-tier branch produced no observable side-effect
    // (no place, no migrate, no evaluate_at — and, in the natural
    // implementation, no store open at all, because the parse
    // short-circuits before FileBackedTieringStore::open).
    let hot_after = list_by_tier(&data, &acme, Tier::Hot);
    assert_eq!(
        hot_after.len(),
        hot_before.len(),
        "post-call snapshot: Hot tier entry count is unchanged \
         (D-ReadOnly preserved on the fail-fast path)"
    );
    assert_eq!(
        hot_after, hot_before,
        "post-call snapshot: Hot tier contents are byte-equivalent \
         to the pre-call snapshot"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — deterministic lex sort. Places three items {z-item,
// a-item, m-item} in Hot for tenant `acme`, in non-lex insertion
// order. Calls list_items(..., "hot", &mut buf) and asserts the
// stdout lines are in alphabetical (lexicographic) order: a-item,
// m-item, z-item.
//
// Library-direct invocation. This test pins the DD2 sort step
// against mutation testing: removing `sort_unstable()` from the
// implementation must be killed by THIS test (the order on stdout
// would otherwise be the `HashMap` iteration order, which is
// randomised per process and would not reliably yield `a, m, z`).
// --------------------------------------------------------------------

#[test]
fn list_items_emits_lines_in_lexicographic_order_regardless_of_insertion_order() {
    // Given Priya has placed three items under tenant `acme` in
    // tier Hot, in a non-lex insertion order (z first, a second,
    // m third — the order that maximises the chance of catching a
    // missing or buggy sort step).
    let root = temp_root("deterministic_sort_z_a_m");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "z-item", Tier::Hot);
    place_item(&data, &acme, "a-item", Tier::Hot);
    place_item(&data, &acme, "m-item", Tier::Hot);

    // When Priya calls list_items(..., "hot", &mut buf).
    let mut buf = Vec::<u8>::new();
    let result = list_items(&acme, &data, "hot", &mut buf);

    // Then the call returns Ok(()).
    assert!(
        result.is_ok(),
        "list_items hot returns Ok (got Err: {:?})",
        result.err()
    );

    // And stdout is exactly the three-line lex-sorted byte sequence:
    // a-item, m-item, z-item — NOT the insertion order (z, a, m).
    let out = std::str::from_utf8(&buf).expect("stdout is UTF-8");
    assert_eq!(
        out, "a-item\nm-item\nz-item\n",
        "stdout lines are in lexicographic order (a, m, z), \
         NOT the insertion order (z, a, m)"
    );

    // And the lines, parsed individually, are alphabetically
    // ordered (a redundant sortedness check that catches a
    // regression where the sort step is removed but the items
    // happen to land in lex order by coincidence of `HashMap`
    // iteration on this particular three-element key set).
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        vec!["a-item", "m-item", "z-item"],
        "parsed lines are in alphabetical order"
    );

    cleanup(&root);
}
