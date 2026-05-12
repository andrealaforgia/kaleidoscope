// Kaleidoscope Beacon — rule-evaluation + alerting engine
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

//! Multi-file TOML rule loader.
//!
//! Slice 02 loads `*.toml` files from a directory tree. Each file
//! contains zero or more `[[rules]]` tables; each rule is deserialised
//! into a [`Rule`]. The loader is defensive: a single broken rule
//! produces a [`LoaderDiagnostic`] but does not poison the others.
//!
//! ADR-0034 named CUE as the v0 catalogue language; the Knowledge Gap
//! authorised a TOML fallback if the Rust CUE ecosystem could not
//! deliver file + line + field diagnostics. v0 ships TOML. Loom's
//! eventual Git-backed CUE authority will compile down to the same
//! schema, so the migration is a parser swap, not a schema change.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::types::{Rule, Severity};

/// Result of attempting to load one rule directory.
///
/// `rules` is every rule that parsed cleanly. `diagnostics` is every
/// file that failed to parse, with operator-readable error text.
/// Slice 02 contract: a single broken rule is reported and skipped;
/// the rest of the catalogue is preserved.
#[derive(Debug, Default)]
pub struct LoadOutcome {
    pub rules: Vec<Rule>,
    pub diagnostics: Vec<LoaderDiagnostic>,
}

impl LoadOutcome {
    /// True if at least one rule loaded.
    pub fn has_any_rules(&self) -> bool {
        !self.rules.is_empty()
    }

    /// True if any file failed to parse.
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// One per failed `.toml` file. Carries the path, the parser error,
/// and (when the failure is an unknown field) the
/// `nearest_blessed_match` suggestion.
#[derive(Debug, Clone)]
pub struct LoaderDiagnostic {
    pub file: PathBuf,
    pub message: String,
    pub suggestion: Option<String>,
}

impl LoaderDiagnostic {
    /// Operator-readable single-line summary.
    pub fn display(&self) -> String {
        let file = self.file.display();
        let base = format!("{file}: {}", self.message);
        if let Some(suggestion) = &self.suggestion {
            format!("{base}\n    did you mean \"{suggestion}\"?")
        } else {
            base
        }
    }
}

/// Hard failure: cannot read the directory at all, or no files inside.
#[derive(Debug)]
pub enum LoaderError {
    DirectoryNotReadable { path: PathBuf, message: String },
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::DirectoryNotReadable { path, message } => {
                write!(
                    f,
                    "cannot read rule directory {}: {message}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for LoaderError {}

/// Walk `dir` recursively, parse every `*.toml`, and return the
/// per-rule outcome. Non-`.toml` files are ignored silently;
/// non-readable files produce a diagnostic but do not abort the load.
pub fn load_rules(dir: &Path) -> Result<LoadOutcome, LoaderError> {
    let mut outcome = LoadOutcome::default();
    let entries = fs::read_dir(dir).map_err(|err| LoaderError::DirectoryNotReadable {
        path: dir.to_path_buf(),
        message: err.to_string(),
    })?;

    let mut paths: Vec<PathBuf> = Vec::new();
    collect_toml_files(&mut paths, entries)?;
    // Stable order so diagnostics and rule precedence are
    // deterministic across operating systems and filesystems.
    paths.sort();

    for path in paths {
        match parse_file(&path) {
            Ok(rules) => outcome.rules.extend(rules),
            Err(diag) => outcome.diagnostics.push(diag),
        }
    }

    Ok(outcome)
}

fn collect_toml_files(out: &mut Vec<PathBuf>, entries: fs::ReadDir) -> Result<(), LoaderError> {
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let sub = fs::read_dir(&path).map_err(|err| LoaderError::DirectoryNotReadable {
                path: path.clone(),
                message: err.to_string(),
            })?;
            collect_toml_files(out, sub)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            out.push(path);
        }
    }
    Ok(())
}

fn parse_file(path: &Path) -> Result<Vec<Rule>, LoaderDiagnostic> {
    let text = fs::read_to_string(path).map_err(|err| LoaderDiagnostic {
        file: path.to_path_buf(),
        message: format!("read failed: {err}"),
        suggestion: None,
    })?;

    let parsed: FileShape = toml::from_str(&text).map_err(|err| {
        let raw = err.to_string();
        // Surface the unknown-field case with a "did you mean"
        // suggestion. toml's error type does not expose the offending
        // field name directly; we sniff the message text.
        let suggestion = sniff_unknown_field_suggestion(&raw);
        LoaderDiagnostic {
            file: path.to_path_buf(),
            message: raw,
            suggestion,
        }
    })?;

    let mut rules = Vec::with_capacity(parsed.rules.len());
    for raw in parsed.rules {
        match raw.into_rule() {
            Ok(rule) => rules.push(rule),
            Err(message) => {
                return Err(LoaderDiagnostic {
                    file: path.to_path_buf(),
                    message,
                    suggestion: None,
                });
            }
        }
    }
    Ok(rules)
}

/// Sniff the toml error text for an "unknown field" pattern and
/// produce a nearest-match suggestion against the known schema. The
/// algorithm is Levenshtein edit distance, threshold 3.
fn sniff_unknown_field_suggestion(message: &str) -> Option<String> {
    // toml 0.8's error text shape: `unknown field \`X\`, expected one of \`...\``.
    let needle = "unknown field `";
    let idx = message.find(needle)?;
    let after = &message[idx + needle.len()..];
    let end = after.find('`')?;
    let bad = &after[..end];
    nearest_blessed_match(bad)
}

const BLESSED_FIELDS: &[&str] = &[
    "name",
    "query",
    "for_duration",
    "interval",
    "severity",
    "labels",
    "sinks",
    "kind",
    "url",
];

fn nearest_blessed_match(bad: &str) -> Option<String> {
    let mut best: Option<(&&str, usize)> = None;
    for candidate in BLESSED_FIELDS {
        let distance = levenshtein(bad, candidate);
        if distance > 3 {
            continue;
        }
        match best {
            None => best = Some((candidate, distance)),
            Some((_, d)) if distance < d => best = Some((candidate, distance)),
            _ => {}
        }
    }
    best.map(|(s, _)| (*s).to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

// --------------------------------------------------------------------
// Wire shapes for serde_derive. Kept private to the loader so the
// public `Rule` type stays insulated from TOML.
// --------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileShape {
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    name: String,
    query: String,
    #[serde(default = "default_for_duration")]
    for_duration: String,
    #[serde(default = "default_interval")]
    interval: String,
    severity: RawSeverity,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    sinks: Vec<RawSink>,
}

fn default_for_duration() -> String {
    "1m".to_string()
}

fn default_interval() -> String {
    "30s".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawSeverity {
    Info,
    Warning,
    Critical,
}

impl From<RawSeverity> for Severity {
    fn from(value: RawSeverity) -> Self {
        match value {
            RawSeverity::Info => Severity::Info,
            RawSeverity::Warning => Severity::Warning,
            RawSeverity::Critical => Severity::Critical,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSink {
    kind: String,
    #[serde(default)]
    url: Option<String>,
}

impl RawRule {
    fn into_rule(self) -> Result<Rule, String> {
        let for_duration = parse_duration(&self.for_duration, "for_duration")?;
        let interval = parse_duration(&self.interval, "interval")?;
        // Slice 02 keeps sinks on the rule as a separate concern from
        // construction; the orchestrator interprets the list. We only
        // validate the shape here so the rule itself is type-safe.
        for sink in &self.sinks {
            if sink.kind != "webhook" {
                return Err(format!(
                    "unsupported sink kind \"{}\" (slice 02 supports: webhook)",
                    sink.kind
                ));
            }
            if sink.url.is_none() {
                return Err(format!(
                    "sink kind \"webhook\" requires \"url\" (rule \"{}\")",
                    self.name
                ));
            }
        }
        Ok(Rule {
            name: self.name,
            query: self.query,
            for_duration,
            interval,
            severity: self.severity.into(),
            labels: self.labels,
        })
    }
}

fn parse_duration(raw: &str, field: &str) -> Result<Duration, String> {
    humantime::parse_duration(raw).map_err(|err| {
        format!("invalid {field} value \"{raw}\": {err} (expected e.g. \"30s\", \"5m\", \"1h\")")
    })
}
