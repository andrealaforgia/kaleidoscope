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

//! SLO multi-window-multi-burn-rate synthesis.
//!
//! ADR-0036 names the contract: one SLO declaration produces four
//! PromQL alert rules per Google SRE workbook §14.4 Table 14-3 for a
//! 30-day error budget.
//!
//! The synthesis is deterministic: same inputs produce byte-identical
//! PromQL strings across runs. KPI 5 is pinned by the cross-validation
//! tests in `crates/beacon/tests/slice_06_slo_operator_path.rs`
//! (`cross_validation_above_budget_fires_the_page_rules`,
//! `cross_validation_within_budget_fires_nothing`,
//! `cross_validation_page_limits_are_tighter_than_ticket_limits`), which
//! assert the synthesised firing pattern matches a hand-authored
//! reference over a sustained 24-hour error rate, reading each rule's
//! `budget * threshold` limit back out of the emitted PromQL so there is
//! no second source of truth (ADR-0067 F5; the earlier "slice 05b"
//! deferral is closed).

use std::collections::BTreeMap;
use std::time::Duration;

use crate::types::{Rule, Severity, SinkConfig};

/// Operator-authored SLO declaration. Slice 05 supports the 30-day
/// error-budget configuration only; other periods will revise this
/// type when the workbook tables for 7-day and 90-day budgets land.
#[derive(Debug, Clone)]
pub struct Slo {
    /// Service identifier — used in the synthesised rules' `name`
    /// and `labels.slo_service`.
    pub service: String,
    /// PromQL expression for the "good events" numerator.
    pub sli_good_events: String,
    /// PromQL expression for the "total events" denominator.
    pub sli_total_events: String,
    /// Target availability in `(0.0, 1.0)`. Typically `0.999`,
    /// `0.9999`, etc. Error budget is `1 - target_availability`.
    pub target_availability: f64,
    /// 30-day budget only at v0. The field is carried for forward
    /// compatibility; non-30d values are rejected by the loader's SLO
    /// validation (`RawSlo::into_slo`, ADR-0067 F3) before synthesis, so
    /// a `[[slo]]` with any other `error_budget_period` never produces a
    /// rule.
    pub error_budget_period: Duration,
    /// Sinks every synthesised rule will emit to.
    pub sinks: Vec<SinkConfig>,
    /// Path of the source TOML file. Carried into each synthesised
    /// rule's `slo_source` label for correlation (there is no
    /// `annotations` field on `Rule`; ADR-0067 reconciliation of
    /// ADR-0036).
    pub source_path: Option<String>,
}

/// One row of the Google SRE workbook §14.4 Table 14-3 for a 30-day
/// error budget. Listed in firing order: page-level first
/// (most-aggressive thresholds), then ticket-level.
///
/// Source: <https://sre.google/workbook/alerting-on-slos/>
const MWMBR_TABLE: &[MwmbrRow] = &[
    MwmbrRow {
        severity: Severity::Critical,
        page_or_ticket: "page",
        threshold: 14.4,
        long_window: "1h",
        short_window: "5m",
    },
    MwmbrRow {
        severity: Severity::Critical,
        page_or_ticket: "page",
        threshold: 6.0,
        long_window: "6h",
        short_window: "30m",
    },
    MwmbrRow {
        severity: Severity::Warning,
        page_or_ticket: "ticket",
        threshold: 3.0,
        long_window: "1d",
        short_window: "2h",
    },
    MwmbrRow {
        severity: Severity::Warning,
        page_or_ticket: "ticket",
        threshold: 1.0,
        long_window: "3d",
        short_window: "6h",
    },
];

#[derive(Debug, Clone, Copy)]
struct MwmbrRow {
    severity: Severity,
    page_or_ticket: &'static str,
    threshold: f64,
    long_window: &'static str,
    short_window: &'static str,
}

/// Synthesise the four MWMBR alert rules for an SLO. Deterministic:
/// same inputs always produce byte-identical Rule structs.
pub fn synthesise_slo(slo: &Slo) -> Vec<Rule> {
    MWMBR_TABLE
        .iter()
        .map(|row| synthesise_row(slo, row))
        .collect()
}

fn synthesise_row(slo: &Slo, row: &MwmbrRow) -> Rule {
    let budget = 1.0 - slo.target_availability;
    let query = build_query(
        &slo.sli_good_events,
        &slo.sli_total_events,
        budget,
        row.threshold,
        row.long_window,
        row.short_window,
    );

    let name = format!(
        "{}_slo_{}_{}_{}",
        slo.service, row.page_or_ticket, row.long_window, row.short_window
    );

    let mut labels = BTreeMap::new();
    labels.insert("slo_service".to_string(), slo.service.clone());
    labels.insert(
        "slo_window".to_string(),
        format!("{}/{}", row.long_window, row.short_window),
    );
    if let Some(source) = &slo.source_path {
        labels.insert("slo_source".to_string(), source.clone());
    }

    Rule {
        name,
        query,
        // for_duration is 0 because the multi-window construction is
        // its own dwell: the short window already enforces that the
        // burn rate has held for the short window's duration. Adding
        // for_duration on top would double-count the dwell.
        for_duration: Duration::from_secs(0),
        // The synthesised rule's evaluation interval is 30 s — fast
        // enough that the burn-rate alerts catch a fast-burn within
        // one short-window's worth of misses.
        interval: Duration::from_secs(30),
        severity: row.severity,
        labels,
        sinks: slo.sinks.clone(),
        inhibits: Vec::new(),
    }
}

/// Build the MWMBR PromQL expression for one row. The shape is:
///
/// ```text
/// (
///   (sum(rate(<total>[<long>])) - sum(rate(<good>[<long>])))
///   / sum(rate(<total>[<long>])) > (<budget> * <threshold>)
/// ) and (
///   (sum(rate(<total>[<short>])) - sum(rate(<good>[<short>])))
///   / sum(rate(<total>[<short>])) > (<budget> * <threshold>)
/// )
/// ```
///
/// Both windows must exceed the burn-rate threshold for the rule to
/// fire. The short window is the dwell; the long window prevents
/// false pages on transient blips.
fn build_query(
    good: &str,
    total: &str,
    budget: f64,
    threshold: f64,
    long_window: &str,
    short_window: &str,
) -> String {
    let limit = budget * threshold;
    format!(
        "((sum(rate({total}[{long}])) - sum(rate({good}[{long}]))) / sum(rate({total}[{long}])) > {limit}) and \
((sum(rate({total}[{short}])) - sum(rate({good}[{short}]))) / sum(rate({total}[{short}])) > {limit})",
        good = good,
        total = total,
        long = long_window,
        short = short_window,
        limit = format_threshold(limit),
    )
}

/// Format a float threshold deterministically. PromQL's parser
/// accepts `0.0001` and `1e-4` interchangeably; we choose decimal so
/// reviewers can match the workbook's table values by eye.
fn format_threshold(value: f64) -> String {
    // Format with up to 10 fractional digits and strip trailing
    // zeros so common values like 0.0144 render naturally.
    let raw = format!("{value:.10}");
    let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}
