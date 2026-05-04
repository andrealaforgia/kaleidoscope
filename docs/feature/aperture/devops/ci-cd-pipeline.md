# CI/CD Pipeline — `aperture` v0 (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-04.
> **Author**: Apex.
> **Workflow file**: `.github/workflows/ci.yml` (extended, not forked).
> **Companion documents**: `wave-decisions.md`, `platform-architecture.md`,
> `kpi-instrumentation.md`, `branching-strategy.md`,
> `environments.yaml`.

---

## Framing

Aperture is a service. The harness's DEVOPS framing
("library, not service, no deployment target") only partially
transfers: Aperture HAS a deployment target, but Kaleidoscope does
not host it; the operator does. So the "CD" half of CI/CD remains
collapsed, like the harness's, but for a different reason —
**Kaleidoscope's job ends at "the binary builds, the tests pass, the
contract is honoured"**; the operator's job is "deploy the binary
under your orchestrator". The pipeline below is therefore still a CI
pipeline; the deployment-strategy guidance lives in
`platform-architecture.md > Aperture-specific deployment-strategy
guidance`.

The platform on which the pipeline runs is **GitHub Actions** —
inherited verbatim from the harness's DEVOPS wave. The single
workflow file at `.github/workflows/ci.yml` is extended, not forked,
per `wave-decisions.md > A1`.

---

## The five existing gates and their evolution

The harness's ADR-0005 gates apply to Aperture mechanically once
Aperture is in the workspace. DISTILL has already added the crate to
the workspace; this wave plans how each gate's *scope* evolves
through the DELIVER cycle.

### Schedule

| Gate | At DEVOPS-wave close (today) | After DELIVER closes (single edit per gate) |
|---|---|---|
| **Gate 4** — `cargo deny check` | Workspace-wide; covers Aperture transitively today (after the version-pin fix in `crates/aperture/Cargo.toml` per `wave-decisions.md > A3`) | Unchanged |
| **Gate 1** — `cargo test` | `-p otlp-conformance-harness --all-targets --locked` | `--workspace --all-targets --locked` |
| **Gate 2** — `cargo public-api` | `-p otlp-conformance-harness` | Add `-p aperture` (Aperture's library surface is `aperture::testing` only — DESIGN ADR-0007 + DISTILL D2; binary surface is empty) |
| **Gate 3** — `cargo semver-checks` | `-p otlp-conformance-harness` | Add `--package aperture` |
| **Gate 5** — `cargo mutants` | `--package otlp-conformance-harness --no-shuffle --jobs 2` | Add `--package aperture` (cargo-mutants supports multiple `--package`) |

### Why the staged graduation, not a single big-bang switch

Aperture's tests are RED at DISTILL completion (84 active tests + 1
`#[ignore]`d, every active one panics on a `unimplemented!()`
production-surface symbol — DISTILL D7). Adding Aperture to Gate 1
today turns `main` red for every commit until DELIVER's last slice
lands. The graduation pattern is the project's adopted way of
managing RED-on-day-one scaffolds; the harness used the same
discipline (slice tests landed RED in DESIGN/DISTILL, the gates went
green slice by slice through DELIVER).

### How the graduation lands

DELIVER's final commit (after the eighth slice goes green plus the
two invariant tests are wired by their respective slices) makes four
lockstep edits in one commit:

1. `.github/workflows/ci.yml` — Gate 1 invocation:
   `-p otlp-conformance-harness --all-targets --locked` →
   `--workspace --all-targets --locked`.

2. `.github/workflows/ci.yml` — Gate 2 invocation:
   add `-p aperture` (or run `cargo public-api` workspace-wide).

3. `.github/workflows/ci.yml` — Gate 3 invocation:
   add `--package aperture`.

4. `.github/workflows/ci.yml` — Gate 5 invocation:
   `--package otlp-conformance-harness` →
   `--package otlp-conformance-harness --package aperture`.

5. `scripts/hooks/pre-commit` — remove `--exclude aperture` from
   the `cargo test --workspace --exclude aperture --all-targets
   --locked` invocation.

6. `scripts/hooks/pre-push` — extend the `cargo public-api` and
   `cargo semver-checks` invocations to include `-p aperture`.

The local pre-commit-hook edit (5) and the local pre-push-hook edit
(6) keep local and CI symmetric per the `cicd-and-deployment`
skill's "mirror, not duplicate" principle.

---

## Three new Aperture-specific gates (future-wired)

Per `wave-decisions.md > A5`, the three Aperture-specific CI gates
DESIGN named are documented here as future DELIVER work. **They are
not wired into `ci.yml` at this wave's close.** The wiring schedule:

| Gate | DELIVER slice that wires it | Why that slice |
|---|---|---|
| `gate-6-aperture-architectural-rules` (xtask AST walks for `single_validator_per_signal` + dependency direction + no `prost::Message::decode` in aperture/src — per DESIGN D10's table) | Slice 03 (traces) | The third validator (`validate_traces`) becomes the third call site that needs counting. The xtask binary lands at Slice 03; the gate goes blocking when the count is non-trivial. |
| `gate-7-aperture-no-telemetry` (network-namespace integration test at `tests/no_telemetry_on_telemetry.rs`) | Slice 06 (forwarding sink) | The net-ns fixture cannot be exercised meaningfully before ForwardingSink lands (the only allowed outbound traffic). Linux-only; macOS skips with a clear message. |
| `gate-8-aperture-probe-gold` (behavioural-layer probe gold-test at `tests/probe_gold_runner.rs`) | Slice 06 | `Probe` and `wire_then_probe_then_use` land at Slice 06; the gold-test asserts startup refusal against a `wiremock` fixture that lies. |

### Job shapes (sketch only; DELIVER finalises)

```yaml
# Future, not in ci.yml today. Lands at Slice 03 commit.
gate-6-aperture-architectural-rules:
  name: Gate 6 — Aperture architectural rules (xtask AST walks)
  runs-on: ubuntu-latest
  needs: gate-1-test
  steps:
    - uses: actions/checkout@<sha>
    - uses: dtolnay/rust-toolchain@<sha>
      with:
        toolchain: stable
    - uses: actions/cache@<sha>
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-stable-${{ hashFiles('**/Cargo.lock') }}
    - name: cargo run -p xtask -- check-architecture-rules
      run: cargo run -p xtask --release -- check-architecture-rules
```

```yaml
# Future, not in ci.yml today. Lands at Slice 06 commit.
gate-7-aperture-no-telemetry:
  name: Gate 7 — Aperture no telemetry on telemetry (Linux only)
  runs-on: ubuntu-latest
  needs: gate-1-test
  steps:
    - uses: actions/checkout@<sha>
    - uses: dtolnay/rust-toolchain@<sha>
      with:
        toolchain: stable
    - uses: actions/cache@<sha>
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-stable-${{ hashFiles('**/Cargo.lock') }}
    - name: cargo test --test no_telemetry_on_telemetry
      run: cargo test -p aperture --test no_telemetry_on_telemetry --locked
```

```yaml
# Future, not in ci.yml today. Lands at Slice 06 commit.
gate-8-aperture-probe-gold:
  name: Gate 8 — Aperture probe gold-test (behavioural Earned-Trust)
  runs-on: ubuntu-latest
  needs: gate-1-test
  steps:
    - uses: actions/checkout@<sha>
    - uses: dtolnay/rust-toolchain@<sha>
      with:
        toolchain: stable
    - uses: actions/cache@<sha>
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-stable-${{ hashFiles('**/Cargo.lock') }}
    - name: cargo test --test probe_gold_runner
      run: cargo test -p aperture --test probe_gold_runner --locked
```

### Why not wire all three with `if: false` placeholders today?

Per `wave-decisions.md > A5` alternative (a): workflow YAML readability
suffers; `if: false` jobs still consume GitHub's job-graph quota; and
the visual noise during the DELIVER cycle (8+ commits over several
working sessions) outweighs the saved typing of fresh job-add commits
later. The chosen path — name the contract here, wire the job at the
slice that delivers the underlying artefact — is the cleaner pattern.

### Naming conflict note

Gates 6, 7, 8 are numbered to extend the harness's 1–5 sequence. They
are Aperture-specific, not workspace-wide; future Kaleidoscope crates
(Codex, Spark, etc.) may reuse the numbers within their own scope, or
adopt fresh numbers depending on the convention of the day. For now,
6 / 7 / 8 minimise renumbering risk against the harness's existing
1–5.

---

## Local pre-commit hook graduation

The local pre-commit hook at `scripts/hooks/pre-commit` invokes:

```bash
cargo test --workspace --exclude aperture --all-targets --locked
```

The `--exclude aperture` is provisional. Graduation:

| Phase | Local hook invocation | CI Gate 1 invocation |
|---|---|---|
| DEVOPS close (today) | `cargo test --workspace --exclude aperture --all-targets --locked` | `cargo test -p otlp-conformance-harness --all-targets --locked` |
| DELIVER cycle (slices 1–7 landing) | unchanged | unchanged |
| DELIVER close (last commit, all slices green + invariants wired) | `cargo test --workspace --all-targets --locked` | `cargo test --workspace --all-targets --locked` |

The two invocations are then identical, satisfying "mirror, not
duplicate". This is the same pattern the harness DEVOPS established
when it added the hooks post-review.

### Considered alternative graduation strategies

The brief named three options for Gate 1 evolution and asked DEVOPS
to pick. They map to the local hook the same way:

- **Option (a)**: switch to `--workspace --all-targets --locked` once
  all aperture tests are green. **Recommended.** Clean; matches the
  long-term shape; one-line edit at DELIVER close.
- **Option (b)**: per-package list with explicit `-p` flags that
  grows commit-by-commit. **Rejected** — invites drift; every
  DELIVER commit becomes a hook YAML edit.
- **Option (c)**: marker convention (`#[ignore]` on RED tests until
  DELIVER lands them). **Rejected** — contradicts DISTILL's RED-on-
  day-one strategy (D7); unmarking is per-test edits DISTILL
  deliberately avoided.

This wave honours option (a). DELIVER's last commit performs the
single-line edit at both ends.

---

## Trigger rules (unchanged)

```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

permissions:
  contents: read
```

Inherited verbatim from the harness's DEVOPS A1, A2, A3. No
Aperture-specific change.

---

## Toolchain matrix (unchanged)

`ubuntu-latest` only, with stable Rust 1.85 (per `rust-toolchain.toml`)
for Gates 1, 4, 5; pinned nightly (`NIGHTLY_PIN`, currently
`nightly-2026-04-15`) for Gates 2, 3.

The future Aperture-specific Gates 6, 7, 8 use stable 1.85. Gate 7
is `#[cfg(target_os = "linux")]`; on macOS or Windows runners the
test would compile to `fn main() { eprintln!("skip: net-ns is
Linux-only"); }`. Since the CI matrix is Linux-only at v0, the skip
case never fires in CI.

---

## Cache strategy (extends naturally)

Three cache namespaces, inherited from the harness:

- `cargo-stable-` (Gates 1 and 5; will cover Gates 6, 7, 8 when
  wired)
- `cargo-nightly-` (Gates 2 and 3)
- `cargo-stable-mutants-` (Gate 5 only, with fallback to
  `cargo-stable-`)

Aperture's larger dependency tree (tonic + axum + hyper + tower +
reqwest + tracing-subscriber + figment + ...) increases the
build-cache size compared with the harness alone. The cache key
hashes `**/Cargo.lock`, so any addition to the workspace's
dependency tree invalidates the cache cleanly.

Expected cache-miss build time (cold):
- Gate 1 (workspace once Aperture lands): 3–6 minutes (estimate
  based on the dep tree size; harness alone was 1–3 min).
- Gate 5 (mutation testing): proportional to test-suite size; the
  harness alone was ~45 s for 39 mutants; Aperture adds a comparable
  test footprint, so ~90 s expected post-graduation. Still well under
  the 60 s mutation-test threshold-per-feature (which is per-feature,
  not per-workspace; harness and aperture are separate `--package`
  invocations whose times sum but each individually stays under the
  threshold).

---

## Mutation-test budget rule (carried forward)

From the harness's DEVOPS wave; applies per-package to Aperture's
mutation run:

> **Threshold**: 60 seconds wall-clock for any single feature's
> full mutation run.
> **Switch-over rule**: when a single feature's full mutation run
> exceeds 60 seconds wall-clock for two consecutive merges to `main`,
> change Gate 5's invocation for that feature to add `--in-diff
> origin/main`.

For Aperture, the multi-package invocation `cargo mutants --package
otlp-conformance-harness --package aperture --no-shuffle --jobs 2`
runs both packages serialised in cargo-mutants's own ordering. The
threshold applies per-package: if Aperture's mutation run exceeds
60 s wall-clock for two consecutive merges, the maintainer changes
the invocation to `--package otlp-conformance-harness --package
aperture --no-shuffle --jobs 2 --in-diff origin/main` (the
`--in-diff` flag applies to both packages naturally; cargo-mutants
restricts mutations to changed source files in either crate).

The `timeout-minutes: 30` ceiling on `gate-5-mutants` is the
runaway-safety net regardless.

---

## Quality gate classification

Per the `cicd-and-deployment` skill's gate taxonomy, Aperture's
pipeline classifications:

| Category | Stage | Gate | Type |
|---|---|---|---|
| Local | Pre-commit | `cargo fmt --check` | Blocking (developer) |
| Local | Pre-commit | `cargo clippy --all-targets --locked -- -D warnings` | Blocking (developer) |
| Local | Pre-commit | `cargo deny --all-features check` | Blocking (developer) |
| Local | Pre-commit | `cargo test --workspace [--exclude aperture] --all-targets --locked` | Blocking (developer) |
| Local | Pre-push | `cargo public-api` (Gate 2 mirror) | Blocking (developer) |
| Local | Pre-push | `cargo semver-checks` (Gate 3 mirror) | Blocking (developer) |
| PR | Pull request | All five (eventually eight) gates as advisory checks (branch-protection is relaxed; checks run but do not block) | Advisory (per harness DEVOPS post-merge correction "branch protection relaxed to pure trunk-based") |
| CI | Commit stage | Gate 4 (cargo deny) — workspace | Blocking (pipeline; trunk-based discipline) |
| CI | Commit stage | Gate 1 (cargo test) — `-p` graduates per A2 | Blocking (pipeline) |
| CI | Commit stage | Gate 2 (cargo public-api) — `-p` graduates per A2 | Blocking (pipeline) |
| CI | Commit stage | Gate 3 (cargo semver-checks) — `-p` graduates per A2 | Blocking (pipeline) |
| CI | Commit stage | Gate 5 (cargo mutants) — `--package` graduates per A4 | Blocking (pipeline) |
| CI | Commit stage (future) | Gate 6 (Aperture architectural rules) — wired at Slice 03 | Blocking (pipeline) |
| CI | Commit stage (future) | Gate 7 (Aperture no-telemetry, Linux) — wired at Slice 06 | Blocking (pipeline) |
| CI | Commit stage (future) | Gate 8 (Aperture probe gold-test) — wired at Slice 06 | Blocking (pipeline) |
| Capacity | Release cadence (deferred) | KPI 5 1-hour load test (deferred to release wave per A8/Q5) | Blocking (release pipeline; future) |
| Capacity | Release cadence (deferred) | KPI 8 1000-restart load test (same) | Blocking (release pipeline; future) |
| Deploy | n/a | n/a | n/a (operator-side) |
| Production | n/a | n/a | n/a (operator-side) |

There is still no acceptance / production-deploy stage on the
Kaleidoscope side; the operator owns those (see
`platform-architecture.md > What Kaleidoscope does *not* ship`).

The "advisory at PR" classification is technically correct under the
post-merge correction the harness DEVOPS wave applied: branch
protection is now linear-history + no-force-push only, with no
required-status-checks and no enforce-admins. The trunk-based
discipline ("main is socially always green" via fast feedback +
fix-forward) supplies the social pressure CI's status-check
enforcement would otherwise supply.

---

## Pipeline security (extends the harness's table)

| Stage | Check | Tool | Status for Aperture |
|---|---|---|---|
| Commit (CI) | Licence policy + advisory database | `cargo deny check` (Gate 4) | Workspace-wide; extended to Aperture today (after A3 fix) |
| Commit (CI) | Pin policy (`opentelemetry-proto = "=0.27.0"`) | `cargo deny check` `bans` table (Gate 4) | Same; aperture pins the same version |
| Commit (CI) | Yanked-versions ban | `cargo deny check` `advisories` table (Gate 4) | Same |
| Commit (CI) | Wildcard-version ban | `cargo deny check` `bans.wildcards` (Gate 4) | Aperture's sibling-crate path-dep is fixed in A3 to satisfy this rule |
| Commit (CI) | Public-API surface drift | `cargo public-api` (Gate 2) | Graduates to cover Aperture per A2 |
| Commit (CI) | SemVer-correctness | `cargo-semver-checks` (Gate 3) | Same |
| Commit (CI) | Test-quality (mutation) | `cargo mutants` (Gate 5) | Graduates per A4 |
| Commit (CI, future) | Architectural-rule enforcement | xtask AST walks (Gate 6) | Wired at Slice 03 |
| Commit (CI, future) | No telemetry on telemetry | net-ns integration test (Gate 7) | Wired at Slice 06 |
| Commit (CI, future) | Probe contract enforcement | wiremock-driven gold-test (Gate 8) | Wired at Slice 06 |
| Workflow | Action version pinning | Third-party actions pinned to commit SHAs in `ci.yml` | Inherited; no new third-party actions in this wave |
| Workflow | Token scope | `permissions: read` at workflow level | Inherited |
| Workflow | Secrets | None declared; none used | Inherited |

No SAST tool is configured because Aperture's surface is the same
shape as the harness's plus a plain-Tokio runtime: no `eval`, no
shell-out, no FFI. The workspace `[lints]` declares `unsafe_code =
"forbid"` (added at DESIGN per `workspace-layout.md`), structurally
preventing memory-safety regressions through unsafe blocks.

No DAST tool is configured because there is no Kaleidoscope-side
runtime (a developer can run `cargo run -p aperture` locally for
exploratory testing; that is not the production runtime). When pilot
operators run Aperture in their environments, network-level DAST is
the operator's responsibility.

No SBOM tool is configured beyond `cargo deny`'s licence enumeration;
the SBOM equivalent for a Rust workspace is `Cargo.lock`, which is
committed and exact-pinned. If a downstream consumer (or pilot
operator) needs a CycloneDX or SPDX export, `cargo cyclonedx` or
`cargo sbom` can be run on demand against the locked dependency tree.
A future Aegis-phase work item may add an automated SBOM artefact to
the release workflow when one exists.

---

## Trunk-based development discipline (carried forward)

Per the harness DEVOPS post-merge correction "branch protection
relaxed to pure trunk-based":

- Every commit on `main` triggers the full pipeline.
- Every PR runs the same gates before review (advisory, not
  blocking).
- Branch protection: linear history, no force-push, no deletions.
  No required-status-checks; no enforce-admins.
- The discipline is "main is socially always green" via fast local
  feedback (the local hooks) + fix-forward speed (any red commit
  produces an immediate fix commit).

Aperture inherits this discipline unchanged. The `wave-decisions.md
> A6` documents that the local pre-commit hook keeps Aperture-
excluded during DELIVER and graduates at DELIVER close, keeping the
local-vs-CI symmetry intact throughout.

---

## DORA-metric posture

Carried forward from the harness's DEVOPS wave; same Elite-band
posture for the Kaleidoscope-side metrics. See `platform-architecture.md
> DORA metrics posture` for the full split (Kaleidoscope-side vs
operator-side).

---

## Rejected simple alternatives

Per the skill's "simplest solution check" requirement.

### Alternative 1 — Fork into `aperture-ci.yml`, leave `ci.yml` for harness

- **What**: a separate workflow file for Aperture's gates.
- **Expected impact**: visual separation of concerns between the
  two crates' CI.
- **Why insufficient**: duplicates toolchain bootstrap (40+ lines of
  boilerplate per workflow); splits CI signal across files (a
  contributor checking "did all my gates pass?" must inspect two
  workflow runs); complicates branch protection if it ever returns
  (two sets of status-check names to keep in sync). The harness's
  ADR-0005 gates are workspace-level concerns, not crate-level; one
  workflow extending them is the correct shape.

**Rejected.**

### Alternative 2 — Add all three Aperture-specific gates today as `if: false` placeholders

- **What**: pre-create `gate-6-aperture-architectural-rules`,
  `gate-7-aperture-no-telemetry`, `gate-8-aperture-probe-gold` jobs
  with `if: false` on every step, ready to be activated by DELIVER's
  per-slice commits.
- **Expected impact**: future graduation reduces to flipping
  `if: false` to `if: true`; the YAML structure stays stable
  throughout the DELIVER cycle.
- **Why insufficient**: workflow YAML readability suffers from three
  always-skipping jobs cluttering the run summary; `if: false` jobs
  consume GitHub job-graph quota even when they no-op; and the
  graduation cost (a fresh job-add per slice) is essentially the
  same as removing `if: false` (both are equivalent diffs). Naming
  the contract in this document and wiring the job at the slice that
  delivers it is the cleaner pattern.

**Rejected.**

### Alternative 3 — Add Aperture to Gate 1 today, accept red `main` for the entire DELIVER cycle

- **What**: change Gate 1 to `cargo test --workspace --all-targets
  --locked` immediately; the 84 RED Aperture tests fail until DELIVER
  lands them.
- **Expected impact**: trivially the long-term shape of Gate 1; no
  graduation step needed.
- **Why insufficient**: trunk-based development requires `main` to be
  releasable — "main is socially always green" per the harness's
  post-merge correction. Knowingly running a red CI for an entire
  DELIVER cycle (multiple days of working sessions, eight slices)
  trains contributors to ignore CI, defeats fix-forward speed
  (every commit's CI is red regardless of whether the commit's
  intent was to land green tests), and pollutes the GitHub Actions
  history with 8+ red runs. The graduation pattern (RED on day one
  via DISTILL's scaffold; CI scoped to harness during DELIVER;
  graduation at DELIVER close) is the project's adopted discipline
  for the same reason DISTILL chose `unimplemented!()` panics over
  `#[ignore]`.

**Rejected.**

### Alternative 4 — Skip `cargo deny` graduation; manually allow Aperture's path-dep wildcard

- **What**: leave `crates/aperture/Cargo.toml`'s sibling-crate path-
  dep without an explicit `version = "0.1.0"`; relax `deny.toml`'s
  `bans.wildcards = "deny"` to `"warn"`.
- **Expected impact**: zero file edits in `crates/aperture/`; the
  cargo-deny gate becomes informational on the wildcard rule.
- **Why insufficient**: the wildcards check is the load-bearing
  defence against accidental `version = "*"` pins on registry deps,
  not just on path deps. Relaxing it to `"warn"` retreats from the
  protection it gives across the whole workspace; the cost (one line
  added to one Cargo.toml) is trivial compared with the regression
  surface. The version-pin pattern (path + version) is the
  canonical Rust idiom anyway; this is a tiny correctness fix, not
  a configuration loosening.

**Rejected.**

### Alternative 5 — Bring the KPI 5 / KPI 8 load tests into per-commit CI today

- **What**: wire the 1-hour load test (KPI 5) and the 1000-restart
  load test (KPI 8) into `ci.yml` as new gates that run on every
  commit.
- **Expected impact**: KPIs 5 and 8 measured continuously; no
  release-wave deferral.
- **Why insufficient**: per `outcome-kpis.md`, KPI 8's budget is
  5–15 minutes of runner wall-clock; KPI 5's is 1 hour (or
  cadence-compressed to several minutes). Per-commit CI minutes is a
  scarce resource; the load tests are stable across commits except
  when the load-bearing code changes (Slice 05 backpressure logic,
  Slice 08 drain orchestrator). Running them every commit is
  expensive; running them every release is appropriate. The
  release-wave deferral (per `wave-decisions.md > A8/Q5`) is the
  correct cadence.

**Rejected.**

The chosen design (single workflow, staged graduation through
DELIVER, three new gates wired per the slice that delivers each,
release-cadence load tests deferred) is the minimum that honours
ADR-0005's contract, DESIGN's three new gates, the project's
trunk-based discipline, and the budget reality of GitHub Actions
minutes.

---

## Summary

The Aperture pipeline is a thin extension of the harness's
five-gate CI contract:

- **Five existing gates** (Gates 1–5) extend their scope to Aperture
  in lockstep at DELIVER close. Today they remain harness-scoped
  (with one tiny `Cargo.toml` repair landing now to keep Gate 4
  green workspace-wide).
- **Three new gates** (Gates 6, 7, 8) for Aperture-specific
  invariants (architectural rules, no-telemetry-on-telemetry, probe
  gold-test) are documented here for DELIVER's per-slice wiring.
- **One single workflow file** (`ci.yml`), inherited and extended.
- **Local hooks** stay in lockstep via the same DELIVER-final-commit
  graduation.
- **Two release-cadence load tests** (KPI 5, KPI 8) are deferred to
  a future release wave that does not yet exist.
- **No new tools, no new third-party actions, no new toolchain
  requirements** at this wave's close.

Every choice traces to an upstream artefact: the harness's DEVOPS
precedent, ADR-0005, ADR-0006 / 0009 / 0010, DISCUSS Q4 / Q6 / D1 /
D7 / D8, DESIGN's D10 architectural-rule enforcement, DISTILL's D7
RED-scaffold strategy. No new architectural ground; the wiring is
mechanical.
</content>
</invoke>
