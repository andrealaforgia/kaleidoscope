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

//! Public types: [`Rule`], [`Incident`], [`Severity`].

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use serde::Serialize;

/// Severity of an alert. Determines pager routing at the binary layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Operator-authored sink configuration. The orchestrator builds a
/// concrete `Sink` adapter from each entry at startup; the library
/// only stores the wire-shaped config.
#[derive(Debug, Clone)]
pub struct SinkConfig {
    /// Adapter discriminator. Slice 02 supports `"webhook"`; slice 04
    /// extends to `"smtp" | "mattermost" | "zulip" | "oncall"`.
    pub kind: String,
    /// Webhook target URL. Other sink kinds will carry their own
    /// configuration fields when slice 04 lands.
    pub url: Option<String>,
}

/// A single alert rule. Slice 01 carries the minimum field set: name,
/// PromQL query, dwell time, severity, sinks. Slice 02 adds optional
/// labels, annotations, and the inhibits-list.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Stable identifier. Mirror of CUE `name`.
    pub name: String,
    /// PromQL expression. Empty result set means "the condition is not
    /// active"; any non-empty result set means "the condition is
    /// active".
    pub query: String,
    /// How long the condition must hold continuously before the rule
    /// transitions Pending → Firing.
    pub for_duration: Duration,
    /// Evaluation interval — how often the binary should re-query the
    /// backend for this rule.
    pub interval: Duration,
    /// Severity attached to emitted incidents.
    pub severity: Severity,
    /// Static labels attached to every incident this rule produces.
    pub labels: BTreeMap<String, String>,
    /// Sink adapters to which incidents are emitted on Firing /
    /// Resolved transitions.
    pub sinks: Vec<SinkConfig>,
    /// Names of rules whose Firing emissions should be suppressed
    /// while this rule is Firing. ADR-0035 grouping + inhibition
    /// primitive — collapses storms into one notification naming the
    /// upstream rule.
    pub inhibits: Vec<String>,
}

/// Operator-visible firing record. Each transition to `Firing` or
/// `Resolved` produces one [`Incident`].
#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub name: String,
    pub query: String,
    pub severity: Severity,
    pub labels: BTreeMap<String, String>,
    /// ISO-8601 representation derived from `started_at`. Set by the
    /// [`Incident::firing`] / [`Incident::resolved`] constructors.
    pub started_at: SystemTime,
    /// Present on the Resolved emission only.
    pub resolved_at: Option<SystemTime>,
}

impl Incident {
    /// Construct a Firing incident from a rule + the time the
    /// condition first held.
    pub fn firing(rule: &Rule, started_at: SystemTime) -> Self {
        Self {
            name: rule.name.clone(),
            query: rule.query.clone(),
            severity: rule.severity,
            labels: rule.labels.clone(),
            started_at,
            resolved_at: None,
        }
    }

    /// Construct a Resolved incident from a rule + the original
    /// firing time + the time the condition went inactive.
    pub fn resolved(rule: &Rule, started_at: SystemTime, resolved_at: SystemTime) -> Self {
        Self {
            name: rule.name.clone(),
            query: rule.query.clone(),
            severity: rule.severity,
            labels: rule.labels.clone(),
            started_at,
            resolved_at: Some(resolved_at),
        }
    }
}
