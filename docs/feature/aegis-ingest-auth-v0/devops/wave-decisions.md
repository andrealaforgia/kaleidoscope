# Wave Decisions — aegis-ingest-auth-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-06
- **Mode**: Autonomous overnight run. **SLIM** wave — an INTERNAL,
  single-crate change to the EXISTING live `aperture` crate that REUSES the
  in-workspace `aegis` crate verbatim; NO new third-party dependency, NO new
  crate, NO deploy surface, NO new infrastructure, NO public-API break. The
  ONE genuinely DEVOPS-flavoured item is a NEW OPERATIONAL DEPLOYMENT
  PRECONDITION (the auth config the operator must provision), documented below.
- **Inputs read**: `design/wave-decisions.md` (DD1-DD7; the refuse-to-start
  fail-closed posture; the `secret_file` config; the new in-workspace aegis path
  dep; the tenant ripple map; Reuse = REUSE-aegis-verbatim + EXTEND-aperture),
  `docs/product/architecture/adr-0068-aegis-ingest-auth.md`,
  `docs/product/architecture/brief.md` (§"Application Architecture —
  aegis-ingest-auth-v0", incl. its DEVOPS handoff + For-Acceptance-Designer
  notes), `discuss/outcome-kpis.md` (KPI-1..4), ADR-0005 (the five workspace
  gates), `.github/workflows/ci.yml`, `scripts/hooks/{pre-commit,pre-push}`,
  `Cargo.lock` (aegis 0.1.0 + jsonwebtoken 9.3.1 already present),
  `crates/aperture/{Cargo.toml,src/}` (live source), `CLAUDE.md`, and the prior
  slim-DEVOPS shape (`aperture-serve-loop-error-surfacing-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | DD1-DD7 resolved (jwt config table + never-logged `secret_file`; per-transport bearer extraction + exact reject mapping; the `TenantScoped<T>` tenant ripple; refuse-to-start fail-closed; aegis owns the per-request audit event; ingest-only scope + role-gating deferred; DD7 "JWKS" doc-fix flagged ADJACENT); the touched-file set (`config/mod.rs`, `transport.rs`, `app.rs`, `ports/mod.rs`, `compose.rs`/`sinks.rs` touchpoints, all under `crates/aperture/`); Reuse = REUSE-aegis-verbatim + EXTEND-aperture; the semver posture (not in Gate 2/3, no bump) — all consumed | − none; DESIGN hands DEVOPS the mutation scope (the modified aperture files), the no-new-external-dep posture, and the operational precondition, all addressed below |
| ADR-0068 | the mechanism (handler-path auth step before `ingest_*`, Option A); the fail-closed refuse-to-start (DD4, mirroring ADR-0061 `into_config` -> exit 2); the secret-never-logged structural invariant (DD1); the one-audit-event-per-request contract (DD5); the public-API / semver confirmation (aperture + aegis not in Gate 2/3, pre-1.0, NEVER 1.0.0); the Enforcement note (cargo deny enforces the non-wildcard aegis path dep) | − none; ADR-0068 already states the no-break / no-bump posture and the cargo-deny enforcement — confirmed against CI + Cargo.lock below |
| `brief.md` aegis section | DEVOPS handoff (INTERNAL single-crate, REUSE aegis verbatim, modified files, Gate 2/Gate 3 do NOT fire, mutation scope = modified aperture files only, aegis OUT of mutation scope, no external integration / no contract test, in-process HS256 boundary); For-Acceptance-Designer driving ports (gRPC metadata + HTTP header + binary config) | − none; the brief explicitly states "no new third-party crate beyond what aegis already pulls (jsonwebtoken, already present transitively)" — confirmed against Cargo.lock below |
| `discuss/outcome-kpis.md` | KPI-1 (north star: 100% of accepted batches carry an authenticated tenant_id), KPI-2 (100% of invalid/tokenless rejected, nothing stored), KPI-3 (every denial a distinct aegis reason), KPI-4 (no startup ships an unauthenticated path by omission); the guardrails (accept latency + shape non-regression; backpressure/shutdown unchanged; zero secret bytes in logs); the DEVOPS handoff (correlate audit↔sink events; reason-distribution panel; KPI-1<100% + secret-in-logs = CRITICAL alerts; baselines 0% by construction) | − none; baselines are 0% by construction (no auth today) — no baseline measurement needed, consumed into the KPI instrumentation note |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | − Gate 2/Gate 3 enrolled for only 4 graduated packages; neither aperture nor aegis is among them (consistent with no-break, finding below) |
| `.github/workflows/ci.yml` | `gate-5-mutants-aperture` (:505-602) exists, `--in-diff` path-filtered on `crates/aperture/**`; `gate-5-mutants-aegis` (:2000-2081) exists, `--in-diff` on `crates/aegis/**`; Gate 1 (:136-184); Gate 4 (`cargo deny --all-features check`, :83-114) | − Gate 2 / Gate 3 enrol ONLY otlp-conformance-harness, spark, sieve, codex (pre-push lines 54, 77); aegis src is untouched so its gate-5 job short-circuits — both noted, neither a blocker |
| `Cargo.lock` | aegis is already a workspace member (version 0.1.0); jsonwebtoken 9.3.1 already present (pulled transitively by aegis) | − none; confirms NO new external/license/advisory surface enters the lockfile |
| `crates/aperture/Cargo.toml` + `src/` | no aegis dep today (correct pre-DELIVER state); all modified files (`app.rs`, `transport.rs`, `config/`, `ports/`, `compose.rs`, `sinks.rs`) live under `crates/aperture/src/` | − none; confirms the `crates/aperture/**` mutation glob covers the entire modified-file set |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = Gate 4 + Gate 1 (the local mirror); pre-push = Gate 2/Gate 3 for the 4 graduated pkgs | − pre-push (lines 54, 77) confirms aperture + aegis absent from the public-api/semver package loop (consistent with internal-only) |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and no CI-config change is made by
this wave.** The feature modifies existing source files inside the single live
crate `aperture` (`src/config/mod.rs`, `src/transport.rs`, `src/app.rs`,
`src/ports/mod.rs`, plus the `src/compose.rs` Validator-construction and
`src/sinks.rs`/`src/ports` SinkRecord tenant-tagging touchpoints) and adds an
**in-workspace** `aegis = { path = "../aegis" }` dependency. aperture already
owns a path-filtered `gate-5-mutants-aperture` `--in-diff` job that mutates
exactly its changed lines automatically.

**Confirmed against the live source / lockfile**: `crates/aperture/Cargo.toml`
has NO aegis dep today (the correct pre-DELIVER state); `aegis` is already a
workspace member at version `0.1.0` in `Cargo.lock`; `jsonwebtoken 9.3.1`
(aegis's HS256 engine) is already present in `Cargo.lock` and already vetted by
the existing Gate 4 pass. **No new external/license/advisory surface is
introduced** — aegis + jsonwebtoken are already vetted.

**The ONE genuinely DEVOPS-flavoured item** is an OPERATIONAL DEPLOYMENT
PRECONDITION: turning auth on (it is always on once the listeners bind) makes a
successful aperture startup DEPEND on operator-provisioned auth config — a
readable HS256 `secret_file` + a tenant `catalogue_path` + `issuer`/`audience`,
or aperture exits 2 and binds nothing. This is a deployment precondition to
document (captured in `environments.yaml > deployment_assumptions` and the
Constraints below); it adds NO CI infrastructure.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production code, NO tests, and NO CI-config
changes exist yet for this feature. That absence is the EXPECTED and CORRECT
state — it is NOT a finding and NOT a rejection reason. This wave's job is to
(a) CONFIRM the existing ADR-0005 CI contract covers the feature, (b) DOCUMENT
the new auth-config deployment precondition, and (c) produce `environments.yaml`
+ this file. Review THAT, not the non-existence of code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers the change.

## Decision summary (D1-D9, all existing / inherited — brownfield, NOT a deploy)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | Internal change to a live library + binary. aperture is the operator-run / orchestrator-run OTLP gateway; Kaleidoscope deploys nothing. No deploy step is added or required. (But see the OPERATIONAL deployment precondition below: a startup now requires the auth config or it refuses to start.) |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface added by this wave. (The refuse-to-start posture makes aperture a more honest orchestration citizen — a misconfigured gateway exits 2 instead of silently serving an open door — but introduces no orchestration artefact.) |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the modified aperture files automatically (Gate 5 via the existing `gate-5-mutants-aperture --in-diff` job; Gate 4 confirms the non-wildcard aegis path dep). Gate 2/Gate 3 do NOT cover aperture or aegis — see CI Contract finding (consistent with no-break). No new gate. |
| D5 | Observability | **Existing convention — aegis audit event on aperture's stderr stream** | The feature emits auth deny/allow AUDIT events reusing aegis's `reason()` taxonomy (8 reasons) + the aperture stderr convention (DD5): aegis emits exactly one structured event per validate call (`tenant_id`/`role`/`decision`/`subject`/`reason`); aperture adds only the pre-validate `reason=missing_claim` line + a `transport=` field on the deny axis. Refuse-to-start emits `event=config_validation_failed` + exit 2. NO new metric, NO new dashboard, NO new observability stack. The 4 outcome KPIs are measured by correlating the existing audit↔sink events (see KPI Instrumentation below). |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; aperture is stateless (no WAL/snapshot/on-disk format), the wire / accept-response contracts are unchanged on the happy path (the tenant rides INSIDE the SinkRecord). CAVEAT: a revert re-OPENS the gateway (re-admits tokenless writes), so it is a deliberate SECURITY decision, not a neutral rollback — prefer fix-forward (`environments.yaml > rollback`). |
| D7 | Continuous learning | **N/A** | No live telemetry loop in this wave; the KPIs are measured by correlating the existing audit/sink events (the K6 raw-observation idiom), and baselines are 0% by construction (no auth on the path today). |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5 / CLAUDE.md)** | Already pinned in CLAUDE.md. Mutation scope = the modified aperture files (`config/mod.rs`, `transport.rs`, `app.rs`, `ports/mod.rs`, and the `compose.rs`/`sinks.rs` touchpoints). Covered by the existing `gate-5-mutants-aperture --in-diff` job. aegis is OUT of scope (reused verbatim; its gate-5 job short-circuits on an empty diff). **No CLAUDE.md change needed.** |

## CI Contract — confirmation and findings

### Gate 5 (mutants, 100% kill) — CONFIRMED, no new job

| Touched path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|--------------|------------------------|---------------------|-------------|----------|
| `crates/aperture/src/config/mod.rs` | the `[aperture.security.auth.jwt]` table parse (`deny_unknown_fields`) + the `into_config` refuse-to-start invariant (absent/incomplete/unreadable -> `ConfigError` -> exit 2) + the builder setters | `gate-5-mutants-aperture` | 505-602 | ✓ `--in-diff` on `crates/aperture/**` |
| `crates/aperture/src/transport.rs` | `extract_bearer_{grpc,http}`; `reject_to_{status,http}`; the `Arc<aegis::Validator>` wiring on the services + `HttpState`; the 6 handler auth steps; the one pre-validate `reason=missing_claim` deny line | `gate-5-mutants-aperture` | 505-602 | ✓ same job |
| `crates/aperture/src/app.rs` | the 3 `ingest_*` tenant parameters; the 3 `SinkRecord::*` constructions; the 3 `summarise_record` arms | `gate-5-mutants-aperture` | 505-602 | ✓ same job |
| `crates/aperture/src/ports/mod.rs` | `TenantScoped<T>` + the `SinkRecord` variant payloads carry the tenant | `gate-5-mutants-aperture` | 505-602 | ✓ same job |
| `crates/aperture/src/compose.rs` + `src/sinks.rs` | the `Validator` construction at composition (`load_catalogue` + `Validator::new`) + the SinkRecord tenant-tagging touchpoints | `gate-5-mutants-aperture` | 505-602 | ✓ same job |

The job runs `cargo mutants --package aperture --in-diff "$DIFF_FILE"` against
`git diff "$BASELINE" HEAD -- 'crates/aperture/**'` (baseline cascade
`origin/main` -> `HEAD~1` -> full; ci.yml:566-596). Every modified file lives
under `crates/aperture/`, so the single glob covers the entire modified-file set
and the `--in-diff` filter mutates ONLY the lines this feature changes. A mutant
that accepts a tokenless request, skips the refuse-to-start, drops the tenant
from the record, weakens a reject `reason`, or collapses the
`config_validation_failed`/exit-2 path must be killed by the token-matrix /
fail-closed gold tests (KPI-1..4). **No per-feature wiring, no new gate-5 job.**
aperture was already enrolled in the per-crate `--in-diff` model (the close of
`gate-5-mutants-batch-v0`), so this feature inherits gating for free.

### Gate 5 (aegis) — CONFIRMED short-circuit, NOT a new job

A separate `gate-5-mutants-aegis` job exists (ci.yml:2000-2081) with the same
`--in-diff` baseline cascade scoped to `crates/aegis/**`. This feature REUSES
aegis verbatim and modifies NO aegis source — the DD7 "JWKS" doc overstatement
fix is ADJACENT (a `docs:` fix-forward or trivial micro-wave), NOT folded into
this feature, precisely so a non-behavioural change does not pull aegis back into
the 100%-mutation scope. With aegis src untouched, its diff (`crates/aegis/**`)
is EMPTY and the job short-circuits to a zero-second exit ("No aegis-touching
changes ...; skipping mutation testing."). **aegis is correctly OUT of this
feature's mutation scope. No aegis mutation job is added; a non-behavioural
doc-fix carries no mutation surface — do NOT add one speculatively.**

### Gate 4 (cargo deny) — CONFIRMED: NO new external/license/advisory surface, NO semver bump dependency

**This is one of the two load-bearing confirmations.**

1. **The new dependency is IN-WORKSPACE.** The feature adds
   `aegis = { path = "../aegis" }` to `crates/aperture/Cargo.toml` — a
   non-wildcard PATH dep (the workspace cargo-deny rule). `aegis` is already a
   workspace member at version `0.1.0` in `Cargo.lock`.
2. **No new third-party crate enters the lockfile.** aegis's HS256 engine is
   `jsonwebtoken 9.3.1`, already present in `Cargo.lock` (pulled transitively by
   aegis, which is already a workspace member) and already vetted by the existing
   Gate 4 (`cargo deny --all-features check`, ci.yml:83-114) pass. aegis is
   AGPL-3.0 (the workspace license); jsonwebtoken's license + advisory status is
   already cleared by the existing deny config.
3. **Therefore Gate 4 is a NO-OP CONFIRMATION, not a new check.** No new
   external surface, no new license to allow, no new advisory to triage. The
   only delta cargo-deny sees is the aperture->aegis path edge, which it enforces
   as non-wildcard (ADR-0068 Enforcement).

### Gate 2 (public-api) + Gate 3 (semver) — CONFIRMED: do NOT fire, NO semver bump (the other load-bearing confirmation)

**DESIGN confirmed NO public-API break, and CI inspection confirms neither
aperture nor aegis is even enrolled in Gate 2/Gate 3 — the two facts agree.**

1. **Neither aperture nor aegis is enrolled.** Gate 2 (`cargo public-api`) and
   Gate 3 (`cargo semver-checks`) are enrolled for ONLY the four **graduated**
   packages — `otlp-conformance-harness`, `spark`, `sieve`, `codex`
   (`scripts/hooks/pre-push` lines 54, 77:
   `for pkg in otlp-conformance-harness spark sieve codex`). Neither aperture nor
   aegis is in the loop.
2. **And there is no break to flag anyway.** Per ADR-0068 §"Public-API / semver
   posture" and `design/wave-decisions.md` §"Semver posture": aperture's `Config`
   fields are `pub(crate)`; `ingest_*` and `SinkRecord` are `pub` but only
   aperture constructs them (no external consumer; the `ingest_*`/`SinkRecord`
   change is breaking to IN-CRATE callers only, additive-in-spirit — every record
   gains a guaranteed tenant). aegis is **UNCHANGED** by this feature (reused
   verbatim; the DD7 doc-fix is adjacent). So even if aperture/aegis WERE
   enrolled, Gate 2/Gate 3 would find no public diff.
3. **Therefore NO semver bump is needed. aperture and aegis stay pre-1.0.**
   DELIVER must NOT bump `crates/aperture/Cargo.toml` nor `crates/aegis/Cargo.toml`.
   There is no public-api baseline to update (none exists for either crate).
   (Were a public type ever to leak in a future change, it would be semver-MINOR
   at most, pre-1.0, **NEVER 1.0.0** — Andrea's call; not in scope here.)
4. **Decision: do NOT enrol aperture/aegis into Gate 2/Gate 3 in this wave.**
   Graduating a crate into the public-surface lock is a separate, deliberate
   decision (as it was for spark/sieve/codex). Flagged, not actioned.

### Gate 1 — CONFIRMED unchanged

**Gate 1 (`cargo test --workspace --all-targets --locked`, ci.yml:136-184)**
runs the token-matrix acceptance suite (valid token -> accept + tenant-tagged
record; the 8-reason reject matrix -> reject + matching reason + empty sink + one
deny audit line) and the fail-closed-config refusal test (no jwt table /
unreadable secret_file -> exit 2 + `config_validation_failed` + no listener, no
secret bytes), plus the non-regression guardrails (existing `slice_0*` ingest
suites once they supply a token + auth config; `invariant_single_validator` stays
green because the aegis `validate` call is a different symbol in the transport
layer, not a harness `validate_*` call site). DISTILL authors these; DELIVER
turns them green. Identically in the local pre-commit hook and CI. No change.

## Infrastructure Summary

- **New infrastructure**: none. No crate, no container, no service, no cloud
  resource, no IaC, no orchestration, no new always-running task.
- **New external dependency**: none. The added `aegis = { path = "../aegis" }`
  is an IN-WORKSPACE path dep already in `Cargo.lock` (0.1.0); its transitive
  `jsonwebtoken 9.3.1` is already in the lockfile and already vetted by Gate 4.
  No new external/license/advisory surface.
- **CI changes**: none. The five ADR-0005 gates are inherited unchanged; the
  single relevant Gate 5 job (`gate-5-mutants-aperture`) already path-filters
  `--in-diff` onto all modified aperture files (they all live under
  `crates/aperture/`). `gate-5-mutants-aegis` short-circuits on an empty diff.
  No new job, no edit to an existing job.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for an
  internal single-crate change, NOT deploy targets. See `environments.yaml`.
- **Deployment precondition (NEW, operational)**: a successful aperture startup
  now REQUIRES a complete, readable `[aperture.security.auth.jwt]` block (HS256
  `secret_file` + tenant `catalogue_path` + `issuer`/`audience`), or aperture
  refuses to start (exit 2, `config_validation_failed`, no listener). The secret
  must be file-permission-restricted and never committed. Captured in
  `environments.yaml > deployment_assumptions` and Constraint C-DEVOPS-2 below.
- **Auth test environment**: in-suite-minted HS256 tokens (jsonwebtoken
  `encode`, already a workspace dep) against the real aperture binary + a
  throwaway test secret/catalogue — a TEST concern, no infra, no real IdP, no
  JWKS, no network at validation time. Recorded in `environments.yaml >
  auth_test_environment`.
- **Observability**: the aegis one-event-per-validate audit line on aperture's
  existing stderr stream + the one aperture-owned pre-validate `missing_claim`
  line + the `config_validation_failed`/exit-2 refuse-to-start signal (existing
  platform conventions); no new metric, no new dashboard, no new stack.
- **Rollback**: `git revert` (trunk-based); aperture is stateless and the
  happy-path wire/response contracts are unchanged. CAVEAT: a revert re-opens the
  gateway (re-admits tokenless writes) — a deliberate SECURITY decision, not a
  neutral rollback; prefer fix-forward.

## KPI Instrumentation (outcome-kpis.md handoff)

No new telemetry stack — the four KPIs are measured by CORRELATING the existing
stderr JSON audit events with the sink events:

- **KPI-1 (north star — 100% of accepted batches carry an authenticated
  `tenant_id`)**: correlate `decision=allow` events with `sink_accepted` events
  per request. Target 100%; baseline 0% (by construction). **KPI-1 < 100% (any
  accepted batch without an authenticated tenant) is a CRITICAL alert.**
- **KPI-2 (100% of invalid/tokenless requests rejected, nothing stored)**: per
  rejected request, assert reject status (`UNAUTHENTICATED`/`401`) AND absence of
  `sink_accepted`.
- **KPI-3 (every denial reports a distinct aegis reason)**: tally the `reason`
  field distribution across deny events (8 reasons; zero "unknown/other"
  bucket). Reason-distribution panel for Priya.
- **KPI-4 (no startup ships an unauthenticated path by omission)**: observe
  exit code + `config_validation_failed` + absence of `listener_bound` at
  startup.
- **Guardrail (hard)**: **any secret-bytes-in-logs occurrence is a CRITICAL
  alert** (System Constraint 4). Accept-latency + accept-response-shape
  non-regression, backpressure, and graceful-shutdown behaviour must NOT degrade.

Wiring these correlations into a fleet dashboard is the standard DEVOPS
instrumentation task; it reuses the existing stderr audit stream and adds no
stack. Baselines are 0% by construction — no baseline-measurement step needed.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** The existing
  `gate-5-mutants-aperture` job covers all modified files via `--in-diff` (they
  all live under `crates/aperture/`); `gate-5-mutants-aegis` short-circuits on an
  empty diff. DELIVER must NOT add a per-feature gate-5 job and must NOT add an
  aegis mutation job for the DD7 doc-fix.
- **C-DEVOPS-2 — REFUSE-TO-START deployment precondition (the one operational
  item).** A deployment now REQUIRES a complete, readable
  `[aperture.security.auth.jwt]` block: a readable HS256 `secret_file` + a tenant
  `catalogue_path` + `issuer` + `audience`. Absent/incomplete/unreadable -> exit
  2 + `event=config_validation_failed` + NO listener binds (the ADR-0061
  `into_config` seam). There is NO opt-out flag. SECRET HANDLING: the HS256
  secret is supplied BY FILE REFERENCE (never inline); the operator MUST
  file-permission-restrict the secret file (readable only by the aperture process
  user) and MUST NEVER commit it; config errors name the file BY PATH ONLY; no
  token/secret appears in any audit/deny/error line (structural per DD1). Dev/test
  runs use a throwaway secret + one-tenant catalogue. Release notes must call out
  that tokenless callers will now be rejected (the intended security change).
- **C-DEVOPS-3 — NO new external dependency; cargo-deny (Gate 4) satisfied.**
  The added `aegis = { path = "../aegis" }` is an in-workspace, non-wildcard path
  dep already in `Cargo.lock` (0.1.0); its transitive `jsonwebtoken 9.3.1` is
  already present and already vetted. NO new external/license/advisory surface.
  Gate 4 is a no-op confirmation. DELIVER must keep the dep non-wildcard.
- **C-DEVOPS-4 — NO public-API break, NO semver bump.** aperture's change is
  fully internal (`Config` `pub(crate)`; `ingest_*`/`SinkRecord` `pub` but
  aperture-only constructed; breaking to in-crate callers only). aegis is
  UNCHANGED. Neither crate is enrolled in Gate 2/Gate 3, AND there is no break to
  flag anyway. DELIVER must NOT bump `crates/aperture/Cargo.toml` nor
  `crates/aegis/Cargo.toml`, and there is NO public-api baseline to update. NEVER
  1.0.0 — Andrea's call; out of scope.
- **C-DEVOPS-5 — Auth tests must be deterministic and run in BOTH the local
  pre-commit hook AND CI Gate 1.** In-suite-minted HS256 tokens + reject/accept +
  reason-string + sink-empty/tenant-tagged + audit-line-count (exactly one) +
  exit-code assertions, NO wall-clock threshold — so the hook does not flake under
  overnight load (the p95-flake class does NOT apply; these are boolean / exit-code
  assertions, not p95 latency).
- **C-DEVOPS-6 — Falsifiability + non-regression are mandatory.** Each reject AC
  must produce the matching gRPC `UNAUTHENTICATED` / HTTP `401` +
  `WWW-Authenticate: Bearer` with the exact aegis `reason`, an EMPTY sink, and
  EXACTLY ONE deny audit line; the fail-closed-config controls must produce exit
  2 + `config_validation_failed` + no listener + NO secret bytes. The ACCEPT
  control (a valid catalogued token still ingests byte-shape-identical AND the
  record carries the tenant) bounds the regression to unauthenticated callers.
  `invariant_single_validator` must stay green.
- **C-DEVOPS-7 — Guardrails must stay green.** Existing `slice_0*` ingest suites
  (once they supply a token + auth config — DELIVER updates `tests/common` once),
  backpressure, and graceful-shutdown behaviour must not regress; accept latency
  + accept-response shape unchanged; zero secret bytes in any log/event/error
  (a Critical guardrail). Gate 5 must reach 100% kill on the modified aperture
  files (D9).
- **C-DEVOPS-8 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy
  is already pinned (D9).
- **C-DEVOPS-9 — DD7 aegis doc-fix is ADJACENT, not in this wave.** The aegis
  `lib.rs` "JWKS" -> "HS256 pre-shared key" doc correction is a separate `docs:`
  fix-forward or trivial micro-wave; it must NOT be folded here (it would pull
  aegis into the 100%-mutation scope for a non-behavioural change).

## Upstream Changes

**None expected.** DESIGN resolved DD1-DD7 into locked ACs for DISTILL; this
DEVOPS wave CONFIRMS (rather than corrects) the existing ADR-0005 CI contract
covers them, the no-new-external-dep / cargo-deny posture, and the
no-public-API-break / no-semver-bump posture — all against the live source, the
lockfile, and the workflow. No shared assumption needed correcting; ADR-0068 and
the brief already state these postures and CI/lockfile inspection agrees. No
story re-scoping; no DISCUSS/DESIGN delta.

**Adjacent (NOT this feature, DD7):** the aegis `lib.rs:18-23,39-41` "JWKS" doc
overstatement -> "validates against a configured issuer + audience using a
pre-shared HS256 key (RS256/JWKS is v1)". Disposition: a `docs:` fix-forward on
the closed wave or a trivial micro-wave. Logged so it is not lost; its
non-behavioural nature is exactly why it is kept out of this feature's mutation
scope (so `gate-5-mutants-aegis` stays a short-circuit).

## Production Readiness (scoped to an internal, stateless-service change + a new config precondition)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance tests defined for the accept path + the 8-reason reject matrix
      + the fail-closed-config refusal, driven through the real binary with
      in-suite-minted HS256 tokens; DISTILL authors them, DELIVER turns them
      green (KPI-1..4).
- [x] Mutation gate (Gate 5, 100% kill) auto-covers all modified aperture files
      via the existing `gate-5-mutants-aperture --in-diff` job (D9); aegis
      short-circuits (out of scope).
- [x] cargo-deny (Gate 4) confirmed: in-workspace non-wildcard aegis path dep;
      jsonwebtoken already vetted; NO new external/license/advisory surface.
- [x] No-public-API-break / no-semver-bump confirmed against the live source,
      the lockfile, and the workflow (aperture + aegis stay pre-1.0).
- [x] Auth decisions surfaced on existing channels (DD5): aegis's one-event-per-
      validate audit line + the one aperture pre-validate `missing_claim` line +
      the `config_validation_failed`/exit-2 refuse-to-start signal.
- [x] No new event family / metric / dashboard / observability stack; the 4 KPIs
      are correlations over the existing audit↔sink event streams.
- [x] **NEW operational precondition documented**: refuse-to-start without the
      auth config; the operator must provision + permission-restrict + never
      commit the HS256 secret file + the tenant catalogue (C-DEVOPS-2,
      `environments.yaml > deployment_assumptions`). Release notes flag the
      tokenless-caller rejection.
- [x] Rollback posture: `git revert`; aperture is stateless, happy-path wire/
      response contracts unchanged — but a revert RE-OPENS the gateway, so it is
      a deliberate security decision (prefer fix-forward).
- [n/a] Canary / blue-green / rolling — no deployment surface (the change adds a
      startup precondition but no rollout).
- [n/a] On-call / runbook — operators run / orchestrate the binary; the aegis
      audit events + `config_validation_failed`/exit-2 ARE the operator-facing
      signals. A fleet alert on KPI-1 / secret-in-logs is the standard DEVOPS
      instrumentation task (KPI Instrumentation above).

## Peer Review

The `nw-platform-architect-reviewer` Agent could not be invoked as a nested
subagent from within this subagent context (the identical constraint was
recorded for the prior slim-DEVOPS features, e.g.
`aperture-serve-loop-error-surfacing-v0/devops/wave-decisions.md`). Per the
established slim-DEVOPS precedent on this project, a structured self-review was
conducted against the reviewer's exact dimensions (external validity ->
evidence-based findings -> severity-driven -> DORA -> handoff completeness); see
`self-review.md`. The dispatch carried the nWave-order reminder (no
code/tests/CI exist at DEVOPS time — that absence is expected, not a rejection
reason). Verdict: **APPROVED_PENDING_INDEPENDENT_REVIEW**, 0 blocking issues.
An independent top-level `nw-platform-architect-reviewer` run is recommended
before DISTILL.

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (the existing
  `gate-5-mutants-aperture` and `gate-5-mutants-aegis` jobs are untouched;
  trunk-based, no required checks).
- Does not add an aegis mutation job for the DD7 doc-fix (non-behavioural; would
  pull aegis into the 100%-mutation scope for nothing).
- Does not enrol aperture/aegis into Gate 2/Gate 3 (a separate graduation
  decision; flagged, not actioned).
- Does not write production code or the auth / fail-closed-config tests (crafter
  owns DELIVER; acceptance-designer owns the test specs in DISTILL).
- Does not provision the operator's HS256 secret file or tenant catalogue (a
  deployment-time operational responsibility; this wave only documents the
  precondition).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not bump any `Cargo.toml` version (aperture + aegis stay pre-1.0 — NO
  break, NO bump, NEVER 1.0.0).
- Does not change the aegis `Cargo.toml` dep specifier to a wildcard (the path
  dep stays non-wildcard for cargo-deny).
- Does not proceed into DISTILL.
