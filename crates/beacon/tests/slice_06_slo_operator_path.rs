// Kaleidoscope Beacon — slice 06 SLO operator-path acceptance test
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

//! Slice 06 — the SLO operator path (DISTILL, feature
//! `beacon-slo-operator-path-v0`).
//!
//! These are the LOADER-level acceptance tests: an operator declares an
//! `[[slo]]` table in a real `--rules` TOML file on disk; the real
//! `load_rules` parses it, validates it (`target_availability` strictly
//! in `(0,1)`; `error_budget_period == 30d`), expands it via
//! `synthesise_slo` VERBATIM into its four MWMBR rules, and merges them
//! into the same `LoadOutcome.rules` catalogue the hand-authored
//! `[[rules]]` populate. The driving port here is the real on-disk
//! `--rules` TOML tree plus the real public `load_rules` entry point
//! (ADR-0067 "Test seam"). The SIGHUP/reload arm lives in
//! `crates/beacon-server/tests/slo_reload.rs` (the operator binary).
//!
//! # Strategy C (real local I/O)
//!
//! Every loader test writes a REAL `*.toml` file into a REAL writable
//! temp `--rules` directory and runs the REAL `load_rules`. No InMemory
//! double: the wiring under proof is `RawSlo` deserialisation, the
//! `into_slo` validation, the `FileShape` `slo` field, the
//! `deny_unknown_fields`/"did you mean" path, and the duplicate-name
//! merge scan — exactly the parts an InMemory loader could not catch.
//! Tagged `@real-io`. (DEVOPS environments.yaml `slo_reload_test_environment`,
//! seam i.)
//!
//! # The real synthesised names (ADR-0067 F2, the shipped authority)
//!
//! `synthesise_slo` names rules `{service}_slo_{page|ticket}_{long}_{short}`
//! (`slo.rs:124-127`), i.e. WITH the `_slo_` infix:
//!   checkout_slo_page_1h_5m, checkout_slo_page_6h_30m,
//!   checkout_slo_ticket_1d_2h, checkout_slo_ticket_3d_6h.
//! The DISCUSS user stories illustrate names WITHOUT the infix
//! (`checkout_page_1h_5m`); the shipped code is the authority and every
//! assertion below pins the REAL `_slo_` names (ADR-0067 "Note on the
//! DISCUSS illustrative names").
//!
//! # RED-not-BROKEN at the DISTILL commit (Mandate 7)
//!
//! The `[[slo]]` wire shape does NOT exist yet (DELIVER adds `RawSlo` +
//! `into_slo` + the `FileShape.slo` field + the duplicate-name scan).
//! Today `FileShape` is `deny_unknown_fields` with only `rules`
//! (`loader.rs:260-265`), so a file containing `[[slo]]` FAILS to parse
//! ("unknown field `slo`") and the whole file is SKIPPED into a
//! diagnostic. Therefore every test that asserts "four synthesised rules
//! loaded" FAILS today on BEHAVIOUR (zero SLO rules in the catalogue,
//! one diagnostic), not on a missing symbol: these tests compile against
//! the EXISTING public surface only (`load_rules`, `LoadOutcome`,
//! `synthesise_slo`, `Slo`, `Severity`) and name no not-yet-existing API.
//! Each such test is `#[ignore = "RED until DELIVER:
//! beacon-slo-operator-path-v0"]` so `cargo test` stays GREEN at this
//! commit; DELIVER removes the `#[ignore]`s once the loader wires the
//! path. Run them with `--ignored` to see them FAIL on the assertion.
//!
//! The F5 cross-validation tests and the negative-control regression
//! guard call the ALREADY-SHIPPED `synthesise_slo` / `load_rules`
//! directly and are therefore left UN-ignored (they PASS today and must
//! keep passing): they are the guardrails, not the RED outer loop.

use std::path::{Path, PathBuf};
use std::time::Duration;

use beacon::{load_rules, synthesise_slo, Severity, SinkConfig, Slo};

// --------------------------------------------------------------------
// Temp `--rules` dir helper, in the established slice-02 style (no
// `tempfile` dep). Writable + test-owned + dropped at end.
// --------------------------------------------------------------------
struct TmpRules {
    path: PathBuf,
}

impl TmpRules {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "beacon-slo-operator-path-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp rules dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, file: &str, body: &str) {
        std::fs::write(self.path.join(file), body).expect("write rule file");
    }
}

impl Drop for TmpRules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One well-formed `[[slo]]` for service `checkout`, the canonical
/// happy-path declaration. `target_availability` strictly inside
/// `(0,1)`, a 30d budget, one webhook sink. The intended schema from
/// ADR-0067 F1: `service`, `good_events_query`, `total_events_query`,
/// `target_availability`, `error_budget_period`, `[[slo.sinks]]`.
fn checkout_slo_toml(sink_url: &str) -> String {
    format!(
        r#"
[[slo]]
service = "checkout"
good_events_query = "http_requests_total{{job=\"checkout\",code!~\"5..\"}}"
total_events_query = "http_requests_total{{job=\"checkout\"}}"
target_availability = 0.999
error_budget_period = "30d"

[[slo.sinks]]
kind = "webhook"
url = "{sink_url}"
"#
    )
}

/// The four REAL synthesised names for a `checkout` SLO (the `_slo_`
/// infix is the shipped authority, ADR-0067 F2 / slo.rs:124-127).
const CHECKOUT_SLO_RULE_NAMES: [&str; 4] = [
    "checkout_slo_page_1h_5m",
    "checkout_slo_page_6h_30m",
    "checkout_slo_ticket_1d_2h",
    "checkout_slo_ticket_3d_6h",
];

fn loaded_rule_names(outcome: &beacon::LoadOutcome) -> Vec<String> {
    outcome.rules.iter().map(|r| r.name.clone()).collect()
}

// ====================================================================
// US-01 — an SLO declared in the file synthesises and loads (RED).
// ====================================================================

/// WALKING SLICE (US-01, AC-1/2/5): one `[[slo]]` table in a real rule
/// file loads the FOUR synthesised MWMBR rules into the live catalogue,
/// carrying the REAL `_slo_` names. This is the loader half of the
/// operator path; the binary/startup half is in slo_reload.rs. RED
/// today: `[[slo]]` poisons its file, so zero SLO rules load.
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn one_slo_table_synthesises_four_named_rules_into_the_catalogue() {
    // @walking_skeleton @driving_port @real-io  (US-01)
    let rules = TmpRules::new("ws-load");
    rules.write(
        "checkout.toml",
        &checkout_slo_toml("https://ops.acme/alerts"),
    );

    let outcome = load_rules(rules.path()).expect("load");

    assert_eq!(
        outcome.diagnostics.len(),
        0,
        "a valid SLO must load without diagnostics; got: {:?}",
        outcome.diagnostics
    );
    let names = loaded_rule_names(&outcome);
    assert_eq!(
        names.len(),
        4,
        "one [[slo]] must synthesise exactly four rules; got: {names:?}"
    );
    for expected in CHECKOUT_SLO_RULE_NAMES {
        assert!(
            names.iter().any(|n| n == expected),
            "the live catalogue must hold the synthesised rule {expected:?}; got: {names:?}"
        );
    }
}

/// US-01 AC-2 (canonical thresholds + windows + severities): the four
/// loaded rules carry the workbook thresholds (14.4/1h/5m, 6/6h/30m,
/// 3/1d/2h, 1/3d/6h) — observed as the `budget * threshold` limit in the
/// synthesised PromQL for budget 0.001 (target 0.999) — and the
/// page=critical / ticket=warning severities. RED today (no SLO rules).
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn loaded_slo_rules_carry_canonical_thresholds_and_severities() {
    // @driving_port @real-io  (US-01)
    let rules = TmpRules::new("ws-thresholds");
    rules.write(
        "checkout.toml",
        &checkout_slo_toml("https://ops.acme/alerts"),
    );

    let outcome = load_rules(rules.path()).expect("load");
    let by_name = |name: &str| {
        outcome
            .rules
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("rule {name} must be loaded"))
            .clone()
    };

    // budget = 1 - 0.999 = 0.001; limit = budget * threshold.
    let page_1h_5m = by_name("checkout_slo_page_1h_5m");
    assert!(
        page_1h_5m.query.contains("0.0144"), // 0.001 * 14.4
        "page 1h/5m must carry threshold 14.4 (limit 0.0144); got: {}",
        page_1h_5m.query
    );
    assert_eq!(page_1h_5m.severity, Severity::Critical);

    let page_6h_30m = by_name("checkout_slo_page_6h_30m");
    assert!(
        page_6h_30m.query.contains("0.006"), // 0.001 * 6
        "page 6h/30m must carry threshold 6 (limit 0.006); got: {}",
        page_6h_30m.query
    );
    assert_eq!(page_6h_30m.severity, Severity::Critical);

    let ticket_1d_2h = by_name("checkout_slo_ticket_1d_2h");
    assert!(
        ticket_1d_2h.query.contains("0.003"), // 0.001 * 3
        "ticket 1d/2h must carry threshold 3 (limit 0.003); got: {}",
        ticket_1d_2h.query
    );
    assert_eq!(ticket_1d_2h.severity, Severity::Warning);

    let ticket_3d_6h = by_name("checkout_slo_ticket_3d_6h");
    assert!(
        ticket_3d_6h.query.contains("0.001"), // 0.001 * 1
        "ticket 3d/6h must carry threshold 1 (limit 0.001); got: {}",
        ticket_3d_6h.query
    );
    assert_eq!(ticket_3d_6h.severity, Severity::Warning);
}

/// US-01 boundary (the tightest realistic target): a `0.9999` (four
/// nines) SLO loads its four rules and the page 1h/5m limit becomes
/// `0.00144` (budget 0.0001 * 14.4); synthesis does not choke on the
/// tighter budget. RED today (no SLO rules).
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn four_nines_target_loads_with_tighter_threshold() {
    // @driving_port @real-io  (US-01 boundary)
    let rules = TmpRules::new("ws-four-nines");
    rules.write(
        "checkout.toml",
        r#"
[[slo]]
service = "checkout"
good_events_query = "http_requests_total{job=\"checkout\",code!~\"5..\"}"
total_events_query = "http_requests_total{job=\"checkout\"}"
target_availability = 0.9999
error_budget_period = "30d"
"#,
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert_eq!(outcome.rules.len(), 4, "four nines must load four rules");
    let page = outcome
        .rules
        .iter()
        .find(|r| r.name == "checkout_slo_page_1h_5m")
        .expect("page 1h/5m rule loaded");
    assert!(
        page.query.contains("0.00144"),
        "four-nines page 1h/5m limit must be 0.00144 (budget 0.0001 * 14.4); got: {}",
        page.query
    );
}

/// US-01 `@property` (determinism across loads): the same on-disk SLO
/// loaded twice yields byte-identical synthesised rules. RED today (no
/// SLO rules load, so the two empty catalogues would trivially "match"
/// — guarded by also asserting four rules loaded).
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn synthesised_slo_rules_are_byte_identical_across_two_loads() {
    // @property @driving_port @real-io  (US-01)
    let rules = TmpRules::new("ws-determinism");
    rules.write(
        "checkout.toml",
        &checkout_slo_toml("https://ops.acme/alerts"),
    );

    let first = load_rules(rules.path()).expect("first load");
    let second = load_rules(rules.path()).expect("second load");

    assert_eq!(first.rules.len(), 4, "first load must hold four SLO rules");
    assert_eq!(
        second.rules.len(),
        4,
        "second load must hold four SLO rules"
    );
    for (a, b) in first.rules.iter().zip(second.rules.iter()) {
        assert_eq!(a.name, b.name, "names must be byte-identical across loads");
        assert_eq!(
            a.query, b.query,
            "PromQL must be byte-identical across loads"
        );
        assert_eq!(a.severity, b.severity);
        assert_eq!(a.labels, b.labels);
    }
}

// ====================================================================
// US-02 — a malformed target availability is refused (RED).
// ====================================================================

/// US-02 (THE primary safety negative): `target_availability = 1.0`
/// (budget 0, predicate `error_rate > 0`, the always-fire gun) is
/// REFUSED at load with the EXACT ADR-0067 F3 message naming the file,
/// the value, and the open range; ZERO rules load from that SLO. RED
/// today (no validation exists; today the file simply fails to parse on
/// the unknown `slo` field, which is a DIFFERENT failure than the
/// intended validation refusal — so the message assertion fails RED).
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn target_availability_one_is_refused_with_clear_message_no_rule_loaded() {
    // @driving_port @real-io  (US-02 error path)
    let rules = TmpRules::new("target-one");
    rules.write(
        "checkout.toml",
        r#"
[[slo]]
service = "checkout"
good_events_query = "http_requests_total{job=\"checkout\",code!~\"5..\"}"
total_events_query = "http_requests_total{job=\"checkout\"}"
target_availability = 1.0
error_budget_period = "30d"
"#,
    );

    let outcome = load_rules(rules.path()).expect("load");

    assert_eq!(
        outcome.rules.len(),
        0,
        "no always-fire rule may be synthesised from target_availability = 1.0"
    );
    assert_eq!(
        outcome.diagnostics.len(),
        1,
        "the bad SLO must surface exactly one diagnostic"
    );
    let diag = &outcome.diagnostics[0];
    assert!(
        diag.message.contains("invalid target_availability")
            && diag.message.contains("1")
            && diag
                .message
                .contains("must be strictly greater than 0 and strictly less than 1")
            && diag.message.contains("checkout"),
        "diagnostic must name the bad value, the open range, and the SLO (ADR-0067 F3); got: {}",
        diag.message
    );
    assert!(
        diag.file.ends_with("checkout.toml"),
        "diagnostic must name the offending file; got: {}",
        diag.file.display()
    );
}

/// US-02 boundary (`0.0` and `1.5`, both outside the open interval):
/// both are refused at load with the same clear range diagnostic and
/// load zero rules. RED today.
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn target_availability_outside_open_interval_is_refused() {
    // @driving_port @real-io  (US-02 boundary)
    for bad in ["0.0", "1.5"] {
        let rules = TmpRules::new("target-outside");
        rules.write(
            "checkout.toml",
            &format!(
                r#"
[[slo]]
service = "checkout"
good_events_query = "http_requests_total{{job=\"checkout\",code!~\"5..\"}}"
total_events_query = "http_requests_total{{job=\"checkout\"}}"
target_availability = {bad}
error_budget_period = "30d"
"#
            ),
        );

        let outcome = load_rules(rules.path()).expect("load");
        assert_eq!(
            outcome.rules.len(),
            0,
            "target_availability = {bad} must load zero rules"
        );
        assert_eq!(outcome.diagnostics.len(), 1, "target {bad} must diagnose");
        assert!(
            outcome.diagnostics[0]
                .message
                .contains("must be strictly greater than 0 and strictly less than 1"),
            "target {bad} must carry the open-range message; got: {}",
            outcome.diagnostics[0].message
        );
    }
}

/// US-02 negative control (a valid target loads): `0.999` is strictly
/// inside `(0,1)`, so its four rules load with no diagnostic. RED today
/// (the SLO path does not exist).
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn valid_target_availability_loads_its_four_rules() {
    // @driving_port @real-io  (US-02 negative control)
    let rules = TmpRules::new("target-valid");
    rules.write(
        "checkout.toml",
        &checkout_slo_toml("https://ops.acme/alerts"),
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert_eq!(outcome.diagnostics.len(), 0, "0.999 is a valid target");
    assert_eq!(outcome.rules.len(), 4, "a valid target loads four rules");
}

// ====================================================================
// US-03 — a non-30-day budget is refused (RED, doc-honesty).
// ====================================================================

/// US-03 (the doc-honesty negative): `error_budget_period = "7d"` is
/// REFUSED at load with the EXACT ADR-0067 F3 message naming the SLO and
/// stating only `30d` is supported at v0; zero rules load. Makes the
/// slo.rs:49-51 doc claim true. RED today.
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn seven_day_budget_is_refused_with_clear_message_no_rule_loaded() {
    // @driving_port @real-io  (US-03 error path)
    let rules = TmpRules::new("budget-7d");
    rules.write(
        "checkout.toml",
        r#"
[[slo]]
service = "checkout"
good_events_query = "http_requests_total{job=\"checkout\",code!~\"5..\"}"
total_events_query = "http_requests_total{job=\"checkout\"}"
target_availability = 0.999
error_budget_period = "7d"
"#,
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert_eq!(
        outcome.rules.len(),
        0,
        "a 7d budget must load zero rules (thresholds are 30d-only)"
    );
    assert_eq!(outcome.diagnostics.len(), 1, "the 7d SLO must diagnose");
    let diag = &outcome.diagnostics[0];
    assert!(
        diag.message.contains("unsupported error_budget_period")
            && diag.message.contains("7d")
            && diag.message.contains("only \"30d\" is supported at v0")
            && diag.message.contains("checkout"),
        "diagnostic must name the bad period, the supported value, and the SLO (ADR-0067 F3); got: {}",
        diag.message
    );
}

/// US-03 boundary (`90d`, the quarterly-budget habit): refused with the
/// same clear message; zero rules. RED today.
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn ninety_day_budget_is_refused() {
    // @driving_port @real-io  (US-03 boundary)
    let rules = TmpRules::new("budget-90d");
    rules.write(
        "checkout.toml",
        r#"
[[slo]]
service = "checkout"
good_events_query = "http_requests_total{job=\"checkout\",code!~\"5..\"}"
total_events_query = "http_requests_total{job=\"checkout\"}"
target_availability = 0.999
error_budget_period = "90d"
"#,
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert_eq!(outcome.rules.len(), 0, "a 90d budget must load zero rules");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("only \"30d\" is supported at v0"),
        "90d must carry the 30d-only message; got: {}",
        outcome.diagnostics[0].message
    );
}

/// US-03 negative control (a 30d budget loads): `"30d"` matches the
/// supported value, so its four rules load. RED today.
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn thirty_day_budget_loads_its_four_rules() {
    // @driving_port @real-io  (US-03 negative control)
    let rules = TmpRules::new("budget-30d");
    rules.write(
        "checkout.toml",
        &checkout_slo_toml("https://ops.acme/alerts"),
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert_eq!(outcome.diagnostics.len(), 0, "30d is the supported budget");
    assert_eq!(outcome.rules.len(), 4, "a 30d budget loads four rules");
}

// ====================================================================
// US-04 — synthesised SLO rules coexist with hand-authored rules (RED +
// a PASSING negative control).
// ====================================================================

/// US-04 happy path (coexistence): a dir with `checkout.toml` (one
/// `[[slo]]`) and `disk.toml` (two hand-authored `[[rules]]`) loads SIX
/// rules — four synthesised plus two hand-authored — into one catalogue.
/// RED today (the `[[slo]]` file poisons itself; only the two
/// hand-authored rules would load → 2, not 6).
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn synthesised_slo_rules_coexist_with_hand_authored_rules() {
    // @driving_port @real-io  (US-04)
    let rules = TmpRules::new("coexist");
    rules.write(
        "checkout.toml",
        &checkout_slo_toml("https://ops.acme/alerts"),
    );
    rules.write(
        "disk.toml",
        r#"
[[rules]]
name = "disk-pressure"
query = "disk_free < 0.1"
severity = "warning"

[[rules]]
name = "disk-inodes"
query = "disk_inodes_free < 0.05"
severity = "warning"
"#,
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert_eq!(
        outcome.diagnostics.len(),
        0,
        "a mixed dir must load cleanly; got: {:?}",
        outcome.diagnostics
    );
    assert_eq!(
        outcome.rules.len(),
        6,
        "four synthesised + two hand-authored = six rules; got: {:?}",
        loaded_rule_names(&outcome)
    );
    let names = loaded_rule_names(&outcome);
    for expected in CHECKOUT_SLO_RULE_NAMES {
        assert!(names.iter().any(|n| n == expected), "missing {expected}");
    }
    assert!(names.iter().any(|n| n == "disk-pressure"));
    assert!(names.iter().any(|n| n == "disk-inodes"));
}

/// US-04 error path (collision surfaced, not silently shadowed): a
/// hand-authored rule named `checkout_slo_page_1h_5m` (colliding with a
/// synthesised SLO rule name) REFUSES the load with a diagnostic naming
/// the duplicate; neither rule is silently dropped (ADR-0067 F2). RED
/// today (the `[[slo]]` poisons its file, so no collision is reached).
#[test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
fn name_collision_is_surfaced_not_silently_shadowed() {
    // @driving_port @real-io  (US-04 error path)
    let rules = TmpRules::new("collision");
    rules.write(
        "checkout.toml",
        &checkout_slo_toml("https://ops.acme/alerts"),
    );
    rules.write(
        "disk.toml",
        r#"
[[rules]]
name = "checkout_slo_page_1h_5m"
query = "up == 0"
severity = "critical"
"#,
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert!(
        outcome.has_diagnostics(),
        "a name collision must surface a diagnostic, never a silent shadow"
    );
    let collision = outcome
        .diagnostics
        .iter()
        .find(|d| d.message.contains("checkout_slo_page_1h_5m"))
        .unwrap_or_else(|| {
            panic!(
                "a diagnostic must name the duplicated rule; got: {:?}",
                outcome.diagnostics
            )
        });
    assert!(
        collision.message.to_lowercase().contains("duplicate")
            || collision.message.to_lowercase().contains("collision"),
        "the diagnostic must describe the collision; got: {}",
        collision.message
    );
}

/// US-04 negative control (PASSING TODAY — the regression guard): a
/// rules-only directory (no `[[slo]]` anywhere) loads exactly as before
/// this feature. This calls the SHIPPED `load_rules` over a plain
/// `[[rules]]` file and asserts the unchanged behaviour; it must STAY
/// GREEN through DELIVER (the `slo` FileShape vector defaults empty).
/// UN-ignored on purpose: it is the byte-identical-rules-only-path
/// guardrail (KPI 3).
#[test]
fn rules_only_directory_loads_exactly_as_before() {
    // @real-io  (US-04 negative control — PASSES TODAY, guardrail)
    let rules = TmpRules::new("rules-only");
    rules.write(
        "disk.toml",
        r#"
[[rules]]
name = "disk-pressure"
query = "disk_free < 0.1"
for_duration = "1m"
interval = "30s"
severity = "warning"

[[rules.sinks]]
kind = "webhook"
url = "https://ops.acme/alerts"
"#,
    );

    let outcome = load_rules(rules.path()).expect("load");
    assert_eq!(
        outcome.diagnostics.len(),
        0,
        "a clean rules-only dir has no diagnostics"
    );
    assert_eq!(
        outcome.rules.len(),
        1,
        "one hand-authored rule loads as one rule"
    );
    assert_eq!(outcome.rules[0].name, "disk-pressure");
    assert_eq!(outcome.rules[0].severity, Severity::Warning);
}

// ====================================================================
// F5 — the 24-hour cross-validation test (PASSING TODAY, deliverable).
//
// The DESIGN F5 call delivers the test that slo.rs:24-26 claims exists.
// It grounds the ALREADY-SHIPPED `synthesise_slo` output (the embedded
// `budget * threshold` limit) plus a hand-authored firing predicate
// against a deterministic synthetic 24-hour trace. Because
// `synthesise_slo` exists and is deterministic (no clock, no RNG), these
// tests PASS today and are UN-ignored: they are the firing-correctness
// guardrail, the honest backing for the doc claim, NOT the RED outer
// loop. The reference is a HAND-AUTHORED expected-firing pattern (NOT a
// .cue schema — ADR-0036's CUE references are corrected by ADR-0067).
// ====================================================================

/// A `checkout` SLO with target 0.999 (budget 0.001), used as the engine
/// input for the cross-validation. Mirrors the slice_05 fixture shape.
fn checkout_slo_fixture() -> Slo {
    Slo {
        service: "checkout".to_string(),
        sli_good_events: "http_requests_total{job=\"checkout\",code!~\"5..\"}".to_string(),
        sli_total_events: "http_requests_total{job=\"checkout\"}".to_string(),
        target_availability: 0.999,
        error_budget_period: Duration::from_secs(30 * 24 * 3600),
        sinks: vec![SinkConfig {
            kind: "webhook".to_string(),
            url: Some("https://ops.acme/alerts".to_string()),
            ..Default::default()
        }],
        source_path: Some("rules/checkout.toml".to_string()),
    }
}

/// Extract the numeric `budget * threshold` limit a synthesised rule
/// gates both its windows on. The PromQL shape (slo.rs:158-191) is
/// `... > {limit}) and (... > {limit})`; we read the limit back from the
/// FIRST `> ` so the cross-validation evaluates the SAME number the
/// engine emitted (no second source of truth for the threshold).
fn limit_of(rule: &beacon::Rule) -> f64 {
    let after = rule
        .query
        .split_once("> ")
        .expect("synthesised PromQL contains a threshold comparison")
        .1;
    let token: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    token.parse().expect("threshold parses as a float")
}

/// The hand-authored firing predicate: a synthesised MWMBR rule fires
/// iff BOTH its windows' observed error rate exceed the rule's limit
/// (the engine ANDs the two windows, slo.rs:183-184). The 24h trace is
/// summarised as a single sustained error rate (constant over the day),
/// so both windows observe the same rate — the deterministic, in-process
/// analogue of the workbook's firing table.
fn fires(rule: &beacon::Rule, observed_error_rate: f64) -> bool {
    observed_error_rate > limit_of(rule)
}

/// F5 ARM A (above budget MUST fire the page rules): a sustained error
/// rate of 5% (0.05) against a 0.999 SLO (budget 0.001) is far above
/// every page limit (0.0144, 0.006), so BOTH page rules fire — matching
/// the hand-authored reference. PASSES TODAY (grounds the shipped
/// engine).
#[test]
fn cross_validation_above_budget_fires_the_page_rules() {
    // @property  (F5 cross-validation, above-budget arm — PASSES TODAY)
    let rules = synthesise_slo(&checkout_slo_fixture());
    let sustained_error_rate = 0.05; // 5%, a sustained fast burn

    let page_1h_5m = rules
        .iter()
        .find(|r| r.name == "checkout_slo_page_1h_5m")
        .unwrap();
    let page_6h_30m = rules
        .iter()
        .find(|r| r.name == "checkout_slo_page_6h_30m")
        .unwrap();

    // Hand-authored reference: at 5% sustained error, both page windows
    // burn far above their limits.
    assert!(
        fires(page_1h_5m, sustained_error_rate),
        "an above-budget 5% error rate MUST fire the page 1h/5m rule"
    );
    assert!(
        fires(page_6h_30m, sustained_error_rate),
        "an above-budget 5% error rate MUST fire the page 6h/30m rule"
    );
}

/// F5 ARM B (within budget MUST fire nothing — the negative control): a
/// sustained error rate of 0.0005 (0.05%) against a 0.999 SLO is BELOW
/// the budget (0.001) and below every limit, so NO rule fires — matching
/// the hand-authored reference. PASSES TODAY. This is the load-bearing
/// negative arm: a within-budget rate must never page.
#[test]
fn cross_validation_within_budget_fires_nothing() {
    // @property  (F5 cross-validation, within-budget arm — PASSES TODAY)
    let rules = synthesise_slo(&checkout_slo_fixture());
    let within_budget_error_rate = 0.0005; // 0.05%, comfortably inside budget

    for rule in &rules {
        assert!(
            !fires(rule, within_budget_error_rate),
            "a within-budget 0.05% error rate must NOT fire {}",
            rule.name
        );
    }
}

/// F5 (the firing ORDER is page-before-ticket — workbook fidelity): the
/// page rules carry tighter limits than the ticket rules, so as the
/// error rate climbs, page rules fire first. Asserts the limit ordering
/// the workbook table encodes. PASSES TODAY.
#[test]
fn cross_validation_page_limits_are_tighter_than_ticket_limits() {
    // @property  (F5 cross-validation, ordering — PASSES TODAY)
    let rules = synthesise_slo(&checkout_slo_fixture());
    let limit = |name: &str| limit_of(rules.iter().find(|r| r.name == name).unwrap());

    let page_1h_5m = limit("checkout_slo_page_1h_5m");
    let page_6h_30m = limit("checkout_slo_page_6h_30m");
    let ticket_1d_2h = limit("checkout_slo_ticket_1d_2h");
    let ticket_3d_6h = limit("checkout_slo_ticket_3d_6h");

    assert!(
        page_1h_5m > page_6h_30m,
        "the 1h/5m page must gate on the tightest (largest) limit"
    );
    assert!(
        page_6h_30m > ticket_1d_2h,
        "the 6h/30m page must gate tighter than the 1d/2h ticket"
    );
    assert!(
        ticket_1d_2h > ticket_3d_6h,
        "the 1d/2h ticket must gate tighter than the 3d/6h ticket"
    );
}
