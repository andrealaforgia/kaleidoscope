// Kaleidoscope Loom — slice 01 validate acceptance test
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

//! Slice 01 — `loom validate` walking skeleton
//!
//! Maps to `docs/feature/loom-v0/slices/slice-01-validate.md`.
//! Companion story: US-LO-01.
//!
//! Three test categories:
//!   1. directory carrying valid rules → exit code 0
//!   2. directory carrying broken rules → exit code 1 + diagnostics
//!   3. unreadable directory → exit code 2

use std::fs;
use std::path::{Path, PathBuf};

use loom::validate;

fn temp_dir(test_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "loom-slice01-{}-{}",
        test_name,
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
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, body).expect("write rule file");
}

// --------------------------------------------------------------------
// AC-1.2 — valid directory exits 0
// --------------------------------------------------------------------

#[test]
fn valid_directory_returns_exit_code_zero() {
    let dir = temp_dir("valid");
    write(
        &dir,
        "service-down.toml",
        r#"
[[rules]]
name = "service_down"
query = "up == 0"
severity = "critical"

[[rules.sinks]]
kind = "webhook"
url = "https://ops.acme/alerts"
"#,
    );
    let outcome = validate(&dir);
    assert_eq!(outcome.rules_loaded, 1);
    assert!(outcome.diagnostics.is_empty());
    assert!(outcome.fatal.is_none());
    assert_eq!(outcome.exit_code(), 0);
}

#[test]
fn empty_directory_returns_exit_code_zero_with_zero_rules() {
    // An empty directory is not an error: zero rules loaded, zero
    // diagnostics, exit 0. The operator may be in a fresh repo.
    let dir = temp_dir("empty");
    let outcome = validate(&dir);
    assert_eq!(outcome.rules_loaded, 0);
    assert!(outcome.diagnostics.is_empty());
    assert!(outcome.fatal.is_none());
    assert_eq!(outcome.exit_code(), 0);
}

#[test]
fn directory_with_multiple_valid_files_returns_zero() {
    let dir = temp_dir("multi-valid");
    for i in 0..5 {
        write(
            &dir,
            &format!("rule-{i}.toml"),
            &format!(
                r#"
[[rules]]
name = "rule_{i}"
query = "up == 0"
severity = "warning"
"#
            ),
        );
    }
    let outcome = validate(&dir);
    assert_eq!(outcome.rules_loaded, 5);
    assert!(outcome.diagnostics.is_empty());
    assert_eq!(outcome.exit_code(), 0);
}

// --------------------------------------------------------------------
// AC-1.3 — broken rule files exit 1 with diagnostics
// --------------------------------------------------------------------

#[test]
fn unknown_field_returns_exit_code_one_with_diagnostic() {
    let dir = temp_dir("unknown-field");
    write(
        &dir,
        "typo.toml",
        r#"
[[rules]]
name = "x"
query = "up"
severity = "info"
nme = "this_typo"
"#,
    );
    let outcome = validate(&dir);
    assert_eq!(outcome.rules_loaded, 0);
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.exit_code(), 1);
}

#[test]
fn one_broken_file_among_many_does_not_poison_the_rest() {
    let dir = temp_dir("mixed");
    write(
        &dir,
        "good.toml",
        r#"
[[rules]]
name = "good_rule"
query = "up == 0"
severity = "warning"
"#,
    );
    write(
        &dir,
        "broken.toml",
        r#"
[[rules]]
name = "broken"
query = "up == 0"
severity = "info"
unknown_field = "boom"
"#,
    );
    let outcome = validate(&dir);
    // One rule loaded, one file rejected, exit 1 (any failure → 1).
    assert_eq!(outcome.rules_loaded, 1);
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.exit_code(), 1);
}

#[test]
fn diagnostic_display_includes_file_path_and_message() {
    let dir = temp_dir("display");
    write(
        &dir,
        "typo.toml",
        r#"
[[rules]]
name = "x"
query = "up"
severity = "info"
queery = "boom"
"#,
    );
    let outcome = validate(&dir);
    assert_eq!(outcome.diagnostics.len(), 1);
    let display = outcome.diagnostics[0].display();
    assert!(display.contains("typo.toml"));
    // The suggestion ("did you mean 'query'") should be present
    // because edit distance from "queery" to "query" is 1.
    assert!(display.contains("did you mean"));
}

// --------------------------------------------------------------------
// AC-1.4 — unreadable directory exits 2
// --------------------------------------------------------------------

#[test]
fn missing_directory_returns_exit_code_two_with_fatal_message() {
    let path = std::env::temp_dir().join("loom-slice01-missing-dir-that-does-not-exist");
    // Ensure the path really doesn't exist.
    let _ = fs::remove_dir_all(&path);

    let outcome = validate(&path);
    assert_eq!(outcome.rules_loaded, 0);
    assert!(outcome.fatal.is_some());
    assert_eq!(outcome.exit_code(), 2);
}

// --------------------------------------------------------------------
// KPI 1 — feedback latency ≤ 100 ms on a 50-rule corpus.
// --------------------------------------------------------------------

#[test]
fn validate_completes_under_100_ms_on_50_rule_corpus() {
    let dir = temp_dir("fifty");
    for i in 0..50 {
        write(
            &dir,
            &format!("rule-{i:02}.toml"),
            &format!(
                r#"
[[rules]]
name = "rule_{i:02}"
query = "up == 0"
severity = "warning"
labels = {{ team = "acme-observability", index = "{i}" }}
"#,
            ),
        );
    }
    let start = std::time::Instant::now();
    let outcome = validate(&dir);
    let elapsed = start.elapsed();
    assert_eq!(outcome.rules_loaded, 50);
    assert_eq!(outcome.exit_code(), 0);
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "KPI 1: validate must complete under 100ms on 50-rule corpus; took {elapsed:?}"
    );
}
