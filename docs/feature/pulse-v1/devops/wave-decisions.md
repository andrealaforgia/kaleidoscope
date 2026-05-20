# Pulse v1 — DEVOPS wave decisions

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-20
- **Wave**: DEVOPS
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default; pure
  trunk-based, no required-status-checks per memory
  `project_kaleidoscope_pure_trunk_based`)
- **Predecessor handoff**: `design/wave-decisions.md` DEVOPS handoff
  annotation; `discuss/outcome-kpis.md` (KPI1/KPI2/KPI3)

## Posture

Pulse v1 inherits the five-gate workspace CI contract from ADR-0005.
Four of the five gates carry forward UNCHANGED. Gate 5 is the
exception: there is **no `gate-5-mutants-pulse` job in `ci.yml`
today** (verified — `grep -c "gate-5-mutants-pulse"
.github/workflows/ci.yml` returns 0; the existing Gate 5 jobs cover
only aperture, codex, harness, kaleidoscope-cli, self-observe, sieve,
spark). Pulse has never been mutation-gated. Pulse v1 is the moment
to add it.

This is therefore NOT a pure-inheritance wave. It is inheritance for
Gates 1-4 plus ONE new Gate 5 job, mirrored byte-for-byte from the
existing `gate-5-mutants-self-observe` job.

## A1 — NEW `gate-5-mutants-pulse` job (the one real change)

**Verdict: ADD a new per-package Gate 5 job, `gate-5-mutants-pulse`.**

### Why a new job, not inheritance

The other v0-to-v1 carry-forwards in the platform plane (Cinder,
Sluice, Lumen) all landed their durable adapters into crates that
ALREADY had a `gate-5-mutants-<crate>` job, so their DEVOPS waves
were pure Gate 5 inheritance via the `--in-diff` cascade. Pulse is
different: the `pulse` crate has no Gate 5 job at all. The v0 walking
skeleton shipped before pulse was wired into the mutation matrix.
Adding `crates/pulse/src/file_backed.rs` — the durable write and
recovery path, the most correctness-critical code in the crate — with
zero mutation coverage would be the single largest mutation-coverage
gap in the workspace. The per-feature MT strategy in `CLAUDE.md`
(100% kill rate, scoped to modified files, per ADR-0005 Gate 5)
cannot be honoured for pulse without a job to run it. v1 is the
correct moment: the file that most needs mutation testing is the file
this feature introduces.

### Why mirror self-observe specifically

`gate-5-mutants-self-observe` (`ci.yml:862-947`) is the most recently
added per-package Gate 5 job (added by the `cinder-to-pulse-bridge-v0`
DISTILL commit, refined by `cinder-to-otlp-json-bridge-v0`). It
encodes the current-best baseline cascade (`origin/main → HEAD~1 →
full`), the empty-diff short-circuit, the precompiled-binary install,
and the 30-day artefact retention. Mirroring the newest job (rather
than the older harness/aperture jobs) means pulse inherits the latest
conventions with zero drift.

### The six substitutions (and ONLY these six)

The new job is `gate-5-mutants-self-observe` copied verbatim with
exactly six string substitutions. Everything else — `runs-on`,
`needs: [gate-2-public-api, gate-3-semver]`, `timeout-minutes: 30`,
the checkout/toolchain/cache/install step shapes, the baseline
cascade logic, `--no-shuffle --jobs 2`, the artefact retention — is
byte-for-byte identical.

| # | Field | self-observe value | pulse value |
|---|-------|--------------------|-------------|
| 1 | job key | `gate-5-mutants-self-observe` | `gate-5-mutants-pulse` |
| 2 | step `name` | `Gate 5 — cargo mutants (self-observe)` | `Gate 5 — cargo mutants (pulse)` |
| 3 | `--in-diff` path filter | `crates/self-observe/**` | `crates/pulse/**` |
| 4 | `--package` arg | `--package self-observe` | `--package pulse` |
| 5 | cache key suffix | `...-cargo-mutants-self-observe-...` | `...-cargo-mutants-pulse-...` |
| 6 | artefact name | `mutants-out-self-observe` | `mutants-out-pulse` |

(The cache-step display name and the cache `restore-keys` prefix
follow substitution 5 mechanically — they are part of the same
`-self-observe` → `-pulse` token. The diff-echo log lines and step
comments naming "self-observe" follow substitution 3/4 mechanically.
These are cosmetic consequences of the six, not additional changes.)

The full byte-for-byte YAML snippet is in `ci-cd-pipeline.md` for
Crafty to copy-paste.

### Landing discipline

Per the constraint and per the Cinder/Lumen precedent, this DEVOPS
wave does **NOT** edit `ci.yml`. `@nw-software-crafter` (Crafty)
lands the new `gate-5-mutants-pulse` job atomic with the
`file_backed.rs` implementation in the DELIVER commit, so the job and
the code it gates arrive together and the first CI run on the
implementation commit exercises the new gate immediately.

## A2 — Gate 1 auto-discovers the two new `[[test]]` blocks

Gate 1 (`cargo test --workspace --all-targets --locked`) carries
forward UNCHANGED. The DELIVER commit adds two `[[test]]` blocks to
`crates/pulse/Cargo.toml`:

```toml
[[test]]
name = "v1_slice_01_wal_durability"
path = "tests/v1_slice_01_wal_durability.rs"

[[test]]
name = "v1_slice_02_snapshot"
path = "tests/v1_slice_02_snapshot.rs"
```

`--workspace --all-targets` discovers these automatically; the
workflow invocation needs no edit. The acceptance tests written by
`@nw-acceptance-designer` in DISTILL (from US-PV1-01, US-PV1-02) run
under Gate 1 and ARE the measurement of KPI1, KPI2 and KPI3 — see
`kpi-instrumentation.md`.

## A3 — `serde` + `serde_json` enter pulse's `[dependencies]` (NO new external crate)

**Correction to the DESIGN handoff annotation.** The DESIGN
`wave-decisions.md` DEVOPS handoff states "no new `[dependencies]`
(`serde`, `serde_json`, `aegis` already present)". This is NOT
accurate for the `pulse` crate. Verified by reading
`crates/pulse/Cargo.toml`: pulse v0's `[dependencies]` block contains
**only** `aegis`. `serde` and `serde_json` are NOT direct
dependencies of pulse v0.

What IS true: `serde` and `serde_json` are declared in the workspace
`[workspace.dependencies]` (`Cargo.toml:55-56`) and are already pulled
into the lockfile by `aegis` (`crates/aegis/Cargo.toml:20-21`) and
the other durable crates. So pulse v1's serde derives (DD2/DD5/D5)
require adding to pulse's OWN `[dependencies]`:

```toml
serde = { workspace = true }
serde_json = { workspace = true }
```

This is a new entry in `crates/pulse/Cargo.toml`, but it adds **zero
new external crates to the workspace dependency graph** — both are
already resolved in `Cargo.lock`. The distinction matters for Gate 4:

**Gate 4 (`cargo deny check`) carries forward UNCHANGED and is a
no-op-for-this-feature pass.** `cargo deny` operates on the resolved
workspace graph; since `serde`/`serde_json` are already in the graph,
the licence/advisory/ban checks see no new crate. No `deny.toml`
change is required. The DESIGN handoff's CONCLUSION (Gate 4 unaffected,
no new external dependency) is correct; only its PREMISE (serde
already in pulse's manifest) was wrong.

## A4 — No new toolchain pin

Gates carry forward on the existing `stable` toolchain
(`rust-toolchain.toml`), identical to every other Gate 5 job. The
durable adapter is pure `std` plus the already-resolved
`serde`/`serde_json`; no MSRV bump (memory
`feedback_msrv_creep_is_ecosystem_reality` does not trigger — no
transitive dep raises its `rust-version`), no nightly feature, no new
component. The new `gate-5-mutants-pulse` job uses the same
`dtolnay/rust-toolchain` stable step as its self-observe template.

## Gates NOT modified (summary)

| Gate | Status | Reason |
|------|--------|--------|
| Gate 1 (`cargo test --workspace`) | UNCHANGED | new `[[test]]` blocks auto-discovered (A2) |
| Gate 2 (`cargo public-api`) | UNCHANGED | pulse not in the Gate 2 scope set {harness, spark, sieve, codex}; not graduated by this feature |
| Gate 3 (`cargo semver-checks`) | UNCHANGED | same scope as Gate 2; not graduated |
| Gate 4 (`cargo deny check`) | UNCHANGED | zero new external crates in the resolved graph (A3) |
| Gate 5 (`cargo mutants`) | **NEW JOB** | `gate-5-mutants-pulse` added (A1) |
| Prism Gates 6-11 (TS/React) | UNCHANGED | Rust-only commit; path filter excludes it |

## Pre-commit and pre-push hooks

| Hook | Action required |
|------|-----------------|
| `scripts/hooks/pre-commit` | None. Runs `cargo test --workspace` (mirrors Gate 1); the two new test files are auto-discovered (A2). |
| `scripts/hooks/pre-push` | None. The per-pkg loop for Gates 2/3 iterates `[otlp-conformance-harness, spark, sieve, codex]`; pulse is not graduated to Gates 2/3 by this feature, so it is not added. |

The pre-push hook does NOT run `cargo mutants` (mutation testing is a
CI-and-peer-review concern, not a per-push gate, per the per-feature
MT strategy). The new Gate 5 job therefore needs no local-hook mirror.

## DORA framing (library, no deploy)

- **Deployment frequency**: N/A (no deploy). Analog: merge-to-main;
  this feature targets one merge at DELIVER close.
- **Lead time**: commit to available-to-downstream = time-to-merge;
  the five gates' aggregate wall-clock bounds it. The new
  `gate-5-mutants-pulse` job runs in parallel with the other Gate 5
  jobs (independent `needs`), so it does not lengthen the critical
  path beyond the existing slowest Gate 5 job.
- **Change failure rate**: failed Gate 1 or Gate 5 over the next 10
  pulse-touching commits. Target 0%. The new Gate 5 job makes pulse
  mutation regressions observable for the first time.
- **Time to restore**: revert-and-fix-forward per memory
  `feedback_fix_forward_post_merge_correction`.

## Earned-trust note

The single driven dependency is the local filesystem (DESIGN
Earned-Trust note). The KPI3 parallel-store equality test is the
behavioural gold-test exercising the actual append/flush/truncate
substrate; the new `gate-5-mutants-pulse` job is the test-quality
probe that proves those gold-tests can distinguish the correct
adapter from a behaviourally-mutated one. `BufWriter::flush` is not
fsync; `kill -9`-between-flush-and-fsync recovery is documented v2
scope, not a v1 gate.
