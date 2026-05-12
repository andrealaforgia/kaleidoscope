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

//! Per-rule state machine. Pure function: `(state, outcome, rule, now)
//! → (next_state, emission)`. No I/O, no Date::now(), no async.

use std::time::SystemTime;

use crate::types::{Incident, Rule};

/// What the Prometheus backend said for this rule's query at `now`.
///
/// The walking skeleton treats every non-empty result set as
/// "condition is active". Slice 02+ may refine this if the operator
/// needs threshold-shaped queries (`up == 0` is already a
/// threshold-shaped query at the PromQL level so refinement is
/// optional).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryOutcome {
    /// At least one series satisfied the rule's query at `now`.
    Active,
    /// No series satisfied the rule's query at `now`.
    Inactive,
}

/// State of a single rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleState {
    /// Condition has never been observed active in this evaluator
    /// lifetime, or has been observed inactive after the last
    /// `Resolved` emission.
    Inactive,
    /// Condition is active but has not yet held for `for_duration`.
    /// `since` is the wall-clock at which the condition first
    /// transitioned from inactive to active.
    Pending { since: SystemTime },
    /// Condition has held for at least `for_duration`. `since` is the
    /// wall-clock at which the rule first transitioned to firing
    /// (i.e. `pending.since + for_duration`).
    Firing { since: SystemTime },
}

impl RuleState {
    /// Constructor for callers seeding an empty state.
    pub const fn inactive() -> Self {
        Self::Inactive
    }
}

/// One of the emissions a transition may produce. `None` means "no
/// emission this cycle"; emissions are sent to every sink configured
/// on the rule.
#[derive(Debug, Clone)]
pub enum Emission {
    /// The rule has just transitioned Pending → Firing.
    Firing(Incident),
    /// The rule has just transitioned Firing → Inactive.
    Resolved(Incident),
}

/// Pure transition. Takes the previous state, the current query
/// outcome, the rule (carries `for_duration`), and `now`. Returns the
/// next state and any incident emission.
///
/// The function is total: every (state, outcome) pair has a defined
/// transition.
pub fn transition(
    state: RuleState,
    outcome: QueryOutcome,
    rule: &Rule,
    now: SystemTime,
) -> (RuleState, Option<Emission>) {
    match (state, outcome) {
        // No change while inactive and still inactive.
        (RuleState::Inactive, QueryOutcome::Inactive) => (RuleState::Inactive, None),

        // First time the condition is observed active: enter Pending.
        // No emission yet.
        (RuleState::Inactive, QueryOutcome::Active) => (RuleState::Pending { since: now }, None),

        // Pending and condition went away: back to Inactive. No
        // emission (no Firing was ever announced).
        (RuleState::Pending { .. }, QueryOutcome::Inactive) => (RuleState::Inactive, None),

        // Pending and condition still active: check dwell time.
        (RuleState::Pending { since }, QueryOutcome::Active) => {
            let dwell = now.duration_since(since).unwrap_or_default();
            if dwell >= rule.for_duration {
                let next = RuleState::Firing { since: now };
                let incident = Incident::firing(rule, now);
                (next, Some(Emission::Firing(incident)))
            } else {
                (RuleState::Pending { since }, None)
            }
        }

        // Firing and condition still active: no new emission.
        (RuleState::Firing { since }, QueryOutcome::Active) => (RuleState::Firing { since }, None),

        // Firing and condition went away: emit Resolved + return to
        // Inactive so the next active observation starts a fresh
        // dwell timer.
        (RuleState::Firing { since }, QueryOutcome::Inactive) => {
            let incident = Incident::resolved(rule, since, now);
            (RuleState::Inactive, Some(Emission::Resolved(incident)))
        }
    }
}
