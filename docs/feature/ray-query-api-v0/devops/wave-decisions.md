# Wave Decisions — `ray-query-api-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-23
- **Mode**: propose. British English. No em dashes.

This wave receives ADR-0048 / the DESIGN wave-decisions DEVOPS Handoff
Annotation and records the platform/delivery decisions for the NEW crate
`crates/trace-query-api` (lib + thin binary), the HTTP read path for
traces. The substantive decisions are: a new Gate 5 mutation job, a new
per-crate graduation tag, the Gate 4 (cargo deny) verdict, the Gate 2/3
scope, and the slice-01 environment shape (no Docker yet). This is the
THIRD HTTP read-API DEVOPS wave in the workspace; lumen-query-api-v0 is
the direct precedent and is followed in shape.

## Inputs read (in dependency order)

1. `CLAUDE.md` — Rust idiomatic paradigm; per-feature mutation testing,
   100% kill rate (declared, not modified here).
2. `docs/feature/ray-query-api-v0/discuss/outcome-kpis.md` — KPI 1
   (north star: in-window spans returned), KPI 2 (field fidelity 100%),
   KPI 3 (p95 <= 500 ms on ubuntu-latest, <= 1000 spans), KPI 4
   (guardrail: fail-closed, zero cross-tenant leak).
3. `docs/feature/ray-query-api-v0/design/wave-decisions.md` — Morgan's
   DESIGN decisions and the explicit DEVOPS Handoff Annotation (new crate
   -> new Gate 5 job; new per-crate tag; no new external dep; Gate 4 sees
   nothing new; new Earned-Trust probe; per-feature MT 100%).
4. `docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`
   — the response contract (plain JSON array of raw `Span`s), the NEW
   crate placement, the route with required `service`, the status
   mapping, the unchanged ray trait, the probe.
5. `.github/workflows/ci.yml` — read `gate-5-mutants-log-query-api` in
   full (lines 1123-1208) as the byte-for-byte mirror template; the
   directly symmetric precedent crate (one structural-key divergence
   away).
6. `deny.toml` — Gate 4 policy; `wildcards = "deny"` (line 84).
7. `Cargo.lock` and `crates/query-api/Cargo.toml` — confirmed every
   dependency (axum, hyper, serde, serde_json, tokio, tower, tower-http,
   aegis, ray) is already present in the workspace; confirmed the
   no-wildcard pinning style to mirror. `trace-query-api` itself is
   absent from the lock, confirming the crate is new.
8. `docs/feature/lumen-query-api-v0/devops/` — the direct precedent;
   three files of the same shape, mirrored here with per-crate
   substitutions.

## Pre-wave decisions (carried in, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| D1 | `deployment_target` | New thin binary exists; NO deploy artefact in slice 01 (in-process oneshot only) | ADR-0048 + brief |
| D2 | `container_orchestration` | N/A; Docker DEFERRED beyond slice 01 | brief |
| D3 | `cicd_platform` | GitHub Actions (existing) | ADR-0005 |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI) | repo |
| D5 | `git_branching_strategy` | Trunk-Based Development | memory `project_kaleidoscope_pure_trunk_based` |
| D6 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md |

## In-wave decisions (A = Apex / DEVOPS decision)

### [A1] Deployment: a new binary, but NO Docker artefact in slice 01

The crate ships a thin `[[bin]]` (a trace query server) but slice 01
exercises it IN-PROCESS via the tower `oneshot` pattern: the Router is
driven with no bound TCP port, against a real durable
FileBackedTraceStore. No listener binds under test, no container is
built. A multi-stage Dockerfile for the trace-query server (analogous
to the existing kaleidoscope-cli Dockerfile) is DEFERRED to a later
slice when an operator actually runs the binary as a service. Recorded
as deferred, not overlooked. `target_environments` is a single `clean`
environment (platforms [linux, macos] plus CI ubuntu-latest); no
external services.

### [A2] CI: the existing five gates plus ONE new Gate 5 job

`gate-5-mutants-trace-query-api`, mirrored byte-for-byte from the real
`gate-5-mutants-log-query-api` (`.github/workflows/ci.yml:1123-1208`),
the directly symmetric precedent (same axum/tokio HTTP read-path shape
over a durable per-tenant store, same `--in-diff` cascade). The full
YAML block and the per-crate substitution table are in
`ci-cd-pipeline.md`. Substitutions: package `trace-query-api`, path
filter `crates/trace-query-api/**`, cache-key namespace
`cargo-mutants-trace-query-api`, artefact `mutants-out-trace-query-api`,
job/step names, and the mutation-target comment (which adds the
missing-service 400 to the empty-vs-error / half-open / bounds-parser /
fail-closed list, the one structural divergence ADR-0048 Decision 1
introduces). Everything else (the `needs: [gate-2-public-api,
gate-3-semver]`, `timeout-minutes: 30`, pinned action SHAs, the
`origin/main -> HEAD~1 -> full` baseline cascade, the empty-diff
short-circuit, `--no-shuffle --jobs 2`) is preserved identically.

Apex does NOT modify ci.yml. The crafter ADDS this block in the same
atomic commit that creates the crate. The cinder-bridge post-merge
correction is the cautionary precedent: a DEVOPS spec that assumed the
job already existed went unfixed for a feature window. Here the job is
NEW and its addition is an explicit DELIVER obligation.

### [A3] Gate 4 (cargo deny): NO change required; pin without wildcards

The new crate adds NO new external dependency. axum (0.7), hyper (1.4),
serde, serde_json, tokio, tower/tower-http (dev), aegis, and ray are ALL
already in the workspace and in `Cargo.lock`, verified by grep (each
resolves to existing entries; `trace-query-api` is absent from the
lock, confirming the crate is new). `regex` is NOT pulled in (no label
matchers in slice 01). No new licence, advisory, or yanked crate enters
the tree; `deny.toml` needs NO edit.

CONSTRAINT for DELIVER: `deny.toml` sets `wildcards = "deny"` (line 84).
Every dependency in `crates/trace-query-api/Cargo.toml` MUST be pinned
with an explicit version, never `*`, exactly as
`crates/query-api/Cargo.toml` and `crates/log-query-api/Cargo.toml`
already do. A wildcard fails Gate 4 even with no new crate.

### [A4] Gates 2 and 3: NOT graduated for trace-query-api this feature

`trace-query-api` is NOT added to Gate 2 (`cargo public-api`) or Gate 3
(`cargo semver-checks`) scope, nor to the pre-push hook's per-package
loop. This is consistent with the self-observe and log-query-api
precedents (cinder-to-pulse-bridge-v0 A1; lumen-query-api-v0 A4): a
thin v0 crate whose only consumer is the workspace is not locked under
the public-api / semver gates until it stabilises or a real external
consumer appears. ADR-0048 Decision 5 is the surface audit trail; the
public port is just `router(store, tenant)`.

### [A5] Branching, mutation scope, KPI gating

- **Branching**: Trunk-Based Development, project default (memory),
  encoded in the workflow already. No per-feature deviation.
- **Mutation testing**: per-feature, 100% kill rate (CLAUDE.md;
  ADR-0005 Gate 5), scoped to `crates/trace-query-api/src` via the
  `--in-diff` cascade. Primary targets per ADR-0048: the half-open
  boundary, the empty-vs-error distinction, the missing-service 400,
  the bounds parser, and the fail-closed refusal.
- **KPIs as CI signals**: KPI 1/2/3/4 are gated via Gate 1 (cargo test
  --workspace) running the crate's tower-oneshot acceptance tests to
  GREEN. KPI 3's p95 <= 500 ms budget is stated against ubuntu-latest
  for <= 1000 spans, cross-checked with ray's `record_query` recorder.
  Per project memory, CI is feedback not a merge gate; these are
  correctness signals. (See environments.yaml `kpi_collection` for the
  per-KPI map.)

## Graduation tag

Closing this feature requires a NEW per-crate tag
**`trace-query-api/v0.1.0`**, matching the crate manifest
`version = "0.1.0"`, exactly as the sibling crates are tagged. This is
a v0 slice: the tag is **v0.1.0, NOT v1.0.0**. The DESIGN handoff
flagged this; it is recorded here as the graduation obligation at
feature close.

## Infrastructure summary

- **Deployment**: a new thin binary, NO deploy artefact in slice 01;
  Docker DEFERRED (A1).
- **CI**: GitHub Actions ubuntu-latest; ADR-0005's five gates inherited;
  ONE new parallel Gate 5 job `gate-5-mutants-trace-query-api` (A2).
- **Gate 4**: no deny.toml change; no-wildcard pin constraint (A3).
- **Gates 2/3**: trace-query-api NOT graduated this feature (A4).
- **Branching**: Trunk-Based Development, unchanged (A5).
- **Mutation testing**: per-feature, scoped to
  `crates/trace-query-api/src`, 100% kill rate (A5).
- **External integrations**: NONE. Reads the in-process first-party ray
  store through the `TraceStore` trait; no network service, no consumer
  contract pinned yet (why the plain-array contract was chosen).
- **Graduation**: per-crate tag `trace-query-api/v0.1.0`.

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Create `crates/trace-query-api` (lib + thin bin) and ADD `gate-5-mutants-trace-query-api` to `.github/workflows/ci.yml` (per ci-cd-pipeline.md) in ONE atomic commit | Mutation testing covers the new crate from its first commit; avoid the cinder-bridge post-merge gap. |
| At DISTILL | Pin every dependency in `crates/trace-query-api/Cargo.toml` with explicit versions; no `*` | `deny.toml` `wildcards = "deny"` (A3). |
| At DISTILL | DO NOT add `trace-query-api` to Gate 2, Gate 3, or the pre-push hook loop | A4 defers graduation. |
| At DISTILL | DO NOT modify Gate 1's `cargo test --workspace` invocation | Tests auto-discovered. |
| At each DELIVER slice | Turn the slice's mutants 100% killed before review approval | CLAUDE.md MT strategy; ADR-0005 Gate 5. |
| At feature close | Tag `trace-query-api/v0.1.0` matching the manifest | Per-crate graduation tag (NOT v1.0.0). |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced by this wave**:

| Artefact | Path |
|----------|------|
| Environment inventory (single `clean` env) | `docs/feature/ray-query-api-v0/devops/environments.yaml` |
| CI/CD pipeline addendum (the exact new Gate 5 job block) | `docs/feature/ray-query-api-v0/devops/ci-cd-pipeline.md` |
| DEVOPS wave decisions log (this file) | `docs/feature/ray-query-api-v0/devops/wave-decisions.md` |

**Deliverables explicitly NOT produced** (N/A for this slice):

| Skipped artefact | Reason |
|------------------|--------|
| `kpi-instrumentation.md` | KPI collection is fully captured inline in environments.yaml `kpi_collection`; all four KPIs gate via Gate 1 acceptance tests, no separate runtime instrumentation stack to design for slice 01 |
| `observability-design.md` / `monitoring-alerting.md` | No deployed listener in slice 01; the CI gates are the alerting surface. ray's `record_query` recorder seam already exists for KPI 3 timing |
| `infrastructure-integration.md` | No external integrations; reads the in-process ray store |
| `branching-strategy.md` | Trunk-based is project default; no deviation |
| Dockerfile / deployment manifest | Deferred beyond slice 01 (A1) |

## Contradictions with the DESIGN handoff

None. The DESIGN DEVOPS Handoff Annotation specified exactly: a new
`gate-5-mutants-trace-query-api` job, a new per-crate graduation tag,
no new external dependency / no Gate 4 change, a new Earned-Trust
probe, and per-feature mutation 100% scoped to the new crate's src.
This wave records each of those, adds the verified no-wildcard Gate 4
constraint, pins the graduation tag at v0.1.0, confirms the Gate 2/3
deferral against the self-observe and log-query-api precedents, and
pins the single `clean` environment with Docker deferred. The
Earned-Trust probe is a DESIGN/DELIVER correctness concern (three
orthogonal layers per ADR-0048 Decision 7); DEVOPS notes it is enforced
by the crate's own tests under Gate 1, requiring no separate DEVOPS
artefact. The forward-looking `query-http-common` extraction
recommendation is a downstream feature, not a DEVOPS concern for this
slice; recorded in DESIGN, noted here only as not actioned.
