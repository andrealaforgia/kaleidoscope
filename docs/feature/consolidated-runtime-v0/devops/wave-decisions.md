# Wave Decisions — `consolidated-runtime-v0` (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-06-13. Autonomous overnight run.
> **Feature**: `consolidated-runtime-v0` — item C1 (the spine) of the consolidation roadmap.
> **Decision record**: **ADR-0076** (`docs/product/architecture/adr-0076-consolidated-runtime.md`).
> **DESIGN handoff**: `docs/feature/consolidated-runtime-v0/design/wave-decisions.md` (DD1-DD5) + brief `## Application Architecture — consolidated-runtime-v0`.
> **Outcome KPIs**: `docs/feature/consolidated-runtime-v0/discuss/outcome-kpis.md` (north-star = live-visibility; KPI 2 freshness p95 < 1 s).
> **Mode**: pre-code. nWave ORDER DISCUSS->DESIGN->**DEVOPS**->DISTILL->DELIVER — this wave runs BEFORE the crate's code exists (expected; the gate-5 --in-diff job is a harmless zero-second pass until the crate lands).

> ### ANDREA-VETO FLAG (carried forward verbatim from DISCUSS W1 — single point of reversal)
>
> Designed to the **single-process shared-`Arc<Store>`** model. A veto reshapes the MECHANISM to a distributed WAL-watch reload adapter (ADR-0076 A2); the user-visible outcome and every acceptance scenario stay identical. This DEVOPS CI + environment design is mechanism-agnostic at the wire level (same five ports, same freshness KPI, same fail-closed startup), so a veto does NOT invalidate it.

---

## Pre-decided decisions applied (from the orchestrator brief)

| # | Topic | Decision for C1 |
|---|-------|-----------------|
| D1 | Deployment target | **N/A** — C1 is a new in-workspace binary; deploy/compose is C2. |
| D2 | (deploy infra) | **N/A**. |
| D3 | CI platform | **GitHub Actions** (existing `.github/workflows/ci.yml`). |
| D4 | Infrastructure | **Existing infra extended/confirmed** — no new infra; one new gate-5 job added. |
| D5 | Observability | **The freshness (live-visibility) KPI** + the existing per-binary structured events. No new metric stack at C1; freshness is measured via the DISTILL/acceptance send->query latency; plus the fsync-in-lock watch-item measurement. |
| D6 | Deployment strategy | **N/A** (no deploy at C1). |
| D8 | Branching | **Trunk-based** (pure; CI is feedback, not a gate — `kaleidoscope_pure_trunk_based`). |
| D9 | Mutation strategy | **Per-feature, 100% kill** (already in CLAUDE.md / ADR-0005 Gate 5; not re-asked). |

---

## DEVOPS decisions

### A1 — CI coverage for the new crate: workspace gates auto-cover; ONE gate-5 job ADDED

**The workspace gates auto-cover `kaleidoscope-runtime` with NO edit:**

- **Gate 4** (`cargo deny --all-features check`, ci.yml:83-114) walks the WHOLE dependency graph — the new crate's deps are covered automatically.
- **Gate 1** (`cargo test --workspace --all-targets --locked`, ci.yml:136-182) is **workspace-wide**: it compiles and runs every target in the new crate (unit tests + the `tests/slice_*.rs` live-visibility acceptance suite) the moment the crate exists. `cargo fmt --check` and `cargo clippy --all-targets` are likewise workspace-wide. No per-crate wiring needed for test/lint/format coverage.
- **Gate 2** (`cargo public-api`) + **Gate 3** (`cargo semver-checks`) are enrolled for the FOUR graduated **library** packages only (otlp-conformance-harness, spark, sieve, codex). `kaleidoscope-runtime` is a **bin crate** with no consumer-facing library surface, so it is correctly **NOT enrolled** (do NOT add speculatively; never 1.0.0 — Andrea's call).

**The ONE thing not auto-covered is per-package mutation.** Gate 5 is split per-package with explicit `--package` + `--in-diff` jobs. So this wave **ADDED** one job:

> **`gate-5-mutants-kaleidoscope-runtime`** — added to `.github/workflows/ci.yml`, inserted after the last gate-5 job (`gate-5-mutants-sluice`), before the Prism gates comment block. It mirrors **`gate-5-mutants-kaleidoscope-gateway`** (the sibling composition-root binary) **EXACTLY**, changing only the package name and the diff path:
> - `--package kaleidoscope-runtime`
> - `--in-diff` on `git diff "$BASELINE" HEAD -- 'crates/kaleidoscope-runtime/**'`
> - baseline cascade `origin/main -> HEAD~1 -> full` (the established robust shape)
> - empty-diff short-circuit to a zero-second exit
> - `timeout-minutes: 30`
> - per-package cache namespace `${{ runner.os }}-cargo-mutants-kaleidoscope-runtime-...`
> - `mutants.out/` artefact upload, `retention-days: 30`
> - `needs: [gate-2-public-api, gate-3-semver]`
>
> **Why a new crate wants its own gate-5 job**: the new crate carries composition logic (the tenant-precedence resolution, the wire->probe->use bind/probe ORDERING, the fail-closed startup branch) — exactly the branch-bearing surface ADR-0076 Enforcement and ADR-0005 Gate 5 (100% kill) protect. **Harmless until the crate lands**: the crate does not exist at this commit, so the `--in-diff` diff is empty and the job is a trivial zero-second pass. `main.rs` may end up `#[mutants::skip]` like the other binaries; the composition module is then the mutation surface.
>
> **Pitfall avoided**: no workflow-level `${{ env.X }}` is referenced from the job-level env block (the new job has no job-level env at all — it mirrors the gateway job which uses none); the known GitHub-Actions context-ordering pitfall (`github_actions_job_level_env`) does not apply.

### A2 — Environment inventory: ONE process, five listeners, one root, one tenant

Full inventory in `devops/environments.yaml`. Summary:

- **The consolidated-runtime environment**: ONE OS process / ONE tokio runtime / ONE tracing subscriber, binding ingest gRPC **:4317** + ingest HTTP **:4318** + metrics query **:9090** + logs query **:9091** + traces query **:9092**. ONE `KALEIDOSCOPE_PILLAR_ROOT` (sub-dirs pulse/lumen/ray), sole-writer. ONE `KALEIDOSCOPE_TENANT` drives all four roles (per-role vars override). Auth off by default, never removed (optional ingest auth ADR-0068 + optional read auth ADR-0074 intact). Fail-closed wire->probe->use startup across all five listeners (`health.startup.refused` + non-zero exit on any bind/probe failure).
- **The minimal run command** (C1 deliverable — NOT the polished one-command product, which is C2):
  ```
  KALEIDOSCOPE_PILLAR_ROOT=/tmp/kaleidoscope-pillars KALEIDOSCOPE_TENANT=acme cargo run -p kaleidoscope-runtime
  ```
  (equivalently the built binary `./target/debug/kaleidoscope`). Boots all five ports over the shared `Arc<Store>`s; send OTLP to :4318/:4317, query back from :9090/:9091/:9092 with no restart.
- **The in-process acceptance environment**: the live-visibility loop runs IN ONE PROCESS (build the composition root in the test process, ingest, then query — no second process, no store drop/reopen). All five listeners bind **EPHEMERAL `127.0.0.1:0`** + sweep/retry, deliberately avoiding the fixed-port flake (`aperture_fixed_port_4317_flake`). The single-process ingest-then-query test is the load-bearing guard that the sink and router hold the SAME Arc.
- **Build/test matrix**: `clean` (local fast subset, ADR-0072), `with-pre-commit` (Gate 4 + fast `--lib` subset), `ci` (GitHub Actions, the deep `--all-targets` acceptance suite + Gate 4 + Gate 5). CI is feedback, not a gate.

### A3 — Observability: the freshness (live-visibility) KPI + the fsync-in-lock watch-item (D5)

**No new metric stack at C1.** The consolidated process surfaces the existing per-binary structured JSON-to-stderr events through one tracing subscriber (`query_http_common::init_tracing`; aperture's install no-ops). The freshness signal for v0 is **measured by the acceptance test** (the ingest-ack -> query-returns interval), per outcome-kpis.md ("for v0 the acceptance test is the measurement"). A runtime-emitted freshness metric is a C2/C3 concern once the run story + generator land.

**Freshness-KPI instrumentation plan:**

- **North star (KPI 1, live-visibility)**: fraction of send-then-query attempts where post-startup telemetry is returned with no restart. Target **100%**; baseline **0%**. Measured by the single-process ingest-then-query value assertion, per signal — **deterministic**, the contractual CI gate. If not 100%, the feature has not delivered.
- **Leading indicator (KPI 2, freshness latency)**: ingest-ack -> query-returns interval, **p95 < 1 s**. Measured by the same acceptance test, **timestamped** (wall-clock from the ingest 200-ack to the query returning the value). SLO-shaped **guardrail** the C2 run story must not regress; NOT a hard CI gate at C1. To avoid the `p95_wallclock_flakes_overnight` class, the latency assertion uses a generous local budget and treats CI as the indicative measure; threshold-raising is never the flake fix.
- **Signal coverage (KPI 3)**: 1/3 after Slice 1 (metrics), 3/3 after Slice 2 (logs + traces); one-command startup = all five ports on one process.
- **Guardrails (must NOT degrade)**: cross-tenant leaks = 0 (KPI 4, CRITICAL on any leak); read-auth stays fail-closed when configured; per-record fsync durability unchanged; no port-bind conflicts (all five bind); ingest accept rate not reduced versus the standalone gateway.

**The fsync-in-lock watch-item — MEASURED, not assumed (the one MEDIUM watch-item carried from DESIGN):**

The per-record fsync runs INSIDE the ingest write lock (`append_wal -> fsync_file`, `pulse/src/file_backed.rs:325,515`). A query concurrent with a heavy fsync-heavy ingest batch BLOCKS until that batch's fsync completes and the lock releases. This is a **latency characteristic, NOT a correctness issue**, and needs NO store change for C1 — it does not threaten live-visibility, tenant isolation, or durability.

- **Why measure, not pre-optimise**: a non-issue for the local single-experimenter workload + the p95 < 1 s target; under sustained high-throughput ingest a read could see added latency.
- **Where to measure**: the freshness KPI test (KPI 2) is the natural home — add a variant that issues a query CONCURRENT with an fsync-heavy ingest batch and records the read latency. Report the measured number. Only if it breaches the p95 < 1 s budget under a realistic local load does optimisation (fsync outside the lock, batched fsync) become a SEPARATE item.
- **Owner**: DELIVER (write the measurement variant) + DEVOPS (track the number against the budget). **Do NOT pre-optimise the store in C1.**

### A4 — The C1 / C2 boundary (honest scope)

**C1 ships**: the new `kaleidoscope-runtime` crate (bin `kaleidoscope`), its CI coverage (workspace gates + the new `gate-5-mutants-kaleidoscope-runtime` job), the environment inventory, the minimal run command, and the freshness-KPI instrumentation design (measured by the acceptance test for v0).

**C1 does NOT ship** (these are C2/C3/C4): the full one-command compose/run story (docker-compose or a friendly launcher wrapping the binary), the getting-started doc, the load generator, and any runtime-emitted freshness metric/dashboard. This wave records the minimal run command only — it does NOT build the compose.

### A5 — Rollback

Trunk-based `git revert`. The feature is additive (one bin crate; the four existing binaries' sources are untouched, DD5) with no on-disk format change (pulse/lumen/ray untouched), so a revert needs no data migration and leaves the standalone binaries as the supported run path, exactly as before C1. Prefer fix-forward for any post-merge defect on this closed wave (`feedback_fix_forward_post_merge_correction`).

---

## CI Contract (what changed in this wave)

| Gate | Covers the new crate? | How |
|------|-----------------------|-----|
| Gate 4 — cargo deny | **Auto** | Workspace-wide dep-graph walk. |
| Gate 1 — cargo test `--workspace --all-targets --locked` | **Auto** | Workspace-wide; runs the new crate's unit + acceptance suite once it exists. |
| fmt / clippy `--all-targets` | **Auto** | Workspace-wide. |
| Gate 2 — cargo public-api | **N/A (correctly not enrolled)** | Bin crate, no consumer-facing library surface; graduated-library-only. |
| Gate 3 — cargo semver-checks | **N/A (correctly not enrolled)** | Same. |
| Gate 5 — cargo mutants (per-package) | **NEW job added** | `gate-5-mutants-kaleidoscope-runtime`, `--package kaleidoscope-runtime --in-diff 'crates/kaleidoscope-runtime/**'`, mirrors gate-5-mutants-kaleidoscope-gateway exactly. 100% kill. |

The new gate-5 job is a zero-second pass until the crate lands (empty `--in-diff`). The fixed-port flake is avoided by ephemeral binds in the acceptance suite.

---

## Self-review (no nested reviewer invoked this run — recorded verdict against the platform-architect critique dimensions)

| Dimension | Verdict | Note |
|-----------|---------|------|
| CI coverage incl mutation for the new crate | **PASS** | Workspace gates auto-cover (test/deny/fmt/clippy); one gate-5 job added for per-package 100%-kill mutation, mirroring the sibling composition-root binary job exactly. |
| Environment inventory complete | **PASS** | One process / five listeners / one root / one tenant; minimal run command; in-process ephemeral-port acceptance environment; build/test matrix. |
| Observability / freshness-KPI aligned to outcome-kpis.md | **PASS** | North-star live-visibility (deterministic value assertion) + KPI 2 freshness p95 < 1 s (timestamped, SLO-shaped guardrail) + guardrails; no new metric stack (acceptance test is the v0 measurement, per outcome-kpis.md). |
| fsync-in-lock = measured, not assumed | **PASS** | Carried as the one MEDIUM watch-item; concrete measurement home (the KPI-2 test, concurrent-fsync variant); owner named; do-not-pre-optimise stated. |
| C1 / C2 boundary honest | **PASS** | C1 = binary + CI + inventory + minimal command + KPI design; C2 = the compose/run story, explicitly NOT built here. |
| No overstated readiness | **PASS** | Pre-code wave; the gate-5 job is honestly a zero-second pass until the crate lands; no claim that the feature is tested before DELIVER writes it. |
| Pitfall: job-level env literal | **PASS** | The new job references no workflow-level `${{ env.X }}` (no job-level env block at all); the known context-ordering pitfall does not apply. |
| Rollback designed | **PASS** | Trunk-based revert; additive, no migration; fix-forward preferred. |

**Approval (self-review): approved.** Critical issues: 0. High issues: 0. One MEDIUM watch-item (fsync-in-lock read latency) owned by DELIVER+DEVOPS as a measure-don't-pre-optimise item.

---

## Handoff

- **DISTILL (`acceptance-designer`)**: bind ephemeral ports + sweep/retry; the single-process ingest-then-query test is the core live-visibility guard; instrument the ingest-ack -> query-returns interval (KPI 2) timestamped in the same test; add a concurrent-fsync-heavy variant to measure the DD2 watch-item.
- **DELIVER (`nw-software-crafter`)**: build `crates/kaleidoscope-runtime` (bin `kaleidoscope`) per ADR-0076 DD1-DD5; the new `gate-5-mutants-kaleidoscope-runtime` job activates automatically once the crate lands (100% kill); main.rs may be `#[mutants::skip]`; measure the fsync/lock latency rather than pre-optimise.
- **Operations / C2**: the minimal run command in `environments.yaml` is the C1 entry; the full one-command compose/run story is the C2 roadmap item.
