// Kaleidoscope Beacon — slice 03 grouping + inhibition acceptance test
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

//! Slice 03 — Grouping + inhibition
//!
//! Maps to `docs/feature/beacon-v0/slices/slice-03-grouping-and-inhibition.md`.
//! Companion story: US-BE-03.
//!
//! Riley pages at 03:14. With 20 alert rules and no inhibition, a
//! Prometheus outage trips all 20 at once and the pager goes off 20
//! times. That is the named operational anti-pattern. ADR-0035's
//! inhibition primitive collapses the storm into one notification.
//!
//! KPI 3: on a 20-rule simultaneous-failure scenario where one
//! upstream rule (`prometheus_unavailable`) inhibits the other 19,
//! the number of sink emissions is `1 + (resolutions)`, not 20.

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
// Plain emission passthrough — no inhibition relations.
// --------------------------------------------------------------------

#[test]
fn solo_rule_emits_firing_unchanged_when_no_inhibitors() {
    let rules = vec![rule("alone", vec![])];
    let mut resolver = InhibitionResolver::new(&rules);
    let out = resolver.observe("alone", firing_for("alone"));
    assert_eq!(out.len(), 1);
    assert!(matches!(out[0], Emission::Firing(_)));
}

#[test]
fn solo_rule_emits_resolved_unchanged_when_no_inhibitors() {
    let rules = vec![rule("alone", vec![])];
    let mut resolver = InhibitionResolver::new(&rules);
    let _ = resolver.observe("alone", firing_for("alone"));
    let out = resolver.observe("alone", resolved_for("alone"));
    assert_eq!(out.len(), 1);
    assert!(matches!(out[0], Emission::Resolved(_)));
}

#[test]
fn no_emission_when_observe_carries_none() {
    let rules = vec![rule("alone", vec![])];
    let mut resolver = InhibitionResolver::new(&rules);
    let out = resolver.observe("alone", None);
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// Two-rule inhibition.
// --------------------------------------------------------------------

#[test]
fn inhibitor_firing_suppresses_inhibited_firing() {
    let rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut resolver = InhibitionResolver::new(&rules);

    let up = resolver.observe("upstream", firing_for("upstream"));
    assert_eq!(up.len(), 1);
    assert_eq!(name_of(&up[0]), "upstream");

    let down = resolver.observe("downstream", firing_for("downstream"));
    assert!(down.is_empty(), "downstream Firing must be suppressed");
    assert_eq!(resolver.pending_count(), 1);
}

#[test]
fn inhibitor_resolving_releases_pending_inhibited_firing() {
    let rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut resolver = InhibitionResolver::new(&rules);
    let _ = resolver.observe("upstream", firing_for("upstream"));
    let _ = resolver.observe("downstream", firing_for("downstream"));
    assert_eq!(resolver.pending_count(), 1);

    let out = resolver.observe("upstream", resolved_for("upstream"));
    // Expect: upstream Resolved + downstream Firing (released).
    let kinds: Vec<&str> = out.iter().map(name_of).collect();
    assert!(kinds.contains(&"upstream"));
    assert!(kinds.contains(&"downstream"));
    let firing_count = out
        .iter()
        .filter(|e| matches!(e, Emission::Firing(_)))
        .count();
    let resolved_count = out
        .iter()
        .filter(|e| matches!(e, Emission::Resolved(_)))
        .count();
    assert_eq!(firing_count, 1);
    assert_eq!(resolved_count, 1);
    assert_eq!(resolver.pending_count(), 0);
}

#[test]
fn inhibited_rule_resolving_while_suppressed_emits_nothing() {
    // If the downstream's Firing was suppressed and never delivered,
    // a subsequent Resolved must not appear to the operator either —
    // they were never told there was a problem.
    let rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut resolver = InhibitionResolver::new(&rules);
    let _ = resolver.observe("upstream", firing_for("upstream"));
    let _ = resolver.observe("downstream", firing_for("downstream"));
    let out = resolver.observe("downstream", resolved_for("downstream"));
    assert!(out.is_empty(), "no emission for never-delivered Firing");
    assert_eq!(resolver.pending_count(), 0);
}

#[test]
fn inhibited_firing_passes_through_when_inhibitor_inactive() {
    // Inhibitor must be actually Firing for inhibition to apply.
    let rules = vec![
        rule("upstream", vec!["downstream"]),
        rule("downstream", vec![]),
    ];
    let mut resolver = InhibitionResolver::new(&rules);
    let out = resolver.observe("downstream", firing_for("downstream"));
    assert_eq!(out.len(), 1);
    assert!(matches!(out[0], Emission::Firing(_)));
}

// --------------------------------------------------------------------
// Multiple inhibitors of one rule.
// --------------------------------------------------------------------

#[test]
fn multiple_inhibitors_still_suppress_until_all_resolve() {
    let rules = vec![
        rule("upstream_a", vec!["target"]),
        rule("upstream_b", vec!["target"]),
        rule("target", vec![]),
    ];
    let mut resolver = InhibitionResolver::new(&rules);
    let _ = resolver.observe("upstream_a", firing_for("upstream_a"));
    let _ = resolver.observe("upstream_b", firing_for("upstream_b"));
    let _ = resolver.observe("target", firing_for("target"));
    assert_eq!(resolver.pending_count(), 1);

    // Resolving only one inhibitor: target stays suppressed.
    let out = resolver.observe("upstream_a", resolved_for("upstream_a"));
    let names: Vec<&str> = out.iter().map(name_of).collect();
    assert!(names.contains(&"upstream_a"));
    assert!(!names.contains(&"target"), "target must stay suppressed");
    assert_eq!(resolver.pending_count(), 1);

    // Resolving the second inhibitor: target releases.
    let out = resolver.observe("upstream_b", resolved_for("upstream_b"));
    let names: Vec<&str> = out.iter().map(name_of).collect();
    assert!(names.contains(&"upstream_b"));
    assert!(names.contains(&"target"));
    assert_eq!(resolver.pending_count(), 0);
}

// --------------------------------------------------------------------
// KPI 3 — 20-rule storm collapse.
// --------------------------------------------------------------------

#[test]
fn twenty_rule_simultaneous_failure_collapses_into_one_emission() {
    // One inhibitor + 19 downstream rules it inhibits.
    let downstream_names: Vec<String> = (0..19).map(|i| format!("svc_{i}_down")).collect();
    let mut rules = vec![rule(
        "prometheus_unavailable",
        downstream_names.iter().map(String::as_str).collect(),
    )];
    for name in &downstream_names {
        rules.push(rule(name, vec![]));
    }
    let mut resolver = InhibitionResolver::new(&rules);

    let mut emissions: Vec<Emission> = Vec::new();
    // Inhibitor fires first (operator's mental model: the upstream
    // outage is what the orchestrator observes earliest).
    emissions.extend(resolver.observe(
        "prometheus_unavailable",
        firing_for("prometheus_unavailable"),
    ));
    // All 19 downstream rules fire next, suppressed.
    for name in &downstream_names {
        emissions.extend(resolver.observe(name.as_str(), firing_for(name)));
    }

    assert_eq!(
        emissions.len(),
        1,
        "KPI 3: 20 simultaneous Firings must collapse to one emission, got {}",
        emissions.len()
    );
    assert_eq!(name_of(&emissions[0]), "prometheus_unavailable");
    assert_eq!(resolver.pending_count(), 19);
}

#[test]
fn twenty_rule_storm_resolves_cleanly_when_inhibitor_resolves() {
    let downstream_names: Vec<String> = (0..19).map(|i| format!("svc_{i}_down")).collect();
    let mut rules = vec![rule(
        "prometheus_unavailable",
        downstream_names.iter().map(String::as_str).collect(),
    )];
    for name in &downstream_names {
        rules.push(rule(name, vec![]));
    }
    let mut resolver = InhibitionResolver::new(&rules);

    let _ = resolver.observe(
        "prometheus_unavailable",
        firing_for("prometheus_unavailable"),
    );
    for name in &downstream_names {
        let _ = resolver.observe(name.as_str(), firing_for(name));
    }

    let out = resolver.observe(
        "prometheus_unavailable",
        resolved_for("prometheus_unavailable"),
    );
    // Expect: upstream Resolved + 19 downstream Firings.
    assert_eq!(out.len(), 20);
    let firing_count = out
        .iter()
        .filter(|e| matches!(e, Emission::Firing(_)))
        .count();
    let resolved_count = out
        .iter()
        .filter(|e| matches!(e, Emission::Resolved(_)))
        .count();
    assert_eq!(firing_count, 19);
    assert_eq!(resolved_count, 1);
    assert_eq!(resolver.pending_count(), 0);
}

#[test]
fn determinism_repeat_observations_produce_byte_identical_emissions() {
    // The resolver is deterministic: two replays of the same event
    // sequence produce the same output.
    let downstream_names: Vec<String> = (0..5).map(|i| format!("svc_{i}_down")).collect();
    let mut rules = vec![rule(
        "upstream",
        downstream_names.iter().map(String::as_str).collect(),
    )];
    for name in &downstream_names {
        rules.push(rule(name, vec![]));
    }

    fn run(rules: &[Rule], names: &[String]) -> Vec<String> {
        let mut resolver = InhibitionResolver::new(rules);
        let mut out: Vec<String> = Vec::new();
        out.extend(
            resolver
                .observe("upstream", firing_for("upstream"))
                .iter()
                .map(|e| name_of(e).to_string()),
        );
        for name in names {
            out.extend(
                resolver
                    .observe(name.as_str(), firing_for(name))
                    .iter()
                    .map(|e| name_of(e).to_string()),
            );
        }
        out.extend(
            resolver
                .observe("upstream", resolved_for("upstream"))
                .iter()
                .map(|e| name_of(e).to_string()),
        );
        out
    }

    let a = run(&rules, &downstream_names);
    let b = run(&rules, &downstream_names);
    assert_eq!(a, b);
}

// --------------------------------------------------------------------
// firing_now diagnostic — useful for the orchestrator.
// --------------------------------------------------------------------

#[test]
fn firing_now_reflects_current_state() {
    let rules = vec![rule("a", vec![]), rule("b", vec![]), rule("c", vec![])];
    let mut resolver = InhibitionResolver::new(&rules);
    assert!(resolver.firing_now().is_empty());

    let _ = resolver.observe("a", firing_for("a"));
    let _ = resolver.observe("b", firing_for("b"));
    let now: Vec<&str> = resolver.firing_now().into_iter().collect();
    assert_eq!(now, vec!["a", "b"]);

    let _ = resolver.observe("a", resolved_for("a"));
    let now: Vec<&str> = resolver.firing_now().into_iter().collect();
    assert_eq!(now, vec!["b"]);
}
