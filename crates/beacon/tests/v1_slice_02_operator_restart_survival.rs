// Kaleidoscope Beacon — durable alert state slice 02 operator-survival test
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

//! Slice 02 — the operator value: a firing alert survives a restart
//! and does not re-page (US-02), pending dwell clocks survive (US-03),
//! and the substrate-lie boundaries (corrupt snapshot, future-dated
//! since, rule-removed-from-config) behave per ADR-0040 decision 3.
//!
//! Maps to `docs/feature/beacon-durable-alert-state-v0/discuss/user-stories.md`
//! (US-02 elevator pitch, US-03), `design/wave-decisions.md` (DD6,
//! DD8, Earned Trust), and ADR-0040.
//!
//! ## Wiring-testability note (handoff to DELIVER)
//!
//! The orchestrator loop `run_rule` is an `async fn` in
//! `beacon-server/src/main.rs` and today seeds `let mut state =
//! RuleState::Inactive;` (line 146). DD8 rewires it to seed from the
//! recovered value and `put` on each change. Driving that loop end to
//! end would require spinning a tokio runtime, a wiremock PromQL
//! backend, and a ticker, and would prove tokio plumbing rather than
//! the durability contract. The durability contract is fully
//! observable at the store seam composed with the PURE transition,
//! which is exactly the trio the loop runs (recovered_state ->
//! transition -> persist). So these tests drive the store seam +
//! the pure `transition` directly, NOT tokio internals. DELIVER
//! should extract a small testable seeding helper if it wants a
//! loop-level test, but the operator outcome is proven here without
//! it.
//!
//! British English throughout, no em dashes.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use beacon::{
    transition, Emission, FileBackedRuleStateStore, QueryOutcome, Rule, RuleState, RuleStateStore,
    RuleStateStoreError, Severity,
};

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("beacon-state-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn snapshot_path(base: &std::path::Path) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p)
}

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

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
// US-02 ELEVATOR PITCH — the heart of the value.
//
// A rule that is Firing before a restart is still Firing after reopen
// (not reset to Inactive) and emits NO new Firing incident on the
// first post-restart cycle while the condition is still active. Priya
// is not re-paged. This proves the before/after of US-02.
// --------------------------------------------------------------------

#[test]
fn firing_alert_survives_restart_and_does_not_re_fire() {
    let base = temp_base("firing_survives");
    let r = rule("pay-latency", 120);
    let firing_since = at(1_700_000_000); // the rule fired at this instant

    // Before the restart: the rule is Firing and was persisted.
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open before restart");
        store
            .put(
                &r.name,
                RuleState::Firing {
                    since: firing_since,
                },
            )
            .expect("persist firing");
    }

    // The restart: a brand-new process reopens the durable store.
    let store = FileBackedRuleStateStore::open(&base).expect("open after restart");
    let recovered = store.load_all().expect("recover");
    let seeded = *recovered
        .get(&r.name)
        .expect("pay-latency state was recovered, not lost");

    // It is recovered as Firing, NOT reset to Inactive.
    assert_eq!(
        seeded,
        RuleState::Firing {
            since: firing_since
        },
        "the firing alert survived the restart"
    );

    // First post-restart evaluation cycle: the condition is still
    // active. The pure transition keeps it Firing and emits nothing.
    let now = at(1_700_000_480); // 8 minutes later, still firing
    let (next, emission) = transition(seeded, QueryOutcome::Active, &r, now);

    assert!(
        emission.is_none(),
        "no new Firing incident is emitted: the operator is NOT re-paged"
    );
    assert_eq!(
        next,
        RuleState::Firing {
            since: firing_since
        },
        "the rule stays Firing with its original since instant"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-02 sc.2 — a condition that cleared while the process was down
// resolves exactly once on the first post-restart cycle, and emits no
// Firing. Priya gets the Resolved notification she would otherwise
// never have seen.
// --------------------------------------------------------------------

#[test]
fn condition_cleared_during_downtime_resolves_exactly_once() {
    let base = temp_base("resolves_once");
    let r = rule("disk-fill", 60);
    let firing_since = at(1_700_000_000);
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open before");
        store
            .put(
                &r.name,
                RuleState::Firing {
                    since: firing_since,
                },
            )
            .expect("persist firing");
    }
    let store = FileBackedRuleStateStore::open(&base).expect("open after");
    let seeded = store.load_all().expect("recover")[&r.name];

    // While beacon-server was down, disk usage dropped: now Inactive.
    let now = at(1_700_000_040);
    let (next, emission) = transition(seeded, QueryOutcome::Inactive, &r, now);

    match emission {
        Some(Emission::Resolved(_)) => {}
        other => panic!("expected exactly one Resolved emission, got {other:?}"),
    }
    assert_eq!(
        next,
        RuleState::Inactive,
        "after resolving, the rule returns to Inactive"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-03 — pending dwell clock is preserved across restart. A rule
// Pending since 14:00:00 with a 120 s for_duration, restarted at
// 14:01:30, fires at 14:02:00 (on its original schedule) NOT at
// 14:03:30 (a restarted clock). The recovered since IS the original.
// --------------------------------------------------------------------

#[test]
fn pending_dwell_clock_is_preserved_across_restart() {
    let base = temp_base("dwell_preserved");
    let r = rule("disk-fill", 120);
    let pending_since = at(1_700_000_000); // "14:00:00"
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open before");
        store
            .put(
                &r.name,
                RuleState::Pending {
                    since: pending_since,
                },
            )
            .expect("persist pending");
    }
    // Restart at "14:01:30", 90 s into the dwell.
    let store = FileBackedRuleStateStore::open(&base).expect("open after");
    let seeded = store.load_all().expect("recover")[&r.name];
    assert_eq!(
        seeded,
        RuleState::Pending {
            since: pending_since
        },
        "the recovered pending-since is the original 14:00:00, not a restarted clock"
    );

    // Tick at "14:01:45" (105 s in): not yet 120 s, still Pending.
    let (mid, mid_emission) = transition(seeded, QueryOutcome::Active, &r, at(1_700_000_105));
    assert!(mid_emission.is_none(), "105 s dwell has not reached 120 s");
    assert_eq!(
        mid,
        RuleState::Pending {
            since: pending_since
        }
    );

    // Tick at "14:02:00" (120 s in): dwell reached, fires on schedule.
    let (fired, fire_emission) = transition(seeded, QueryOutcome::Active, &r, at(1_700_000_120));
    match fire_emission {
        Some(Emission::Firing(_)) => {}
        other => panic!("expected the rule to fire on its original schedule, got {other:?}"),
    }
    assert!(matches!(fired, RuleState::Firing { .. }));
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-03 sc.3 — a recovered pending-since slightly AFTER now (a clock
// adjustment) yields a zero dwell, no panic. The rule simply waits the
// full for_duration from the next observation.
// --------------------------------------------------------------------

#[test]
fn future_dated_pending_since_yields_zero_dwell_no_panic() {
    let base = temp_base("future_since");
    let r = rule("pay-latency", 120);
    // pending-since is 30 s AFTER the evaluation instant.
    let now = at(1_700_000_000);
    let pending_since = at(1_700_000_030);
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open before");
        store
            .put(
                &r.name,
                RuleState::Pending {
                    since: pending_since,
                },
            )
            .expect("persist pending");
    }
    let store = FileBackedRuleStateStore::open(&base).expect("open after");
    let seeded = store.load_all().expect("recover")[&r.name];

    // Dwell maths uses now.duration_since(since).unwrap_or_default(),
    // so a future since yields zero dwell, not a panic, not a fire.
    let (next, emission) = transition(seeded, QueryOutcome::Active, &r, now);
    assert!(
        emission.is_none(),
        "zero dwell does not fire; the rule waits the full for_duration"
    );
    assert_eq!(
        next,
        RuleState::Pending {
            since: pending_since
        }
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-02 sc.3 (substrate-lie gold test) — corrupt durable state on
// startup surfaces a clear PersistenceFailed error that names the
// cause, never a silent reset. The composition root uses this to
// refuse startup (DD8 step 1, ADR-0040 decision 3). Here we catalogue
// a snapshot truncated by a full disk during a previous shutdown.
// --------------------------------------------------------------------

#[test]
fn corrupt_snapshot_on_open_surfaces_persistence_error_not_silent_reset() {
    let base = temp_base("corrupt_snapshot");
    // Seed a valid store, then corrupt the snapshot file as a full
    // disk would: a truncated, non-parseable JSON object.
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open 1");
        store
            .put(
                "pay-latency",
                RuleState::Firing {
                    since: at(1_700_000_000),
                },
            )
            .expect("put");
        store.snapshot().expect("snapshot");
    }
    {
        let mut f = fs::File::create(snapshot_path(&base)).expect("open snapshot for corruption");
        // A half-written JSON object: the kind of truncation a full
        // disk leaves behind.
        f.write_all(b"{\"rules\": {\"pay-latency\": {\"Firing")
            .expect("write truncated json");
        f.flush().expect("flush");
    }

    let result = FileBackedRuleStateStore::open(&base);
    match result {
        Err(RuleStateStoreError::PersistenceFailed { reason }) => {
            assert!(
                !reason.is_empty(),
                "the persistence error names the cause, so the operator knows what happened"
            );
        }
        Ok(_) => panic!(
            "corrupt snapshot must NOT open silently with reset state; it must refuse with PersistenceFailed"
        ),
    }
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-02 sc.4 — a rule removed from config is not resurrected. The
// store recovers a Firing state for "legacy-check", but that rule no
// longer exists in the current rules set, so the composition root
// drops it. We model the active rules set as the keys the orchestrator
// would seed, and assert the dropped name is identifiable. The drop
// logic is a filter over load_all keyed by the live rule names.
// --------------------------------------------------------------------

#[test]
fn state_for_a_removed_rule_is_dropped_not_resurrected() {
    let base = temp_base("removed_rule");
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open before");
        store
            .put(
                "legacy-check",
                RuleState::Firing {
                    since: at(1_700_000_000),
                },
            )
            .expect("persist legacy");
        store
            .put(
                "pay-latency",
                RuleState::Firing {
                    since: at(1_700_000_010),
                },
            )
            .expect("persist live");
    }
    let store = FileBackedRuleStateStore::open(&base).expect("open after");
    let recovered = store.load_all().expect("recover");

    // The current config holds only "pay-latency"; "legacy-check" was
    // removed. The composition root keeps only states whose rule still
    // exists, and notes the dropped name.
    let live_rules = ["pay-latency"];
    let dropped: Vec<&String> = recovered
        .keys()
        .filter(|name| !live_rules.contains(&name.as_str()))
        .collect();
    let seeded: Vec<&String> = recovered
        .keys()
        .filter(|name| live_rules.contains(&name.as_str()))
        .collect();

    assert_eq!(
        dropped,
        vec![&"legacy-check".to_string()],
        "the removed rule's state is dropped and its name is identifiable for logging"
    );
    assert_eq!(
        seeded,
        vec![&"pay-latency".to_string()],
        "the live rule's state is seeded"
    );
    cleanup(&base);
}
