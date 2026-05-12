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

//! Inhibition resolver — storm suppression at the emission layer.
//!
//! ADR-0035 names the contract: a rule declaring `inhibits: ["X",
//! "Y"]` in its TOML config will suppress the Firing emissions of
//! rules `X` and `Y` while this rule is itself Firing. When this
//! rule transitions to Resolved, the previously-suppressed rules
//! emit their pending Firing incidents if they are still Firing.
//!
//! Slice 03's KPI 3: on a 20-rule storm where one upstream rule is
//! declared as the inhibitor of the other 19, the number of sink
//! emissions is `1 + (resolutions)`, not 20.
//!
//! The resolver is **stateful but pure**: every method is total,
//! deterministic, and takes its inputs by value. No I/O. The
//! orchestrator owns the lifecycle.

use std::collections::{BTreeSet, HashMap};

use crate::state_machine::Emission;
use crate::types::{Incident, Rule};

/// Cross-rule storm-suppression state.
///
/// The resolver holds:
///
/// 1. The static inhibits-relation derived from the rule catalogue
///    (rule name → set of names it inhibits).
/// 2. The current "is firing?" flag per rule.
/// 3. The pending Firing incidents that have been suppressed and
///    are waiting for their inhibitor to resolve.
#[derive(Debug, Clone)]
pub struct InhibitionResolver {
    /// rule name → names of rules whose Firing it suppresses.
    inhibits: HashMap<String, Vec<String>>,
    /// rule name → currently Firing?
    firing: HashMap<String, bool>,
    /// rule name → suppressed Firing incident (waiting for inhibitor to resolve).
    pending: HashMap<String, Incident>,
}

impl InhibitionResolver {
    /// Construct a resolver from the rule catalogue. Every rule
    /// becomes a key in the firing map (initially false). Inhibitor
    /// → inhibited links are derived from `rule.inhibits`.
    pub fn new(rules: &[Rule]) -> Self {
        let mut inhibits = HashMap::with_capacity(rules.len());
        let mut firing = HashMap::with_capacity(rules.len());
        for rule in rules {
            inhibits.insert(rule.name.clone(), rule.inhibits.clone());
            firing.insert(rule.name.clone(), false);
        }
        Self {
            inhibits,
            firing,
            pending: HashMap::new(),
        }
    }

    /// Observe one rule's transition. Returns the emissions that
    /// should reach the sinks, after inhibition + suppression
    /// resolution.
    ///
    /// Semantics per ADR-0035:
    ///
    /// - On `Firing(incident)`:
    ///   - If any of this rule's INHIBITORS is currently Firing: store
    ///     `incident` in `pending` and emit nothing. The operator is
    ///     not bothered while the upstream is still active.
    ///   - Otherwise: emit `Firing(incident)`.
    /// - On `Resolved(incident)`:
    ///   - If this rule had a pending Firing (suppressed and never
    ///     delivered): clear it, do not emit a Resolved (because no
    ///     Firing was ever delivered).
    ///   - Otherwise: emit `Resolved(incident)`.
    ///   - Then: for every rule this rule inhibits, if it has a
    ///     pending Firing and no OTHER inhibitor is Firing, release
    ///     it now.
    pub fn observe(&mut self, rule_name: &str, emission: Option<Emission>) -> Vec<Emission> {
        let mut out = Vec::new();
        match emission {
            Some(Emission::Firing(incident)) => {
                self.firing.insert(rule_name.to_string(), true);
                if self.has_active_inhibitor_of(rule_name) {
                    self.pending.insert(rule_name.to_string(), incident);
                } else {
                    out.push(Emission::Firing(incident));
                }
            }
            Some(Emission::Resolved(incident)) => {
                self.firing.insert(rule_name.to_string(), false);
                if self.pending.remove(rule_name).is_some() {
                    // The Firing was suppressed and never delivered;
                    // no Resolved to emit either. Pending cleared.
                } else {
                    out.push(Emission::Resolved(incident));
                }
                // This rule may be an inhibitor that just released.
                // For each rule it inhibits, if it has a pending
                // Firing and no other inhibitor is currently Firing,
                // emit the pending Firing now.
                let downstream = self.inhibits.get(rule_name).cloned().unwrap_or_default();
                for inhibited_name in downstream {
                    if self.has_active_inhibitor_of(&inhibited_name) {
                        continue;
                    }
                    if let Some(pending) = self.pending.remove(&inhibited_name) {
                        out.push(Emission::Firing(pending));
                    }
                }
            }
            None => {}
        }
        out
    }

    /// Does any rule currently Firing inhibit `target_name`?
    fn has_active_inhibitor_of(&self, target_name: &str) -> bool {
        for (candidate, inhibited) in &self.inhibits {
            if candidate == target_name {
                continue;
            }
            if !inhibited.iter().any(|n| n == target_name) {
                continue;
            }
            if matches!(self.firing.get(candidate), Some(true)) {
                return true;
            }
        }
        false
    }

    /// Names of every rule currently Firing. Useful for diagnostics
    /// and for cross-cutting checks at the orchestrator level.
    pub fn firing_now(&self) -> BTreeSet<&str> {
        self.firing
            .iter()
            .filter_map(|(name, on)| if *on { Some(name.as_str()) } else { None })
            .collect()
    }

    /// Number of incidents currently suppressed by inhibition.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}
