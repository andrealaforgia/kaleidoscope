// Kaleidoscope Loom — slice 02 plan acceptance test
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

//! Slice 02 — `loom plan` acceptance test
//!
//! Maps to `docs/feature/loom-v0/slices/slice-02-plan.md`.
//! Companion story: US-LO-02. KPI 2: plan output is byte-equal
//! across successive invocations on the same inputs.

use std::fs;
use std::path::{Path, PathBuf};

use loom::plan;

fn temp_dir(label: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "loom-slice02-{label}-{}",
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
// Empty / identical / completely-different scenarios.
// --------------------------------------------------------------------

#[test]
fn two_empty_directories_yield_empty_plan() {
    let from = temp_dir("empty-from");
    let to = temp_dir("empty-to");
    let outcome = plan(&from, &to);
    assert!(outcome.added.is_empty());
    assert!(outcome.removed.is_empty());
    assert!(outcome.changed.is_empty());
    assert!(outcome.fatal.is_none());
    assert_eq!(outcome.exit_code(), 0);
}

#[test]
fn identical_directories_yield_empty_plan() {
    let from = temp_dir("same-from");
    let to = temp_dir("same-to");
    write(&from, "rule.toml", &rule_toml("alpha", "up == 0", "info"));
    write(&to, "rule.toml", &rule_toml("alpha", "up == 0", "info"));
    let outcome = plan(&from, &to);
    assert!(outcome.added.is_empty());
    assert!(outcome.removed.is_empty());
    assert!(outcome.changed.is_empty());
}

#[test]
fn rule_only_in_from_appears_as_added() {
    let from = temp_dir("added-from");
    let to = temp_dir("added-to");
    write(
        &from,
        "rule.toml",
        &rule_toml("alpha", "up == 0", "warning"),
    );
    let outcome = plan(&from, &to);
    assert_eq!(outcome.added, vec!["alpha"]);
    assert!(outcome.removed.is_empty());
    assert!(outcome.changed.is_empty());
}

#[test]
fn rule_only_in_to_appears_as_removed() {
    let from = temp_dir("removed-from");
    let to = temp_dir("removed-to");
    write(&to, "rule.toml", &rule_toml("alpha", "up == 0", "warning"));
    let outcome = plan(&from, &to);
    assert!(outcome.added.is_empty());
    assert_eq!(outcome.removed, vec!["alpha"]);
    assert!(outcome.changed.is_empty());
}

#[test]
fn rule_present_with_different_severity_appears_as_changed() {
    let from = temp_dir("severity-from");
    let to = temp_dir("severity-to");
    write(
        &from,
        "rule.toml",
        &rule_toml("alpha", "up == 0", "critical"),
    );
    write(&to, "rule.toml", &rule_toml("alpha", "up == 0", "warning"));
    let outcome = plan(&from, &to);
    assert!(outcome.added.is_empty());
    assert!(outcome.removed.is_empty());
    assert_eq!(outcome.changed.len(), 1);
    assert_eq!(outcome.changed[0].name, "alpha");
}

#[test]
fn changed_rule_carries_per_field_delta() {
    let from = temp_dir("delta-from");
    let to = temp_dir("delta-to");
    write(
        &from,
        "rule.toml",
        &rule_toml("alpha", "up == 0", "critical"),
    );
    write(&to, "rule.toml", &rule_toml("alpha", "up == 0", "warning"));
    let outcome = plan(&from, &to);
    let change = &outcome.changed[0];
    assert_eq!(change.fields.len(), 1);
    let field = &change.fields[0];
    assert_eq!(field.field, "severity");
    assert_eq!(field.before, "warning");
    assert_eq!(field.after, "critical");
}

// --------------------------------------------------------------------
// Output ordering — alphabetical within each category.
// --------------------------------------------------------------------

#[test]
fn added_rules_are_sorted_alphabetically() {
    let from = temp_dir("sort-from");
    let to = temp_dir("sort-to");
    write(&from, "z.toml", &rule_toml("zebra", "up == 0", "info"));
    write(&from, "a.toml", &rule_toml("alpha", "up == 0", "info"));
    write(&from, "m.toml", &rule_toml("middle", "up == 0", "info"));
    let outcome = plan(&from, &to);
    assert_eq!(outcome.added, vec!["alpha", "middle", "zebra"]);
}

// --------------------------------------------------------------------
// render() output format + determinism.
// --------------------------------------------------------------------

#[test]
fn render_summarises_with_added_removed_changed_counts() {
    let from = temp_dir("render-from");
    let to = temp_dir("render-to");
    write(&from, "a.toml", &rule_toml("alpha", "up == 0", "info"));
    write(&from, "b.toml", &rule_toml("beta", "up == 0", "warning"));
    write(&to, "b.toml", &rule_toml("beta", "up == 0", "info"));
    write(&to, "c.toml", &rule_toml("gamma", "up == 0", "info"));

    let outcome = plan(&from, &to);
    let rendered = outcome.render(false);
    assert!(rendered.contains("+ added: alpha"));
    assert!(rendered.contains("- removed: gamma"));
    assert!(rendered.contains("~ changed: beta"));
    assert!(rendered.contains("summary: 1 added, 1 removed, 1 changed"));
}

#[test]
fn render_with_diff_flag_includes_per_field_deltas() {
    let from = temp_dir("diff-from");
    let to = temp_dir("diff-to");
    write(&from, "r.toml", &rule_toml("alpha", "up == 0", "critical"));
    write(&to, "r.toml", &rule_toml("alpha", "up == 0", "warning"));

    let outcome = plan(&from, &to);
    let rendered = outcome.render(true);
    assert!(rendered.contains("    severity: warning → critical"));
}

#[test]
fn render_without_diff_flag_omits_per_field_deltas() {
    let from = temp_dir("nodiff-from");
    let to = temp_dir("nodiff-to");
    write(&from, "r.toml", &rule_toml("alpha", "up == 0", "critical"));
    write(&to, "r.toml", &rule_toml("alpha", "up == 0", "warning"));

    let outcome = plan(&from, &to);
    let rendered = outcome.render(false);
    assert!(!rendered.contains("severity:"));
    assert!(rendered.contains("~ changed: alpha"));
}

#[test]
fn render_is_deterministic_across_100_invocations() {
    // KPI 2: byte-equal output across 100 runs on the same inputs.
    let from = temp_dir("det-from");
    let to = temp_dir("det-to");
    for i in 0..5 {
        write(
            &from,
            &format!("rule_{i}.toml"),
            &rule_toml(&format!("rule_{i}"), "up == 0", "warning"),
        );
    }
    for i in 1..6 {
        // overlap 1-4 with same severity; 5 vs 0 are added/removed
        write(
            &to,
            &format!("rule_{i}.toml"),
            &rule_toml(&format!("rule_{i}"), "up == 0", "warning"),
        );
    }
    // Make one rule actually differ.
    write(
        &to,
        "rule_2.toml",
        &rule_toml("rule_2", "up == 0", "critical"),
    );

    let first = plan(&from, &to).render(true);
    for _ in 0..99 {
        let again = plan(&from, &to).render(true);
        assert_eq!(first, again, "KPI 2: plan render must be deterministic");
    }
}

// --------------------------------------------------------------------
// Exit codes.
// --------------------------------------------------------------------

#[test]
fn plan_with_broken_source_file_exits_with_code_one() {
    let from = temp_dir("brk-from");
    let to = temp_dir("brk-to");
    write(
        &from,
        "broken.toml",
        r#"
[[rules]]
name = "alpha"
query = "up == 0"
severity = "info"
unknown_field = "boom"
"#,
    );
    let outcome = plan(&from, &to);
    assert_eq!(outcome.exit_code(), 1);
    assert!(!outcome.diagnostics_from.is_empty());
}

#[test]
fn plan_with_unreadable_source_exits_with_code_two() {
    let from = std::env::temp_dir().join("loom-slice02-missing-from-dir-doesnt-exist");
    let to = temp_dir("brk2-to");
    let _ = fs::remove_dir_all(&from);
    let outcome = plan(&from, &to);
    assert_eq!(outcome.exit_code(), 2);
    assert!(outcome.fatal.is_some());
}
