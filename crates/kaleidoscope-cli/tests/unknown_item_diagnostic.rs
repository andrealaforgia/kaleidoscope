// Kaleidoscope CLI — unknown-item diagnostic wording acceptance test
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

//! # Acceptance tests — unknown-item diagnostic names the bare quoted id
//!
//! Feature: `cinder-unknown-item-diagnostic-v0` (US-01).
//!
//! Priya, a platform SRE, runs `kaleidoscope-cli` against Cinder. When
//! she names an item id that was never placed, BOTH the `migrate` and
//! `get-tier` subcommands must fail closed (exit non-zero) AND print a
//! diagnostic that names the id she typed — **bare and quoted**
//! (`"ghost"`) — matching the wording the CLI `--help` documents
//! (`main.rs:208`, `:245`):
//!
//! ```text
//! cinder migrate: cannot migrate unknown item "<item_id>" for tenant <tenant>
//! ```
//!
//! Today the single shared `MigrateError::UnknownItem` Display arm
//! (`crates/cinder/src/store.rs:55-58`) renders `{item:?}` — the Debug
//! form of the `ItemId` newtype — so the operator sees
//! `unknown item ItemId("ghost") for tenant acme`. The `ItemId(...)`
//! wrapper is internal vocabulary she never typed; she reads it as an
//! internal fault rather than a plain not-found. The DELIVER fix renders
//! `{:?}` on `item.as_str()` (Debug of the `&str`) so the id is quoted
//! but the newtype name is gone.
//!
//! ## Driving port (Mandate 1 — hexagonal boundary)
//!
//! Every scenario drives through the binary entry point — a real
//! `kaleidoscope-cli` subprocess (`CARGO_BIN_EXE_kaleidoscope-cli`)
//! against a real temp data dir on the real filesystem. No internal
//! component (`MigrateError`, `ItemId`, the store) is constructed or
//! asserted on directly. The observable outcome is exit code + captured
//! stderr/stdout — exactly what Priya sees.
//!
//! ## nWave order — RED, not BROKEN (Mandate 7)
//!
//! DISTILL runs BEFORE DELIVER, so the fix does not exist yet. No NEW
//! production symbol is needed: the CLI, both subcommands, and the
//! diagnostic path all already exist and compile. The two unknown-item
//! tests are **behaviourally RED** — they fail today on the live
//! `ItemId(`-leak (the "stderr does NOT contain `ItemId(`" assertion
//! fails, and the documented quoted phrase is absent) and pass ONLY
//! after the DELIVER one-arm fix. They are NOT broken/non-compiling
//! scaffolds, so no `todo!()` stub is required. They carry
//! `#[ignore = "RED until DELIVER: ..."]` so the pre-commit
//! `cargo test --workspace` stays green; prove RED with
//! `cargo test -p kaleidoscope-cli --test unknown_item_diagnostic
//! -- --ignored`.
//!
//! The two control tests are NOT ignored: they assert behaviour that is
//! unchanged by the fix (known-item success; unknown-item fail-closed
//! exit code) and therefore pass today AND after DELIVER — they guard
//! against the wording fix accidentally regressing the exit-code/success
//! contract.
//!
//! ## Falsifiability (DEVOPS C-DEVOPS-4)
//!
//! The load-bearing RED assertion is the ABSENCE of `ItemId(`. The bare
//! quoted substring `"ghost"` appears INSIDE the leaked
//! `ItemId("ghost")` too, so quoted-presence alone would not
//! discriminate old from new wording. The discriminating pair is:
//! (a) the full documented phrase `unknown item "ghost" for tenant acme`
//! is present, AND (b) `ItemId(` is absent. Both are false on today's
//! output and true only on the fixed output. Probed empirically against
//! the built binary at DISTILL time:
//! `cannot migrate unknown item ItemId("ghost") for tenant acme`.
//!
//! ## Harness
//!
//! `tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`, `bin`
//! mirror the sibling `migrate_subcommand.rs` / `get_tier_subcommand.rs`
//! harness shape (rule-of-three extraction deferred per the cluster's
//! D-NewTestFile convention).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};

// --------------------------------------------------------------------
// Helpers (mirror migrate_subcommand.rs / get_tier_subcommand.rs).
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
    p.push(format!("kal-cli-unknown-diag-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// `cinder_base(data_dir)` — kept in lock-step with the private helper
/// at `crates/kaleidoscope-cli/src/lib.rs:122-124`. The binary opens
/// Cinder against exactly this path, so seeding MUST use the same join.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Places one item under `tenant` in `tier`. The store is dropped
/// immediately so the WAL is flushed before the binary reopens it.
fn place_item(data_dir: &Path, tenant: &TenantId, item_id: &str, tier: Tier) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    cinder
        .place(tenant, &ItemId::new(item_id), tier, SystemTime::now())
        .expect("place");
    drop(cinder);
}

/// Absolute path of the binary under test
/// (`CARGO_BIN_EXE_kaleidoscope-cli`). Cargo builds it before the test
/// run because the crate has both `[lib]` and `[[bin]]` targets — same
/// pattern as `cli_binary_smoke.rs`.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// ====================================================================
// Scenario 1 (RED until DELIVER) — AC: unknown-item-migrate-names-the-
// bare-quoted-id.
//
//   Scenario: Unknown item on migrate names the bare quoted id
//     Given no item "ghost" has been placed under tenant "acme"
//     When Priya runs migrate for item "ghost" under tenant "acme"
//     Then the command exits non-zero
//     And stderr contains: cannot migrate unknown item "ghost" for tenant acme
//     And stderr does NOT contain the internal newtype text: ItemId(
//
// RED today: the live arm emits `ItemId("ghost")`, so both the
// quoted-phrase-present and the ItemId(-absent assertions fail. Passes
// only after the DELIVER one-arm fix.
// ====================================================================

#[test]
#[ignore = "RED until DELIVER: store.rs:57 arm still emits ItemId(\"ghost\") (Debug of the newtype) instead of the quoted bare id"]
fn unknown_item_migrate_names_the_bare_quoted_id() {
    // Given a fresh data_dir with NO placement for (acme, ghost).
    let root = temp_root("migrate_unknown_ghost");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya runs migrate for the unplaced id `ghost`.
    let output = Command::new(bin())
        .arg("migrate")
        .arg("acme")
        .arg(&data)
        .arg("ghost")
        .arg("warm")
        .output()
        .expect("spawn kaleidoscope-cli migrate");

    // Then the command exits non-zero (fail-closed; unchanged).
    assert!(
        !output.status.success(),
        "unknown-item migrate exits non-zero (status: {:?})",
        output.status
    );

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    // And stderr names the bare quoted id in the documented phrase.
    assert!(
        stderr.contains("unknown item \"ghost\" for tenant acme"),
        "stderr contains the documented quoted-id phrase \
         `unknown item \"ghost\" for tenant acme` (got: {stderr:?})"
    );

    // And stderr does NOT leak the internal newtype name (the
    // load-bearing falsifiable assertion: `\"ghost\"` also occurs inside
    // `ItemId(\"ghost\")`, so the ABSENCE of `ItemId(` is what
    // discriminates the fixed wording from today's leak).
    assert!(
        !stderr.contains("ItemId("),
        "stderr does NOT contain the internal newtype text `ItemId(` \
         (got: {stderr:?})"
    );

    cleanup(&root);
}

// ====================================================================
// Scenario 2 (RED until DELIVER) — AC: unknown-item-get-tier-names-the-
// bare-quoted-id. Proves get-tier is covered by the SAME shared arm
// (no separate fix). Uses a composite id with a slash to show the id is
// reproduced verbatim, quoted, slash and all.
//
//   Scenario: Unknown item on get-tier names the bare quoted id
//     Given no item "acme/batch-00042" has been placed under tenant "globex"
//     When Priya runs get-tier for item "acme/batch-00042" under tenant "globex"
//     Then the command exits non-zero
//     And stderr contains: cannot migrate unknown item "acme/batch-00042" for tenant globex
//     And stderr does NOT contain the internal newtype text: ItemId(
//
// RED today: the live arm emits `ItemId("acme/batch-00042")`.
// ====================================================================

#[test]
#[ignore = "RED until DELIVER: the shared store.rs:57 arm leaks ItemId(...) on the get-tier unknown-item path too"]
fn unknown_item_get_tier_names_the_bare_quoted_id() {
    // Given a fresh data_dir with NO placement for
    // (globex, acme/batch-00042).
    let root = temp_root("get_tier_unknown_composite");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya runs get-tier for the unplaced composite id under the
    // wrong tenant.
    let output = Command::new(bin())
        .arg("get-tier")
        .arg("globex")
        .arg(&data)
        .arg("acme/batch-00042")
        .output()
        .expect("spawn kaleidoscope-cli get-tier");

    // Then the command exits non-zero (fail-closed; unchanged).
    assert!(
        !output.status.success(),
        "unknown-item get-tier exits non-zero (status: {:?})",
        output.status
    );

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    // And stderr names the bare quoted id verbatim (slash preserved).
    assert!(
        stderr.contains("unknown item \"acme/batch-00042\" for tenant globex"),
        "stderr contains the documented quoted-id phrase \
         `unknown item \"acme/batch-00042\" for tenant globex` \
         (got: {stderr:?})"
    );

    // And stderr does NOT leak the internal newtype name (load-bearing
    // falsifiable assertion — see scenario 1).
    assert!(
        !stderr.contains("ItemId("),
        "stderr does NOT contain the internal newtype text `ItemId(` \
         (got: {stderr:?})"
    );

    cleanup(&root);
}

// ====================================================================
// Control A (PASSES today AND after DELIVER) — AC: known-item-and-exit-
// 1-behaviour-unchanged (success half). Guards against the wording fix
// accidentally regressing the known-item success path.
//
//   Scenario: Known item migrates and exit codes are unchanged
//     Given item "blk-7781" is placed in Hot under tenant "acme"
//     When Priya runs migrate for item "blk-7781" to "warm" under tenant "acme"
//     Then the command exits zero
//     And stdout reads: migrated tenant=acme item=blk-7781 from=hot to=warm
//
// Un-ignored: this behaviour is invariant across the fix.
// ====================================================================

#[test]
fn known_item_migrates_unchanged() {
    // Given `blk-7781` is placed in Hot under tenant `acme`.
    let root = temp_root("control_known_item");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");
    let acme = tenant("acme");
    place_item(&data, &acme, "blk-7781", Tier::Hot);

    // When Priya runs migrate for the known item to `warm`.
    let output = Command::new(bin())
        .arg("migrate")
        .arg("acme")
        .arg(&data)
        .arg("blk-7781")
        .arg("warm")
        .output()
        .expect("spawn kaleidoscope-cli migrate");

    // Then the command exits zero.
    assert!(
        output.status.success(),
        "known-item migrate exits zero (status: {:?}, stderr: {:?})",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // And stdout is exactly the one-line transition report.
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(
        stdout, "migrated tenant=acme item=blk-7781 from=hot to=warm\n",
        "stdout is the exact unchanged transition report"
    );

    cleanup(&root);
}

// ====================================================================
// Control B (PASSES today AND after DELIVER) — AC: known-item-and-exit-
// 1-behaviour-unchanged (fail-closed half). The unknown-item path still
// fails closed with exit 1; only the WORDING changes in DELIVER. This
// control deliberately asserts ONLY the exit code (not the wording), so
// it is invariant across the fix and pins the fail-closed contract
// against any accidental exit-code regression.
//
//   Scenario: Unknown item still fails closed (exit code unchanged)
//     Given no item "ghost" has been placed under tenant "acme"
//     When Priya runs migrate for item "ghost" under tenant "acme"
//     Then the command exits non-zero (fail-closed)
//
// Un-ignored: the exit code is invariant; only the wording (asserted by
// scenario 1) is RED today.
// ====================================================================

#[test]
fn unknown_item_still_fails_closed() {
    // Given a fresh data_dir with NO placement for (acme, ghost).
    let root = temp_root("control_fails_closed");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya runs migrate for the unplaced id `ghost`.
    let output = Command::new(bin())
        .arg("migrate")
        .arg("acme")
        .arg(&data)
        .arg("ghost")
        .arg("warm")
        .output()
        .expect("spawn kaleidoscope-cli migrate");

    // Then the command exits non-zero (fail-closed; unchanged by the
    // wording fix).
    assert!(
        !output.status.success(),
        "unknown-item migrate still fails closed with non-zero exit \
         (status: {:?})",
        output.status
    );

    // And no success line is written to stdout (fail-closed writes
    // nothing to stdout — invariant across the fix).
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.is_empty(),
        "stdout is empty on the fail-closed path (got: {stdout:?})"
    );

    cleanup(&root);
}
