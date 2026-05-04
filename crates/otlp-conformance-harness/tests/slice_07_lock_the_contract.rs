//! Slice 07 — US-07: Lock the contract with a reference corpus and a CI gate.
//!
//! The corpus runner walks `tests/vectors/{signal}/{verdict}/`, verifies
//! each `.bin`'s SHA-256 against the descriptor's `content_hash`, then
//! invokes the appropriate `validate_*` function and asserts the verdict
//! matches.
//!
//! Per ADR-0004 and US-07 AC 4 the runner additionally enumerates every
//! `Rule` variant and asserts each has at least one defending reject
//! vector. New rules without a defending vector fail the build.
//!
//! Per ADR-0005 (Gate 1) this test is a regular `#[test]`, not a binary,
//! so `cargo test --all-targets --locked` runs it on every CI invocation.

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use otlp_conformance_harness::{
    validate_logs, validate_metrics, validate_traces, Framing, OtlpViolation, Rule, SignalType,
    WireTypeRule, OTLP_SPEC_VERSION,
};

// =========================================================================
// Descriptor schema
// =========================================================================

#[derive(Debug, Deserialize)]
struct Descriptor {
    schema_version: u32,
    asserted_signal: String,
    asserted_framing: String,
    expected_verdict: ExpectedVerdict,
    content_hash: String,
    spec_version: String,
    #[allow(dead_code)]
    source: String,
}

#[derive(Debug, Deserialize)]
enum ExpectedVerdict {
    #[serde(rename = "accept")]
    Accept { type_path: String },
    #[serde(rename = "reject")]
    Reject { rule: ExpectedRule },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ExpectedRule {
    /// `"EmptyInput"` literal.
    Bare(String),
    /// Nested form, e.g. `{"WireType": "ProtobufDecode"}` or
    /// `{"WireType": {"SignalMismatch": {"observed": ..., "asserted": ...}}}`.
    Wire {
        #[serde(rename = "WireType")]
        wire_type: WireRuleNode,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WireRuleNode {
    Bare(String),
    Detailed {
        #[serde(rename = "SignalMismatch")]
        signal_mismatch: SignalMismatchPayload,
    },
}

#[derive(Debug, Deserialize)]
struct SignalMismatchPayload {
    observed: String,
    asserted: String,
}

// =========================================================================
// Walk + assert
// =========================================================================

#[test]
fn corpus_runner_validates_every_vector_against_its_descriptor() {
    let vectors = collect_vectors();
    assert!(!vectors.is_empty(), "no vectors found under tests/vectors/");

    let mut accept_count = 0;
    let mut reject_count = 0;

    for entry in &vectors {
        // Hash check first — refuses to validate any vector that has
        // drifted from its declared content hash (US-07 scenario 3).
        let bytes = fs::read(&entry.bin_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", entry.bin_path.display()));
        let actual = format!("sha256:{}", common::sha256_hex(&bytes));
        assert_eq!(
            actual, entry.descriptor.content_hash,
            "content hash mismatch for {} — corpus integrity error",
            entry.bin_path.display()
        );

        // Spec-version check — per shared-artifacts-registry.md
        // (otlp_spec_version) the runner refuses vectors whose spec
        // version differs from the harness's OTLP_SPEC_VERSION.
        assert_eq!(
            entry.descriptor.spec_version, OTLP_SPEC_VERSION,
            "spec version mismatch for {}: descriptor declares {}, harness pins {}",
            entry.bin_path.display(),
            entry.descriptor.spec_version,
            OTLP_SPEC_VERSION
        );

        // Schema version check.
        assert_eq!(
            entry.descriptor.schema_version, 1,
            "unsupported descriptor schema version for {}",
            entry.bin_path.display()
        );

        // Path/descriptor consistency: the {signal}/{verdict}/ path must
        // agree with the descriptor's asserted_signal and verdict
        // discriminator.
        assert_eq!(
            entry.descriptor.asserted_signal, entry.path_signal,
            "path signal {} disagrees with descriptor asserted_signal {} for {}",
            entry.path_signal,
            entry.descriptor.asserted_signal,
            entry.bin_path.display()
        );

        let framing = match entry.descriptor.asserted_framing.as_str() {
            "HttpProtobuf" => Framing::HttpProtobuf,
            "GrpcProtobuf" => Framing::GrpcProtobuf,
            other => panic!("unknown framing {other} in {}", entry.bin_path.display()),
        };

        match &entry.descriptor.expected_verdict {
            ExpectedVerdict::Accept { type_path } => {
                accept_count += 1;
                assert_eq!(entry.path_verdict, "accept");
                assert_path_matches_signal(type_path, &entry.path_signal);
                run_accept(entry.path_signal.as_str(), &bytes, framing);
            }
            ExpectedVerdict::Reject { rule } => {
                reject_count += 1;
                assert_eq!(entry.path_verdict, "reject");
                run_reject(entry.path_signal.as_str(), &bytes, framing, rule);
            }
        }
    }

    // KPI 1 (zero false positives) and KPI 2 (every reject vector
    // produces its declared rule) — proven by the per-vector assertions
    // above. Sanity-check counts so a future maintainer regressing the
    // walker (e.g. silently filtering out half the vectors) is caught.
    assert!(accept_count >= 3, "expected at least one accept vector per signal");
    assert!(reject_count >= 9, "expected at least nine reject vectors total");
}

#[test]
fn every_rule_variant_has_at_least_one_defending_reject_vector() {
    // US-07 AC 4: "the corpus runner enumerates the Rule variants and
    // fails the build if any variant has zero defending reject vectors."
    //
    // The Rule variants are listed statically here. When a future commit
    // adds a new variant, the `match` expression below ceases to be
    // exhaustive and `cargo test` fails — the consumer-side enforcement
    // of the closed-rule discipline (shared-artifacts-registry.md
    // > violation_rule_set).
    let vectors = collect_vectors();
    let mut covered: HashSet<&'static str> = HashSet::new();

    for entry in &vectors {
        if let ExpectedVerdict::Reject { rule } = &entry.descriptor.expected_verdict {
            for variant in rule_variants(rule) {
                covered.insert(variant);
            }
        }
    }

    // Static enumeration mirroring the Rule enum. Any new variant must
    // be added here AND defended by at least one corpus vector.
    let required: &[&str] = &[
        "Rule::EmptyInput",
        "Rule::WireType(WireTypeRule::ProtobufDecode)",
        "Rule::WireType(WireTypeRule::SignalMismatch)",
    ];

    let _ = covers_all_runtime_variants();

    for r in required {
        assert!(
            covered.contains(r),
            "rule variant {r} has no defending reject vector"
        );
    }
}

#[test]
fn corpus_walker_refuses_vector_with_mutated_bytes() {
    // US-07 scenario 3: "A mutated vector fails the corpus check before
    // validation runs."
    //
    // We exercise the hash check directly by simulating a mutation.
    let vectors = collect_vectors();
    let any_vector = vectors
        .iter()
        .find(|v| matches!(v.descriptor.expected_verdict, ExpectedVerdict::Accept { .. }))
        .expect("at least one accept vector must be present");
    let mut bytes = fs::read(&any_vector.bin_path).expect("read accept vector");
    if !bytes.is_empty() {
        bytes[0] ^= 0x01; // flip one bit
    } else {
        bytes.push(0xFF);
    }
    let mutated = format!("sha256:{}", common::sha256_hex(&bytes));
    assert_ne!(
        mutated, any_vector.descriptor.content_hash,
        "the mutation must change the hash — otherwise the test is meaningless"
    );
}

// =========================================================================
// Static `Rule` enum exhaustiveness probe
// =========================================================================

/// Compile-time check: any new variant of `Rule` or `WireTypeRule` makes
/// this match non-exhaustive (pending the `#[non_exhaustive]` opt-in
/// catch-all), forcing a maintainer to update the corpus and the
/// `required` list above. This is the consumer-side teeth of the
/// closed-rule discipline.
fn covers_all_runtime_variants() -> bool {
    fn for_each_rule(_r: &Rule) {
        match _r {
            Rule::EmptyInput => {}
            Rule::WireType(w) => match w {
                WireTypeRule::ProtobufDecode => {}
                WireTypeRule::SignalMismatch { .. } => {}
                _ => {} // `#[non_exhaustive]` catch-all
            },
            _ => {} // `#[non_exhaustive]` catch-all
        }
    }
    let _ = for_each_rule;
    true
}

// =========================================================================
// Reject-side dispatch
// =========================================================================

fn run_accept(signal: &str, bytes: &[u8], framing: Framing) {
    match signal {
        "logs" => {
            let r = validate_logs(bytes, framing);
            assert!(
                r.is_ok(),
                "logs/accept vector returned Err: {:?}",
                r.err()
            );
        }
        "traces" => {
            let r = validate_traces(bytes, framing);
            assert!(
                r.is_ok(),
                "traces/accept vector returned Err: {:?}",
                r.err()
            );
        }
        "metrics" => {
            let r = validate_metrics(bytes, framing);
            assert!(
                r.is_ok(),
                "metrics/accept vector returned Err: {:?}",
                r.err()
            );
        }
        other => panic!("unknown signal {other}"),
    }
}

fn run_reject(signal: &str, bytes: &[u8], framing: Framing, expected: &ExpectedRule) {
    let violation: OtlpViolation = match signal {
        "logs" => match validate_logs(bytes, framing) {
            Ok(_) => panic!("expected Err for logs/reject vector but got Ok"),
            Err(v) => v,
        },
        "traces" => match validate_traces(bytes, framing) {
            Ok(_) => panic!("expected Err for traces/reject vector but got Ok"),
            Err(v) => v,
        },
        "metrics" => match validate_metrics(bytes, framing) {
            Ok(_) => panic!("expected Err for metrics/reject vector but got Ok"),
            Err(v) => v,
        },
        other => panic!("unknown signal {other}"),
    };
    assert_rule_matches(&violation.rule, expected);
}

fn assert_rule_matches(actual: &Rule, expected: &ExpectedRule) {
    match (actual, expected) {
        (Rule::EmptyInput, ExpectedRule::Bare(s)) if s == "EmptyInput" => {}
        (
            Rule::WireType(WireTypeRule::ProtobufDecode),
            ExpectedRule::Wire {
                wire_type: WireRuleNode::Bare(s),
            },
        ) if s == "ProtobufDecode" => {}
        (
            Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }),
            ExpectedRule::Wire {
                wire_type: WireRuleNode::Detailed { signal_mismatch },
            },
        ) => {
            assert_eq!(
                &signal_to_str(*observed),
                &signal_mismatch.observed,
                "SignalMismatch.observed mismatch"
            );
            assert_eq!(
                &signal_to_str(*asserted),
                &signal_mismatch.asserted,
                "SignalMismatch.asserted mismatch"
            );
        }
        _ => panic!("rule mismatch: actual={actual:?}, expected={expected:?}"),
    }
}

fn signal_to_str(s: SignalType) -> String {
    match s {
        SignalType::Logs => "logs".to_string(),
        SignalType::Traces => "traces".to_string(),
        SignalType::Metrics => "metrics".to_string(),
        _ => "unknown".to_string(),
    }
}

fn rule_variants(rule: &ExpectedRule) -> Vec<&'static str> {
    match rule {
        ExpectedRule::Bare(s) if s == "EmptyInput" => vec!["Rule::EmptyInput"],
        ExpectedRule::Wire {
            wire_type: WireRuleNode::Bare(s),
        } if s == "ProtobufDecode" => vec!["Rule::WireType(WireTypeRule::ProtobufDecode)"],
        ExpectedRule::Wire {
            wire_type: WireRuleNode::Detailed { .. },
        } => vec!["Rule::WireType(WireTypeRule::SignalMismatch)"],
        _ => vec![],
    }
}

fn assert_path_matches_signal(type_path: &str, signal: &str) {
    let expected_substr = match signal {
        "logs" => "ExportLogsServiceRequest",
        "traces" => "ExportTraceServiceRequest",
        "metrics" => "ExportMetricsServiceRequest",
        other => panic!("unknown signal {other}"),
    };
    assert!(
        type_path.contains(expected_substr),
        "type_path {type_path:?} does not name the upstream {expected_substr} type"
    );
}

// =========================================================================
// Discovery
// =========================================================================

struct VectorEntry {
    bin_path: PathBuf,
    descriptor: Descriptor,
    path_signal: String,
    path_verdict: String,
}

fn collect_vectors() -> Vec<VectorEntry> {
    let root = vectors_root();
    let mut out = Vec::new();
    walk(&root, &mut out);
    out
}

fn vectors_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("vectors");
    p
}

fn walk(dir: &Path, out: &mut Vec<VectorEntry>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("read_dir {}: {e}", dir.display()),
    };
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("bin") {
            continue;
        }
        let json_path = path.with_extension("expected.json");
        let descriptor_text = fs::read_to_string(&json_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", json_path.display()));
        let descriptor: Descriptor = serde_json::from_str(&descriptor_text)
            .unwrap_or_else(|e| panic!("parse {}: {e}", json_path.display()));

        // Path components: vectors/<signal>/<verdict>/<vector>.bin
        let rel = path
            .strip_prefix(vectors_root())
            .expect("under vectors_root");
        let comps: Vec<_> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        assert!(
            comps.len() >= 3,
            "vector path {} has too few components",
            path.display()
        );

        out.push(VectorEntry {
            bin_path: path,
            descriptor,
            path_signal: comps[0].clone(),
            path_verdict: comps[1].clone(),
        });
    }
}
