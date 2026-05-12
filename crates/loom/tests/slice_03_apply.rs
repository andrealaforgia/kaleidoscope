// Kaleidoscope Loom — slice 03 apply acceptance test
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

//! Slice 03 — `loom apply` acceptance test
//!
//! Maps to `docs/feature/loom-v0/slices/slice-03-apply.md`.
//! Companion story: US-LO-03. KPI 3: the second `loom apply` on
//! the same input writes zero files.

use std::fs;
use std::path::{Path, PathBuf};

use loom::apply;

fn temp_dir(label: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "loom-slice03-{label}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    path.push(unique);
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn write(dir: &Path, name: &str, body: &str) {
    fs::write(dir.join(name), body).expect("write rule file");
}

fn rule_toml(name: &str, query: &str, severity: &str) -> String {
    format!(
        r#"
[[rules]]
name = "{name}"
query = "{query}"
severity = "{severity}"
"#
    )
}

// --------------------------------------------------------------------
// Basic apply paths.
// --------------------------------------------------------------------

#[test]
fn apply_writes_new_rule_from_source_into_empty_destination() {
    let from = temp_dir("write-from");
    let to = temp_dir("write-to");
    write(
        &from,
        "alpha.toml",
        &rule_toml("alpha", "up == 0", "warning"),
    );

    let outcome = apply(&from, &to);
    assert!(outcome.fatal.is_none());
    assert_eq!(outcome.exit_code(), 0);
    assert_eq!(outcome.written.len(), 1);
    assert!(outcome.removed.is_empty());
    assert!(outcome.unchanged.is_empty());

    // File arrived in destination.
    assert!(to.join("alpha.toml").exists());
}

#[test]
fn apply_removes_rule_present_in_destination_but_not_source() {
    let from = temp_dir("rm-from");
    let to = temp_dir("rm-to");
    write(&to, "orphan.toml", &rule_toml("orphan", "up == 0", "info"));

    let outcome = apply(&from, &to);
    assert_eq!(outcome.exit_code(), 0);
    assert!(outcome.written.is_empty());
    assert_eq!(outcome.removed.len(), 1);
    assert!(!to.join("orphan.toml").exists());
}

#[test]
fn apply_overwrites_rule_with_differing_content() {
    let from = temp_dir("ov-from");
    let to = temp_dir("ov-to");
    write(
        &from,
        "alpha.toml",
        &rule_toml("alpha", "up == 0", "critical"),
    );
    write(&to, "alpha.toml", &rule_toml("alpha", "up == 0", "warning"));

    let outcome = apply(&from, &to);
    assert_eq!(outcome.exit_code(), 0);
    assert_eq!(outcome.written.len(), 1);
    let content = fs::read_to_string(to.join("alpha.toml")).expect("read");
    assert!(content.contains("severity = \"critical\""));
}

// --------------------------------------------------------------------
// KPI 3 — idempotency.
// --------------------------------------------------------------------

#[test]
fn second_apply_on_same_input_writes_zero_files() {
    let from = temp_dir("idem-from");
    let to = temp_dir("idem-to");
    for i in 0..5 {
        write(
            &from,
            &format!("rule_{i}.toml"),
            &rule_toml(&format!("rule_{i}"), "up == 0", "warning"),
        );
    }

    // First apply: writes everything.
    let first = apply(&from, &to);
    assert_eq!(first.written.len(), 5);
    assert_eq!(first.removed.len(), 0);
    assert_eq!(first.unchanged.len(), 0);

    // Second apply: no writes, all unchanged.
    let second = apply(&from, &to);
    assert_eq!(
        second.written.len(),
        0,
        "KPI 3: second apply must write zero files; wrote {}",
        second.written.len()
    );
    assert_eq!(second.removed.len(), 0);
    assert_eq!(second.unchanged.len(), 5);
}

#[test]
fn second_apply_render_summary_reports_all_unchanged() {
    let from = temp_dir("rendsum-from");
    let to = temp_dir("rendsum-to");
    write(
        &from,
        "alpha.toml",
        &rule_toml("alpha", "up == 0", "warning"),
    );
    apply(&from, &to);

    let second = apply(&from, &to);
    let rendered = second.render();
    assert!(rendered.contains("0 written"));
    assert!(rendered.contains("0 removed"));
    assert!(rendered.contains("1 unchanged"));
}

// --------------------------------------------------------------------
// Validation gate: broken source means no writes.
// --------------------------------------------------------------------

#[test]
fn broken_source_blocks_apply_and_returns_exit_code_one() {
    let from = temp_dir("brk-from");
    let to = temp_dir("brk-to");
    write(
        &from,
        "broken.toml",
        r#"
[[rules]]
name = "x"
query = "up"
severity = "info"
unknown_field = "boom"
"#,
    );
    // Pre-existing file in destination — must be untouched if apply fails.
    write(&to, "keep.toml", &rule_toml("keep_me", "up == 0", "info"));

    let outcome = apply(&from, &to);
    assert_eq!(outcome.exit_code(), 1);
    assert!(!outcome.diagnostics.is_empty());
    assert!(outcome.written.is_empty());
    assert!(outcome.removed.is_empty());
    // Pre-existing file survives — validation gate held.
    assert!(to.join("keep.toml").exists());
}

#[test]
fn unreadable_source_returns_exit_code_two() {
    let from = std::env::temp_dir().join("loom-slice03-missing-source-dir-x");
    let to = temp_dir("unread-to");
    let _ = fs::remove_dir_all(&from);
    let outcome = apply(&from, &to);
    assert_eq!(outcome.exit_code(), 2);
    assert!(outcome.fatal.is_some());
}

// --------------------------------------------------------------------
// Non-TOML files in destination are preserved untouched.
// --------------------------------------------------------------------

#[test]
fn apply_preserves_non_toml_files_in_destination() {
    let from = temp_dir("preserve-from");
    let to = temp_dir("preserve-to");
    write(
        &from,
        "alpha.toml",
        &rule_toml("alpha", "up == 0", "warning"),
    );
    write(&to, "README.md", "# docs the operator hand-authored");
    write(&to, "deploy.sh", "#!/bin/sh\necho deploying");

    let outcome = apply(&from, &to);
    assert_eq!(outcome.exit_code(), 0);

    // README and shell script untouched.
    assert!(to.join("README.md").exists());
    assert!(to.join("deploy.sh").exists());
    let readme = fs::read_to_string(to.join("README.md")).expect("read README");
    assert_eq!(readme, "# docs the operator hand-authored");
}

// --------------------------------------------------------------------
// Nested directory handling.
// --------------------------------------------------------------------

#[test]
fn apply_handles_nested_subdirectories() {
    let from = temp_dir("nested-from");
    let to = temp_dir("nested-to");
    fs::create_dir_all(from.join("svc/payments")).expect("mkdir");
    fs::write(
        from.join("svc/payments/rules.toml"),
        rule_toml("payments_down", "up == 0", "critical"),
    )
    .expect("write");

    let outcome = apply(&from, &to);
    assert_eq!(outcome.exit_code(), 0);
    assert!(to.join("svc/payments/rules.toml").exists());
}
