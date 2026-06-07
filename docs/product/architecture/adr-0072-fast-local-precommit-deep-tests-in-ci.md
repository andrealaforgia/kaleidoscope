# ADR-0072 — The local pre-commit hook runs a fast subset; the deep tests gate in CI

- **Status**: Accepted
- **Date**: 2026-06-07
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `speed-up-local-precommit-v0`
- **Supersedes**: none. (The hook's Step 4 invocation is CHANGED by this
  feature, but no prior ADR pinned `cargo test --workspace --all-targets
  --locked` as the *local* Step 4; ADR-0005 pins it as **CI Gate 1**, which
  is preserved unchanged.)
- **Superseded by**: none
- **Related**: ADR-0070 (`perf-kpi-ci-non-gating-v0` — the direct sibling.
  ADR-0070 moved the *perf wall-clock assertions* off gating Gate 1 because
  durability fsync made them slow/flaky on CI, but it explicitly left the
  durability **test bodies** running locally and CI-side, paying full fsync
  cost — see ADR-0070 §7 "No local hook change". THIS ADR is the missing
  LOCAL-side sibling: it moves the slow tests' *local blocking* off the
  commit path while leaving them gating in CI. Same trunk-based "CI is
  feedback" framing, same honesty-trade posture, different lever and
  location). ADR-0005 (the five-gate CI contract; Gate 1 is `cargo test
  --workspace --all-targets --locked`; cited and **NOT modified** — this ADR
  does not touch CI Gate 1). ADR-0049 (`earned-trust-fsync-probe-v0`) and
  ADR-0060 (`store-fsync-durability-v0` — added per-record `sync_all` to the
  WAL append path across the file-backed stores; these are the durable-cost
  root cause that makes the `tests/*.rs` durability bins I/O-bound; cited,
  NOT modified). The next free ADR number is 0072 (0071,
  `aperture-presubscriber-probe-refusal-visibility`, is the highest existing;
  verified by `ls docs/product/architecture/adr-*.md`).

## Context

`scripts/hooks/pre-commit` runs five steps in order (verified in code this
wave):

- Step 0 — toolchain symmetry check (fast; warns when a non-rustup cargo
  masks the `rust-toolchain.toml` pin).
- Step 1 — `cargo fmt --all -- --check` (fast).
- Step 2 — `cargo clippy --all-targets --locked -- -D warnings` (moderate;
  compile-bound, not fsync-bound).
- Step 3 — `cargo deny --all-features check` (Gate 4; fast).
- Step 4 — **`cargo test --workspace --all-targets --locked` (Gate 1)** —
  the slow part (`scripts/hooks/pre-commit:92-93`).

`--all-targets --workspace` compiles and runs every integration test binary
across the workspace. **Structural inventory measured this wave (filesystem,
not assumed):**

- **26** workspace crates (`Cargo.toml` members).
- **165** integration test binaries (`crates/**/tests/*.rs`) — each is a
  *separate* `--all-targets` compile-and-run unit.
- **26** of those 165 are the fsync-heavy durability family — files matching
  `*wal_durability*`, `*snapshot*`, `*torn_tail*`, `*crash_durability*`,
  `*fsync_probe*`, `*snapshot_atomicity*`, `*filebacked_durable_recovery*`,
  spread across cinder (x4), pulse (x6), lumen (x4), ray (x4), strata (x3),
  sluice (x3), beacon (x1), and log-query-api (x1). These pay a per-record
  `sync_all` on the WAL append path (ADR-0049 §4 / ADR-0060 §3).
- Plus the subprocess suites that spawn real binaries: aperture
  `slice_10_ingest_auth`, `serve_loop_error_surfacing`, `cli_smoke`,
  `probe_gold_runner`, `probe_refusal_visibility`; kaleidoscope-cli
  `*_roundtrip` / subcommand bins; beacon-server smoke/reload.

These I/O-bound durability + subprocess bins are what make Step 4 take
**10-20 minutes** under parallel load; one prior commit's hook wedged for
hours under leaked-process contention (DISCUSS persona, US-01). The wait
trains the maintainer (human or crafter agent) toward
`git commit --no-verify`, which silently drops **all** gates — the worst
outcome for "main is socially always green".

**The deep gate already lives in CI.** `.github/workflows/ci.yml`
`gate-1-test` runs `cargo test --workspace --all-targets --locked`
(ci.yml:182). The local hook's Step 4 is therefore a slow **duplicate** of
the authoritative CI gate, not unique coverage.

**Posture.** Kaleidoscope is pure trunk-based: `main` has no
required-status-checks and no `enforce_admins`; "CI is feedback, not a gate"
(project memory `project_kaleidoscope_pure_trunk_based`). The local hook is a
courtesy, not a hard gate. Moving the slow tests' *gating* to CI — where the
same invocation already runs — is consistent with that stated posture, and
is exactly the move ADR-0070 made for the perf assertions.

### Measurement honesty (read this)

The DESIGN wave for this feature ran in a harness whose solution-architect
agent has **no shell-execution tool** (Read / Write / Edit / Glob / Grep
only). `cargo` and `time` therefore could NOT be run from DESIGN. The
wall-clock seconds for fmt / clippy / deny / `--lib` were **not measured in
this wave**. Per the test-don't-assume and Earned-Trust principles,
fabricating seconds would be worse than declaring the gap, so they are NOT
invented here. What IS measured and load-bearing is the **structural** fact
above: `cargo test --workspace --lib` runs **none** of the 165 integration
binaries (it builds and runs only in-crate `#[cfg(test)]` unit tests in
`src/`, no `tests/*.rs` harness, no doctests), and therefore deterministically
excludes **all 26 fsync-bound durability bins and every subprocess bin** —
which are precisely the bins that produce the 10-20 min. The residual fast
hook (toolchain + fmt + clippy + deny + `--lib`) carries **no** per-record
`sync_all` and **no** subprocess spawn. Prior observation (DISCUSS US-01
example 3) recorded the slimmed hook at "roughly 3-4 minutes even with clippy
`--all-targets` under heavy parallel load" — an observation, not a
DESIGN-session measurement.

**The <= 5 min bar is a DELIVER-confirmed measurement, owned by Apex
(platform-architect), gated by the US-01 timing AC.** Apex MUST `time` the
slimmed hook on real hardware when implementing it and record the seconds.
If the measured total exceeds 5 min, the trim order is fixed here (Decision
2): trim clippy to `--lib` first; the cut is already maximally aggressive on
the test side.

## Decision

### 1. Step 4 runs `cargo test --workspace --lib --locked` (D1; US-01, US-02)

The local hook's Step 4 changes from
`cargo test --workspace --all-targets --locked` to
**`cargo test --workspace --lib --locked`**.

- `--lib` runs every crate's in-`src/` `#[cfg(test)]` unit tests across all
  26 crates, and **only** those: no `tests/*.rs` integration binary, no
  durability/snapshot/torn-tail/crash bin, no subprocess bin, no doctest.
- This is the **simplest honest cut** (DISCUSS D1 recommendation (a)): it
  excludes the slow surface *deterministically by construction*, not by a
  fragile deny-list of binary names that drifts as new `tests/*.rs` land. A
  curated `--all-targets`-minus-slow set (D1 option (b)) was rejected (see
  Alternatives) precisely because it must be maintained against the 165-bin
  inventory and silently mis-classifies a new slow bin as fast.
- `--locked` is preserved (refuses to mutate `Cargo.lock`; honours the
  ADR-0003 pin locally as CI does).
- US-02 is satisfied because `--lib` *does* run unit tests: a broken
  `#[cfg(test)]` unit test still fails the hook and rejects the commit. The
  cut rules OUT the empty-subset failure mode (a hook that runs zero tests).

### 2. Clippy stays `--all-targets --locked` locally; trim to `--lib` only if measured over budget (D2)

Step 2 is **unchanged**: `cargo clippy --all-targets --locked -- -D
warnings`. Clippy is **compile-bound, not fsync-bound** — it type-checks the
test code, it does not *run* the durability suites, so it does not pay the
`sync_all` I/O cost that dominates Step 4. It is the high-value cheap gate
(it catches test-code lints too) and there is no structural reason it blows
the budget. **Trim order, fixed here for DELIVER:** if Apex's measured
fast-hook total exceeds 5 min, trim clippy to `cargo clippy --workspace
--lib --locked -- -D warnings` (drops test-target type-checking from the
local gate; CI's `--all-targets` clippy still covers it). This is the only
sanctioned trim and it is conditioned on a real measurement.

### 3. The CI-watch mechanism: `scripts/ci-watch.sh` + a documented cadence (D3)

A new shell script **`scripts/ci-watch.sh`** is the concrete, low-friction
CI-results watcher — the **safety net** for the deep tests now off the local
blocking path. Contract (DESIGN owns the contract; Apex owns the bash):

- Wraps the GitHub CLI (`gh`, already the project's CI-inspection tool):
  - `gh run list --branch main --limit N` to fetch the latest `main` runs;
  - for a failed run, `gh run view <id> --log-failed` to surface the failing
    job/step (so a `gate-1-test` red or a `gate-5-mutants` red is shown
    directly, not just "a run failed").
- Prints, at a glance: the latest `main` run's **conclusion**
  (`success` / `failure` / `in_progress`), the **run URL**, and — on
  failure — the failing job name + the `--log-failed` tail. It MUST surface
  **gate-1 (deep tests)** and **gate-5-mutants** failures clearly, because
  those are the two gates a slimmed local hook no longer pre-runs.
- Degrades honestly: if `gh` is absent or unauthenticated, it prints a clear
  "install/auth `gh`" message and exits non-zero, rather than silently
  reporting green.
- Is invoked, never auto-run: it is a courtesy command, consistent with the
  hook being a courtesy.

**Cadence (documented in CLAUDE.md `## CI watch` and the brief):** run
`scripts/ci-watch.sh` **after every push to main**, and **poll on a periodic
tick while an agent works a multi-slice task** (the agent's working session).
The cadence target is detection of a deep-only regression **within one
cadence interval** (same working session / < 1 hour), not days (US-04 KPI).
The cadence is the mitigation that makes Decision 1 honest (Decision 5); it
MUST be concrete, which is why it is a script + a written cadence, not a
hand-wave.

### 4. The CI deep gate is untouched; toolchain + fmt + deny stay local (D4; US-03)

`.github/workflows/ci.yml` `gate-1-test` keeps running
`cargo test --workspace --all-targets --locked` (ci.yml:182), **unchanged**.
No test file is deleted from any crate. CI is not weakened — it remains the
authoritative deep gate and is now the *single home* for deep gating.
Hook Steps 0 (toolchain), 1 (fmt), 3 (deny) are **unchanged** (all fast,
all high-value). Only Step 4's scope changes (Decision 1) and Step 2's scope
is conditionally trimmable (Decision 2).

### 5. The honesty trade, recorded citably (D5)

With the deep tests off the local blocking path, **a local commit CAN reach
`main` carrying a deep-only regression (a durability / snapshot / torn-tail /
crash / subprocess / integration break) that the fast local hook did not
run.** That regression is caught by **CI gate-1 (and gate-5-mutants), plus
the Decision-3 watch cadence**, not by a local block.

This is **acceptable under the trunk-based "CI is feedback, not a gate"
posture (project memory `project_kaleidoscope_pure_trunk_based`), PROVIDED
the cadence is real** — which Decision 3 makes it (a one-command script + a
written cadence). It is the same trade ADR-0070 accepted for the perf
signal: visible feedback over a local block that costs more than it is worth.
The difference from ADR-0070 is the failure class (correctness/durability
here, perf there) and the catch mechanism (CI gate-1 + cadence here;
non-gating `perf-kpis` job + human trend there). The fix-forward posture
(project memory `feedback_fix_forward_post_merge_correction`) is the
remediation path when the cadence surfaces a red.

### 6. Slow durability tests stay slow in CI — out of scope, flagged (D6)

This feature speeds the **local gate**, not the durability tests themselves.
The 26 fsync-bound bins remain I/O-bound *in CI* (they pay the honest
per-record `sync_all` of ADR-0049/0060 — that cost is the durability, not a
defect). A future feature could speed them (e.g. a faster test-fsync backend
or a batched-fsync test mode behind an env guard, mirroring the ADR-0058
guard pattern). **Flagged here as future work; explicitly NOT fixed in this
feature.** Successor working title: `faster-test-fsync-backend-v0`.

### 7. No crate change, no version bump, no public-API impact

This feature touches `scripts/hooks/pre-commit` (Step 4 scope; conditionally
Step 2), the new `scripts/ci-watch.sh`, and documentation
(`docs/`, CLAUDE.md `## CI watch`). **No `crates/*/src` source, no test body,
no `Cargo.toml` version bump.** Public-API / SemVer note: **none** — Gate 2
(`cargo public-api`) and Gate 3 (`cargo semver-checks`) see no surface
change. No crate is bumped; never 1.0.0 (CLAUDE.md; project memory
`semver_one_zero_is_andreas_call`).

## Reuse Analysis (MANDATORY)

| Capability needed | Existing asset | Verdict | Justification |
|---|---|---|---|
| Deterministically exclude every slow integration / durability / subprocess test bin from the local run | `cargo test`'s built-in `--lib` selector | **REUSE (cargo primitive)** | `--lib` runs only in-`src/` unit tests, never any of the 165 `tests/*.rs` bins. No bespoke deny-list, no `--exclude` enumeration to maintain. The cut is a one-flag change to the existing Step 4. |
| Still catch unit-test breaks locally | `--lib` runs every crate's `#[cfg(test)]` unit tests | **REUSE** | Same selector also satisfies US-02 (a broken unit test still reds the hook). One flag does both jobs. |
| Keep the deep gate enforced somewhere | CI `gate-1-test` (`cargo test --workspace --all-targets --locked`, ci.yml:182) — already runs the full deep suite on every push/PR | **REUSE (unchanged)** | The deep coverage already lives in CI. The local Step 4 was a slow duplicate. De-duplicating locally removes nothing CI does not already run (US-03). No CI edit. |
| Inspect CI run status from the terminal | The GitHub CLI `gh` (`gh run list`, `gh run view --log-failed`) | **REUSE (existing tool)** | `gh` is the project's existing CI-inspection path. `ci-watch.sh` is a thin wrapper, not a new mechanism. |
| The fast cheap gates (toolchain, fmt, deny) | Hook Steps 0/1/3 — already fast, already local | **REUSE (unchanged)** | No change. They stay exactly as today (D4). |
| Compile-bound lint gate | Hook Step 2 clippy `--all-targets` | **REUSE (unchanged; conditionally trimmable)** | Clippy is compile-bound not fsync-bound; kept as-is, trimmed to `--lib` only if Apex measures it over budget (D2). |
| Hook install / wiring | `scripts/hooks/install.sh` (sets `core.hooksPath`, chmods hooks) | **REUSE (unchanged)** | The slimmed hook rides the existing install path; no install change. (Its echoed summary "test" line stays accurate — still a test step, narrower scope.) |
| The honesty-trade + non-gating precedent framing | ADR-0070 (perf non-gating sibling) | **REUSE (precedent)** | This ADR mirrors ADR-0070's structure and trunk-based framing; it is the local-side sibling ADR-0070 explicitly left undone (§7). |
| A wrapper to summarise the latest main CI run + a cadence | none (no CI-watch script or documented cadence exists today) | **CREATE (new asset #1: `scripts/ci-watch.sh` + CLAUDE.md `## CI watch`)** | Justified: there is no existing low-friction watcher and no documented cadence. The local wait was the de-facto watch; removing it (Decision 1) requires an explicit replacement, or the honesty trade (Decision 5) is unbacked. The script is composed entirely of reused primitives (`gh`). |
| The local-fast / CI-deep ADR + honesty trade home | none (no ADR records this local-side posture; ADR-0070 covers perf only) | **CREATE (new asset #2: this ADR)** | The local Step 4 scope and the deep-regression honesty trade need a citable home, mirroring how ADR-0070 is the citable home for the perf trade. |

**Reuse verdict**: **REUSE** the `cargo --lib` primitive (the whole local
test-scope change is one existing flag), the existing CI `gate-1-test`
(unchanged), the `gh` CLI, the fast hook steps, and the `install.sh` wiring;
**EXTEND** `scripts/hooks/pre-commit` (Step 4 scope) and the docs; **CREATE**
only two new assets — `scripts/ci-watch.sh` (a thin `gh` wrapper) and this
ADR. No code, no test body, no crate version touched. Both new assets are
SHELL scripts / docs, so they are written by the **platform-architect (Apex)
in DELIVER**, NOT by the crafter (CLAUDE.md: the crafter writes only
`crates/<name>/src/`).

## Consequences

### Positive

- **The local commit gate finishes fast** (US-01). The 10-20 min Step 4
  becomes a unit-only run that pays no per-record `sync_all` and spawns no
  subprocess; the slimmed hook is `toolchain + fmt + clippy + deny + --lib`.
  Wall-clock to be confirmed <= 5 min by Apex's DELIVER measurement.
- **The cheap, common mistakes are still caught locally** (US-02): a broken
  unit test (via `--lib`), a fmt drift, a clippy lint each still red the hook
  and reject the commit. No regression in cheap-mistake detection.
- **The deep suite still gates, in CI** (US-03): `gate-1-test` is untouched;
  every durability / snapshot / torn-tail / crash / subprocess / integration
  test still runs on every push/PR. No test deleted; CI not weakened.
- **The deep coverage has eyes again** (US-04): `scripts/ci-watch.sh` + the
  documented cadence replace the lost local-wait signal with an explicit,
  one-command watch that surfaces gate-1 and gate-5-mutants reds.
- **The honesty trade is recorded once, citably** (D5), mirroring ADR-0070.
- **`--no-verify` gets less tempting** (side benefit): a fast hook is one a
  maintainer keeps using, which keeps ALL the cheap gates honoured.
- **Aligned with the trunk-based posture**: moving the slow gate's *gating*
  to CI (where it already runs) matches "CI is feedback, not a gate".

### Negative

- **A deep-only regression can land on `main` un-blocked** (Decision 5,
  accepted). It is caught by CI gate-1/gate-5 + the cadence, then
  fix-forwarded — not stopped at commit. This is the deliberate trade; it is
  honest only because the cadence (Decision 3) is concrete.
- **The cadence depends on a human/agent actually running it.** If the watch
  is ignored, a deep-only red lingers. Mitigated: it is one command, written
  into CLAUDE.md, and surfaces gate-1/gate-5 directly. (Risk also noted in
  DISCUSS risks.)
- **`--lib` slightly *under*-covers vs a curated fast-integration set**: a
  *fast, non-fsync* integration test (e.g. a pure query-API slice that does
  no `sync_all`) also stops running locally. Accepted: the simplest honest
  cut beats a drift-prone curated list; those fast integration tests still
  gate in CI. If a specific fast integration suite proves high-value to run
  locally, it can be added back explicitly in a successor — measured, not
  guessed.
- **The durability tests stay slow in CI** (Decision 6). Accepted and
  flagged as future work; out of scope here.

## Alternatives considered

### Keep Step 4 as the full `--all-targets` run (status quo) (rejected)

For: zero change; deep coverage blocks locally. Against: this IS the problem
— 10-20 min per commit, a wedged-for-hours incident, and the training toward
`--no-verify` that silently drops every gate. The deep run already duplicates
CI gate-1, so the local block buys nothing CI does not already enforce.
Rejected: it is the verified pain.

### Curated `--all-targets`-minus-slow set via deny-list / `--exclude` (D1 option b) (rejected)

For: keeps fast integration tests running locally. Against: it must be
maintained against a **165-binary inventory** that grows every feature; a new
slow `tests/*.rs` bin is silently mis-classified as fast and re-introduces
the 10-20 min creep, or a new fast bin is forgotten and never runs locally.
The exclusion list is exactly the kind of convention that erodes (cf.
principle 11). `--lib` is deterministic by construction and needs zero
maintenance. Rejected for fragility; recorded as a measured successor if a
specific fast suite earns a local slot.

### Delete the slow durability/subprocess tests (rejected, hard)

For: no slow run anywhere. Against: discards the entire durability acceptance
surface — the `sync_all`/torn-tail/crash coverage is the *point* of the
Earned-Trust durability lineage (ADR-0049/0060). A constraint of this feature
(US-03) is "no test is deleted; CI is not weakened". Rejected, hard —
de-gating locally is not deleting.

### Parallelism-only: keep `--all-targets` but raise `--test-threads` / `--jobs` (rejected)

For: no scope change; same coverage locally. Against: the slow suites are
**I/O-bound on per-record `sync_all`**, not CPU-bound — more threads contend
on the same disk and, under the prior leaked-process incident, *worsened* the
wedge. Parallelism does not move an fsync-bound wall-clock the way it moves a
CPU-bound one, and it does nothing about the subprocess spawns. It cannot
hit <= 5 min reliably and risks the wedge it is meant to cure. Rejected as
insufficient and risky.

### Faster test-fsync backend now (deferred, not rejected)

For: would let the durability tests run fast *everywhere*, restoring local
deep coverage cheaply. Against: it is a substantive change to the durability
test substrate (a guarded fast-fsync mode, env-gated like ADR-0058) that
deserves its own feature, threat-model, and probe — far larger than slimming
a hook. Deferred to a successor (`faster-test-fsync-backend-v0`, Decision 6),
not done here. This is the *right* long-term fix; this feature is the cheap,
immediate relief that does not block on it.

## Verification

- **Structural (the acceptance is structural — see the brief's "For
  Acceptance Designer" note)**: the hook's Step 4 reads `cargo test
  --workspace --lib --locked` (NOT `--all-targets`); `scripts/ci-watch.sh`
  exists and wraps `gh run list --branch main` + `gh run view --log-failed`;
  `.github/workflows/ci.yml` `gate-1-test` still reads `cargo test
  --workspace --all-targets --locked` (unchanged); no `crates/**/tests/*.rs`
  file is deleted (165-bin count unchanged).
- **Wall-clock (DELIVER, Apex)**: `time` the slimmed hook on real hardware;
  record fmt / clippy / deny / `--lib` seconds + total; confirm <= 5 min
  (US-01 timing AC). If over, apply the Decision-2 clippy trim and re-measure.
- **Cheap-mistake negative controls (US-02)**: inject a broken unit test, a
  fmt drift, and a clippy lint; each must red the slimmed hook and reject the
  commit.
- **Deep-only negative control (US-03)**: a deliberate durability/torn-tail
  break passes the fast local hook (commit created) but reds CI `gate-1-test`
  and is surfaced by `scripts/ci-watch.sh` — proving the slim-down moved the
  *catch location*, not the *coverage*.
- **No public-API impact**: Gate 2 / Gate 3 see no surface change; no crate
  version bump.

## External-integration handoff

None in the application sense (no third-party API, no consumer-driven
contract test). The one external dependency is the **GitHub Actions / `gh`
CLI** boundary that `scripts/ci-watch.sh` reads. Per Earned Trust, the script
MUST degrade honestly when `gh` is absent/unauthenticated or the network is
down (print a clear remediation message and exit non-zero — never report
green on an un-probed substrate). This is a probe responsibility on the
DELIVER implementation of the script, flagged to Apex.
