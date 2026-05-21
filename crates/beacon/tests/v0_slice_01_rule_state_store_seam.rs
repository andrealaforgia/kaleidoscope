// Kaleidoscope Beacon — durable alert state slice 01 store-seam acceptance test
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

//! Slice 01 — the `RuleStateStore` seam (US-01).
//!
//! Maps to `docs/feature/beacon-durable-alert-state-v0/discuss/user-stories.md`
//! (US-01) and ADR-0040 decision 1 (the store sits beside the pure
//! transition).
//!
//! These tests drive the store through its public port only
//! (`load_all` + `put`, DD3). The `InMemoryRuleStateStore` is the v0
//! test seam (DD5): behaviour-preserving, loses state on a fresh
//! process, used as the fast unit double. The operator value (durable
//! survival) lands in slice 02; this slice only proves the seam holds
//! state and isolates rules by name, identically to the
//! local-variable behaviour it replaces.
//!
//! British English throughout, no em dashes.

// One scenario asserts `assert_eq!(emission.is_none(), true)` to read
// as a literal Given/When/Then expectation rather than `assert!(..)`.
// That is a deliberate readability choice in the acceptance spec, not a
// behavioural weakness, so silence the style lint at file scope.
#![allow(clippy::bool_assert_comparison)]

use std::time::{Duration, SystemTime};

use beacon::{
    transition, InMemoryRuleStateStore, QueryOutcome, Rule, RuleState, RuleStateStore, Severity,
};

/// A minimal rule carrying just the identity and dwell time the store
/// seam needs. Sinks/labels/inhibits are irrelevant to state holding.
fn rule(name: &str, for_secs: u64) -> Rule {
    Rule {
        name: name.to_string(),
        query: "up == 0".to_string(),
        for_duration: Duration::from_secs(for_secs),
        interval: Duration::from_secs(15),
        severity: Severity::Critical,
        labels: Default::default(),
        sinks: Vec::new(),
        inhibits: Vec::new(),
    }
}

// --------------------------------------------------------------------
// US-01 — put then load_all round-trips a single rule's state
// --------------------------------------------------------------------

#[test]
fn put_then_load_all_round_trips_one_rule() {
    let store = InMemoryRuleStateStore::new();
    let since = SystemTime::now();

    store
        .put("pay-latency", RuleState::Pending { since })
        .expect("put");

    let recovered = store.load_all().expect("load_all");
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Pending { since })
    );
}

// --------------------------------------------------------------------
// US-01 sc.1 — the seam mirrors the pre-refactor steady-state
// behaviour: read previous state, call the unchanged pure transition,
// write the next state back. This is exactly the loop run_rule drives.
// --------------------------------------------------------------------

#[test]
fn store_holds_next_state_after_a_transition_cycle() {
    let store = InMemoryRuleStateStore::new();
    let r = rule("pay-latency", 120);
    let now = SystemTime::now();

    // Seed Inactive, as a fresh rule would be.
    store.put(&r.name, RuleState::Inactive).expect("seed");

    // One evaluator cycle: read previous, transition, write next.
    let previous = store.load_all().expect("load")[&r.name];
    let (next, emission) = transition(previous, QueryOutcome::Active, &r, now);
    store.put(&r.name, next).expect("persist next");

    // Inactive + Active enters Pending with no emission (pre-refactor
    // behaviour, unchanged).
    assert_eq!(emission.is_none(), true);
    assert_eq!(
        store.load_all().expect("reload")[&r.name],
        RuleState::Pending { since: now }
    );
}

// --------------------------------------------------------------------
// US-01 sc.2 — per-rule isolation: one rule's transition never
// disturbs another rule's stored state.
// --------------------------------------------------------------------

#[test]
fn per_rule_state_is_isolated_by_name() {
    let store = InMemoryRuleStateStore::new();
    let since = SystemTime::now();

    store.put("disk-fill", RuleState::Inactive).expect("seed b");
    store
        .put("pay-latency", RuleState::Pending { since })
        .expect("transition a");

    let recovered = store.load_all().expect("load_all");
    assert_eq!(
        recovered.get("disk-fill"),
        Some(&RuleState::Inactive),
        "disk-fill is undisturbed by pay-latency going Pending"
    );
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Pending { since })
    );
}

// --------------------------------------------------------------------
// US-01 — latest-wins in memory: a second put on the same key
// overwrites the first. The store holds the current value, not a
// history (the in-memory analogue of DD4's keyed-latest-wins).
// --------------------------------------------------------------------

#[test]
fn put_on_existing_key_overwrites_latest_wins() {
    let store = InMemoryRuleStateStore::new();
    let pending_since = SystemTime::now();
    let firing_since = pending_since + Duration::from_secs(120);

    store
        .put(
            "pay-latency",
            RuleState::Pending {
                since: pending_since,
            },
        )
        .expect("put pending");
    store
        .put(
            "pay-latency",
            RuleState::Firing {
                since: firing_since,
            },
        )
        .expect("put firing");

    let recovered = store.load_all().expect("load_all");
    assert_eq!(recovered.len(), 1, "one key, not two");
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Firing {
            since: firing_since
        }),
        "the last put wins"
    );
}

// --------------------------------------------------------------------
// US-01 sc.3 — a fresh InMemoryRuleStateStore is empty, so a restart
// still loses state at this slice (the durability defect is fixed in
// slice 02, not here). load_all on an empty store is empty.
// --------------------------------------------------------------------

#[test]
fn load_all_on_a_fresh_store_is_empty() {
    let store = InMemoryRuleStateStore::new();
    let recovered = store.load_all().expect("load_all");
    assert!(
        recovered.is_empty(),
        "a fresh in-memory store recovers nothing; restart still loses state at slice 01"
    );
}

// --------------------------------------------------------------------
// US-01 — the trait is object-safe so the orchestrator can hold the
// store behind a Box<dyn RuleStateStore> and swap the durable adapter
// for this test double without changing the loop (DD5 maintainability
// scenario). Drives the store purely through the trait object.
// --------------------------------------------------------------------

#[test]
fn store_is_usable_behind_a_trait_object() {
    let store: Box<dyn RuleStateStore> = Box::new(InMemoryRuleStateStore::new());
    let since = SystemTime::now();

    store
        .put("pay-latency", RuleState::Firing { since })
        .expect("put via dyn");

    let recovered = store.load_all().expect("load via dyn");
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Firing { since })
    );
}
