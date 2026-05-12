// Kaleidoscope Loom — Git-backed change-control surface
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

//! `loom plan` — deterministic per-rule diff between a source
//! directory (Git working tree) and a destination directory
//! (deployed catalogue).

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use beacon::{load_rules, LoaderDiagnostic, Rule, Severity};

/// Result of one `loom plan` invocation.
#[derive(Debug)]
pub struct PlanOutcome {
    /// Rules present in `from` but not in `to`. Sorted by name.
    pub added: Vec<String>,
    /// Rules present in `to` but not in `from`. Sorted by name.
    pub removed: Vec<String>,
    /// Rules present in both but with differing content. Sorted by name.
    pub changed: Vec<RuleChange>,
    /// Loader diagnostics on the source directory.
    pub diagnostics_from: Vec<LoaderDiagnostic>,
    /// Loader diagnostics on the destination directory.
    pub diagnostics_to: Vec<LoaderDiagnostic>,
    /// Hard error if either directory could not be walked.
    pub fatal: Option<String>,
}

/// Per-field delta for one rule. Field name is one of the
/// `Rule` struct fields the diff covers; `before` and `after` are
/// `Display`-formatted strings for human review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub field: &'static str,
    pub before: String,
    pub after: String,
}

/// One rule's worth of field-level changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleChange {
    pub name: String,
    pub fields: Vec<FieldChange>,
}

impl PlanOutcome {
    /// Exit code per slice 02 AC-2.3.
    ///
    /// - `0` — plan computed, no loader diagnostics
    /// - `1` — any loader diagnostic on either side
    /// - `2` — directory unreadable
    pub fn exit_code(&self) -> u8 {
        if self.fatal.is_some() {
            return 2;
        }
        if !self.diagnostics_from.is_empty() || !self.diagnostics_to.is_empty() {
            return 1;
        }
        0
    }

    /// Render the plan as the operator-readable text format named in
    /// US-LO-02 AC-2.2 / AC-2.4. Deterministic: same inputs produce
    /// byte-equal output.
    pub fn render(&self, include_diff: bool) -> String {
        let mut out = String::new();
        for name in &self.added {
            writeln!(&mut out, "+ added: {name}").expect("write");
        }
        for name in &self.removed {
            writeln!(&mut out, "- removed: {name}").expect("write");
        }
        for change in &self.changed {
            writeln!(&mut out, "~ changed: {}", change.name).expect("write");
            if include_diff {
                for field in &change.fields {
                    writeln!(
                        &mut out,
                        "    {}: {} → {}",
                        field.field, field.before, field.after
                    )
                    .expect("write");
                }
            }
        }
        writeln!(
            &mut out,
            "summary: {} added, {} removed, {} changed",
            self.added.len(),
            self.removed.len(),
            self.changed.len(),
        )
        .expect("write");
        out
    }
}

/// Compute the per-rule diff. `from` is the source (Git working
/// tree); `to` is the destination (deployed catalogue). The diff
/// describes what `loom apply` would do.
pub fn plan(from: &Path, to: &Path) -> PlanOutcome {
    let (from_rules, from_diagnostics, fatal_from) = match load_rules(from) {
        Ok(o) => (o.rules, o.diagnostics, None),
        Err(err) => (Vec::new(), Vec::new(), Some(err.to_string())),
    };
    let (to_rules, to_diagnostics, fatal_to) = match load_rules(to) {
        Ok(o) => (o.rules, o.diagnostics, None),
        Err(err) => (Vec::new(), Vec::new(), Some(err.to_string())),
    };

    let fatal = match (fatal_from, fatal_to) {
        (Some(a), Some(b)) => Some(format!("from: {a}; to: {b}")),
        (Some(a), None) => Some(format!("from: {a}")),
        (None, Some(b)) => Some(format!("to: {b}")),
        (None, None) => None,
    };

    if fatal.is_some() {
        return PlanOutcome {
            added: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
            diagnostics_from: from_diagnostics,
            diagnostics_to: to_diagnostics,
            fatal,
        };
    }

    let from_map: HashMap<&str, &Rule> = from_rules.iter().map(|r| (r.name.as_str(), r)).collect();
    let to_map: HashMap<&str, &Rule> = to_rules.iter().map(|r| (r.name.as_str(), r)).collect();

    let mut added: Vec<String> = from_map
        .keys()
        .filter(|n| !to_map.contains_key(*n))
        .map(|n| (*n).to_string())
        .collect();
    added.sort();

    let mut removed: Vec<String> = to_map
        .keys()
        .filter(|n| !from_map.contains_key(*n))
        .map(|n| (*n).to_string())
        .collect();
    removed.sort();

    let mut changed_names: Vec<&str> = from_map
        .keys()
        .filter(|n| to_map.contains_key(*n))
        .filter(|n| from_map[**n] != to_map[**n])
        .copied()
        .collect();
    changed_names.sort();
    let changed: Vec<RuleChange> = changed_names
        .into_iter()
        .map(|name| diff_rules(name, to_map[name], from_map[name]))
        .collect();

    PlanOutcome {
        added,
        removed,
        changed,
        diagnostics_from: from_diagnostics,
        diagnostics_to: to_diagnostics,
        fatal: None,
    }
}

/// Compute the per-field diff between two rules with the same name.
/// `before` is the deployed rule; `after` is the rule in the Git
/// working tree (what apply would write).
fn diff_rules(name: &str, before: &Rule, after: &Rule) -> RuleChange {
    let mut fields = Vec::new();

    if before.query != after.query {
        fields.push(FieldChange {
            field: "query",
            before: before.query.clone(),
            after: after.query.clone(),
        });
    }
    if before.for_duration != after.for_duration {
        fields.push(FieldChange {
            field: "for_duration",
            before: format!("{:?}", before.for_duration),
            after: format!("{:?}", after.for_duration),
        });
    }
    if before.interval != after.interval {
        fields.push(FieldChange {
            field: "interval",
            before: format!("{:?}", before.interval),
            after: format!("{:?}", after.interval),
        });
    }
    if before.severity != after.severity {
        fields.push(FieldChange {
            field: "severity",
            before: severity_label(before.severity).to_string(),
            after: severity_label(after.severity).to_string(),
        });
    }
    if before.labels != after.labels {
        fields.push(FieldChange {
            field: "labels",
            before: format_labels(&before.labels),
            after: format_labels(&after.labels),
        });
    }
    if before.sinks != after.sinks {
        fields.push(FieldChange {
            field: "sinks",
            before: format!("({} sinks)", before.sinks.len()),
            after: format!("({} sinks)", after.sinks.len()),
        });
    }
    if before.inhibits != after.inhibits {
        fields.push(FieldChange {
            field: "inhibits",
            before: format_inhibits(&before.inhibits),
            after: format_inhibits(&after.inhibits),
        });
    }

    RuleChange {
        name: name.to_string(),
        fields,
    }
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Critical => "critical",
    }
}

fn format_labels(labels: &std::collections::BTreeMap<String, String>) -> String {
    if labels.is_empty() {
        return "{}".to_string();
    }
    let pairs: Vec<String> = labels.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{{{}}}", pairs.join(", "))
}

fn format_inhibits(inhibits: &[String]) -> String {
    if inhibits.is_empty() {
        return "[]".to_string();
    }
    format!("[{}]", inhibits.join(", "))
}
