# ADR-0005 — CI contract for `otlp-conformance-harness`

- **Status**: Accepted
- **Date**: 2026-05-03
- **Author**: `nw-solution-architect` (Morgan)
- **Supersedes**: none
- **Superseded by**: none

## Context

US-07 names the corpus runner and the rule-coverage enumeration but explicitly defers the **CI workflow runner choice** (GitHub Actions, Gitea Actions, Forgejo Actions, Drone, Buildbot, etc.) to the DEVOPS wave (`platform-architect`). DESIGN's job is to specify the **contract** the CI must honour — *what gates must pass, what their inputs and outputs are, and what their failure modes are* — without prescribing the workflow runner.

The shared-artefacts registry's CI invariants enumerate four cross-cutting properties:

1. Every accept-path vector must round-trip without violation.
2. Every reject-path vector must produce its declared rule.
3. The false-positive rate is zero.
4. The harness emits no telemetry of its own.

US-07's technical notes point at `cargo test --all-targets`. The shared-artefacts registry (`crate_public_surface`) flags `cargo public-api` or `cargo-semver-checks` as candidates for public-API enforcement; US-04 AC 2 explicitly defers the choice to DESIGN.

This ADR specifies the five gates the CI must run on every commit affecting `crates/otlp-conformance-harness/**`, names their tools (open-source, well-maintained), and documents what each gate's exit code means.

## Decision

### Required CI gates (five, all blocking)

#### Gate 1 — Test suite

```sh
cargo test -p otlp-conformance-harness --all-targets --locked
```

**Tool**: Rust standard `cargo test`.
**Inputs**: the crate's source plus `tests/` directory.
**Outputs**: pass/fail.
**Failure mode**: any test (unit, integration, doc, the corpus runner) fails.
**Owns**: KPI 1, KPI 2, KPI 3, KPI 6, the four CI invariants from `shared-artifacts-registry.md`.
**`--locked`**: the build refuses to proceed if `Cargo.lock` would be modified, enforcing the exact-version pinning of ADR-0003 even at CI level.

#### Gate 2 — Public API surface lock

```sh
cargo public-api --diff-git-checkouts main HEAD -p otlp-conformance-harness
```

**Tool**: [`cargo-public-api`](https://github.com/cargo-public-api/cargo-public-api) (MIT/Apache-2.0).
**Inputs**: the crate's public surface as of `HEAD` versus `main`.
**Outputs**: a diff of public items.
**Failure mode**: any change to the public surface that is not accompanied by an explicit version-bump commit.
**Owns**: `shared-artifacts-registry.md > crate_public_surface`, US-04 AC 2 (the type-path identity contract — the diff includes function return types, so a path change from `opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest` to anything else is caught).

The ADR-0001 "free `pub fn` plus a small set of types" surface is small enough that the diff should be empty for non-version-bump commits.

#### Gate 3 — SemVer compliance

```sh
cargo semver-checks check-release -p otlp-conformance-harness --baseline-rev main
```

**Tool**: [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks) (MIT/Apache-2.0).
**Inputs**: the crate's public API (deeper than `cargo public-api`'s textual diff — `cargo-semver-checks` understands SemVer rules).
**Outputs**: pass/fail with structured violations if any.
**Failure mode**: a public-API change that should be a major-version bump (e.g. removing a variant, changing a function signature) but is shipped under a minor bump.
**Owns**: SemVer correctness for downstream consumers.

`cargo public-api` and `cargo-semver-checks` are complementary — the former is a textual diff useful for review and ADR-0002's `#[non_exhaustive]` audit; the latter is a SemVer-aware compatibility checker. Both are required.

#### Gate 4 — Dependency policy

```sh
cargo deny check
```

**Tool**: [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) (MIT/Apache-2.0).
**Inputs**: `Cargo.toml`, `Cargo.lock`, the workspace's `deny.toml` configuration.
**Outputs**: pass/fail.
**Failure mode**: any of:
  - A transitive dependency uses a disqualified licence (BSL/SSPL/FSL/RSAL per roadmap section A.1) — `licenses` table.
  - The `opentelemetry-proto` dependency is not pinned via `=` (per ADR-0003) — `bans` or `advisories` table.
  - A known-vulnerable dependency version is in use — `advisories` table (RustSec advisory database).
  - A duplicate dependency exists when not on the allowlist — `bans` table.
**Owns**: `shared-artifacts-registry.md > otlp_wire_format` (pin policy), the licence-cleanliness engineering practice (roadmap A.1), the no-yanked-versions practice.

Recommended `deny.toml` excerpt:

```toml
[licenses]
allow = ["Apache-2.0", "MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC", "MPL-2.0", "CC0-1.0", "Unicode-DFS-2016", "Unicode-3.0"]
confidence-threshold = 0.8

[bans]
multiple-versions = "deny"
deny = [
  { name = "openssl", reason = "use rustls" },
]

[bans.skip]  # only for documented exceptions
# (none for v0)

[advisories]
db-urls = ["https://github.com/rustsec/advisory-db"]
yanked = "deny"
ignore = []  # any ignored advisory must come with a comment justifying it
```

#### Gate 5 — Mutation testing

```sh
cargo mutants --package otlp-conformance-harness --check
```

**Tool**: [`cargo-mutants`](https://github.com/sourcefrog/cargo-mutants) (MIT/Apache-2.0).
**Inputs**: the crate's source plus the test suite.
**Outputs**: pass if every mutation is caught by at least one test; fail if any mutation survives.
**Failure mode**: a code change that should change behaviour (e.g. flipping a `>` to `<`) is not detected by any test.
**Owns**: the test-suite quality guarantee. The Sentinel review at iteration 1 explicitly scrutinised mutation-resistance (e.g. US-02 byte-locus must defeat `ByteOffset(0)` mutations); this gate is what enforces that scrutiny continuously.
**Threshold**: 100% caught for v0. Acceptable because the harness is small (~ a few hundred lines of code at v0). Tightened thresholds with explicit waivers (per-mutation, with rationale) are deferred to v0.x if the absolute count proves too painful in practice.

### Gate execution order

The gates run in **descending speed order** to fail fast:

1. Gate 4 (`cargo deny check`) — milliseconds.
2. Gate 1 (`cargo test --all-targets`) — seconds.
3. Gate 2 (`cargo public-api`) — seconds.
4. Gate 3 (`cargo semver-checks`) — seconds.
5. Gate 5 (`cargo mutants`) — minutes (proportional to test-suite size).

A failed gate aborts subsequent gates: any non-zero exit code from gates 1–4 prevents gate 5's expensive mutation run. The CI runner enforces this naturally with `set -e` or step-by-step conditional dependencies.

### Non-required, recorded-only

KPI 7 (validation latency p99) is **informational only**. A Criterion benchmark (`benches/validate.rs`) ships in slice 07; the CI may run it as a non-blocking step, recording the output in build artefacts. There is no SLA gate in v0 (per outcome-kpis.md).

### What the DEVOPS wave decides

The DEVOPS wave (`platform-architect`) decides:

- **Workflow runner** (GitHub Actions, Gitea Actions, Forgejo Actions, Drone, etc.).
- **Caching strategy** (Cargo registry cache, sccache, etc.).
- **Triggering** (which paths trigger the workflow; merge-queue integration; required-status-check configuration).
- **Artefact storage** (Criterion outputs, mutation-test reports, public-API diffs).
- **Verdict-counts artefact** (per-signal, per-rule counts, written to a build-step artefact for KPI 4 reporting per outcome-kpis.md).

DESIGN does **not** prescribe any of those. The contract above is what the runner must execute; the runner-specific YAML is DEVOPS's deliverable.

## Alternatives Considered

### Option A — Five gates (`test`, `public-api`, `semver-checks`, `deny`, `mutants`) (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Each gate owns a distinct quality concern; together they cover every CI invariant in `shared-artifacts-registry.md`.
- All five tools are open-source, well-maintained, in widespread use (millions of downloads on crates.io between them).
- The gates are runner-agnostic; any CI runner that can run shell commands can execute them.

**Cons**:
- `cargo mutants` is the longest-running gate. For a small crate this is tens of seconds to a couple of minutes; for larger crates it could become painful. Acceptable for v0; revisit if the run-time exceeds five minutes.

### Option B — Three gates (`test`, `deny`, `public-api` only)

**Pros**:
- Faster CI run.

**Cons**:
- Misses SemVer compatibility checking (`cargo-semver-checks`) — a public-API change that breaks SemVer would not be caught until consumers complained.
- Misses test-suite quality (mutation testing) — Sentinel's iteration-1 review explicitly noted that the user-stories' scenarios were tightened for mutation resistance; without `cargo mutants`, that tightening is not enforced over time.

**Rejected** for the regression risk.

### Option C — `cargo test` only

**Pros**:
- Simplest possible CI.

**Cons**:
- Misses every cross-cutting concern: licence policy, public-API stability, SemVer correctness, mutation resistance.
- The CI invariants in `shared-artifacts-registry.md` cannot be enforced by `cargo test` alone.

**Rejected** outright.

### Option D — Add `cargo-audit` as a sixth gate

**Pros**:
- A separate vulnerability check.

**Cons**:
- `cargo deny check` already runs the RustSec advisory database via its `advisories` table — `cargo-audit` is redundant under that configuration.

**Rejected** for redundancy with Gate 4.

### Option E — Add `clippy` as a sixth gate

**Pros**:
- Lint-driven code-quality enforcement.

**Cons**:
- Clippy is a code-style and idiom checker, not a contract gate. Lint violations are useful in PR review but should not block merge for stylistic disagreements that have nothing to do with the harness's contract.
- Style enforcement belongs in pre-commit hooks or local development workflow, not in the public CI contract.

**Rejected** as not contract-relevant. The DEVOPS wave may add `cargo clippy -- -D warnings` as a non-required step at its discretion; the DESIGN wave does not specify it.

## Consequences

### Positive

- The five gates collectively defend every CI invariant in `shared-artifacts-registry.md` plus every relevant outcome KPI (1, 2, 3, 4, 6).
- All five tools are runner-agnostic, MIT/Apache-2.0 licensed, in active maintenance, with widespread industry adoption.
- The DEVOPS wave receives a clean shopping list and chooses the runner without re-litigating the contract.
- The `--locked` flag on `cargo test` ensures the dependency-pinning policy of ADR-0003 cannot be relaxed at CI level.
- Mutation testing makes the test-suite quality observable continuously, not just at peer-review time.

### Negative

- `cargo mutants` adds CI latency. Acceptable for v0; observable, tunable.
- All five tools are independent crates that must be installed in the CI image. This is a one-time setup cost that the DEVOPS wave amortises across the workflow definition.

### Risk register

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| `cargo mutants` runtime exceeds tolerable CI budget as the corpus grows | Medium | Low | If exceeded, scope the mutation run to the source modules (skip `tests/`); if still too slow, bound to the changed files via `cargo-mutants --in-diff main`. |
| `cargo public-api` produces noisy diffs against unstable nightly Rust | Low | Low | Pin to the stable Rust toolchain in CI (per `rust-toolchain.toml`); `cargo public-api` works on stable. |
| `cargo deny`'s licence table requires per-dependency exceptions for transitive Apache-2.0 with a strange SPDX expression | Low | Low | The configuration's `confidence-threshold = 0.8` accommodates SPDX heuristics; explicit per-package allowlists handled case-by-case. |
| `cargo semver-checks` produces false positives on `#[non_exhaustive]` evolution | Low | Medium | The tool understands `#[non_exhaustive]`; documented behaviour. If a false positive emerges, the gate is bypassed for that PR with an explicit comment-justified `--baseline-rev` override. |

### Trade-off ATAM

This decision is a sensitivity point for **Maintainability — Analyzability** (every gate produces structured output the maintainer can reason about) and for **Reliability — Maturity** (the gates collectively defend the harness's contract over its lifetime).

It is a trade-off point against **Performance Efficiency — CI Time** (slightly negative: five gates take longer than one). The trade-off is correctly biased because the harness is a leaf dependency for the rest of Kaleidoscope; CI minutes saved here are minutes lost everywhere downstream when an unenforced contract drifts.
