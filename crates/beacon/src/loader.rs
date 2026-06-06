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

use crate::slo::{synthesise_slo, Slo};
use crate::types::{Rule, Severity, SinkConfig};

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

    // Track which file each rule name first came from so a collision
    // diagnostic can point the operator at where the duplicate was
    // declared. Synthesised SLO rules and hand-authored rules share this
    // one namespace.
    let mut origin: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in paths {
        match parse_file(&path) {
            Ok(rules) => {
                for rule in &rules {
                    origin
                        .entry(rule.name.clone())
                        .or_insert_with(|| path.clone());
                }
                outcome.rules.extend(rules);
            }
            Err(diag) => outcome.diagnostics.push(diag),
        }
    }

    // Duplicate-name scan over the WHOLE merged catalogue. A collision
    // can span two files (a hand-authored rule in `disk.toml` colliding
    // with a synthesised name from `checkout.toml`), so it runs here,
    // after every file's rules and synthesised SLO rules have been
    // collected, not inside `parse_file`. A duplicate name REFUSES the
    // load with a diagnostic naming the offending rule, never a silent
    // shadow (ADR-0067 F2).
    detect_duplicate_names(&mut outcome, &origin);

    Ok(outcome)
}

/// Scan the merged catalogue for any rule `name` that appears more than
/// once and, for each such name, drop the colliding rules and raise one
/// [`LoaderDiagnostic`]. A collision is never silently shadowed: the
/// operator loses no alerting coverage without being told (ADR-0067 F2).
fn detect_duplicate_names(outcome: &mut LoadOutcome, origin: &BTreeMap<String, PathBuf>) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for rule in &outcome.rules {
        *counts.entry(rule.name.as_str()).or_insert(0) += 1;
    }
    let duplicates: Vec<String> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name.to_string())
        .collect();
    for name in duplicates {
        let file = origin.get(&name).cloned().unwrap_or_default();
        outcome.diagnostics.push(LoaderDiagnostic {
            file,
            message: format!(
                "duplicate rule name \"{name}\": the merged catalogue holds more than one rule named \"{name}\" (a synthesised SLO rule and a hand-authored rule, or two SLOs, collide); rename one so neither is silently shadowed"
            ),
            suggestion: None,
        });
        outcome.rules.retain(|rule| rule.name != name);
    }
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
    // Second pass: each `[[slo]]` is validated and converted to the
    // domain `Slo` (ADR-0067 F3), then expanded via the existing
    // `synthesise_slo` VERBATIM into its four MWMBR rules, which are
    // appended to this file's catalogue (per-file: hand-authored rules
    // first, then synthesised SLO rules, ADR-0067 F2). A malformed SLO
    // fails the whole file exactly as a malformed rule does
    // (report-and-fail-the-file), so no degenerate always-fire rule is
    // ever synthesised, merged, or evaluated.
    let source_path = path.to_string_lossy().into_owned();
    for raw in parsed.slo {
        match raw.into_slo(&source_path) {
            Ok(slo) => rules.extend(synthesise_slo(&slo)),
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
    "inhibits",
    "channel",
    "topic",
    "auth_token_env",
    // SLO (`[[slo]]`) keys, so a near-miss on an SLO key earns the same
    // "did you mean" suggestion a near-miss on a rule key does
    // (ADR-0067 F1).
    "service",
    "good_events_query",
    "total_events_query",
    "target_availability",
    "error_budget_period",
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
    #[serde(default)]
    slo: Vec<RawSlo>,
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
    #[serde(default)]
    inhibits: Vec<String>,
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
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    auth_token_env: Option<String>,
}

impl RawRule {
    fn into_rule(self) -> Result<Rule, String> {
        let for_duration = parse_duration(&self.for_duration, "for_duration")?;
        let interval = parse_duration(&self.interval, "interval")?;
        let sinks = convert_sinks(self.sinks, &format!("rule \"{}\"", self.name))?;
        Ok(Rule {
            name: self.name,
            query: self.query,
            for_duration,
            interval,
            severity: self.severity.into(),
            labels: self.labels,
            sinks,
            inhibits: self.inhibits,
        })
    }
}

/// Validate and convert a list of `[[*.sinks]]` wire entries into
/// [`SinkConfig`]s. Shared by `[[rules]]` and `[[slo]]` so both apply the
/// identical supported-kind / url / topic checks (ADR-0067 F1: the SLO's
/// sinks reuse `RawSink` and its validation verbatim). `owner` names the
/// rule or SLO the sink belongs to, for the error message.
fn convert_sinks(raw_sinks: Vec<RawSink>, owner: &str) -> Result<Vec<SinkConfig>, String> {
    // Slice 04 supported sink kinds. SMTP arrives at v1.
    const SUPPORTED: &[&str] = &["webhook", "mattermost", "zulip", "oncall"];
    let mut sinks = Vec::with_capacity(raw_sinks.len());
    for sink in raw_sinks {
        if !SUPPORTED.contains(&sink.kind.as_str()) {
            return Err(format!(
                "unsupported sink kind \"{}\" (slice 04 supports: {})",
                sink.kind,
                SUPPORTED.join(", ")
            ));
        }
        // Every supported sink kind needs a URL.
        if sink.url.is_none() {
            return Err(format!(
                "sink kind \"{}\" requires \"url\" ({owner})",
                sink.kind
            ));
        }
        // Zulip's incoming-webhook contract requires a topic.
        if sink.kind == "zulip" && sink.topic.is_none() {
            return Err(format!("sink kind \"zulip\" requires \"topic\" ({owner})"));
        }
        sinks.push(SinkConfig {
            kind: sink.kind,
            url: sink.url,
            channel: sink.channel,
            topic: sink.topic,
            auth_token_env: sink.auth_token_env,
        });
    }
    Ok(sinks)
}

/// Wire shape for an `[[slo]]` table (ADR-0067 F1). Kept private to the
/// loader, mirroring `RawRule`. `deny_unknown_fields` so an unknown SLO
/// sub-key (`targt_availability`) earns a parse error + "did you mean".
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSlo {
    service: String,
    good_events_query: String,
    total_events_query: String,
    target_availability: f64,
    #[serde(default = "default_error_budget_period")]
    error_budget_period: String,
    #[serde(default)]
    sinks: Vec<RawSink>,
}

fn default_error_budget_period() -> String {
    "30d".to_string()
}

/// The one supported error budget period at v0 (ADR-0067 F3). The MWMBR
/// workbook thresholds (14.4, 6, 3, 1) assume a 30-day budget; any other
/// period is refused until the workbook tables for it land.
const SUPPORTED_BUDGET_PERIOD: Duration = Duration::from_secs(30 * 24 * 3600);

impl RawSlo {
    /// Validate (ADR-0067 F3) then convert to the domain [`Slo`]. Run
    /// BEFORE `synthesise_slo`, so a degenerate always-fire rule is never
    /// synthesised, merged, or evaluated. Mirrors `RawRule::into_rule`'s
    /// `Result<_, String>` -> per-file `LoaderDiagnostic` error path.
    fn into_slo(self, source_path: &str) -> Result<Slo, String> {
        // 1. target_availability strictly in (0.0, 1.0). Rejects the
        //    always-fire gun at 1.0 (budget 0), the nonsensical 0.0, and
        //    the out-of-range > 1.0 / negative.
        if !(self.target_availability > 0.0 && self.target_availability < 1.0) {
            return Err(format!(
                "invalid target_availability {} (must be strictly greater than 0 and strictly less than 1) in SLO \"{}\"",
                self.target_availability, self.service
            ));
        }
        // 2. error_budget_period must be 30d. Any other duration is
        //    refused (the workbook thresholds are 30d-only).
        let budget = parse_duration(&self.error_budget_period, "error_budget_period")?;
        if budget != SUPPORTED_BUDGET_PERIOD {
            return Err(format!(
                "unsupported error_budget_period \"{}\" (only \"30d\" is supported at v0) in SLO \"{}\"",
                self.error_budget_period, self.service
            ));
        }
        let sinks = convert_sinks(self.sinks, &format!("SLO \"{}\"", self.service))?;
        Ok(Slo {
            service: self.service,
            sli_good_events: self.good_events_query,
            sli_total_events: self.total_events_query,
            target_availability: self.target_availability,
            error_budget_period: budget,
            sinks,
            source_path: Some(source_path.to_string()),
        })
    }
}

fn parse_duration(raw: &str, field: &str) -> Result<Duration, String> {
    humantime::parse_duration(raw).map_err(|err| {
        format!("invalid {field} value \"{raw}\": {err} (expected e.g. \"30s\", \"5m\", \"1h\")")
    })
}
