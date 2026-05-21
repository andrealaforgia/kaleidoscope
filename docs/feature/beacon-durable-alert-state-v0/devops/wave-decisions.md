# beacon-durable-alert-state-v0 — DEVOPS wave decisions

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Wave**: DEVOPS
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default; pure
  trunk-based, no required-status-checks per memory
  `project_kaleidoscope_pure_trunk_based`)
- **Predecessor handoff**: `design/wave-decisions.md` DEVOPS handoff
  annotation (the `gate-5-mutants-beacon` expectation, lines 281-296);
  `discuss/outcome-kpis.md` (KPI1 recovery completeness, KPI2 zero
  re-fire, KPI3 persist p95, KPI4 recover p95)
- **Direct precedent**: `docs/feature/strata-v1/devops/` — strata was
  the last never-mutation-gated pillar to gain a Gate 5 job when its v1
  introduced a durable FileBackedProfileStore. beacon is the identical
  situation: a never-gated crate gaining real durable logic. This wave
  follows strata-v1 exactly.

## Posture

beacon-durable-alert-state-v0 inherits the five-gate workspace CI
contract from ADR-0005. Four of the five gates carry forward UNCHANGED.
Gate 5 is the exception: there is **no `gate-5-mutants-beacon` job in
`ci.yml` today** (verified — `grep -c "gate-5-mutants-beacon"
.github/workflows/ci.yml` returns **0**; the existing Gate 5 jobs cover
only aperture, codex, harness, kaleidoscope-cli, pulse, ray,
self-observe, sieve, spark, strata). beacon has **never** been
mutation-gated. This feature is the moment to add it, because this
feature is the moment beacon gains durable logic worth mutating.

This is therefore NOT a pure-inheritance wave. It is inheritance for
Gates 1-4 plus ONE new Gate 5 job, mirrored byte-for-byte from the
existing `gate-5-mutants-self-observe` job — exactly as strata-v1 did
(`gate-5-mutants-strata`).

## A1 — NEW `gate-5-mutants-beacon` job (the one real change)

**Verdict: ADD a new per-package Gate 5 job, `gate-5-mutants-beacon`,
byte-mirrored from `gate-5-mutants-self-observe` with six substitutions,
keeping the `--in-diff` baseline cascade unchanged.**

### Why a new job, not inheritance

beacon shipped its walking skeleton and subsequent slices
(state_machine, inhibition, loader, slo, sinks, types) as a *pure
evaluation engine*: `transition` is a pure, total, side-effect-free
function (ADR-0037), and per-rule state lived as a transient `let mut
state` inside `run_rule`. There was no durable logic, so beacon was
never wired into the Gate 5 mutation matrix.

This feature changes that. beacon now gains **real durable logic for
the first time**: a `FileBackedRuleStateStore` with

- WAL append on every state transition (DESIGN DD5),
- snapshot flush + WAL truncation (DD5),
- **keyed-latest-wins** recovery on `open()` — replay Put records in
  file order, last write per `rule_id` wins, no sort step (DD4), the
  load-bearing contrast against all six pillars' append-and-sort, and
- a `PersistenceFailed` error path that makes beacon-server **refuse to
  start** rather than silently reset corrupt state (DD6, DD8 step 1).

That is precisely the code class ADR-0005 Gate 5 (100% kill rate)
exists to protect. Per CLAUDE.md the project mutation-tests durable
adapters at a 100% kill rate scoped to modified files; that gate cannot
be honoured for beacon without a job to run it. Recovery's
keyed-latest-wins semantics are exactly the kind of logic a mutation
("last wins" -> "first wins", `!=` -> `==`, drop the overwrite) can
silently break while leaving a weak test green. The new job is the only
mechanism that proves the durability tests actually distinguish the
correct adapter from a behaviourally-mutated one.

### --in-diff confirmed — the large crate is not a problem

beacon's `src` is ~1470 lines across state_machine, inhibition, loader,
slo, sinks, types and lib — larger than the storage crates. This does
**not** blow up the mutation job, because the new
`gate-5-mutants-beacon` job uses the **same `--in-diff` baseline
cascade** (`origin/main` -> `HEAD~1` -> full) as every other Gate 5 job.
`--in-diff` scopes mutation to the hunks in the commit's diff against
the baseline, not the whole crate. On the DELIVER commit the diff
touches only the new `state_store` module (and the additive serde derive
on `RuleState`), so cargo-mutants mutates the new durable code and the
touched lines — not the 1470 lines of pre-existing pure-evaluation
source. The empty-diff short-circuit (a commit not touching
`crates/beacon/**` exits in zero seconds) is preserved verbatim. This is
confirmed identical to the self-observe and strata templates.

### Why mirror self-observe specifically

`gate-5-mutants-self-observe` (`ci.yml:862-947`) is the canonical
per-package Gate 5 template — the byte source `gate-5-mutants-strata`
was itself mirrored from. It encodes the current-best baseline cascade
(`origin/main -> HEAD~1 -> full`), the empty-diff short-circuit, the
precompiled-binary install, `--no-shuffle --jobs 2`, and the 30-day
artefact retention. Mirroring it (rather than the older
harness/aperture jobs) means beacon inherits the latest conventions
with zero drift and stays byte-identical to the strata job.

### The six substitutions (and ONLY these six)

The new job is `gate-5-mutants-self-observe` copied verbatim with
exactly six string substitutions. Everything else — `runs-on`,
`needs: [gate-2-public-api, gate-3-semver]`, `timeout-minutes: 30`, the
checkout/toolchain/cache/install step shapes, the baseline-cascade
logic, the empty-diff short-circuit, `--no-shuffle --jobs 2`, the
artefact retention — is byte-for-byte identical.

| # | Field | self-observe value | beacon value |
|---|-------|--------------------|--------------|
| 1 | job key | `gate-5-mutants-self-observe` | `gate-5-mutants-beacon` |
| 2 | step `name` | `Gate 5 — cargo mutants (self-observe)` | `Gate 5 — cargo mutants (beacon)` |
| 3 | `--in-diff` path filter | `crates/self-observe/**` | `crates/beacon/**` |
| 4 | `--package` arg | `--package self-observe` | `--package beacon` |
| 5 | cache key suffix | `...-cargo-mutants-self-observe-...` | `...-cargo-mutants-beacon-...` |
| 6 | artefact name | `mutants-out-self-observe` | `mutants-out-beacon` |

(The cache-step display name and the cache `restore-keys` prefix follow
substitution 5 mechanically — they are part of the same `-self-observe`
-> `-beacon` token. The diff-echo log lines and step comments naming
the crate follow substitutions 3/4 mechanically. These are cosmetic
consequences of the six, not additional changes.)

The full byte-for-byte YAML snippet is in `ci-cd-pipeline.md` for Crafty
to copy-paste.

### Landing discipline

Per the constraint and per the strata precedent, this DEVOPS wave does
**NOT** edit `ci.yml`. `@nw-software-crafter` (Crafty) lands the new
`gate-5-mutants-beacon` job atomic with the `state_store` implementation
in the DELIVER commit, so the job and the code it gates arrive together
and the first CI run on the implementation commit exercises the new gate
immediately. Insert it adjacent to the other Gate 5 jobs (e.g. after
`gate-5-mutants-strata`, before `gate-5-mutants-kaleidoscope-cli`).

## A2 — Gate 1 auto-discovers the new beacon tests and beacon-server wiring

Gate 1 (`cargo test --workspace --all-targets --locked`) carries
forward UNCHANGED. The DELIVER commit adds the durable-store unit and
integration tests (the round-trip recovery test, the
restart-survival/zero-re-fire test, and the persist/recover micro-
benchmarks) plus the beacon-server wiring changes in `run_rule` (DD8).
`--workspace --all-targets` discovers every new `[[test]]` block and the
re-wired binary automatically; the workflow invocation needs no edit.

The acceptance tests written in DISTILL (from US-01/02/03, including the
three substrate-lie gold tests: corrupt snapshot, truncated WAL line,
future-dated `since`) run under Gate 1 and ARE the measurement of KPI1,
KPI2, KPI3 and KPI4 — see `kpi-instrumentation.md`. The beacon-server
recover-then-refuse startup path (DD8 step 1) and the
`recovered alert state ...` log line are exercised here too.

## A3 — Zero new external crates: serde + serde_json already in beacon's manifest

**Confirmed against the source — and, unlike strata, the DESIGN handoff
premise is correct here.** Verified by reading `crates/beacon/Cargo.toml`
(lines 27-28):

```toml
serde = { workspace = true }
serde_json = { workspace = true }
```

Both are ALREADY direct dependencies of the `beacon` crate today
(beacon loads CUE rules and already needs serde for that). The feature's
serde derives — the plain `#[derive(Serialize, Deserialize)]` on
`RuleState` (DD7) and the `WalRecord::Put` codec (DD4) — therefore
require **no manifest change at all** beyond what is already present,
and add **zero new external crates** to the workspace dependency graph.
`SystemTime` serialises natively via serde (DD7), so no `serde_with`, no
`hex`, no custom codec, no newtype is needed — the `Instant` problem
that would have forced a custom conversion does not exist
(`RuleState` uses `SystemTime`, DESIGN serialisation-risk resolution).

This is the cleanest possible case: strata-v1's A3 had to *correct* the
DESIGN handoff and ADD `serde`/`serde_json` to strata's manifest (they
were pulled transitively via aegis but not declared). beacon needs no
such correction — serde is already declared. The distinction matters for
Gate 4:

**Gate 4 (`cargo deny check`) carries forward UNCHANGED and is a
no-op-for-this-feature pass.** `cargo deny` operates on the resolved
workspace graph; since no crate enters the graph, the licence /
advisory / ban checks see nothing new. No `deny.toml` change is
required.

## A4 — No new toolchain pin

Gates carry forward on the existing `stable` toolchain
(`rust-toolchain.toml`), identical to every other Gate 5 job. The
durable adapter is pure `std` plus the already-declared
`serde`/`serde_json` and plain serde derives; no MSRV bump (memory
`feedback_msrv_creep_is_ecosystem_reality` does not trigger — no
transitive dep raises its `rust-version`), no nightly feature, no new
component. The new `gate-5-mutants-beacon` job uses the same
`dtolnay/rust-toolchain` stable step as its self-observe template.

## Gates NOT modified (summary)

| Gate | Status | Reason |
|------|--------|--------|
| Gate 1 (`cargo test --workspace`) | UNCHANGED | new tests + beacon-server wiring auto-discovered (A2) |
| Gate 2 (`cargo public-api`) | UNCHANGED | beacon not in the Gate 2 scope set {harness, spark, sieve, codex}; not graduated by this feature |
| Gate 3 (`cargo semver-checks`) | UNCHANGED | same scope as Gate 2; not graduated |
| Gate 4 (`cargo deny check`) | UNCHANGED | zero new external crates; serde already declared in beacon's manifest (A3) |
| Gate 5 (`cargo mutants`) | **NEW JOB** | `gate-5-mutants-beacon` added (A1) |
| Prism Gates 6-11 (TS/React) | UNCHANGED | Rust-only commit; path filter excludes it |

## Pre-commit and pre-push hooks

| Hook | Action required |
|------|-----------------|
| `scripts/hooks/pre-commit` | None. Runs `cargo test --workspace` (mirrors Gate 1); the new test files and beacon-server wiring are auto-discovered (A2). |
| `scripts/hooks/pre-push` | None. The per-pkg loop for Gates 2/3 iterates the graduated scope set; beacon is not graduated by this feature, so it is not added. |

The pre-push hook does NOT run `cargo mutants` (mutation testing is a
CI-and-peer-review concern, not a per-push gate, per the per-feature MT
strategy in CLAUDE.md). The new Gate 5 job therefore needs no local-hook
mirror.

## DORA framing (library-plus-binary, no deploy)

- **Deployment frequency**: N/A (no image shipped by this feature).
  Analog: merge-to-main; this feature targets one merge at DELIVER
  close.
- **Lead time**: commit to available-to-downstream = time-to-merge; the
  five gates' aggregate wall-clock bounds it. The new
  `gate-5-mutants-beacon` job runs in parallel with the other Gate 5
  jobs (independent `needs`), so it does not lengthen the critical path
  beyond the existing slowest Gate 5 job. `--in-diff` keeps the beacon
  job's own runtime proportional to the diff, not the 1470-line crate.
- **Change failure rate**: failed Gate 1 or Gate 5 over the next 10
  beacon-touching commits. Target 0%. The new Gate 5 job makes beacon
  mutation regressions observable for the first time.
- **Time to restore**: revert-and-fix-forward per memory
  `feedback_fix_forward_post_merge_correction`.

## Earned-trust note

The single driven dependency is the local filesystem (DESIGN
Earned-Trust note). The substrate can lie: a snapshot truncated by a
full disk, a half-written WAL line, a future-dated `since`. The DESIGN
mandates recover-then-refuse: a corrupt `open()` returns
`PersistenceFailed` and beacon-server aborts startup with a structured
error and non-zero exit code, never a silent reset (DD8 step 1). The
DISTILL acceptance suite catalogues those substrate lies as gold tests.
The new `gate-5-mutants-beacon` job is the test-quality probe that
proves those gold tests can distinguish the correct adapter from a
behaviourally-mutated one — and is the enforcer that keyed-latest-wins
recovery (DD4) has no silently-mutated twin.
