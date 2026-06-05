// Kaleidoscope CLI — `get-tier` subcommand acceptance test
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

//! # Acceptance tests — `get-tier` subcommand
//!
//! Library function:
//!
//! ```text
//! kaleidoscope_cli::get_tier(tenant, data_dir, item_id, writer)
//!     -> Result<(), Error>
//! ```
//!
//! Binary subcommand:
//!
//! ```text
//! kaleidoscope-cli get-tier <tenant_id> <data_dir> <item_id>
//! ```
//!
//! Drives outcomes for feature `cli-get-tier-subcommand-v0`:
//!
//! - **US-01 / OK1 (success)**: when the item is placed, stdout is
//!   `tier=<lowercase>\n` and exit code is 0.
//! - **US-01 / OK2 (unknown-item fail-fast)**: when the item is
//!   not placed, the library returns `Err(Error::CinderMigrate(
//!   MigrateError::UnknownItem))`; the binary exits non-zero with
//!   the `unknown item "<item_id>" for tenant <tenant>` substring
//!   on stderr.
//! - **US-01 / OK3 (tenant isolation)**: placement under tenant A
//!   is not visible to a `get-tier` for tenant B.
//!
//! RED state: this file imports `kaleidoscope_cli::get_tier` which
//! does not yet exist on `lib.rs`. The compile failure IS the
//! outside-in RED gate; DELIVER adds the function.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::get_tier;

// --------------------------------------------------------------------
// Helpers (tenth duplication; rule-of-three extraction deferred per
// DEVOPS forward-compat note).
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
    p.push(format!("kal-cli-get-tier-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

fn place_item(data_dir: &Path, tenant: &TenantId, item_id: &str, tier: Tier) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    cinder
        .place(tenant, &ItemId::new(item_id), tier, SystemTime::now())
        .expect("place");
    drop(cinder);
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Test #1 — OK1 happy path: get-tier returns tier=cold for a placed
// item.
// --------------------------------------------------------------------

#[test]
fn get_tier_returns_lowercase_tier_for_placed_item() {
    // Given Priya has placed item `acme/batch-00042` in Cold for
    // tenant `acme`.
    let root = temp_root("ok1");
    let acme = tenant("acme");
    place_item(&root, &acme, "acme/batch-00042", Tier::Cold);

    // When Priya calls get_tier(acme, root, "acme/batch-00042",
    // &mut buf).
    let mut buf = Vec::<u8>::new();
    let result = get_tier(&acme, &root, "acme/batch-00042", &mut buf);

    // Then the call returns Ok(()) and stdout reads `tier=cold\n`.
    assert!(result.is_ok(), "get_tier returns Ok on placed item");
    let stdout = String::from_utf8(buf).expect("stdout is utf-8");
    assert_eq!(stdout, "tier=cold\n");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK1 happy path: each tier renders as its lowercase ascii
// keyword. Three placements, three queries.
// --------------------------------------------------------------------

#[test]
fn get_tier_renders_each_tier_as_lowercase_keyword() {
    let root = temp_root("ok1_each_tier");
    let acme = tenant("acme");
    place_item(&root, &acme, "h-item", Tier::Hot);
    place_item(&root, &acme, "w-item", Tier::Warm);
    place_item(&root, &acme, "c-item", Tier::Cold);

    for (item, expected) in [
        ("h-item", "tier=hot\n"),
        ("w-item", "tier=warm\n"),
        ("c-item", "tier=cold\n"),
    ] {
        let mut buf = Vec::<u8>::new();
        get_tier(&acme, &root, item, &mut buf).expect("get_tier ok");
        assert_eq!(String::from_utf8(buf).unwrap(), expected, "item {item}");
    }

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK2 unknown-item fail-fast (library-direct).
// --------------------------------------------------------------------

#[test]
fn get_tier_library_direct_unknown_item_returns_err_without_writing_stdout() {
    let root = temp_root("ok2_lib");
    let acme = tenant("acme");
    // Note: we deliberately do NOT place the item.

    let mut buf = Vec::<u8>::new();
    let result = get_tier(&acme, &root, "ghost-item", &mut buf);

    assert!(result.is_err(), "get_tier returns Err on unknown item");
    let err = format!("{}", result.err().unwrap());
    assert!(
        err.contains("ghost-item"),
        "error message names the missing item verbatim; got {err}"
    );
    assert!(buf.is_empty(), "no stdout bytes on Err path; got {buf:?}");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK2 unknown-item fail-fast (subprocess; binary boundary
// observes exit code and stderr prefix).
// --------------------------------------------------------------------

#[test]
fn get_tier_subcommand_unknown_item_exits_nonzero_with_stderr_naming_item() {
    let root = temp_root("ok2_sub");
    let _acme = tenant("acme");
    // No placement.

    let output = Command::new(bin())
        .args(["get-tier", "acme", root.to_str().unwrap(), "ghost-item"])
        .output()
        .expect("spawn binary");

    assert!(
        !output.status.success(),
        "exit code is non-zero on unknown item"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ghost-item"),
        "stderr names the missing item verbatim; got {stderr}"
    );
    assert!(
        output.stdout.is_empty(),
        "no stdout bytes on error path; got {:?}",
        output.stdout
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — OK3 tenant isolation: an item placed under tenant `acme`
// is invisible to a get-tier for tenant `globex`.
// --------------------------------------------------------------------

#[test]
fn get_tier_for_one_tenant_does_not_see_items_placed_under_another() {
    let root = temp_root("ok3");
    let acme = tenant("acme");
    let globex = tenant("globex");
    place_item(&root, &acme, "acme/batch-00042", Tier::Hot);

    // When globex queries the same item id.
    let mut buf = Vec::<u8>::new();
    let result = get_tier(&globex, &root, "acme/batch-00042", &mut buf);

    // Then UnknownItem is raised; tenant isolation invariant holds.
    assert!(
        result.is_err(),
        "globex must not see acme's placement under the same id"
    );

    cleanup(&root);
}
