//! Slice 08 — claims-honesty-pass-v0 doc-lint / grep guards.
//!
//! Feature: `claims-honesty-pass-v0`. The project's thesis is structural
//! honesty against vendor overstatement; this feature applies that thesis
//! to the project's OWN prose. Each guard below asserts that a specific
//! FALSE claim is ABSENT from a target document AND the CORRECTED claim is
//! PRESENT.
//!
//! ## Why these live here (DEVOPS decision 3)
//!
//! The repo-root `README.md` and several cross-crate doc surfaces belong
//! to no single crate. A guard compiled inside a crate's `tests/` reaches
//! the workspace root via `env!("CARGO_MANIFEST_DIR")` joined with
//! `../../` (crates live at `crates/<crate>/`), the same idiom
//! `slice_07_lock_the_contract.rs` already uses for `tests/vectors`. Per
//! the DEVOPS `wave-decisions.md`, all the workspace-file doc-lint greps
//! are consolidated into THIS single dedicated docs-guard file rather
//! than scattered across crates — one portable anchor, one place to
//! maintain the `../../` hop.
//!
//! ## nWave order — why most of these are `#[ignore]`d (RED, not BROKEN)
//!
//! DISTILL runs BEFORE DELIVER. The prose corrections these guards check
//! for DO NOT EXIST YET — DELIVER makes them. So the false-string-absent
//! and corrected-string-present guards FAIL today (the false string is
//! still present). They are marked `#[ignore = "RED until DELIVER:
//! claims-honesty-pass-v0"]` so `cargo test --workspace` stays GREEN at
//! the DISTILL commit. DELIVER removes each `#[ignore]` immediately after
//! it applies the matching prose correction. The guards COMPILE today
//! (they are plain file-reads + string checks, no new symbols), so they
//! are RED-not-BROKEN, exactly as intended.
//!
//! ## The US-03 bidirectional guard (load-bearing)
//!
//! The US-03 honesty pass must remove the stale `__SCAFFOLD__`-over-green
//! doc comments WITHOUT deleting any marker that describes a TRUE current
//! RED state. So US-03 is expressed in BOTH directions:
//!
//! - `us03_*_stale_*` (RED, `#[ignore]`d): the stale-over-green scaffold
//!   claim is GONE — true only after DELIVER edits the prose.
//! - `us03_in_flight_scaffold_markers_remain_present` (GREEN, NOT
//!   ignored): the genuinely-RED in-flight `__SCAFFOLD__` / `#[ignore]`
//!   markers are STILL PRESENT — true today and must STAY true, so the
//!   prose pass cannot over-reach and silence an honest in-flight marker.

use std::path::PathBuf;

/// Resolve a workspace-root-relative path from this crate's manifest dir.
/// `crates/otlp-conformance-harness/` -> repo root is `../../`.
fn workspace_path(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("..");
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

/// Read a workspace file to a String, panicking with a readable locus on
/// failure (a missing target file is a guard failure, not a silent pass).
fn read_workspace(rel: &str) -> String {
    let path = workspace_path(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} ({}): {e}", rel, path.display()))
}

/// Assert `needle` is ABSENT from the named file.
fn assert_absent(rel: &str, needle: &str) {
    let body = read_workspace(rel);
    assert!(
        !body.contains(needle),
        "{rel}: the false claim {needle:?} must be ABSENT but is still present"
    );
}

/// Assert `needle` is PRESENT in the named file.
fn assert_present(rel: &str, needle: &str) {
    let body = read_workspace(rel);
    assert!(
        body.contains(needle),
        "{rel}: the expected claim {needle:?} must be PRESENT but was not found"
    );
}

// =====================================================================
// US-01 — README "Components at a glance" table stops overstating four
// capabilities. Aligned TO each crate's already-honest lib.rs.
// =====================================================================

/// @US-01
///
/// Spark row: ABSENT the present-tense "Auto-instrumentation SDKs" claim;
/// PRESENT a manual-init OTel SDK wrapper role with auto-instrumentation
/// future-tensed (matching `spark/src/lib.rs`: "thin wrapper around the
/// upstream opentelemetry crates").
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us01_readme_spark_row_names_manual_init_not_auto_instrumentation() {
    assert_absent("README.md", "| **Spark**      | Auto-instrumentation SDKs");
    assert_present("README.md", "manual-init OTel SDK wrapper");
}

/// @US-01
///
/// Strata row AND the cost-table line both describe passive profile
/// storage; "continuous" is future-tensed on BOTH surfaces (matching
/// `strata/src/lib.rs`: "first-party profile storage engine … Library
/// only at v0. No daemon, no network.").
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us01_readme_strata_row_and_cost_line_name_passive_profile_storage() {
    // Component-table row: the bare present-tense "Continuous profiling"
    // claim is gone.
    assert_absent("README.md", "| **Strata**     | Continuous profiling");
    // Cost table: "Strata is included." under a "Continuous profiling …
    // top-tier add-on" framing is qualified so continuous scraping is
    // marked roadmap, not present-tense.
    assert_absent("README.md", "Continuous profiling as a top-tier add-on");
    assert_present("README.md", "profile storage");
}

/// @US-01
///
/// Cinder row: ABSENT the "cold-tier coordinator" present-tense claim;
/// PRESENT a local tier-metadata coordinator with the object-storage cold
/// tier future-tensed (matching `cinder/src/lib.rs`: "stores tier
/// metadata, not payloads … In-memory only at v0").
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us01_readme_cinder_row_names_local_tier_metadata_not_cold_tier_coordinator() {
    assert_absent("README.md", "cold-tier coordinator");
    assert_present("README.md", "tier-metadata");
}

/// @US-01
///
/// Loom row: ABSENT the "Dashboards-as-code" present-tense claim; PRESENT
/// TOML rule-catalogue change control with dashboards future-tensed
/// (matching `loom/src/lib.rs`: "change-control surface", "reads `.toml`").
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us01_readme_loom_row_names_toml_change_control_not_dashboards_as_code() {
    assert_absent(
        "README.md",
        "| **Loom**       | Dashboards-as-code, alert-rules-as-code",
    );
    assert_present("README.md", "change control");
}

// NOTE: the crate-LOCAL doc guards live in their home crates' tests/ so
// each guard sits next to the file it protects:
//   - US-02 (codex stub headers)            -> codex/tests/slice_06_*
//   - US-03 stale-over-green (query-http-common)
//                                            -> query-http-common/tests/slice_01_*
//   - US-03 stale-over-green (trace-query-api)
//                                            -> trace-query-api/tests/slice_04_*
//   - US-05 README step framing doc-guard    -> (below, README is workspace-root)
// Only the workspace-root README greps and the CROSS-CRATE US-03 in-flight
// half are consolidated HERE, per DEVOPS decision 3 (one portable anchor).

// =====================================================================
// US-03 — the CROSS-CRATE in-flight half of the bidirectional guard.
// (The two stale-over-green halves live in their home crates.)
// =====================================================================

/// @US-03
///
/// IN-FLIGHT half (GREEN today; must STAY green). The genuinely-RED
/// in-flight `__SCAFFOLD__` / `#[ignore]` markers named in the feature
/// `wave-decisions.md` remain INTACT — proving the US-03 prose pass did
/// NOT over-reach and silence a marker that describes a TRUE current RED
/// state. This is the load-bearing other half of the bidirectional guard.
///
/// NOT `#[ignore]`d: these markers exist today and the honesty pass must
/// leave them untouched, so this assertion is true now and must remain
/// true after DELIVER.
#[test]
fn us03_in_flight_scaffold_markers_remain_present() {
    // log-query body-regex: a live in-flight `__SCAFFOLD__` RED marker.
    assert_present(
        "crates/log-query-api/tests/slice_05_body_regex.rs",
        "__SCAFFOLD__ log-body-regex-search-v0 RED",
    );
    // crash-durability suites: genuine RED-not-BROKEN `#[ignore]` markers.
    assert_present(
        "crates/lumen/tests/v1_slice_04_crash_durability.rs",
        "RED-not-BROKEN",
    );
    assert_present(
        "crates/ray/tests/v1_slice_04_crash_durability.rs",
        "#[ignore]",
    );
    // aperture tls-config-reject: in-flight RED markers.
    assert_present(
        "crates/aperture/tests/slice_09_tls_config_reject.rs",
        "#[ignore]",
    );
    // log-query pagination + tracing-subscriber in-flight scaffolds.
    assert_present(
        "crates/log-query-api/tests/slice_06_pagination.rs",
        "__SCAFFOLD__",
    );
}

// =====================================================================
// US-04 — the conformance harness stops claiming semantic wire-spec
// validation, and its README status stops claiming "implementation
// absent". (Behaviour assertions live in slice_09.)
// =====================================================================

/// @US-04
///
/// Harness `lib.rs` / `README.md` / `Cargo.toml` no longer claim the
/// harness "validates against the OpenTelemetry OTLP **wire
/// specification**"; they describe structural decode-level validation and
/// name the absent semantic checks. Aligned TO `decode.rs`/`validate.rs`.
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us04_harness_describes_structural_decode_not_wire_spec_conformance() {
    for rel in [
        "crates/otlp-conformance-harness/src/lib.rs",
        "crates/otlp-conformance-harness/README.md",
        "crates/otlp-conformance-harness/Cargo.toml",
    ] {
        assert_absent(rel, "OpenTelemetry OTLP wire specification");
    }
    // The corrected depth wording is grounded in the code's actual checks.
    assert_present(
        "crates/otlp-conformance-harness/src/lib.rs",
        "structural decode-level",
    );
}

/// @US-04
///
/// Harness `README.md` status block no longer claims "implementation
/// intentionally absent / every `validate_*` returns `unimplemented!()`".
/// Aligned TO `lib.rs:17-22` ("implemented and green").
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us04_harness_readme_status_reflects_delivered_green_code() {
    assert_absent(
        "crates/otlp-conformance-harness/README.md",
        "Implementation is intentionally absent at this point",
    );
    assert_absent(
        "crates/otlp-conformance-harness/README.md",
        "every\n`validate_*` function returns `unimplemented!()`",
    );
}

// =====================================================================
// US-05 — the README's "Prometheus-compatible" query_range framing is
// qualified so it no longer implies a stepped grid (ADR-0062). The
// INVARIANCE behaviour test lives in query-api/tests/slice_06.
// =====================================================================

/// @US-05
///
/// The README `/api/v1/query_range` description states that `step` is
/// accepted-but-not-honoured at v0 (raw points, no grid re-sampling),
/// matching the already-honest in-code field doc (`query-api/src/lib.rs`:
/// "`step` is accepted and ignored at v0 (DD5: raw points, no
/// re-stepping)") and ADR-0062.
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us05_readme_query_range_no_longer_implies_a_prometheus_stepped_grid() {
    // The corrected README states `step` is not honoured at v0.
    assert_present("README.md", "raw");
    assert_present("README.md", "step");
    // The bare unqualified "Prometheus-compatible" framing on the
    // query_range endpoint is gone (DELIVER qualifies it; ADR-0062).
    assert_absent(
        "README.md",
        "Prometheus-compatible `/api/v1/query_range` HTTP endpoint over the durable",
    );
}

// =====================================================================
// US-06 — the harness `Framing::GrpcProtobuf` doc-honesty propagation.
// lib.rs/README state the framing is a non-behavioural label echoed into
// violations; the caller strips the gRPC length prefix. (The INVARIANCE
// behaviour test lives in slice_09.)
// =====================================================================

/// @US-06
///
/// Harness `lib.rs` / `README.md` state that `GrpcProtobuf` is a
/// non-behavioural label (echoed into violations, not branched on) and
/// that the caller strips the gRPC length prefix — propagating the
/// already-honest enum doc (`framing.rs:14-18`) up to the loud surfaces.
#[test]
#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]
fn us06_harness_docs_flag_grpc_framing_as_a_non_behavioural_label() {
    // lib.rs propagates the "caller strips the prefix; framing is inert"
    // note up from framing.rs.
    assert_present(
        "crates/otlp-conformance-harness/src/lib.rs",
        "length prefix",
    );
    assert_present("crates/otlp-conformance-harness/README.md", "length prefix");
}
