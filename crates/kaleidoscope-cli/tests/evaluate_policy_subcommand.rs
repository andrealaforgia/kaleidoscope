// Kaleidoscope CLI — `evaluate-policy` subcommand acceptance test
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

//! Acceptance tests for the `evaluate-policy` subcommand.
//!
//! Library function:
//!
//! ```text
//! kaleidoscope_cli::evaluate_policy(
//!     data_dir,
//!     hot_to_warm_secs,
//!     warm_to_cold_secs,
//!     writer,
//!     otlp_log_path,
//! ) -> Result<(), Error>
//! ```
//!
//! Drives outcomes:
//!
//! - OK1: aged items migrate; stdout reports `evaluated migrated=<N>\n`.
//! - OK2: invalid integer args fail fast at the binary boundary.
//! - OK3: a second call with the same `(now, policy)` returns 0
//!   migrations (Cinder API idempotency holds at the CLI layer).
//! - OK4: when `--observe-otlp` is set, exactly N
//!   `cinder.migrate.count` lines land in the sink (one per
//!   internal migration).
//!
//! Tenant-less subcommand — first in the CLI. DISCUSS D5 documents
//! the deviation from the tenant-first convention. The Cinder
//! `evaluate_at` API is cross-tenant by design; the subcommand
//! faithfully maps that shape.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::evaluate_policy;

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
    p.push(format!("kal-cli-eval-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Places one item with an explicit `placed_at`. Required for
/// evaluate-policy tests because the policy compares against
/// `migrated_at` which starts equal to `placed_at` for fresh
/// placements.
fn place_aged(
    data_dir: &Path,
    tenant: &TenantId,
    item_id: &str,
    tier: Tier,
    placed_at: SystemTime,
) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    cinder
        .place(tenant, &ItemId::new(item_id), tier, placed_at)
        .expect("place");
    drop(cinder);
}

fn read_tier(data_dir: &Path, tenant: &TenantId, item_id: &str) -> Option<Tier> {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("reopen cinder");
    cinder.get_tier(tenant, &ItemId::new(item_id))
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Test #1 — OK1 happy path: 3 aged Hot items migrate to Warm.
// --------------------------------------------------------------------

#[test]
fn evaluate_policy_migrates_aged_hot_items_and_reports_count() {
    let root = temp_root("ok1");
    let acme = tenant("acme");
    let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
    for id in ["a", "b", "c"] {
        place_aged(&root, &acme, id, Tier::Hot, two_hours_ago);
    }

    // hot_to_warm = 1 hour (3600); warm_to_cold = 1 day (86400).
    let mut buf = Vec::<u8>::new();
    let result = evaluate_policy(&root, 3600, 86_400, &mut buf, None);
    assert!(result.is_ok(), "evaluate_policy returns Ok");
    let stdout = String::from_utf8(buf).expect("utf8");
    assert_eq!(stdout, "evaluated migrated=3\n");

    // Verify the items are now in Warm.
    for id in ["a", "b", "c"] {
        assert_eq!(read_tier(&root, &acme, id), Some(Tier::Warm), "item {id}");
    }

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK3 idempotent: same args twice produces 0 on second call.
// --------------------------------------------------------------------

#[test]
fn evaluate_policy_is_idempotent_under_repeated_invocation() {
    let root = temp_root("ok3");
    let acme = tenant("acme");
    let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
    place_aged(&root, &acme, "x", Tier::Hot, two_hours_ago);

    let mut buf1 = Vec::<u8>::new();
    evaluate_policy(&root, 3600, 86_400, &mut buf1, None).expect("first call ok");
    assert_eq!(String::from_utf8(buf1).unwrap(), "evaluated migrated=1\n");

    let mut buf2 = Vec::<u8>::new();
    evaluate_policy(&root, 3600, 86_400, &mut buf2, None).expect("second call ok");
    assert_eq!(
        String::from_utf8(buf2).unwrap(),
        "evaluated migrated=0\n",
        "second call must report zero migrations"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK2 invalid hot_to_warm_secs subprocess fail-fast.
// --------------------------------------------------------------------

#[test]
fn evaluate_policy_subcommand_invalid_hot_to_warm_secs_exits_nonzero() {
    let root = temp_root("ok2_hot");
    let output = Command::new(bin())
        .args([
            "evaluate-policy",
            root.to_str().unwrap(),
            "notanumber",
            "86400",
        ])
        .output()
        .expect("spawn binary");

    assert!(!output.status.success(), "exit code non-zero on bad arg");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("notanumber"),
        "stderr names the bad value verbatim; got {stderr}"
    );
    assert!(
        stderr.contains("hot_to_warm"),
        "stderr names which arg was bad; got {stderr}"
    );
    assert!(output.stdout.is_empty(), "no stdout on error path");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK2 invalid warm_to_cold_secs subprocess fail-fast.
// --------------------------------------------------------------------

#[test]
fn evaluate_policy_subcommand_invalid_warm_to_cold_secs_exits_nonzero() {
    let root = temp_root("ok2_warm");
    let output = Command::new(bin())
        .args([
            "evaluate-policy",
            root.to_str().unwrap(),
            "3600",
            "negative-1",
        ])
        .output()
        .expect("spawn binary");

    assert!(!output.status.success(), "exit code non-zero on bad arg");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("negative-1"),
        "stderr names the bad value verbatim; got {stderr}"
    );
    assert!(
        stderr.contains("warm_to_cold"),
        "stderr names which arg was bad; got {stderr}"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — OK4 --observe-otlp emits one cinder.migrate.count line
// per internal migration.
// --------------------------------------------------------------------

#[test]
fn evaluate_policy_with_observe_otlp_emits_one_line_per_migration() {
    let root = temp_root("ok4");
    let acme = tenant("acme");
    let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
    for id in ["m", "n"] {
        place_aged(&root, &acme, id, Tier::Hot, two_hours_ago);
    }

    let otlp = root.join("audit.ndjson");
    let mut buf = Vec::<u8>::new();
    evaluate_policy(&root, 3600, 86_400, &mut buf, Some(&otlp)).expect("evaluate ok");
    assert_eq!(String::from_utf8(buf).unwrap(), "evaluated migrated=2\n");

    let content = fs::read_to_string(&otlp).expect("read otlp");
    let migrate_lines: usize = content
        .lines()
        .filter(|line| line.contains("cinder.migrate.count"))
        .count();
    assert_eq!(
        migrate_lines, 2,
        "expected 2 cinder.migrate.count lines, got {migrate_lines} in:\n{content}"
    );

    cleanup(&root);
}
