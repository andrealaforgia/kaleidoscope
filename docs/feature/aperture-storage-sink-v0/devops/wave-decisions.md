# aperture-storage-sink-v0 - DEVOPS wave decisions

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Wave**: DEVOPS
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default; pure
  trunk-based, no required-status-checks, admins not enforced, per
  memory `project_kaleidoscope_pure_trunk_based`)
- **Predecessor handoff**: `design/wave-decisions.md` DEVOPS handoff
  annotation; `design/application-architecture.md`; ADR-0041;
  `discuss/outcome-kpis.md` (KPI-1..KPI-5)
- **Direct precedent**: `docs/feature/strata-v1/devops/` and the
  pulse-v1 / ray-v1 waves before it - each added a single new
  `gate-5-mutants-<crate>` job, byte-mirrored from
  `gate-5-mutants-self-observe`. This wave repeats that move for the
  net-new `aperture-storage-sink` crate. Gateway-binary operational
  shape follows `docs/feature/aperture/devops/`.

## Posture

`aperture-storage-sink-v0` inherits the five-gate workspace CI contract
from ADR-0005. Gates 1-4 carry forward; Gate 5 gains ONE new job for
the net-new crate. Two new crates arrive in DELIVER (neither exists
today): a library crate `aperture-storage-sink` carrying the OTLP-proto
to pillar-type translation, and a host composition binary
`kaleidoscope-gateway` (a `[[bin]]` target inside that crate) that wires
the sink into aperture via `spawn`. This is therefore inheritance for
Gates 1-4 plus exactly one new Gate 5 job.

## A1 - NEW `gate-5-mutants-aperture-storage-sink` job (the one real change)

**Verdict: ADD a new per-package Gate 5 job,
`gate-5-mutants-aperture-storage-sink`. The `kaleidoscope-gateway`
binary needs NO separate Gate 5 job (reasoning below).**

Grep verified, 2026-05-21:
`grep -c "gate-5-mutants-aperture-storage-sink" .github/workflows/ci.yml`
returns 0, and `grep -c "gate-5-mutants-kaleidoscope-gateway"
.github/workflows/ci.yml` returns 0. Neither crate is mutation-gated
because neither exists yet; Crafty creates them in DELIVER.

### Why the sink crate gets a gate

`aperture-storage-sink` carries the feature's net-new mutable logic:
severity-number mapping, span-kind mapping, span-status mapping,
byte-array trace/span id length-checked decoding, the shared attribute
fold, the `AnyValue`-to-`String` fold, the metric-data oneof selection
(gauge / sum vs skip), the `as_double` / `as_int`-to-`f64` value
mapping (DD11), tenant resolution (`tenant.id` -> `default_tenant` ->
refuse, DD3), the atomic translation-refusal invariant (DD7), and the
skip-not-refuse policy for unsupported metric point types (DD8,
ADR-0041 Decision 3). This is exactly the kind of dense branching logic
mutation testing exists to police, and DD7/DD8 are correctness
invariants a surviving mutant would silently break. With zero mutation
coverage this would be the largest mutation-coverage gap in the
workspace. The per-feature MT strategy in `CLAUDE.md` (100% kill rate,
scoped to modified files, ADR-0005 Gate 5) cannot be honoured without a
job to run it.

### Why the gateway binary gets NO separate gate

`kaleidoscope-gateway` is composition / wiring, not logic. Its body is:
open the three `FileBacked*Store`s under `pillar_root`, build the
`StorageSink`, run the startup probe, call
`aperture::spawn(config_with_stub_kind, Arc::new(sink))`, then run the
SIGTERM / drain loop that `aperture::run` already owns. Every testable
behaviour lives elsewhere: the translation and refusal logic is in the
sink crate (gated by A1); the probe contract is the sink's `Probe` impl
plus the behavioural gold-test against a read-only `pillar_root` (DD5,
ADR-0041 Earned-Trust); the spawn/drain seam is aperture's, already
mutation-gated by `gate-5-mutants-aperture`. A mutant in pure wiring
either fails to compile or is caught by the integration test that boots
the gateway; there is no branching translation logic in the binary for
a mutation to corrupt undetectably. Adding a gate for it would be a
gate over code that has nothing for mutation testing to find. I read
the binary's designed responsibilities in `application-architecture.md`
section 10 and `wave-decisions.md` DD2/DD4/DD9 and found no logic
warranting its own gate. **Decision: no `gate-5-mutants-kaleidoscope-gateway`.**
If DELIVER lands real branching logic in the binary (e.g. non-trivial
config parsing or a custom drain policy distinct from aperture's), that
is the signal to revisit; record it as a post-merge correction.

### Why mirror self-observe specifically

`gate-5-mutants-self-observe` (`ci.yml:862-947`) is the canonical
per-package Gate 5 template and the byte source the pulse, ray, and
strata jobs were all mirrored from. It encodes the current-best
baseline cascade (`origin/main -> HEAD~1 -> full`), the empty-diff
short-circuit, the precompiled-binary install, and 30-day artefact
retention. Mirroring it inherits the latest conventions with zero
drift and stays byte-identical to the strata job added this same day.

### The six substitutions (and ONLY these six)

The new job is `gate-5-mutants-self-observe` copied verbatim with
exactly six string substitutions. Everything else - `runs-on`,
`needs: [gate-2-public-api, gate-3-semver]`, `timeout-minutes: 30`, the
checkout/toolchain/cache/install step shapes, the baseline cascade,
`--in-diff`, `--no-shuffle --jobs 2`, the 30-day retention - is
byte-for-byte identical.

| # | Field | self-observe value | aperture-storage-sink value |
|---|-------|--------------------|-----------------------------|
| 1 | job key | `gate-5-mutants-self-observe` | `gate-5-mutants-aperture-storage-sink` |
| 2 | step `name` | `Gate 5 — cargo mutants (self-observe)` | `Gate 5 — cargo mutants (aperture-storage-sink)` |
| 3 | `--in-diff` path filter | `crates/self-observe/**` | `crates/aperture-storage-sink/**` |
| 4 | `--package` arg | `--package self-observe` | `--package aperture-storage-sink` |
| 5 | cache key suffix | `...-cargo-mutants-self-observe-...` | `...-cargo-mutants-aperture-storage-sink-...` |
| 6 | artefact name | `mutants-out-self-observe` | `mutants-out-aperture-storage-sink` |

The cache-step display name and the cache `restore-keys` prefix follow
substitution 5 mechanically. The diff-echo log strings and the step
comment naming the crate follow substitutions 3/4 mechanically. These
are cosmetic consequences of the six, not additional changes. The
`--in-diff` baseline cascade is preserved verbatim. The full
byte-for-byte YAML snippet is in `ci-cd-pipeline.md` for Crafty to
copy-paste.

### Landing discipline

This DEVOPS wave does **NOT** edit `ci.yml`. `@nw-software-crafter`
(Crafty) lands the new job atomic with the crate's source in the
DELIVER commit, so the job and the code it gates arrive together and
the first CI run on the implementation commit exercises the new gate.
Insert it adjacent to the other Gate 5 jobs (e.g. after
`gate-5-mutants-strata`, before `gate-5-mutants-beacon`).

## A2 - Gate 1 auto-discovers the new crate's tests

Gate 1 (`cargo test --workspace --all-targets --locked`) carries
forward UNCHANGED. The DELIVER commit adds the new crate as a workspace
member and adds the per-slice acceptance `[[test]]` blocks to
`crates/aperture-storage-sink/Cargo.toml` (one per signal slice:
US-01 logs, US-02 traces, US-03 metrics, per DD10). `--workspace
--all-targets` discovers these automatically; the workflow invocation
needs no edit. The round-trip integration tests written by
`@nw-acceptance-designer` in DISTILL (export -> restart -> query, assert
field equality) run under Gate 1 and ARE the measurement of KPI-1/2/3,
KPI-4, and KPI-5 - see `kpi-instrumentation.md`. The behavioural probe
gold-test (read-only `pillar_root` -> `health.startup.refused`, DD5)
also runs under Gate 1.

## A3 - Dependencies: zero new external crates in the workspace graph

The two new crates depend on existing in-tree workspace crates:
`aperture-storage-sink` -> `aperture` (port only), `lumen`, `ray`,
`pulse`, `aegis`; `kaleidoscope-gateway` ([[bin]] in the same crate)
-> the same set plus aperture's `spawn` seam. None of those is external.

For external crates, I verified the workspace manifest and aperture's
manifest on 2026-05-21:

- **`opentelemetry-proto = "=0.27.0"`** is already a
  `[workspace.dependencies]` entry (`Cargo.toml:52`), pulled into the
  lockfile by the harness and by aperture. The sink reads OTLP proto
  types (`ExportLogsServiceRequest`, `ExportTraceServiceRequest`,
  `ExportMetricsServiceRequest`, `KeyValue`, `AnyValue`, etc.) from it.
  It adds the crate to its OWN `[dependencies]` via
  `opentelemetry-proto = { workspace = true, ... }`, but that is an
  already-resolved crate - zero new crate in the graph.
- **`prost`** is likewise a `[workspace.dependencies]` entry
  (`Cargo.toml:53`) and present via aperture; the sink inherits it
  transitively through `opentelemetry-proto`'s `gen-tonic-messages`.
- **`tokio` (1.40)** and **`tonic` (0.12)** are already in aperture's
  `[dependencies]` (`crates/aperture/Cargo.toml:44-45`). The
  `kaleidoscope-gateway` binary needs `tokio` for its async runtime and
  the SIGTERM / drain loop, and reaches the tonic-backed listeners via
  aperture's `spawn`; both are already resolved in `Cargo.lock`.
- **`aegis::TenantId`** is in-tree (REUSE); not external.

**Conclusion: zero new external crates enter the workspace dependency
graph.** Both new crates add `[workspace = true]` references to crates
already in `Cargo.lock`. This mirrors the strata-v1 / ray-v1 / pulse-v1
finding (a crate declaring in its own manifest a dependency already
resolved workspace-wide). **Gate 4 (`cargo deny check`) carries forward
UNCHANGED and is a no-op-for-this-feature pass**: `cargo deny` operates
on the resolved workspace graph; since every needed crate is already in
the graph, the licence / advisory / ban checks see no new crate. No
`deny.toml` change is required.

## A4 - No new toolchain pin

Gates carry forward on the existing `stable` toolchain
(`rust-toolchain.toml`), identical to every other Gate 5 job. The sink
is pure `std` plus the already-resolved `opentelemetry-proto` / `prost`;
the gateway is `tokio` + aperture's `spawn`. No MSRV bump (memory
`feedback_msrv_creep_is_ecosystem_reality` does not trigger - no
transitive dep raises its `rust-version`), no nightly feature, no new
component. The new `gate-5-mutants-aperture-storage-sink` job uses the
same `dtolnay/rust-toolchain` stable step as its self-observe template.

## Gates NOT modified (summary)

| Gate | Status | Reason |
|------|--------|--------|
| Gate 1 (`cargo test --workspace`) | UNCHANGED | new member + `[[test]]` blocks auto-discovered (A2) |
| Gate 2 (`cargo public-api`) | UNCHANGED | the new crate is not in the Gate 2 scope set {harness, spark, sieve, codex}; not graduated by this feature |
| Gate 3 (`cargo semver-checks`) | UNCHANGED | same scope as Gate 2; not graduated |
| Gate 4 (`cargo deny check`) | UNCHANGED | zero new external crates in the resolved graph (A3) |
| Gate 5 (`cargo mutants`) | **NEW JOB** | `gate-5-mutants-aperture-storage-sink` added (A1); no gateway-binary gate |
| Prism Gates 6-11 (TS/React) | UNCHANGED | Rust-only commit; path filter excludes it |

## Pre-commit and pre-push hooks

| Hook | Action required |
|------|-----------------|
| `scripts/hooks/pre-commit` | None. Runs `cargo test --workspace` (mirrors Gate 1); the new crate's tests are auto-discovered (A2). |
| `scripts/hooks/pre-push` | None. The per-pkg loop for Gates 2/3 iterates `[otlp-conformance-harness, spark, sieve, codex]`; the new crate is not graduated to Gates 2/3 by this feature. |

The pre-push hook does NOT run `cargo mutants` (mutation testing is a
CI-and-peer-review concern, not a per-push gate, per the per-feature MT
strategy). The new Gate 5 job therefore needs no local-hook mirror.

## DORA framing (one new deployable, no Kaleidoscope-side deploy)

- **Deployment frequency**: N/A on the Kaleidoscope side (no deploy).
  Analog: merge-to-main; this feature targets the per-slice DELIVER
  merges. Operator-side: the gateway is a long-lived process the
  operator deploys; operator-determined.
- **Lead time**: commit to merged-on-main = time-to-merge, bounded by
  the five gates' aggregate wall-clock. The new
  `gate-5-mutants-aperture-storage-sink` job runs in parallel with the
  other Gate 5 jobs (independent `needs`), so it does not lengthen the
  critical path beyond the existing slowest Gate 5 job.
- **Change failure rate**: failed Gate 1 or Gate 5 over the next
  sink-touching commits. Target 0%. The new Gate 5 job makes the
  translation logic's mutation regressions observable for the first
  time.
- **Time to restore**: revert-and-fix-forward per memory
  `feedback_fix_forward_post_merge_correction`.

## Earned-trust note

The sink's single driven dependency is the local filesystem, reached
through the three `FileBacked*Store`s. The catalogued substrate lie is
"the path opens but is not writable" (read-only mount, full disk,
overlayfs no-op `fsync`). The probe (DD5) ingests an empty batch under
a reserved probe tenant into each store after open; a non-writable
`pillar_root` fails the probe and the gateway refuses to start with
`event=health.startup.refused`. The behavioural-layer gold-test drives
this against a read-only `pillar_root` fixture (Gate 1). The new
`gate-5-mutants-aperture-storage-sink` job is the test-quality probe
that proves the round-trip and refusal tests can distinguish the
correct translator from a behaviourally-mutated one - including the
DD7 atomic-refusal and DD8 skip-not-refuse invariants.
