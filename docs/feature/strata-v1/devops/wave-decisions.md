# Strata v1 — DEVOPS wave decisions

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Wave**: DEVOPS
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default; pure
  trunk-based, no required-status-checks per memory
  `project_kaleidoscope_pure_trunk_based`)
- **Predecessor handoff**: `design/wave-decisions.md` DEVOPS handoff
  annotation; `discuss/outcome-kpis.md` (KPI1/KPI2/KPI3)
- **Direct precedent**: `docs/feature/ray-v1/devops/` and
  `docs/feature/pulse-v1/devops/` — both were the identical situation
  (a never-mutation-gated crate gaining a v1 durable adapter). Strata
  v1 follows them exactly. This is the THIRD time in a row a v1
  storage pillar adds its first mutation gate: pulse, then ray, now
  strata.

## Posture

Strata v1 inherits the five-gate workspace CI contract from ADR-0005.
Four of the five gates carry forward UNCHANGED. Gate 5 is the
exception: there is **no `gate-5-mutants-strata` job in `ci.yml`
today** (verified — `grep -c "gate-5-mutants-strata"
.github/workflows/ci.yml` returns 0; the existing Gate 5 jobs cover
only aperture, codex, harness, kaleidoscope-cli, pulse, ray,
self-observe, sieve, spark). Strata has never been mutation-gated.
Strata v1 is the moment to add it.

This is therefore NOT a pure-inheritance wave. It is inheritance for
Gates 1-4 plus ONE new Gate 5 job, mirrored byte-for-byte from the
existing `gate-5-mutants-self-observe` job — exactly as ray-v1 did one
day ago (`gate-5-mutants-ray`) and pulse-v1 the day before that
(`gate-5-mutants-pulse`). Strata is the last pillar to receive its
first mutation gate; after this every storage pillar in the workspace
is mutation-gated.

## A1 — NEW `gate-5-mutants-strata` job (the one real change)

**Verdict: ADD a new per-package Gate 5 job, `gate-5-mutants-strata`.**

### Why a new job, not inheritance

The earliest v0-to-v1 carry-forwards in the platform plane (Cinder,
Sluice, Lumen) all landed their durable adapters into crates that
ALREADY had a `gate-5-mutants-<crate>` job, so their DEVOPS waves were
pure Gate 5 inheritance via the `--in-diff` cascade. Strata is
different: the `strata` crate has no Gate 5 job at all. The v0 walking
skeleton shipped before strata was wired into the mutation matrix.
Pulse hit this gap, resolved it by adding `gate-5-mutants-pulse`; ray
hit it the next day and added `gate-5-mutants-ray`; strata is the
third and final pillar in this run, repeating the move.

Adding `crates/strata/src/file_backed.rs` — the durable write and
recovery path, the most correctness-critical code in the crate, AND
the home of the single shared `apply_ingest` whose no-drift guarantee
(DESIGN DD4) is *only* enforceable by mutation testing — with zero
mutation coverage would be the single largest mutation-coverage gap in
the workspace. The per-feature MT strategy in `CLAUDE.md` (100% kill
rate, scoped to modified files, per ADR-0005 Gate 5) cannot be
honoured for strata without a job to run it. v1 is the correct moment:
the file that most needs mutation testing is the file this feature
introduces.

DD4 makes this sharp: the architecture rule "live ingest and WAL
replay route through ONE `apply_ingest`, so the on-disk and in-memory
views cannot drift" is verified structurally by there being exactly
one such function, and the DESIGN handoff states explicitly that the
mutation suite is "the enforcement that the single `apply_ingest` has
no divergent twin". Without `gate-5-mutants-strata` there is no
enforcer for the central design invariant of the feature. Strata is
actually the SIMPLEST of the six (a single per-service index, no
derived second index to rebuild — unlike Ray), but a single index
still has exactly one `apply_ingest` that must not be silently
mutated; the enforcer is still required.

### Why mirror self-observe specifically

`gate-5-mutants-self-observe` (`ci.yml:862-947`) is the canonical
per-package Gate 5 template — it is the byte source that
`gate-5-mutants-pulse` and `gate-5-mutants-ray` were both mirrored
from. It encodes the current-best baseline cascade (`origin/main ->
HEAD~1 -> full`), the empty-diff short-circuit, the precompiled-binary
install, and the 30-day artefact retention. Mirroring it (rather than
the older harness/aperture jobs) means strata inherits the latest
conventions with zero drift, and stays byte-identical to the ray job
added yesterday and the pulse job before it.

### The six substitutions (and ONLY these six)

The new job is `gate-5-mutants-self-observe` copied verbatim with
exactly six string substitutions. Everything else — `runs-on`,
`needs: [gate-2-public-api, gate-3-semver]`, `timeout-minutes: 30`,
the checkout/toolchain/cache/install step shapes, the baseline cascade
logic, `--no-shuffle --jobs 2`, the artefact retention — is
byte-for-byte identical.

| # | Field | self-observe value | strata value |
|---|-------|--------------------|--------------|
| 1 | job key | `gate-5-mutants-self-observe` | `gate-5-mutants-strata` |
| 2 | step `name` | `Gate 5 — cargo mutants (self-observe)` | `Gate 5 — cargo mutants (strata)` |
| 3 | `--in-diff` path filter | `crates/self-observe/**` | `crates/strata/**` |
| 4 | `--package` arg | `--package self-observe` | `--package strata` |
| 5 | cache key suffix | `...-cargo-mutants-self-observe-...` | `...-cargo-mutants-strata-...` |
| 6 | artefact name | `mutants-out-self-observe` | `mutants-out-strata` |

(The cache-step display name and the cache `restore-keys` prefix
follow substitution 5 mechanically — they are part of the same
`-self-observe` -> `-strata` token. The diff-echo log lines and step
comments naming "self-observe" follow substitution 3/4 mechanically.
These are cosmetic consequences of the six, not additional changes.)

The full byte-for-byte YAML snippet is in `ci-cd-pipeline.md` for
Crafty to copy-paste.

### Landing discipline

Per the constraint and per the Cinder/Lumen/Pulse/Ray precedent, this
DEVOPS wave does **NOT** edit `ci.yml`. `@nw-software-crafter` (Crafty)
lands the new `gate-5-mutants-strata` job atomic with the
`file_backed.rs` implementation in the DELIVER commit, so the job and
the code it gates arrive together and the first CI run on the
implementation commit exercises the new gate immediately. Insert it
adjacent to the other Gate 5 jobs (e.g. after `gate-5-mutants-ray`,
before `gate-5-mutants-kaleidoscope-cli`).

## A2 — Gate 1 auto-discovers the two new `[[test]]` blocks

Gate 1 (`cargo test --workspace --all-targets --locked`) carries
forward UNCHANGED. The DELIVER commit adds two `[[test]]` blocks to
`crates/strata/Cargo.toml`:

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
`@nw-acceptance-designer` in DISTILL (from US-SV1-01, US-SV1-02) run
under Gate 1 and ARE the measurement of KPI1, KPI2 and KPI3 — see
`kpi-instrumentation.md`. The DESIGN handoff already names these two
blocks as the only Cargo delta beyond dependencies.

## A3 — `serde` + `serde_json` enter strata's `[dependencies]` (NO new external crate)

**Correction to the DESIGN handoff annotation.** The DESIGN
`wave-decisions.md` DEVOPS handoff states "no new `[dependencies]`
(`serde` / `serde_json` / `aegis` already present)" and the Portability
row of the quality-attribute table repeats "no new external crate
(`serde` / `serde_json` / `aegis` already present)". This is NOT
accurate for the `strata` crate. Verified by reading
`crates/strata/Cargo.toml`: strata v0's `[dependencies]` block contains
**only** `aegis`. `serde` and `serde_json` are NOT direct dependencies
of strata v0.

This is the same premise error the pulse-v1 and ray-v1 DEVOPS waves
caught (A3 in both): strata, like pulse and ray, pulls serde
transitively via aegis today but does not declare it.

What IS true: `serde` and `serde_json` are declared in the workspace
`[workspace.dependencies]` and are already pulled into the lockfile by
`aegis` and the other durable crates. So strata v1's serde derives
(DD2/DD5) — plain `Serialize`/`Deserialize` derives on `Profile`,
`ProfileBatch`, `Sample`, `Location`, `Function`, `Mapping`,
`SampleType`, `ValueType`, `ServiceName`, `TimeRange` — require adding
to strata's OWN `[dependencies]`:

```toml
serde = { workspace = true }
serde_json = { workspace = true }
```

This is a new entry in `crates/strata/Cargo.toml`, but it adds **zero
new external crates to the workspace dependency graph** — both are
already resolved in `Cargo.lock`. Note the contrast with Ray: strata
needs **no `hex` and no `serde_with`** because there is no `[u8; N]` or
`Vec<u8>` field on any profile type (DD5, confirmed from
`profile.rs:65-157`). The heaviest fields (`Vec<u64>`, `Vec<i64>`,
`Vec<String>`, `BTreeMap<String, String>`) all serialise as natural
JSON via the plain derive — no custom codec, so even fewer crates than
the ray case. The distinction matters for Gate 4:

**Gate 4 (`cargo deny check`) carries forward UNCHANGED and is a
no-op-for-this-feature pass.** `cargo deny` operates on the resolved
workspace graph; since `serde`/`serde_json` are already in the graph,
the licence/advisory/ban checks see no new crate. No `deny.toml`
change is required. The DESIGN handoff's CONCLUSION (Gate 4 unaffected,
no new external dependency) is correct; only its PREMISE (serde already
in strata's manifest) was wrong.

## A4 — No new toolchain pin

Gates carry forward on the existing `stable` toolchain
(`rust-toolchain.toml`), identical to every other Gate 5 job. The
durable adapter is pure `std` plus the already-resolved
`serde`/`serde_json` and plain serde derives (no hand-rolled hex
module — strata is lighter than ray here); no MSRV bump (memory
`feedback_msrv_creep_is_ecosystem_reality` does not trigger — no
transitive dep raises its `rust-version`), no nightly feature, no new
component. The new `gate-5-mutants-strata` job uses the same
`dtolnay/rust-toolchain` stable step as its self-observe template.

## Gates NOT modified (summary)

| Gate | Status | Reason |
|------|--------|--------|
| Gate 1 (`cargo test --workspace`) | UNCHANGED | new `[[test]]` blocks auto-discovered (A2) |
| Gate 2 (`cargo public-api`) | UNCHANGED | strata not in the Gate 2 scope set {harness, spark, sieve, codex}; not graduated by this feature |
| Gate 3 (`cargo semver-checks`) | UNCHANGED | same scope as Gate 2; not graduated |
| Gate 4 (`cargo deny check`) | UNCHANGED | zero new external crates in the resolved graph (A3) |
| Gate 5 (`cargo mutants`) | **NEW JOB** | `gate-5-mutants-strata` added (A1) |
| Prism Gates 6-11 (TS/React) | UNCHANGED | Rust-only commit; path filter excludes it |

## Pre-commit and pre-push hooks

| Hook | Action required |
|------|-----------------|
| `scripts/hooks/pre-commit` | None. Runs `cargo test --workspace` (mirrors Gate 1); the two new test files are auto-discovered (A2). |
| `scripts/hooks/pre-push` | None. The per-pkg loop for Gates 2/3 iterates `[otlp-conformance-harness, spark, sieve, codex]`; strata is not graduated to Gates 2/3 by this feature, so it is not added. |

The pre-push hook does NOT run `cargo mutants` (mutation testing is a
CI-and-peer-review concern, not a per-push gate, per the per-feature
MT strategy). The new Gate 5 job therefore needs no local-hook mirror.

## DORA framing (library, no deploy)

- **Deployment frequency**: N/A (no deploy). Analog: merge-to-main;
  this feature targets one merge at DELIVER close.
- **Lead time**: commit to available-to-downstream = time-to-merge;
  the five gates' aggregate wall-clock bounds it. The new
  `gate-5-mutants-strata` job runs in parallel with the other Gate 5
  jobs (independent `needs`), so it does not lengthen the critical path
  beyond the existing slowest Gate 5 job.
- **Change failure rate**: failed Gate 1 or Gate 5 over the next 10
  strata-touching commits. Target 0%. The new Gate 5 job makes strata
  mutation regressions observable for the first time.
- **Time to restore**: revert-and-fix-forward per memory
  `feedback_fix_forward_post_merge_correction`.

## Earned-trust note

The single driven dependency is the local filesystem (DESIGN
Earned-Trust note). The KPI3 parallel-store equality test is the
behavioural gold-test exercising the actual append/flush/truncate
substrate across the single per-service index, including the
empty-`service.name` drop edge case (those profiles are intentionally
absent both before and after recovery). The new
`gate-5-mutants-strata` job is the test-quality probe that proves
those gold-tests can distinguish the correct adapter from a
behaviourally-mutated one — and is the only mechanism that kills a
divergent second copy of `apply_ingest` (DD4). `BufWriter::flush` is
not fsync; `kill -9`-between-flush-and-fsync recovery is documented v2
scope, not a v1 gate.
