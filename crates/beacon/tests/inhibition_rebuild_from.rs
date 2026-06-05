// Kaleidoscope Beacon — InhibitionResolver::rebuild_from carryover tests
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

//! `InhibitionResolver::rebuild_from` — the SIGHUP-reload carryover seam
//! (ADR-0063 sub-decision 3 + review clarification 3, the both-ends
//! survival check).
//!
//! The orchestrator snapshots the OLD live resolver (its `firing` flags
//! and its `pending` suppressed-incident map) and rebuilds a NEW resolver
//! from the new rule set, carrying over:
//!
//! - the relation graph from the NEW rules (so added/removed inhibitor
//!   relations take effect);
//! - the `firing` flag for every surviving rule (so suppression on the
//!   very next tick reflects who is currently firing);
//! - each `pending` entry whose inhibited rule survives AND at least one
//!   of its inhibitors survives (both-ends survival). A pending entry
//!   whose inhibited rule was removed, or whose every inhibitor was
//!   removed, is DROPPED so no inhibition leaks and no removed inhibitor
//!   keeps suppressing a survivor.
//!
//! These are pure-domain port-to-port tests: the driving port is the
//! resolver's public API (`new` / `observe` / `carryover` /
//! `rebuild_from` / `pending_count` / `firing_now`).

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use beacon::{Emission, Incident, InhibitionResolver, Rule, Severity};

fn rule(name: &str, inhibits: Vec<&str>) -> Rule {
    Rule {
        name: name.to_string(),
        query: "up == 0".to_string(),
        for_duration: Duration::from_secs(60),
        interval: Duration::from_secs(30),
        severity: Severity::Critical,
        labels: BTreeMap::new(),
        sinks: Vec::new(),
        inhibits: inhibits.into_iter().map(String::from).collect(),
    }
}

fn firing_for(name: &str) -> Option<Emission> {
    let mut labels = BTreeMap::new();
    labels.insert("rule".to_string(), name.to_string());
    Some(Emission::Firing(Incident {
        name: name.to_string(),
        query: "up == 0".to_string(),
        severity: Severity::Critical,
        labels,
        started_at: SystemTime::UNIX_EPOCH,
        resolved_at: None,
    }))
}

fn resolved_for(name: &str) -> Option<Emission> {
    let mut labels = BTreeMap::new();
    labels.insert("rule".to_string(), name.to_string());
    Some(Emission::Resolved(Incident {
        name: name.to_string(),
        query: "up == 0".to_string(),
        severity: Severity::Critical,
        labels,
        started_at: SystemTime::UNIX_EPOCH,
        resolved_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(120)),
    }))
}

fn name_of(emission: &Emission) -> &str {
    match emission {
        Emission::Firing(i) | Emission::Resolved(i) => &i.name,
    }
}

// --------------------------------------------------------------------
// Behaviour 1 — the rebuilt resolver uses the NEW relation graph.
// An inhibits-relation present only in the NEW catalogue takes effect.
// --------------------------------------------------------------------

#[test]
fn rebuilt_resolver_applies_a_newly_added_inhibits_relation() {
    // Old: two unrelated rules, neither inhibits the other.
    let old_rules = vec![rule("upstream", vec![]), rule("downstream", vec![])];
    let old = InhibitionResolver::new(&old_rules);

    // New: upstream now inhibits downstream.
    let new_rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut resolver = InhibitionResolver::rebuild_from(&new_rules, old.carryover());

    let _ = resolver.observe("upstream", firing_for("upstream"));
    let down = resolver.observe("downstream", firing_for("downstream"));
    assert!(
        down.is_empty(),
        "the newly-added inhibits relation must suppress downstream"
    );
    assert_eq!(resolver.pending_count(), 1);
}

// --------------------------------------------------------------------
// Behaviour 2 — the carried `firing` flag drives suppression on the
// very next observe, with no re-fire of the surviving inhibitor.
// --------------------------------------------------------------------

#[test]
fn carried_firing_flag_suppresses_downstream_without_inhibitor_refire() {
    // Old: upstream inhibits downstream; upstream is already Firing.
    let old_rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut old = InhibitionResolver::new(&old_rules);
    let up = old.observe("upstream", firing_for("upstream"));
    assert_eq!(
        up.len(),
        1,
        "precondition: upstream fired once on the old generation"
    );

    // Reload keeps the same two rules. The carried firing flag means the
    // new resolver knows upstream is firing WITHOUT replaying its Firing.
    let new_rules = old_rules.clone();
    let mut resolver = InhibitionResolver::rebuild_from(&new_rules, old.carryover());
    assert_eq!(
        resolver.firing_now().into_iter().collect::<Vec<_>>(),
        vec!["upstream"],
        "the carried firing flag must mark upstream firing after the rebuild"
    );

    // downstream fires for the first time on the new generation: suppressed.
    let down = resolver.observe("downstream", firing_for("downstream"));
    assert!(
        down.is_empty(),
        "a still-firing carried inhibitor must suppress downstream after rebuild"
    );
    assert_eq!(resolver.pending_count(), 1);
}

// --------------------------------------------------------------------
// Behaviour 3 — a pending entry is carried when BOTH ends survive, and
// is released normally when the surviving inhibitor later resolves.
// --------------------------------------------------------------------

#[test]
fn pending_carried_when_both_ends_survive_then_released_on_inhibitor_resolve() {
    let old_rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut old = InhibitionResolver::new(&old_rules);
    let _ = old.observe("upstream", firing_for("upstream"));
    let _ = old.observe("downstream", firing_for("downstream"));
    assert_eq!(
        old.pending_count(),
        1,
        "precondition: downstream is held pending"
    );

    // Reload keeps both rules and the relation. The pending entry for
    // downstream must survive: both ends are present.
    let new_rules = old_rules.clone();
    let mut resolver = InhibitionResolver::rebuild_from(&new_rules, old.carryover());
    assert_eq!(
        resolver.pending_count(),
        1,
        "a pending entry survives when both the inhibited rule and an inhibitor survive"
    );

    // The carried pending is real: when upstream resolves, downstream's
    // held Firing is released.
    let out = resolver.observe("upstream", resolved_for("upstream"));
    let names: Vec<&str> = out.iter().map(name_of).collect();
    assert!(
        names.contains(&"downstream"),
        "the carried-over pending Firing must be released when its inhibitor resolves"
    );
    assert_eq!(resolver.pending_count(), 0);
}

// --------------------------------------------------------------------
// Behaviour 4 — a pending entry is DROPPED when the inhibited rule is
// removed from the new catalogue (it has no home in the new graph).
// --------------------------------------------------------------------

#[test]
fn pending_dropped_when_inhibited_rule_removed() {
    let old_rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut old = InhibitionResolver::new(&old_rules);
    let _ = old.observe("upstream", firing_for("upstream"));
    let _ = old.observe("downstream", firing_for("downstream"));
    assert_eq!(old.pending_count(), 1);

    // New catalogue removes downstream entirely.
    let new_rules = vec![rule("upstream", vec![])];
    let resolver = InhibitionResolver::rebuild_from(&new_rules, old.carryover());
    assert_eq!(
        resolver.pending_count(),
        0,
        "a pending entry whose inhibited rule was removed must be dropped"
    );
}

// --------------------------------------------------------------------
// Behaviour 5 — a pending entry is DROPPED when every inhibitor of the
// held rule is removed (the both-ends survival check, review
// clarification 3): a removed inhibitor cannot keep suppressing a
// survivor.
// --------------------------------------------------------------------

#[test]
fn pending_dropped_when_every_inhibitor_removed() {
    let old_rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut old = InhibitionResolver::new(&old_rules);
    let _ = old.observe("upstream", firing_for("upstream"));
    let _ = old.observe("downstream", firing_for("downstream"));
    assert_eq!(old.pending_count(), 1);

    // New catalogue keeps downstream but removes its only inhibitor.
    let new_rules = vec![rule("downstream", vec![])];
    let resolver = InhibitionResolver::rebuild_from(&new_rules, old.carryover());
    assert_eq!(
        resolver.pending_count(),
        0,
        "a held entry whose every inhibitor was removed must be dropped (both-ends survival)"
    );
}

// --------------------------------------------------------------------
// Behaviour 5b — both-ends survival keeps the entry when ONE of two
// inhibitors survives (at least one is enough), proving the check is an
// existence test over inhibitors, not "all must survive".
// --------------------------------------------------------------------

#[test]
fn pending_kept_when_at_least_one_inhibitor_survives() {
    let old_rules = vec![
        rule("upstream_a", vec!["downstream"]),
        rule("upstream_b", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut old = InhibitionResolver::new(&old_rules);
    let _ = old.observe("upstream_a", firing_for("upstream_a"));
    let _ = old.observe("upstream_b", firing_for("upstream_b"));
    let _ = old.observe("downstream", firing_for("downstream"));
    assert_eq!(old.pending_count(), 1);

    // New catalogue drops upstream_b but keeps upstream_a as an inhibitor.
    let new_rules = vec![
        rule("upstream_a", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let resolver = InhibitionResolver::rebuild_from(&new_rules, old.carryover());
    assert_eq!(
        resolver.pending_count(),
        1,
        "the held entry survives while at least one of its inhibitors survives"
    );
}
