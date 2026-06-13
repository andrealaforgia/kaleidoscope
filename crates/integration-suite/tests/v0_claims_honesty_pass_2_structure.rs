// Kaleidoscope integration suite — structural acceptance for
// claims-honesty-pass-2-v0
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

//! Structural acceptance test for `claims-honesty-pass-2-v0`.
//!
//! The acceptance for this feature is **structural**, exactly like its
//! siblings `v0_perf_kpi_ci_non_gating_structure.rs` (ADR-0070) and
//! `v0_fast_precommit_structure.rs` (ADR-0072): the observable outcome
//! the reader (Devin Okafor, evaluator/contributor) wants lives in
//! committed *doc-comments*, a *Cargo manifest description*, the
//! platform *README*, a *prism README*, and a *playwright config* — not
//! in any runtime behaviour of a crate. So this test reads each
//! corrected file from disk via `std::fs::read_to_string` and asserts,
//! per overstatement locus, BOTH directions of the guard:
//!
//!   (a) the FALSE phrase is ABSENT  — so a non-correction cannot pass.
//!   (b) the TRUE  phrase is PRESENT — so an over-correction (deleting
//!       the claim, or swinging it into a new lie) cannot pass either.
//!
//! There is no service to stand up, no port to drive, no process to
//! spawn — the committed docs/comments/config ARE the driving surface a
//! reader reads (`cargo doc`, the rendered README, the CI page).
//!
//! ## What "done" looks like (the contract — DESIGN wave-decisions table)
//!
//! The DESIGN 9-locus overstatement->truth->proving-code table is the
//! authority. Scenarios below group the 9 loci into five behavioural
//! groups by locus, plus controls:
//!
//! - **pulse-volatility** (US-01, DESIGN row 1): `pulse/src/lib.rs` stops
//!   the unscoped crate-wide "restart loses points" under-claim and states
//!   the durable `FileBackedMetricStore` survives restart.
//! - **pulse-columnar** (US-01, DESIGN rows 2+3): `pulse/src/lib.rs` and
//!   `pulse/Cargo.toml` stop presenting a columnar (Arrow/Parquet/
//!   DataFusion/TSDB) adapter as shipped; columnar is future-tensed; the
//!   durable JSON-over-WAL adapter is named as shipped.
//! - **gateway-comments** (US-02, DESIGN rows 4+5+6): `main.rs` stops the
//!   "RED-ready NO-OP" and "force sink.kind = stub" comments; the test
//!   module prose stops "wired NO-OP" / "RED against the no-op subscriber".
//! - **prism-readme** (US-03, DESIGN rows 7+8): platform `README.md` stops
//!   "Unified query and visualisation frontend" and "compliance dashboards
//!   in Prism"; the Prism row is single-metric-PromQL-shaped.
//! - **prism-e2e-mark** (US-03, DESIGN row 9 — resolved MARK): the
//!   `playwright.config.ts` header marks the browser-matrix gate
//!   not-yet-implemented / scaffold; the `apps/prism/README.md` `pnpm
//!   playwright` note is marked scaffold.
//!
//! Plus the GREEN-today controls (un-ignored): the already-honest
//! `apps/prism/README.md` "single PromQL query panel" source-of-truth
//! stays present; the line `:37` "no daemon, no network" pulse fact (TRUE,
//! DESIGN row 1b — left as-is, NOT asserted against, only its truth
//! preserved); and the gateway test's `#[ignore]` attributes on the
//! fixed-port AC-01 scenarios are NOT removed (DELIVER edits prose only).
//!
//! ## nWave ordering — why the corrections are `#[ignore]`d (RED)
//!
//! DISTILL runs BEFORE DELIVER. The doc/comment/config edits are the
//! DELIVER act and do NOT exist yet. Verified at HEAD (2026-06-07): every
//! false phrase is still present (e.g. `pulse/src/lib.rs:46` still reads
//! "In-memory only at v0; restart loses points."; `main.rs:62` still reads
//! "RED-ready NO-OP that Crafty fills in DELIVER"; `playwright.config.ts:19`
//! still reads the unqualified "Gate 7 (Prism E2E across the browser
//! matrix)"). So each correction scenario is RED against today's tree and
//! is tagged `#[ignore = "RED until DELIVER: ..."]`: `cargo test` is GREEN
//! on the current tree (controls pass, RED scenarios ignored);
//! `cargo test -- --ignored` shows them FAILING — that is the
//! falsifiability evidence. DELIVER removes each `#[ignore]` when the
//! matching edit lands.
//!
//! ## Mandate 7 — RED, not BROKEN
//!
//! This test reads existing files and asserts on their content. No
//! production symbol is missing, so it COMPILES and links; the RED
//! assertions FAIL behaviourally (the false phrases are present today),
//! they do not error on setup. Each file read is `unwrap_or_else(panic!)`
//! with a clear message so a genuinely missing file reports a clean FAILED,
//! not an opaque ERROR.
//!
//! ## Path resolution (DEVOPS watch-item)
//!
//! `CARGO_MANIFEST_DIR` is `<repo>/crates/integration-suite`; two parents
//! up is the repo root. README.md, apps/prism/*, and crates/* are all
//! resolved from there — robust regardless of the caller's working
//! directory. (Mirrors both sibling structural tests.)

use std::fs;
use std::path::PathBuf;

/// The repository root, resolved from this crate's manifest directory.
///
/// `CARGO_MANIFEST_DIR` is `<repo>/crates/integration-suite`; two parents
/// up is the repository root. Robust regardless of the caller's working
/// directory. (Mirrors the perf-kpi / fast-precommit siblings.)
fn repo_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("crate manifest dir has a grandparent (the repo root)")
        .to_path_buf()
}

/// Read a repo-relative file into a string, or FAIL behaviourally (clean
/// FAILED, not an opaque setup ERROR) per Mandate 7.
fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

// =====================================================================
// Scenario: pulse-volatility (US-01 AC-1, DESIGN row 1) — RED, ignored.
//
// `pulse/src/lib.rs` must STOP the unscoped crate-wide volatility
// under-claim and STATE the durable store survives restart.
//
//   FALSE (assert ABSENT): "In-memory only at v0; restart loses points."
//     (the exact unscoped posture line at lib.rs:46 today)
//   TRUE  (assert PRESENT): names `FileBackedMetricStore` AND a
//     "survives" + "restart" durability claim.
//
// DESIGN row 1b guardrail: lib.rs:37 "No daemon, no network." is TRUE
// and stays — this scenario does NOT assert against it.
//
// FALSIFIABILITY (RED today): verified at HEAD — lib.rs:46 still reads
// the exact unscoped false phrase, and the crate doc does not yet carry
// a "survives restart" durability claim. DELIVER corrects the prose and
// removes this `#[ignore]`.
// =====================================================================
#[test]
fn pulse_doc_states_durable_store_survives_restart_not_crate_wide_volatility() {
    let lib = read_repo_file("crates/pulse/src/lib.rs");

    assert!(
        !lib.contains("In-memory only at v0; restart loses points."),
        "pulse/src/lib.rs must NOT carry the unscoped crate-wide claim \
         'In-memory only at v0; restart loses points.' (US-01 / DESIGN \
         row 1): pulse SHIPS a durable FileBackedMetricStore (lib.rs:65 \
         re-export, file_backed.rs), so an unscoped 'loses points' line \
         under-claims real durability. Any residual 'loses points' \
         wording must be scoped to InMemoryMetricStore."
    );

    assert!(
        lib.contains("FileBackedMetricStore"),
        "pulse/src/lib.rs crate doc must NAME the durable \
         `FileBackedMetricStore` (US-01 / DESIGN row 1): the durable \
         adapter that ships and survives restart must be visible in the \
         posture, not hidden."
    );
    assert!(
        lib.contains("survives") && lib.contains("restart"),
        "pulse/src/lib.rs crate doc must state the durable store \
         SURVIVES process RESTART (US-01 / DESIGN row 1), the corrected \
         truth that is verifiable in file_backed.rs (fsync-durable \
         WAL+snapshot). Both 'survives' and 'restart' must appear in the \
         corrected durability claim."
    );
}

// =====================================================================
// Scenario: pulse-columnar (US-01 AC-2 + AC-3, DESIGN rows 2+3) — RED,
// ignored.
//
// `pulse/src/lib.rs` AND `pulse/Cargo.toml` must STOP presenting a
// columnar (Arrow/Parquet/DataFusion/TSDB) adapter as SHIPPED, and must
// name the durable JSON-over-WAL adapter as shipped + columnar as future.
//
//   FALSE (assert ABSENT, lib.rs): the present-tense "(Arrow + Parquet +
//     DataFusion + Prometheus TSDB block) lives behind the same trait"
//     (the lib.rs:20-22 shipped-promise today)
//   FALSE (assert ABSENT, Cargo.toml): "the columnar (Arrow + Parquet +
//     DataFusion + Prometheus TSDB block) adapter lands at v1 behind the
//     same trait" (the description today)
//   TRUE  (assert PRESENT, both): "future" framing for columnar; the
//     durable JSON-over-WAL FileBackedMetricStore named as shipped.
//
// DESIGN rows 2/3 guardrail: columnar is named as a GENUINE future
// direction (future tense), not "never" and not "removed" — so this
// scenario asserts PRESENT "future", not ABSENT "columnar".
//
// FALSIFIABILITY (RED today): verified at HEAD — lib.rs:20-22 and
// Cargo.toml:7 carry the present-tense/lands-at-v1 columnar shipped-promise.
// =====================================================================
#[test]
fn pulse_docs_name_durable_adapter_and_future_tense_columnar() {
    let lib = read_repo_file("crates/pulse/src/lib.rs");
    let cargo = read_repo_file("crates/pulse/Cargo.toml");

    // (a) FALSE present-tense / shipped columnar promises ABSENT.
    assert!(
        !lib.contains(
            "(Arrow + Parquet + DataFusion + Prometheus TSDB block) lives\n//! behind the same trait."
        ) && !lib.contains(
            "(Arrow + Parquet + DataFusion + Prometheus TSDB block) lives behind the same trait"
        ),
        "pulse/src/lib.rs must NOT present a columnar (Arrow + Parquet + \
         DataFusion + Prometheus TSDB block) adapter as SHIPPED / \
         present-tense 'lives behind the same trait' (US-01 / DESIGN \
         row 2): the shipped durable adapter is line-delimited \
         JSON-over-WAL (file_backed.rs); no columnar dep exists in \
         Cargo.toml. Columnar must be future-tensed."
    );
    assert!(
        !cargo.contains(
            "the columnar (Arrow + Parquet + DataFusion + Prometheus TSDB block) adapter lands at v1 behind the same trait"
        ),
        "pulse/Cargo.toml `description` must NOT say the columnar adapter \
         'lands at v1 behind the same trait' (US-01 / DESIGN row 3): that \
         presents an unbuilt substrate as a committed v1 deliverable."
    );

    // (b) TRUE corrected framing PRESENT: columnar named as future; the
    //     durable adapter named as shipped.
    assert!(
        lib.contains("future"),
        "pulse/src/lib.rs must name the columnar substrate as a FUTURE \
         direction (future tense) (US-01 / DESIGN row 2): the roadmap \
         intent is preserved, only the shipped-promise is removed."
    );
    assert!(
        lib.contains("FileBackedMetricStore"),
        "pulse/src/lib.rs must name the durable JSON-over-WAL \
         `FileBackedMetricStore` as the shipped durable adapter (US-01 / \
         DESIGN row 2), so a reader sees what actually ships."
    );
    assert!(
        cargo.contains("FileBackedMetricStore") || cargo.contains("file-backed"),
        "pulse/Cargo.toml `description` must name the durable file-backed \
         adapter as shipped (US-01 / DESIGN row 3), not only the \
         in-memory adapter."
    );
    assert!(
        cargo.contains("future"),
        "pulse/Cargo.toml `description` must name the columnar substrate \
         as a FUTURE direction (US-01 / DESIGN row 3), not a shipped v1 \
         adapter."
    );
}

// =====================================================================
// Scenario: gateway-comments (US-02 AC-1+2+3, DESIGN rows 4+5+6) — RED,
// ignored.
//
// `main.rs` comments + the slice_01 test-module prose must DESCRIBE the
// delivered, GREEN code.
//
//   FALSE (assert ABSENT, main.rs):
//     - "RED-ready NO-OP that Crafty fills in DELIVER" (the :62 comment)
//     - "Force `sink.kind = stub`" (the :118 comment)
//     - "forces\n//!   `sink.kind = stub` internally" / "forces `sink.kind
//        = stub`" (the module doc :24-25)
//   FALSE (assert ABSENT, slice_01 test):
//     - "wired NO-OP" (the module note :42)
//     - "RED against the no-op subscriber" (the per-test prose :207, :280)
//   TRUE  (assert PRESENT, main.rs): "installs the real JSON-to-stderr"
//     subscriber; "relies on" + "Stub default".
//   TRUE  (assert PRESENT, slice_01 test): "installs the real
//     JSON-to-stderr" subscriber; "GREEN" reality.
//
// DESIGN row 6 guardrail: only the no-op/RED WORDING changes; the
// `#[ignore]` attributes on the fixed-port AC-01 scenarios are untouched
// (asserted by the separate gateway-ignore control below).
//
// FALSIFIABILITY (RED today): verified at HEAD — main.rs:62/118/24-25 and
// slice_01:42/207/280 carry the false phrases.
// =====================================================================
#[test]
fn gateway_comments_and_test_prose_describe_the_green_delivered_code() {
    let main_rs = read_repo_file("crates/kaleidoscope-gateway/src/main.rs");
    let test_rs =
        read_repo_file("crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs");

    // (a) FALSE phrases ABSENT — main.rs comments.
    assert!(
        !main_rs.contains("RED-ready NO-OP that Crafty fills in DELIVER"),
        "main.rs must NOT call init_tracing a 'RED-ready NO-OP that Crafty \
         fills in DELIVER' (US-02 / DESIGN row 4): init_tracing \
         (main.rs:153-173) installs a real JSON-to-stderr subscriber today."
    );
    assert!(
        !main_rs.contains("Force `sink.kind = stub`")
            && !main_rs.contains("forces\n//!   `sink.kind = stub` internally")
            && !main_rs.contains("forces `sink.kind = stub`"),
        "main.rs must NOT claim the gateway 'forces sink.kind = stub' \
         (US-02 / DESIGN row 5): the next line is Config::builder().build() \
         (main.rs:121), which RELIES on the builder's Stub default; it does \
         not force the kind."
    );

    // (b) TRUE phrases PRESENT — main.rs comments.
    assert!(
        main_rs.contains("installs the real JSON-to-stderr"),
        "main.rs comment must state init_tracing 'installs the real \
         JSON-to-stderr' subscriber (US-02 / DESIGN row 4), matching the \
         init_tracing body."
    );
    assert!(
        main_rs.contains("relies on") && main_rs.contains("Stub default"),
        "main.rs comment must state the gateway 'relies on' the \
         Config::builder() 'Stub default' (US-02 / DESIGN row 5), the \
         accurate description of Config::builder().build()."
    );

    // (a) FALSE phrases ABSENT — slice_01 test prose.
    assert!(
        !test_rs.contains("wired NO-OP"),
        "slice_01_tracing_subscriber.rs must NOT describe init_tracing as \
         a 'wired NO-OP' (US-02 / DESIGN row 6): the always-run AC-02 \
         scenarios assert health.startup.refused IS present and PASS \
         (GREEN)."
    );
    assert!(
        !test_rs.contains("RED against the no-op subscriber"),
        "slice_01_tracing_subscriber.rs must NOT carry 'RED against the \
         no-op subscriber' (US-02 / DESIGN row 6): that prose describes a \
         RED that is no longer true — the suite is GREEN."
    );

    // (b) TRUE phrases PRESENT — slice_01 test prose.
    assert!(
        test_rs.contains("installs the real JSON-to-stderr"),
        "slice_01_tracing_subscriber.rs prose must describe the installed \
         real JSON-to-stderr subscriber (US-02 / DESIGN row 6)."
    );
    assert!(
        test_rs.contains("GREEN"),
        "slice_01_tracing_subscriber.rs prose must describe the always-run \
         fail-closed scenarios as GREEN (US-02 / DESIGN row 6)."
    );
}

// =====================================================================
// Scenario: prism-readme (US-03 AC-1+2, DESIGN rows 7+8) — RED, ignored.
//
// Platform `README.md` must STOP overstating Prism as a dashboarding
// product and STOP claiming a non-existent compliance-dashboards feature.
//
//   FALSE (assert ABSENT):
//     - "Unified query and visualisation frontend" (the Prism row :184)
//     - "The compliance dashboards in Prism are open templates." (the
//        cost line :222)
//   TRUE  (assert PRESENT):
//     - a single-metric PromQL framing for Prism (e.g. "single-metric
//        PromQL" or "single PromQL"), consistent with apps/prism/README.md
//     - a retained, true cost-model point ("contact sales" economics kept
//        without inventing dashboards) — asserted as the cost line still
//        mentioning Prism truthfully (PRESENT "Prism" in the cost table
//        body, no compliance-dashboards claim).
//
// DESIGN row 7/8 guardrail: the corrected wording is aligned TO
// apps/prism/README.md ("a single PromQL query panel"); the cost row keeps
// an honest economic answer (RESTATE over DELETE).
//
// FALSIFIABILITY (RED today): verified at HEAD — README:184 and :222 carry
// the exact false phrases.
// =====================================================================
#[test]
fn readme_prism_row_and_cost_line_match_the_single_metric_reality() {
    let readme = read_repo_file("README.md");

    // (a) FALSE phrases ABSENT.
    assert!(
        !readme.contains("Unified query and visualisation frontend"),
        "README.md must NOT describe Prism as a 'Unified query and \
         visualisation frontend' (US-03 / DESIGN row 7): apps/prism/README.md \
         (the honest source) says Prism ships 'a single PromQL query panel'. \
         It is a single-metric explorer, not a dashboarding product."
    );
    assert!(
        !readme.contains("The compliance dashboards in Prism are open templates."),
        "README.md must NOT claim 'The compliance dashboards in Prism are \
         open templates.' (US-03 / DESIGN row 8): Prism has no compliance \
         dashboards. Restate the true economic point without inventing a \
         feature."
    );

    // (b) TRUE corrected framing PRESENT: single-metric PromQL.
    assert!(
        readme.contains("single-metric PromQL")
            || readme.contains("single PromQL")
            || readme.contains("single-metric"),
        "README.md must describe the Prism row as a single-metric PromQL \
         query/chart explorer (US-03 / DESIGN row 7), consistent with \
         apps/prism/README.md ('a single PromQL query panel')."
    );
    // (b) TRUE: the cost table keeps an honest Prism answer (RESTATE, not
    //     DELETE) — Prism still named in the cost surface, without the
    //     compliance-dashboards lie.
    assert!(
        readme.contains("Prism"),
        "README.md must keep naming Prism truthfully in the cost surface \
         (US-03 / DESIGN row 8): the cost-model row is RESTATED to keep an \
         honest economic answer, not deleted."
    );
}

// =====================================================================
// Scenario: prism-e2e-mark (US-03 AC-3, DESIGN row 9 — resolved MARK) —
// RED, ignored.
//
// `apps/prism/playwright.config.ts` + the prism README `pnpm playwright`
// note must MARK the browser-matrix e2e gate not-yet-implemented /
// scaffold (no spec runs today), without deleting the config, the browser
// projects, the digest, or the slice-by-slice re-add roadmap.
//
//   FALSE (assert ABSENT, playwright.config.ts): the unqualified
//     "Gate 7 (Prism E2E across the browser matrix)." advertised-as-live
//     (the :19 header today, with no scaffold qualifier)
//   TRUE  (assert PRESENT, playwright.config.ts): "scaffold" or
//     "NOT YET IMPLEMENTED" near the former Gate-7 advertisement.
//   TRUE  (assert PRESENT, apps/prism/README.md): the `pnpm playwright`
//     note marked "scaffold".
//
// DESIGN row 9 guardrail: only the false "gate works" advertisement is
// marked; the per-spec UNIMPLEMENTED bodies, the digest-SSOT invariant,
// and the re-add roadmap are untouched. The control below proves the
// digest constant survives.
//
// FALSIFIABILITY (RED today): verified at HEAD — playwright.config.ts:19
// carries the unqualified "Gate 7 (Prism E2E across the browser matrix)."
// and neither "scaffold" nor "NOT YET IMPLEMENTED" appears in the config.
// =====================================================================
#[test]
fn prism_e2e_browser_matrix_gate_is_marked_scaffold_not_advertised_live() {
    let config = read_repo_file("apps/prism/playwright.config.ts");
    let prism_readme = read_repo_file("apps/prism/README.md");

    // (a) FALSE unqualified advertisement ABSENT.
    assert!(
        !config.contains("Gate 7 (Prism E2E across the browser matrix)."),
        "playwright.config.ts must NOT advertise an unqualified 'Gate 7 \
         (Prism E2E across the browser matrix).' (US-03 / DESIGN row 9): \
         testMatch matches no spec (:50) and every e2e spec throws \
         UNIMPLEMENTED — no browser-matrix gate runs today."
    );

    // (b) TRUE scaffold marker PRESENT.
    assert!(
        config.contains("scaffold") || config.contains("NOT YET IMPLEMENTED"),
        "playwright.config.ts must MARK the browser-matrix e2e gate as \
         'scaffold' / 'NOT YET IMPLEMENTED' (US-03 / DESIGN row 9, MARK \
         decision): a reader must not read this config as a passing \
         quality gate."
    );
    assert!(
        prism_readme.contains("scaffold"),
        "apps/prism/README.md `pnpm playwright` note must be marked \
         'scaffold' (no e2e spec runs yet) (US-03 / DESIGN row 9)."
    );
}

// =====================================================================
// CONTROL: prism-readme-honest-source (GREEN today AND after) — un-ignored.
//
// apps/prism/README.md is the already-honest module-local source of truth
// that every correction aligns TO. It says Prism "v0 ships a single PromQL
// query panel". This control asserts that single-metric source of truth
// STAYS — green now, green after DELIVER. It is the anchor the
// prism-readme RED scenario aligns the platform README to; if a future
// edit inflated the module README too, this control reds.
// =====================================================================
#[test]
fn prism_module_readme_stays_the_single_metric_source_of_truth() {
    let prism_readme = read_repo_file("apps/prism/README.md");

    assert!(
        prism_readme.contains("single\nPromQL query panel")
            || prism_readme.contains("single PromQL query panel"),
        "apps/prism/README.md must remain the single-metric source of \
         truth: 'v0 ships a single PromQL query panel' (US-03). This is \
         the already-honest anchor the platform README is aligned TO; it \
         must not itself be inflated. Found:\n{prism_readme}"
    );
}

// =====================================================================
// CONTROL: gateway-ignore-untouched (GREEN today AND after) — un-ignored.
//
// DESIGN row 6 guardrail: the correction edits PROSE ONLY. The `#[ignore]`
// attributes on the fixed-port AC-01 scenarios in
// slice_01_tracing_subscriber.rs are real (port-flake determinism, NOT an
// absent subscriber) and MUST survive the prose correction. This control
// asserts both fixed-port `#[ignore]` attributes still exist with their
// port-flake reasons — so DELIVER cannot, while rewording, accidentally
// (or over-correctingly) de-ignore them.
//
// Passes today and must keep passing after DELIVER.
// =====================================================================
#[test]
fn gateway_fixed_port_ac_01_ignore_attributes_are_not_removed() {
    let test_rs =
        read_repo_file("crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs");

    assert!(
        test_rs.contains(
            "#[ignore = \"binds the gateway's FIXED default ports (4317/4318); RED-ready, see module docs\"]"
        ),
        "the fixed-port AC-01 'RED-ready' `#[ignore]` attribute must \
         survive the prose correction (US-02 / DESIGN row 6): its ignore \
         reason is port-flake determinism, not an absent subscriber. \
         DELIVER edits prose only — it must NOT remove this #[ignore]."
    );
    assert!(
        test_rs.contains(
            "#[ignore = \"binds the gateway's FIXED default ports (4317/4318); regression guard, see module docs\"]"
        ),
        "the fixed-port AC-01 'regression guard' `#[ignore]` attribute \
         must survive the prose correction (US-02 / DESIGN row 6): same \
         port-flake reason. DELIVER edits prose only."
    );
}

// =====================================================================
// CONTROL: prism-e2e-roadmap-preserved (GREEN today AND after) —
// un-ignored.
//
// DESIGN row 9 MARK guardrail: the MARK correction preserves two genuine
// engineering artefacts a future "build the prism e2e" feature relies on —
// the Prometheus digest-SSOT constant and the slice-by-slice re-add
// roadmap. This control asserts the digest constant and the per-slice plan
// stay present, so MARK (annotate) is not silently turned into REMOVE
// (delete). UPDATED by prism-echarts-paint-e2e-v0, which graduated slices
// 01 + 03 from scaffold to real GREEN specs: the MARK was honestly
// FULFILLED for those two, not deleted, and the digest-SSOT plus the plan
// for the remaining scaffold slices (02/04/05/06) must still survive.
// =====================================================================
#[test]
fn prism_e2e_digest_ssot_and_readd_roadmap_are_preserved() {
    let config = read_repo_file("apps/prism/playwright.config.ts");

    assert!(
        config.contains("PROMETHEUS_IMAGE_DIGEST"),
        "playwright.config.ts must keep the Prometheus digest-SSOT \
         constant (US-03 / DESIGN row 9 MARK): the digest ↔ CI \
         gate-11 atomic-bump invariant is a genuine artefact MARK \
         preserves; it must not be deleted with the advertisement."
    );
    assert!(
        config.contains("Per-slice status:") && config.contains("slice-06-accessibility.spec.ts"),
        "playwright.config.ts must keep the per-slice graduation plan \
         (US-03 / DESIGN row 9 MARK, evolved by prism-echarts-paint-e2e-v0 \
         which graduated slices 01 + 03): the plan for the remaining \
         scaffold slices (02/04/05/06) must survive as the roadmap for \
         graduating the rest, so a partial graduation does not silently \
         delete the plan for what is left."
    );
}
