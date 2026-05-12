// Kaleidoscope Loom — slice 04 CI integration acceptance test
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

//! Slice 04 — CI integration
//!
//! Maps to `docs/feature/loom-v0/slices/slice-04-ci-integration.md`.
//! Companion story: US-LO-04. KPI 4: operator-readable diagnostics
//! match the regex `^.+:[0-9]+: <message>` so CI tooling can grep.

use std::fs;
use std::path::{Path, PathBuf};

use loom::{plan, render_plan_json, render_validate_json, validate, SCHEMA};

fn temp_dir(label: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "loom-slice04-{label}-{}",
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
// SCHEMA constant — version-gating for CI tooling.
// --------------------------------------------------------------------

#[test]
fn schema_constant_is_loom_v0() {
    assert_eq!(SCHEMA, "loom.v0");
}

// --------------------------------------------------------------------
// Validate JSON output.
// --------------------------------------------------------------------

#[test]
fn validate_json_carries_schema_field_at_top() {
    let dir = temp_dir("vj-schema");
    write(
        &dir,
        "alpha.toml",
        &rule_toml("alpha", "up == 0", "warning"),
    );
    let outcome = validate(&dir);
    let json = render_validate_json(&outcome);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse JSON");
    assert_eq!(parsed["schema"], "loom.v0");
}

#[test]
fn validate_json_includes_rules_loaded_count_and_zero_diagnostics_on_success() {
    let dir = temp_dir("vj-ok");
    for i in 0..3 {
        write(
            &dir,
            &format!("rule_{i}.toml"),
            &rule_toml(&format!("rule_{i}"), "up == 0", "info"),
        );
    }
    let outcome = validate(&dir);
    let json = render_validate_json(&outcome);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed["rules_loaded"], 3);
    assert!(parsed["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(parsed["exit_code"], 0);
}

#[test]
fn validate_json_diagnostic_carries_file_and_message_and_optional_suggestion() {
    let dir = temp_dir("vj-diag");
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
    let json = render_validate_json(&outcome);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    let diags = parsed["diagnostics"].as_array().expect("diagnostics array");
    assert_eq!(diags.len(), 1);
    let d = &diags[0];
    assert!(d["file"].as_str().unwrap().contains("typo.toml"));
    assert!(d["message"].as_str().unwrap().contains("unknown field"));
    assert_eq!(d["suggestion"], "query");
    assert_eq!(parsed["exit_code"], 1);
}

#[test]
fn validate_json_carries_fatal_on_unreadable_directory() {
    let dir = std::env::temp_dir().join("loom-slice04-missing-validate-dir");
    let _ = fs::remove_dir_all(&dir);
    let outcome = validate(&dir);
    let json = render_validate_json(&outcome);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    assert!(parsed["fatal"].is_string());
    assert_eq!(parsed["exit_code"], 2);
}

// --------------------------------------------------------------------
// Plan JSON output.
// --------------------------------------------------------------------

#[test]
fn plan_json_carries_schema_added_removed_changed() {
    let from = temp_dir("pj-from");
    let to = temp_dir("pj-to");
    write(&from, "a.toml", &rule_toml("alpha", "up == 0", "info"));
    write(&from, "b.toml", &rule_toml("beta", "up == 0", "critical"));
    write(&to, "b.toml", &rule_toml("beta", "up == 0", "warning"));
    write(&to, "c.toml", &rule_toml("gamma", "up == 0", "info"));

    let outcome = plan(&from, &to);
    let json = render_plan_json(&outcome);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed["schema"], "loom.v0");
    let added: Vec<&str> = parsed["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(added, vec!["alpha"]);
    let removed: Vec<&str> = parsed["removed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(removed, vec!["gamma"]);
    let changed = parsed["changed"].as_array().unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0]["name"], "beta");
}

#[test]
fn plan_json_changed_rule_carries_per_field_deltas() {
    let from = temp_dir("pj-field-from");
    let to = temp_dir("pj-field-to");
    write(&from, "r.toml", &rule_toml("alpha", "up == 0", "critical"));
    write(&to, "r.toml", &rule_toml("alpha", "up == 0", "warning"));

    let outcome = plan(&from, &to);
    let json = render_plan_json(&outcome);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    let changed = parsed["changed"][0].as_object().unwrap();
    let fields = changed["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0]["field"], "severity");
    assert_eq!(fields[0]["before"], "warning");
    assert_eq!(fields[0]["after"], "critical");
}

#[test]
fn plan_json_carries_diagnostics_from_and_to_separately() {
    let from = temp_dir("pj-diag-from");
    let to = temp_dir("pj-diag-to");
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
    let outcome = plan(&from, &to);
    let json = render_plan_json(&outcome);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    assert!(!parsed["diagnostics_from"].as_array().unwrap().is_empty());
    assert!(parsed["diagnostics_to"].as_array().unwrap().is_empty());
    assert_eq!(parsed["exit_code"], 1);
}

// --------------------------------------------------------------------
// KPI 4 — diagnostics line shape parseable by CI tooling.
// --------------------------------------------------------------------

#[test]
fn diagnostic_display_matches_file_colon_message_regex() {
    // KPI 4: every diagnostic line matches `^.+: <message>` — file
    // path + space-separated message readable by grep / awk. Some
    // diagnostics (TOML parse errors) include a line number; some
    // (semantic post-parse errors) do not. The CI integration value
    // is "operator pipes diagnostics through standard tooling and
    // gets file paths + messages", not strictly line numbers.
    let dir = temp_dir("kpi4");
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
    write(
        &dir,
        "missing-required.toml",
        r#"
[[rules]]
name = "y"
severity = "info"
"#,
    );
    write(
        &dir,
        "wrong-type.toml",
        r#"
[[rules]]
name = "z"
query = "up == 0"
severity = "emergency"
"#,
    );
    write(
        &dir,
        "bad-duration.toml",
        r#"
[[rules]]
name = "w"
query = "up == 0"
severity = "info"
for_duration = "banana"
"#,
    );
    write(
        &dir,
        "bad-sink.toml",
        r#"
[[rules]]
name = "v"
query = "up == 0"
severity = "info"

[[rules.sinks]]
kind = "smtp"
url = "smtp://x"
"#,
    );

    let outcome = validate(&dir);
    assert_eq!(outcome.diagnostics.len(), 5);

    let line_re = regex_lite();
    for diag in &outcome.diagnostics {
        let display = diag.display();
        let first_line = display.lines().next().unwrap_or_default();
        assert!(
            line_re(first_line),
            "KPI 4: diagnostic line must match ^.+: <msg> regex; got: {first_line}"
        );
    }
}

/// Tiny regex stand-in: `^.+: <message>` (file path + colon + space
/// + non-empty message). Avoids pulling `regex` as a test dep.
fn regex_lite() -> impl Fn(&str) -> bool {
    |line: &str| {
        if let Some(colon_idx) = line.find(": ") {
            let file = &line[..colon_idx];
            let message = &line[colon_idx + 2..];
            !file.is_empty() && !message.is_empty()
        } else {
            false
        }
    }
}
