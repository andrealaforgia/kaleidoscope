// Kaleidoscope Beacon — slice 02 CUE catalogue acceptance test
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

//! Slice 02 — Catalogue loader
//!
//! Maps to `docs/feature/beacon-v0/slices/slice-02-cue-catalogue.md`.
//! Companion story: US-BE-02.
//!
//! ADR-0034 named CUE as the v0 catalogue language; the Knowledge
//! Gap authorised a TOML fallback if the Rust CUE ecosystem could
//! not deliver file + line + field diagnostics. The slice 02 SPIKE
//! confirmed the gap and the ADR was revised: v0 ships TOML. The
//! `_cue_catalogue` filename is kept for slice-to-test traceability;
//! the schema is TOML-shaped, CUE-shaped semantics.
//!
//! Tests:
//!   1. Empty directory loads zero rules with no diagnostics
//!   2. One valid rule file loads one rule
//!   3. Multiple valid rule files load their rules in deterministic order
//!   4. Unknown field triggers a diagnostic with "did you mean" suggestion
//!   5. Missing required field triggers a diagnostic
//!   6. Type mismatch on severity triggers a diagnostic
//!   7. Invalid for_duration value triggers a diagnostic
//!   8. Broken file is reported but does not poison the others
//!   9. Files outside the .toml suffix are silently ignored
//!  10. Nested subdirectories are walked

use std::fs;
use std::path::{Path, PathBuf};

use beacon::{load_rules, Severity};

fn temp_dir(test_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "beacon-slice02-{}-{}",
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

#[test]
fn empty_directory_loads_zero_rules_with_no_diagnostics() {
    let dir = temp_dir("empty");
    let outcome = load_rules(&dir).expect("load");
    assert!(outcome.rules.is_empty());
    assert!(outcome.diagnostics.is_empty());
    assert!(!outcome.has_any_rules());
}

#[test]
fn one_valid_rule_file_loads_one_rule() {
    let dir = temp_dir("one-rule");
    write(
        &dir,
        "service-down.toml",
        r#"
[[rules]]
name = "service_down"
query = "up == 0"
for_duration = "1m"
interval = "30s"
severity = "critical"

[[rules.sinks]]
kind = "webhook"
url = "https://ops.acme/alerts"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.diagnostics.len(), 0);
    assert_eq!(outcome.rules.len(), 1);
    let rule = &outcome.rules[0];
    assert_eq!(rule.name, "service_down");
    assert_eq!(rule.query, "up == 0");
    assert_eq!(rule.severity, Severity::Critical);
}

#[test]
fn multiple_valid_rule_files_load_in_deterministic_order() {
    let dir = temp_dir("multi");
    write(
        &dir,
        "b.toml",
        r#"
[[rules]]
name = "b_rule"
query = "up == 0"
severity = "warning"
"#,
    );
    write(
        &dir,
        "a.toml",
        r#"
[[rules]]
name = "a_rule"
query = "up == 0"
severity = "info"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.diagnostics.len(), 0);
    assert_eq!(outcome.rules.len(), 2);
    // Stable alphabetic order across runs (sort by path).
    assert_eq!(outcome.rules[0].name, "a_rule");
    assert_eq!(outcome.rules[1].name, "b_rule");
}

#[test]
fn unknown_field_triggers_diagnostic_with_did_you_mean_suggestion() {
    let dir = temp_dir("unknown-field");
    write(
        &dir,
        "typo.toml",
        r#"
[[rules]]
name = "typo_rule"
query = "up == 0"
severity = "critical"
nme = "this_typo"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.rules.len(), 0);
    assert_eq!(outcome.diagnostics.len(), 1);
    let diag = &outcome.diagnostics[0];
    assert!(
        diag.message.contains("unknown field"),
        "expected 'unknown field' in message, got: {}",
        diag.message
    );
    assert_eq!(diag.suggestion.as_deref(), Some("name"));
}

#[test]
fn missing_required_field_triggers_diagnostic() {
    let dir = temp_dir("missing-required");
    write(
        &dir,
        "missing.toml",
        r#"
[[rules]]
name = "missing_query"
severity = "info"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.rules.len(), 0);
    assert_eq!(outcome.diagnostics.len(), 1);
    let diag = &outcome.diagnostics[0];
    assert!(
        diag.message.contains("missing field") || diag.message.contains("query"),
        "expected missing-field diagnostic, got: {}",
        diag.message
    );
}

#[test]
fn type_mismatch_on_severity_triggers_diagnostic() {
    let dir = temp_dir("severity-mismatch");
    write(
        &dir,
        "bad.toml",
        r#"
[[rules]]
name = "bad_severity"
query = "up == 0"
severity = "emergency"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.rules.len(), 0);
    assert_eq!(outcome.diagnostics.len(), 1);
    // toml/serde produces an "unknown variant" style message for enums
    let diag = &outcome.diagnostics[0];
    assert!(
        diag.message.to_lowercase().contains("emergency")
            || diag.message.contains("severity")
            || diag.message.contains("variant"),
        "expected severity variant diagnostic, got: {}",
        diag.message
    );
}

#[test]
fn invalid_for_duration_triggers_diagnostic() {
    let dir = temp_dir("bad-duration");
    write(
        &dir,
        "bad-dur.toml",
        r#"
[[rules]]
name = "bad_duration"
query = "up == 0"
severity = "info"
for_duration = "banana"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.rules.len(), 0);
    assert_eq!(outcome.diagnostics.len(), 1);
    let diag = &outcome.diagnostics[0];
    assert!(
        diag.message.contains("for_duration") || diag.message.contains("banana"),
        "expected for_duration diagnostic, got: {}",
        diag.message
    );
}

#[test]
fn broken_file_does_not_poison_the_other_files() {
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
name = "broken_rule"
query = "up == 0"
severity = "info"
unknown_field = "boom"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.rules.len(), 1);
    assert_eq!(outcome.rules[0].name, "good_rule");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert!(outcome.has_any_rules());
    assert!(outcome.has_diagnostics());
}

#[test]
fn non_toml_files_are_silently_ignored() {
    let dir = temp_dir("non-toml");
    write(&dir, "rule.toml.bak", "this is not toml");
    write(&dir, "README.md", "# docs");
    write(
        &dir,
        "rule.toml",
        r#"
[[rules]]
name = "real_rule"
query = "up == 0"
severity = "info"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.rules.len(), 1);
    assert_eq!(outcome.diagnostics.len(), 0);
}

#[test]
fn nested_subdirectories_are_walked() {
    let dir = temp_dir("nested");
    write(
        &dir,
        "top.toml",
        r#"
[[rules]]
name = "top_rule"
query = "up == 0"
severity = "info"
"#,
    );
    write(
        &dir,
        "subdir/deeper.toml",
        r#"
[[rules]]
name = "deep_rule"
query = "up == 0"
severity = "warning"
"#,
    );
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.rules.len(), 2);
    assert_eq!(outcome.diagnostics.len(), 0);
    let names: Vec<&str> = outcome.rules.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"top_rule"));
    assert!(names.contains(&"deep_rule"));
}

#[test]
fn loader_diagnostic_display_includes_file_and_optional_suggestion() {
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
    let outcome = load_rules(&dir).expect("load");
    assert_eq!(outcome.diagnostics.len(), 1);
    let display = outcome.diagnostics[0].display();
    assert!(display.contains("typo.toml"));
    assert!(display.contains("did you mean"));
    assert!(display.contains("query"));
}
