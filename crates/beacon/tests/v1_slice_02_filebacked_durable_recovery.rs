// Kaleidoscope Beacon — durable alert state slice 02 file-backed recovery test
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

//! Slice 02 — `FileBackedRuleStateStore`: durable recovery (US-02, US-03).
//!
//! Maps to `docs/feature/beacon-durable-alert-state-v0/discuss/user-stories.md`
//! (US-02, US-03), `design/wave-decisions.md` (DD4 keyed-latest-wins,
//! DD5 adapters, DD6 error), ADR-0040 decision 2, and KPIs 1-4 in
//! `discuss/outcome-kpis.md`.
//!
//! The adapter mirrors `strata::FileBackedProfileStore`: base path +
//! `.wal` NDJSON, base path + `.snapshot` JSON, `open()` recovers
//! snapshot then replays WAL, `snapshot()` truncates the WAL, `put()`
//! appends one NDJSON line and updates the in-memory map. The single
//! load-bearing difference (DD4, ADR-0040) is KEYED-LATEST-WINS
//! recovery: no sort, the last Put per rule name wins. The test
//! `keyed_latest_wins_last_put_per_rule_wins_after_reopen` makes that
//! contrast explicit.
//!
//! British English throughout, no em dashes.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use beacon::{FileBackedRuleStateStore, RuleState, RuleStateStore};

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

fn wal_size_bytes(base: &std::path::Path) -> u64 {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    fs::metadata(PathBuf::from(p)).map(|m| m.len()).unwrap_or(0)
}

fn snapshot_exists(base: &std::path::Path) -> bool {
    let mut p = base.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p).exists()
}

/// A fixed wall-clock instant the dwell maths can reason about. Adding
/// to it produces other deterministic instants.
fn at(secs_since_epoch: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs_since_epoch)
}

// --------------------------------------------------------------------
// US-02 sc.1 — put -> drop -> reopen -> load_all recovers the state.
// The smallest durable round-trip: a Firing state persisted before a
// restart is present after reopen.
// --------------------------------------------------------------------

#[test]
fn put_then_drop_then_reopen_recovers_state() {
    let base = temp_base("recover_one");
    let firing_since = at(1_700_000_000);
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open 1");
        store
            .put(
                "pay-latency",
                RuleState::Firing {
                    since: firing_since,
                },
            )
            .expect("put");
    }
    let store2 = FileBackedRuleStateStore::open(&base).expect("open 2");
    let recovered = store2.load_all().expect("load_all");
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Firing {
            since: firing_since
        })
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// DD4 / ADR-0040 — KEYED-LATEST-WINS. THIS is the test that
// distinguishes the rule-state store from the six append-and-sort
// storage pillars. Several puts on the SAME rule name, then reopen:
// the LAST value wins. There is no sort and no history; the value IS
// the current state. A reader who copied a pillar's "push then sort"
// recovery into beacon would break this test.
// --------------------------------------------------------------------

#[test]
fn keyed_latest_wins_last_put_per_rule_wins_after_reopen() {
    let base = temp_base("latest_wins");
    let pending_since = at(1_700_000_000);
    let firing_since = at(1_700_000_120);
    let later_firing_since = at(1_700_000_500);
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open 1");
        // Three puts for ONE rule, in evaluator order. Earlier values
        // must be overwritten, not accumulated.
        store
            .put(
                "pay-latency",
                RuleState::Pending {
                    since: pending_since,
                },
            )
            .expect("put 1");
        store
            .put(
                "pay-latency",
                RuleState::Firing {
                    since: firing_since,
                },
            )
            .expect("put 2");
        store
            .put(
                "pay-latency",
                RuleState::Firing {
                    since: later_firing_since,
                },
            )
            .expect("put 3");
    }
    let store2 = FileBackedRuleStateStore::open(&base).expect("open 2");
    let recovered = store2.load_all().expect("load_all");
    assert_eq!(
        recovered.len(),
        1,
        "one rule, one state: the WAL is keyed-latest-wins, not an append-and-sort log"
    );
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Firing {
            since: later_firing_since
        }),
        "the last Put per rule name wins; earlier Puts are overwritten, not sorted"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-02 — snapshot + post-snapshot WAL recovery. put, snapshot (which
// truncates the WAL), more puts, reopen: the final state is correct,
// composed of the snapshot plus the post-snapshot WAL replay.
// --------------------------------------------------------------------

#[test]
fn snapshot_then_post_snapshot_puts_recover_correctly() {
    let base = temp_base("snap_replay");
    let early = at(1_700_000_000);
    let late = at(1_700_000_300);
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open 1");
        store
            .put("pay-latency", RuleState::Pending { since: early })
            .expect("put pre-snap");
        store
            .put("disk-fill", RuleState::Firing { since: early })
            .expect("put pre-snap b");

        store.snapshot().expect("snapshot");
        assert_eq!(wal_size_bytes(&base), 0, "snapshot truncates the WAL");
        assert!(snapshot_exists(&base));

        // Post-snapshot transition: pay-latency advances to Firing.
        store
            .put("pay-latency", RuleState::Firing { since: late })
            .expect("put post-snap");
    }
    let store2 = FileBackedRuleStateStore::open(&base).expect("open 2");
    let recovered = store2.load_all().expect("load_all");
    assert_eq!(recovered.len(), 2);
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Firing { since: late }),
        "post-snapshot WAL Put overrides the snapshotted value"
    );
    assert_eq!(
        recovered.get("disk-fill"),
        Some(&RuleState::Firing { since: early }),
        "the snapshotted value survives when no post-snapshot Put touches it"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-03 — a RuleState carrying Pending { since } and Firing { since }
// round-trips verbatim across reopen. The SystemTime instant is the
// durable payload that matters (US-03): it must be preserved exactly,
// not merely to second precision, so dwell maths is faithful.
// --------------------------------------------------------------------

#[test]
fn pending_and_firing_since_round_trip_verbatim_across_reopen() {
    let base = temp_base("since_round_trip");
    // A non-round instant with sub-second precision, to prove the
    // serde SystemTime round-trip is exact.
    let pending_since = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789);
    let firing_since = UNIX_EPOCH + Duration::new(1_700_000_120, 987_654_321);
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open 1");
        store
            .put(
                "disk-fill",
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
    }
    let store2 = FileBackedRuleStateStore::open(&base).expect("open 2");
    let recovered = store2.load_all().expect("load_all");
    assert_eq!(
        recovered.get("disk-fill"),
        Some(&RuleState::Pending {
            since: pending_since
        }),
        "pending-since survives verbatim, sub-second precision included"
    );
    assert_eq!(
        recovered.get("pay-latency"),
        Some(&RuleState::Firing {
            since: firing_since
        }),
        "firing-since survives verbatim"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-02 sc.4 — empty store recovers nothing (the durable analogue of
// the in-memory empty case; a never-written store has no WAL content).
// --------------------------------------------------------------------

#[test]
fn fresh_store_recovers_empty() {
    let base = temp_base("fresh_empty");
    let store = FileBackedRuleStateStore::open(&base).expect("open");
    assert!(store.load_all().expect("load_all").is_empty());
    cleanup(&base);
}

// --------------------------------------------------------------------
// KPI 1 — durability completeness (100% guardrail).
//
// A store that snapshotted mid-stream and a store that never
// snapshotted, fed the IDENTICAL sequence of puts, must recover
// identical maps after a drop-and-reopen. Zero loss, zero stale value:
// every rule's final state survives both recovery paths the same way.
// This is KPI 1's "100% of rule states recovered" expressed as a
// parallel-store guardrail, mirroring the strata KPI-3 shape.
// --------------------------------------------------------------------

#[test]
fn snapshotted_and_pure_wal_stores_recover_identically() {
    let base_pure = temp_base("durable_pure");
    let base_snap = temp_base("durable_snap");

    let t0 = at(1_700_000_000);
    let t1 = at(1_700_000_120);
    let t2 = at(1_700_000_300);

    {
        let pure = FileBackedRuleStateStore::open(&base_pure).expect("open pure");
        let snap = FileBackedRuleStateStore::open(&base_snap).expect("open snap");

        for store in [&pure, &snap] {
            store
                .put("pay-latency", RuleState::Pending { since: t0 })
                .expect("put 1");
            store
                .put("disk-fill", RuleState::Firing { since: t0 })
                .expect("put 1b");
        }

        // Only the snapshot store compacts mid-stream.
        snap.snapshot().expect("snapshot");

        for store in [&pure, &snap] {
            // pay-latency advances to Firing; disk-fill resolves to Inactive.
            store
                .put("pay-latency", RuleState::Firing { since: t1 })
                .expect("put 2");
            store.put("disk-fill", RuleState::Inactive).expect("put 2b");
            store
                .put("net-saturation", RuleState::Pending { since: t2 })
                .expect("put 2c");
        }
    }

    let pure2 = FileBackedRuleStateStore::open(&base_pure).expect("reopen pure");
    let snap2 = FileBackedRuleStateStore::open(&base_snap).expect("reopen snap");
    let recovered_pure = pure2.load_all().expect("load pure");
    let recovered_snap = snap2.load_all().expect("load snap");

    assert_eq!(
        recovered_pure, recovered_snap,
        "the snapshot recovery path must recover identically to the pure-WAL path (100% completeness)"
    );
    // And the values are the latest-wins final states.
    assert_eq!(
        recovered_pure.get("pay-latency"),
        Some(&RuleState::Firing { since: t1 })
    );
    assert_eq!(recovered_pure.get("disk-fill"), Some(&RuleState::Inactive));
    assert_eq!(
        recovered_pure.get("net-saturation"),
        Some(&RuleState::Pending { since: t2 })
    );

    cleanup(&base_pure);
    cleanup(&base_snap);
}

// --------------------------------------------------------------------
// KPI 3 — persist p95 <= 2 ms per put on ubuntu-latest (debug build).
//
// 2 ms not the storage pillars' 8 ms (strata ingest) or low-ms range:
// the payload weight is the whole story. A rule-state Put is a tiny
// payload: a single enum variant carrying at most one SystemTime
// (serialised as a duration since UNIX_EPOCH), keyed by a short rule
// name. There is no pprof table set, no sample vectors, no string
// table, no batch of 100 records. One NDJSON line of a small enum is
// materially less JSON-encoding and fsync work than a heavy profile
// batch, so a 2 ms ceiling is honest. The budget is pinned against
// GitHub Actions ubuntu-latest from the FIRST commit with CI-realism
// margin already baked in. This is exactly the discipline the
// 2026-05-19 timing-bump batch taught: Lumen v1 and Cinder v1 were
// calibrated against a fast workstation and failed on CI for roughly
// two weeks before being raised. Better to set it right from DISTILL
// than bump it at DELIVER.
// --------------------------------------------------------------------

#[test]
fn persist_p95_latency_under_two_milliseconds() {
    let base = temp_base("kpi3");
    let store = FileBackedRuleStateStore::open(&base).expect("open");
    let since = at(1_700_000_000);

    // Warmup so the WAL writer and OS page cache are settled.
    for i in 0..50u64 {
        store
            .put(
                "perf-rule",
                RuleState::Firing {
                    since: since + Duration::from_secs(i),
                },
            )
            .expect("warmup");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000u64 {
        let state = RuleState::Firing {
            since: since + Duration::from_secs(1000 + i),
        };
        let t0 = std::time::Instant::now();
        store.put("perf-rule", state).expect("put");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[950];
    assert!(
        p95 <= 2_000,
        "KPI 3: persist p95 must be <= 2 ms (2000 us); got {p95} us (first samples {:?})",
        &samples[..10]
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// KPI 4 — recover p95 <= 1.5 s for 10000 rule states on ubuntu-latest
// (debug build).
//
// 1.5 s not the pillars' 2.5 s: again, the payload weight. Recovery
// here parses a snapshot map of 10000 entries, each a short rule name
// plus a small enum with at most one SystemTime, then replays a short
// post-snapshot WAL with keyed-latest-wins (an O(records) HashMap
// insert, NO sort step, lighter than the pillars' append-and-sort).
// 10000 states is the KPI-4 recovery load (outcome-kpis.md). The 1.5 s
// ceiling is pinned against GitHub Actions ubuntu-latest from the
// first commit with CI-realism margin baked in (the 2026-05-19
// lesson), so it does not need bumping at DELIVER.
// --------------------------------------------------------------------

#[test]
fn recovery_p95_latency_under_one_and_a_half_seconds() {
    let base = temp_base("kpi4");
    let since = at(1_700_000_000);
    {
        let store = FileBackedRuleStateStore::open(&base).expect("open");
        // 10000 distinct rules into the snapshot.
        for i in 0..10_000u64 {
            store
                .put(
                    &format!("rule-{i}"),
                    RuleState::Firing {
                        since: since + Duration::from_secs(i),
                    },
                )
                .expect("seed");
        }
        store.snapshot().expect("snapshot");
        // A short post-snapshot WAL to exercise the replay path too.
        for i in 0..100u64 {
            store
                .put(
                    &format!("rule-{i}"),
                    RuleState::Pending {
                        since: since + Duration::from_secs(100_000 + i),
                    },
                )
                .expect("post-snap put");
        }
    }
    let mut samples: Vec<u128> = Vec::with_capacity(20);
    for _ in 0..20 {
        let t0 = std::time::Instant::now();
        let store = FileBackedRuleStateStore::open(&base).expect("reopen");
        samples.push(t0.elapsed().as_micros());
        let recovered = store.load_all().expect("load_all");
        assert_eq!(recovered.len(), 10_000);
        drop(store);
    }
    samples.sort_unstable();
    // 95th percentile of 20 samples is the 19th by nearest rank, index
    // 18 when 0-indexed. samples[19] would be the maximum (the single
    // worst reopen), which under CI contention is a fragile thing to
    // gate on; samples[18] is the real p95 and tolerates one outlier.
    let p95_us = samples[18];
    let p95_ms = p95_us / 1_000;
    assert!(
        p95_ms <= 1_500,
        "KPI 4: recovery p95 must be <= 1.5 s; got {p95_ms} ms ({p95_us} us) (samples {samples:?})"
    );
    cleanup(&base);
}
