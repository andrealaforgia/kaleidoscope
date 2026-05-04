# ADR-0004 — Conformance test-vector corpus layout

- **Status**: Accepted
- **Date**: 2026-05-03
- **Author**: `nw-solution-architect` (Morgan)
- **Supersedes**: none
- **Superseded by**: none

## Context

US-07 (slice 07) introduces the reference test-vector corpus and the corpus runner. The user story constrains:

- The directory root: `crates/otlp-conformance-harness/tests/vectors/`.
- One sibling `.expected.json` per `.bin` file, declaring the expected verdict.
- A SHA-256 content hash of the `.bin` file embedded in the descriptor under `content_hash` (hex-encoded).
- Descriptor fields: `asserted_signal`, `asserted_framing`, `expected_verdict`, `rule` (for reject vectors), `content_hash`, `source`.

US-07 also requires the corpus runner to enumerate `Rule` variants and fail if any variant has zero defending reject vectors (US-07 AC 4).

DISCUSS leaves three things open:

1. **Directory hierarchy under `tests/vectors/`**: flat? `accept/` and `reject/` subtrees with signal mixed in? per-signal subtrees with `accept/`/`reject/` underneath? per-signal subtrees with `accept/`/`reject/` and rule-specific deeper paths?
2. **The corpus runner's discovery rule**: shape-based on path? declared in a manifest file? a hybrid?
3. **How new vectors are added**: hand-checkin only? a generation script for accept vectors (using the OpenTelemetry SDK)? both?

## Decision

### 1. Directory hierarchy — `{signal}/{verdict}/{vector}.{bin,expected.json}`

```
tests/vectors/
├── logs/
│   ├── accept/
│   │   ├── minimal.bin
│   │   └── minimal.expected.json
│   └── reject/
│       ├── empty.bin
│       ├── empty.expected.json
│       ├── truncated.bin
│       ├── truncated.expected.json
│       ├── bad_varint.bin
│       ├── bad_varint.expected.json
│       ├── bad_tag.bin
│       ├── bad_tag.expected.json
│       ├── traces_misrouted.bin
│       ├── traces_misrouted.expected.json
│       ├── metrics_misrouted.bin
│       └── metrics_misrouted.expected.json
├── traces/
│   ├── accept/
│   │   ├── minimal.bin
│   │   └── minimal.expected.json
│   └── reject/
│       ├── empty.bin
│       ├── empty.expected.json
│       ├── logs_misrouted.bin
│       ├── logs_misrouted.expected.json
│       ├── metrics_misrouted.bin
│       └── metrics_misrouted.expected.json
└── metrics/
    ├── accept/
    │   ├── minimal.bin
    │   └── minimal.expected.json
    └── reject/
        ├── empty.bin
        ├── empty.expected.json
        ├── logs_misrouted.bin
        ├── logs_misrouted.expected.json
        ├── traces_misrouted.bin
        └── traces_misrouted.expected.json
```

The hierarchy is **two levels deep**: signal at level 1 (`logs`/`traces`/`metrics`), verdict at level 2 (`accept`/`reject`). Vector files are leaves. This matches exactly what every `tests/vectors/...` path in the user stories already names (`tests/vectors/logs/reject/empty.bin`, `tests/vectors/logs/accept/minimal.bin`, etc.).

No deeper hierarchy by rule type. A vector's rule lives in its `.expected.json`, not its path. Reasons:

- Some rules (`SignalMismatch`) are inherently inter-signal; a misrouted-traces-into-logs vector lives most naturally under `logs/reject/` (where the caller invoked `validate_logs`), not under a hypothetical `signal_mismatch/` directory.
- Path-based rule enumeration would couple the runner's directory walk to the rule names, requiring path renames every time `Rule` evolves. The descriptor-driven rule enumeration decouples this.

### 2. Descriptor schema (`.expected.json`)

```json
{
  "schema_version": 1,
  "asserted_signal": "logs",
  "asserted_framing": "HttpProtobuf",
  "expected_verdict": {
    "accept": {
      "type_path": "opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest"
    }
  },
  "content_hash": "sha256:<64-hex-chars>",
  "spec_version": "1.5.0",
  "source": "OpenTelemetry Rust SDK 0.27, captured 2026-05-03 by tools/capture_minimal_logs.rs"
}
```

For reject vectors:

```json
{
  "schema_version": 1,
  "asserted_signal": "logs",
  "asserted_framing": "HttpProtobuf",
  "expected_verdict": {
    "reject": {
      "rule": "EmptyInput"
    }
  },
  "content_hash": "sha256:<64-hex-chars>",
  "spec_version": "1.5.0",
  "source": "Hand-crafted: 0-byte file representing the simplest empty-body case"
}
```

For `WireType::SignalMismatch` vectors, the `rule` field carries the structured form:

```json
{
  "expected_verdict": {
    "reject": {
      "rule": {
        "WireType": {
          "SignalMismatch": {
            "observed": "traces",
            "asserted": "logs"
          }
        }
      }
    }
  }
}
```

This matches the on-the-wire pattern of `Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted })`.

The schema is versioned (`schema_version: 1`); incompatible descriptor changes bump this number, the corpus runner refuses unknown versions, and existing descriptors are migrated.

### 3. Corpus runner discovery rule — recursive walk, descriptor-validated

`tests/corpus.rs`:

1. Recursively walks `tests/vectors/` for any file matching `*.bin`.
2. For each `foo.bin`, reads `foo.expected.json` from the same directory.
3. Re-computes SHA-256 of `foo.bin` and asserts it equals the descriptor's `content_hash`. Mismatch is a hard failure (corpus integrity error per US-07 scenario 3).
4. Asserts the path's `{signal}/{verdict}/` matches the descriptor's `asserted_signal` and the descriptor's `expected_verdict` discriminator (a misfile is a hard failure).
5. Invokes the appropriate `validate_*` function with the bytes and the descriptor's `asserted_framing`.
6. Asserts the actual verdict matches the descriptor's `expected_verdict`.
7. After the walk, enumerates the `Rule` variants (statically) and asserts each variant has at least one defending reject vector somewhere in the corpus (US-07 AC 4).

The walk is purely on the filesystem — there is no separate manifest file. The filesystem **is** the manifest. This avoids the manifest-file maintenance burden and means adding a vector is one PR (drop in `foo.bin` + `foo.expected.json`), not two.

### 4. How new vectors are added

Two paths, each appropriate to its case:

- **Accept vectors**: generated by a small Rust capture program at `crates/otlp-conformance-harness/examples/capture_corpus_vectors.rs`. The capture program lives as a Cargo `example` (not a `bin`) so its dependency on the OpenTelemetry SDK does not pollute the harness's runtime dependency tree. The program emits real OTLP payloads with the SDK and writes the `.bin` plus `.expected.json` siblings. **Reproducibility**: the program is deterministic — same SDK version, same input → same bytes. The descriptor's `source` field records the SDK version and capture date.

- **Reject vectors**: hand-crafted. For `EmptyInput`, the `.bin` is zero bytes (trivially hand-crafted). For `ProtobufDecode`, the `.bin` is either a hand-modified valid byte sequence (truncated, or with a flipped tag byte) or a hand-written byte sequence with a known-invalid varint. For `SignalMismatch`, the `.bin` is the *accept* vector for one signal placed in the `reject/` directory of another signal (e.g. `logs/accept/minimal.bin`'s bytes appear as `traces/reject/logs_misrouted.bin`).

The capture program is run on demand (e.g. when bumping the OTLP spec version) and its output is committed to the repository. The corpus is **content-addressed and versioned in git** — there is no auto-regeneration on `cargo test` runs. This means the corpus is auditable: a maintainer reviewing a PR sees the byte-for-byte changes to the corpus, which is the entire point of the regression discipline.

## Alternatives Considered

### Option A — Per-signal then per-verdict hierarchy with descriptor-validated walk (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Matches the path patterns already named in the user stories byte-for-byte.
- Walk is purely filesystem-driven; no manifest to maintain.
- Adding a vector is one PR, two files.
- The `{signal}/{verdict}/` path encodes the gross categorisation; `.expected.json` carries the per-vector specifics.
- Path-based misfile detection: a vector under `logs/accept/` whose descriptor declares `asserted_signal: "traces"` is caught immediately.

**Cons**:
- Two siblings per vector means twice the file count. Acceptable: corpora at this scale (≤30 vectors per signal in the foreseeable future) are easily browsable.

### Option B — Flat hierarchy under `tests/vectors/` with descriptor-driven everything

```
tests/vectors/
├── logs_accept_minimal.bin
├── logs_accept_minimal.expected.json
├── logs_reject_empty.bin
├── logs_reject_empty.expected.json
└── ...
```

**Pros**:
- One directory, easy to glob.

**Cons**:
- Encodes signal and verdict in the *filename* rather than the path, which is brittle (filename parsing in the runner) and ugly.
- Conflicts with every `tests/vectors/{signal}/{verdict}/...` path the user stories already use.
- The corpus quickly gets hard to browse as it grows.

**Rejected** for the user-story conflict.

### Option C — Per-rule subdirectories under each signal/verdict

```
tests/vectors/logs/reject/
├── empty_input/
│   └── empty.bin + empty.expected.json
├── protobuf_decode/
│   ├── truncated.bin + truncated.expected.json
│   ├── bad_varint.bin + bad_varint.expected.json
│   └── bad_tag.bin + bad_tag.expected.json
└── signal_mismatch/
    ├── traces_misrouted.bin + traces_misrouted.expected.json
    └── metrics_misrouted.bin + metrics_misrouted.expected.json
```

**Pros**:
- Very explicit categorisation by rule.
- A maintainer can `ls` the directory and see at a glance how many vectors defend each rule.

**Cons**:
- Path encodes rule name; rule renames require directory renames in lockstep.
- `Rule::WireType(WireTypeRule::SignalMismatch)` becomes a question of whether the path is `signal_mismatch/` or `wire_type/signal_mismatch/`, mirroring the enum nesting in the filesystem. This is exactly the kind of structural duplication the descriptor-driven approach avoids.
- Rule-coverage check (US-07 AC 4) becomes a path-based check rather than a content-based check — slightly less robust.

**Rejected** for the path-coupling cost. Recommended as a future evolution if the corpus grows beyond ~50 vectors per signal/verdict pair, at which point the deeper hierarchy aids browsing more than it costs in path coupling.

### Option D — Single `manifest.toml` declaring every vector and its expected verdict

```toml
# tests/vectors/manifest.toml
[[vector]]
path = "logs/accept/minimal.bin"
asserted_signal = "logs"
asserted_framing = "HttpProtobuf"
expected_verdict = { accept = { type_path = "..." } }
content_hash = "sha256:..."
```

**Pros**:
- Single source of truth; runner reads one file.
- Easier to grep for "every vector at once".

**Cons**:
- Two files to update per vector add: `manifest.toml` *and* the `.bin`. Double the chance of a misalignment.
- The `.expected.json` siblings are already a near-manifest. Adding a parallel manifest is duplication.
- The user stories use the sibling-`.expected.json` pattern explicitly (US-07 AC 2, `shared-artifacts-registry.md > test_vector_corpus`).

**Rejected** for the duplicate-source-of-truth cost and the user-story conflict.

### Option E — Auto-generation of accept vectors on every test run

The capture program runs as part of `cargo test`, regenerating accept vectors from the OpenTelemetry SDK each time.

**Pros**:
- The corpus tracks the upstream SDK automatically.

**Cons**:
- Defeats the entire point of a content-addressed corpus: vectors *must* be stable byte sequences committed to the repository. Regeneration means the vectors are not regression checks but rather generators of expected outputs from the current SDK.
- Forces the OpenTelemetry SDK into the harness's `[dev-dependencies]` (or worse, `[dependencies]`), bloating the build.
- Test runs become non-reproducible across SDK versions.

**Rejected** outright. The corpus is the contract; regenerating it on every run is no contract at all.

## Consequences

### Positive

- The directory layout is exactly what every user story has been writing path strings against.
- The corpus runner is purely filesystem-driven — no manifest-versus-actual drift.
- The SHA-256 hash check catches any vector mutation before the harness is even invoked (US-07 scenario 3).
- The static `Rule` enumeration check (US-07 AC 4) means new rules cannot ship without defending vectors.
- Adding a new vector is one PR with two files, easy to review.
- The capture program lives in `examples/`, isolating the OpenTelemetry SDK dependency from the runtime crate.

### Negative

- The descriptor schema must evolve in lockstep with the violation type. `schema_version` plus a refusal-to-run-on-unknown-versions guard absorbs this risk.
- Reject-vector hand-crafting requires the maintainer to understand the `Rule` enum's representation. Documented in the crate's README (US-07 AC 6).

### Trade-off ATAM

This is a sensitivity point for **Functional Suitability — Correctness** (the corpus is the regression net for KPI 1 zero-false-positive). It is a trade-off point for **Maintainability — Modifiability** (positive: descriptor-driven means rule renames are one-spot edits) versus **Usability — Operability for Maintainers** (slightly negative: descriptor schema must evolve carefully). The trade-off is biased correctly because correctness dominates ergonomics for this artefact.
