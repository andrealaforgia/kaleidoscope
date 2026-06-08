# Wave Decisions — read-path-query-api-auth-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-08
- **Mode**: Autonomous overnight run. **SLIM-ish** wave — an INTERNAL,
  multi-crate change that wires the already-built `aegis::Validator` onto the
  THREE live READ query APIs as an OPTIONAL, additive, fail-closed per-request
  bearer path. FOUR crates touched (the shared `query-http-common` + the three
  thin-wired read APIs). NO new crate, NO new dependency edge (all four crates
  already dep aegis), NO deploy, NO public-API break. The ONE genuine infra task
  was to CONFIRM each of the four crates has a `gate-5-mutants-<crate>` CI job —
  it does, all four already exist, so NO workflow change and NO commit.
- **Inputs read** (on 2026-06-08): `design/wave-decisions.md` (the additive
  model + the Andrea-veto flag; DD1-DD6; the no-bearer-bypass precedence; the
  audience fence; the aegis::Validator REUSE; the four-crate ripple; Reuse =
  REUSE-aegis-core + EXTEND-query-http-common-once + thin-wire-three-APIs),
  `docs/product/architecture/adr-0074-read-path-query-api-auth.md`,
  `discuss/outcome-kpis.md` (KPI-1..6; the north star = % authenticated reads
  scoped to the token's tenant with the isolation negative control passing; the
  guardrails incl. zero secret/token bytes in logs), ADR-0005 (the five
  workspace gates), `.github/workflows/ci.yml` (the five gates + all 30
  gate-5-mutants jobs), `scripts/hooks/{pre-commit,pre-push}` (ADR-0072 fast
  local subset + the graduated-package set), `Cargo.lock` (aegis 0.1.0 +
  jsonwebtoken 9.3.1 already present), the four touched crates' `Cargo.toml`
  (all four already dep aegis path 0.1.0), `CLAUDE.md` (per-feature 100%-kill
  mutation; trunk-based; ADR-0072 / ci-watch), and the closest slim-DEVOPS
  precedent (`aegis-ingest-auth-v0/devops` — the ingest auth this read path
  mirrors).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | DD1-DD6 resolved (optional read-auth config keyed off each binary's env prefix + never-logged secret-by-file-reference; bearer extraction + the pre-validate `missing_claim`; the 3-arm additive precedence + the NO-BEARER-BYPASS property in `query-http-common`; fail-closed-before-store + the Earned-Trust auth startup negative probe; one-audit-event-per-request reusing aegis; scope + the kaleidoscope-query audience fence + DD6 role deferral). The four-crate touched set (the shared `query-http-common` capability-once + the three thin-wired read APIs). Reuse = REUSE aegis core + the query-http-common seam/envelope/redaction/subscriber + the existing tenant-scoped store queries; EXTEND query-http-common + the three APIs; CREATE only the four read-auth config fields + the bearer boundary. The semver posture (workspace-internal, no bump). The Andrea-veto flag (additive -> per-request-only). | − none; DESIGN hands DEVOPS the mutation scope (the four modified crates), the no-new-edge posture, and the conditional deployment precondition, all addressed below |
| ADR-0074 | the mechanism (the capability lands ONCE in query-http-common, the per-request analogue of `resolve_tenant_or_refuse`; the 3-arm precedence; arm 2 = no fall-through to env_tenant = the no-bearer-bypass); the fail-closed-before-store (DD4); the secret-never-logged structural invariant (DD1); the one-audit-event-per-request contract (DD5); the kaleidoscope-query audience fence (DD6); the Enforcement note (the per-feature 100% mutation kill on the four modified crates; cargo deny enforces the non-wildcard aegis path dep) | − none; ADR-0074 already states the no-break / no-bump posture and names the four modified crates as the mutation scope — confirmed against CI + Cargo.lock below |
| `discuss/outcome-kpis.md` | KPI-1 (north star half: authenticated reads scoped to the token's tenant), KPI-4 (north star half: cross-tenant reads see the wrong/absent tenant — isolation negative control), KPI-2 (invalid/tokenless -> 401, nothing read), KPI-3 (every denial a distinct aegis reason), KPI-5 (backward compat — env-tenant byte-for-byte; the no-bearer-bypass), KPI-6 (audience fence); the guardrails (env-tenant happy-path + response-shape non-regression; the unset-tenant 401; zero secret AND zero raw-token bytes in any log/event/error/body — a hard guardrail; no store read on a refused request — a hard guardrail); the DEVOPS handoff (correlate allow<->store-read + deny<->no-store-read; north-star + reason-distribution + audience-fence panels; KPI-1/4 < 100% + secret/token-in-logs + store-read-on-refusal = CRITICAL alerts; baselines 0% by construction) | − none; baselines are 0% per-request by construction (one tenant per process today); KPI-5's baseline is the existing slice suite — no baseline-measurement step needed, consumed into the KPI Instrumentation note |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | − Gate 2/Gate 3 enrol ONLY 4 graduated packages; none of the four touched crates is among them (consistent with no-break, finding below) |
| `.github/workflows/ci.yml` | ALL FOUR gate-5-mutants jobs EXIST: `gate-5-mutants-query-http-common` (:1868), `gate-5-mutants-query-api` (:1095), `gate-5-mutants-log-query-api` (:1182), `gate-5-mutants-trace-query-api` (:1356) — each `--in-diff` path-filtered on `crates/<crate>/**`, baseline cascade origin/main -> HEAD~1 -> full, timeout-minutes 30, mutants.out artefact upload; Gate 1 (:136-182, `cargo test --workspace --all-targets --locked`); Gate 4 (`cargo deny --all-features check`, :83-114) | − Gate 2 / Gate 3 enrol ONLY otlp-conformance-harness, spark, sieve, codex (pre-push line 54); none of the four touched crates is enrolled — both noted, neither a blocker |
| `Cargo.lock` | aegis is already a workspace member (0.1.0); jsonwebtoken 9.3.1 already present (single entry) | − none; confirms NO new external/license/advisory surface enters the lockfile |
| the four crates' `Cargo.toml` | ALL FOUR already declare `aegis = { path = "../aegis", version = "0.1.0" }` (for `aegis::TenantId` today) | − none; confirms NO new dependency edge — the `Validator` import rides the same existing edge |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = Gate 4 + the FAST `cargo test --workspace --lib --locked` subset (ADR-0072); pre-push = Gate 2/Gate 3 for the 4 graduated pkgs | − the deep `--all-targets` auth-acceptance suite gates in CI (gate-1-test), NOT the local hook (ADR-0072); ci-watch.sh is the post-push safety net — noted in environments.yaml |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. All FOUR `gate-5-mutants-<crate>` jobs already exist. No new CI
job is required, and NO CI-config change is made by this wave — therefore NO
commit.** The feature modifies four existing crates — the shared
`query-http-common` (where the auth capability lands ONCE) plus the three thin-
wired read APIs `query-api` / `log-query-api` / `trace-query-api` — all under
`crates/`, each already owning a path-filtered `--in-diff` Gate 5 job that
mutates exactly its changed lines automatically.

**Confirmed against the live source / lockfile / workflow**:

1. **All four gate-5-mutants jobs exist** with correct `--package` + `crates/<crate>/**` diff glob (see CI Contract below). The shared `gate-5-mutants-query-http-common` (the most important — the auth capability lands there) is present and correctly shaped.
2. **No new dependency edge.** All four touched crates ALREADY declare `aegis = { path = "../aegis", version = "0.1.0" }` (for `aegis::TenantId` today); the `aegis::Validator` import rides the same existing edge. aegis is a workspace member (0.1.0); jsonwebtoken 9.3.1 is already in `Cargo.lock` and already vetted by the existing Gate 4. **No new external/license/advisory surface.**
3. **The five workspace gates cover all four crates.** Gate 1 runs `cargo test --workspace --all-targets --locked` (workspace-wide); Gate 4 runs `cargo deny --all-features check` (whole dep graph); Gate 5 covers each of the four crates via its existing per-crate `--in-diff` job. Gate 2/Gate 3 cover only the four graduated packages (none of the four touched crates) — consistent with no-public-API-break.

**The genuinely DEVOPS-flavoured item** is a CONDITIONAL operational deployment
precondition (NOT new CI infra), softened from the ingest door by the additive
model: read auth is OPTIONAL. A WHOLLY ABSENT config = the supported additive
opt-out (env-tenant mode, byte-for-byte today). A COMPLETE config = per-request
bearer auth on (after the auth startup negative probe). A PARTIAL config = the
ONE refuse-to-start case (the half-configured silent-downgrade trap, DD1).
Documented in `environments.yaml > deployment_assumptions`.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production code, NO tests, and NO CI-config
changes exist yet for this feature. That absence is EXPECTED and CORRECT — it is
NOT a finding and NOT a rejection reason. This wave's job is to (a) CONFIRM the
existing ADR-0005 CI contract covers all four crates (incl. each crate's Gate 5
mutation job), (b) DOCUMENT the conditional auth-config deployment precondition +
the KPI/redaction instrumentation, and (c) produce `environments.yaml` + this
file. Review THAT, not the non-existence of code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers all four crates.

## Decision summary (D1-D9, all PRE-DECIDED per the brief / inherited)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | Internal multi-crate change to three EXISTING live read-API binaries + a shared lib. Operators run them; Kaleidoscope deploys nothing. No deploy step added or required. (See the CONDITIONAL deployment precondition: a binary configured WITH auth requires a complete read-auth config or refuses to start; an unconfigured binary runs as today.) |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface added by this wave. |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates; confirmed to cover all four crates** | Gate 1 (workspace test) + Gate 4 (deny) are workspace-wide; Gate 5 covers EACH of the four crates via its EXISTING `gate-5-mutants-<crate> --in-diff` job (all four exist). Gate 2/Gate 3 do NOT cover the four touched crates — consistent with no-break (CI Contract finding). NO new gate, NO CI change. |
| D5 | Observability | **Existing convention — the aegis audit event on each read API's stderr stream** | The feature emits auth allow/deny AUDIT events reusing aegis's `reason()` taxonomy (8 reasons) + the field contract (tenant_id/role/decision/subject/reason), via the shared `query_http_common::init_tracing` JSON-stderr subscriber. The read API supplies `subject` = query_range/log_query/trace_query; the shared capability adds the ONE pre-validate `reason=missing_claim` line. The auth startup negative probe + a partial-config refusal emit health.startup.refused / config_validation_failed. NO new metric, NO new dashboard, NO new observability stack. The six outcome KPIs are measured by correlating the existing audit<->store-read events (KPI Instrumentation below). The redaction guarantee is STRUCTURAL (PathBuf not bytes; opaque-Debug; path-only errors; reason = aegis class name). |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; the three read APIs are stateless query front-ends over the UNTOUCHED stores (pulse/lumen/ray); the HTTP wire/response contracts are unchanged on the happy path. CAVEAT: reverting an auth-ON deployment RE-OPENS it to env-tenant-only resolution (drops per-request isolation) — a deliberate SECURITY decision, prefer fix-forward (`environments.yaml > rollback`). |
| D7 | Continuous learning | **N/A** | No live telemetry loop in this wave; the KPIs are measured by correlating the existing audit/store-read events; baselines are 0% per-request by construction (one tenant per process today). |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5 / CLAUDE.md)** | Already pinned in CLAUDE.md. Mutation scope = the modified files across the FOUR crates, covered by the four existing `gate-5-mutants-<crate> --in-diff` jobs. aegis is OUT of scope (reused verbatim; its gate-5 job short-circuits on an empty diff). **No CLAUDE.md change needed; no re-ask of permission (it is already the project default).** |

## CI Contract — confirmation and findings

### Gate 5 (mutants, 100% kill) — CONFIRMED for all four crates; ZERO jobs added

**This is the wave's one genuine infra confirmation.** The brief flagged a
possible gap: a crate touched by this feature without a per-crate
`gate-5-mutants-<crate>` job would escape per-feature mutation. Verified — there
is NO gap; all four exist:

| Crate (touched) | Existing gate-5 job | ci.yml line | `--package` | diff glob | Verdict |
|---|---|---|---|---|---|
| `query-http-common` (the shared capability — MOST important) | `gate-5-mutants-query-http-common` | 1868 | `query-http-common` | `crates/query-http-common/**` | ✓ EXISTS — no add |
| `query-api` (metrics, thin wiring) | `gate-5-mutants-query-api` | 1095 | `query-api` | `crates/query-api/**` | ✓ EXISTS — no add |
| `log-query-api` (logs, thin wiring) | `gate-5-mutants-log-query-api` | 1182 | `log-query-api` | `crates/log-query-api/**` | ✓ EXISTS — no add |
| `trace-query-api` (traces incl. lookup-by-id, thin wiring) | `gate-5-mutants-trace-query-api` | 1356 | `trace-query-api` | `crates/trace-query-api/**` | ✓ EXISTS — no add |

Each job runs `cargo mutants --package <crate> --in-diff "$DIFF_FILE"` against
`git diff "$BASELINE" HEAD -- 'crates/<crate>/**'` (baseline cascade
`origin/main` -> `HEAD~1` -> full; an empty diff short-circuits to a zero-second
exit), with `timeout-minutes: 30` and a `mutants.out` artefact upload. Every
modified file lives under its crate's `crates/<crate>/`, so each glob covers that
crate's entire modified-file set and `--in-diff` mutates ONLY the changed lines.

**The shared `query-http-common` job is the load-bearing one** — the auth
capability lands there ONCE. Its primary mutation targets for this feature: the
3-arm precedence in `resolve_request_tenant_or_refuse`, the **no-bearer-bypass
branch** (arm 2 must NEVER fall through to `env_tenant`), the bearer extraction,
the pre-validate `missing_claim` event, and the 401 + `WWW-Authenticate` mapping.
A mutant that lets arm 2 downgrade to the env tenant, accepts a tokenless
request, weakens a reject `reason`, or drops the audit event MUST be killed by
the no-bearer-bypass / isolation / 8-reason-matrix / redaction tests (DISTILL
authors; DELIVER turns green). **No per-feature wiring, no new gate-5 job — all
four crates were already enrolled in the per-crate `--in-diff` model (the close
of `gate-5-mutants-batch-v0`), so this feature inherits gating for all four for
free.**

### Gate 1 (test) — CONFIRMED covers all four crates, unchanged

**Gate 1 (`cargo test --workspace --all-targets --locked`, ci.yml:136-182)** is
workspace-wide, so it runs the src + integration tests of all four touched
crates. It runs the DEEP auth-acceptance suite: the valid-token accept; the
positive+negative tenant-isolation control on metrics, logs, traces, AND trace
lookup-by-id; the no-token-401-before-store control; the no-bearer-bypass control
(auth-on + env tenant ALSO set + no bearer -> 401, NOT env-scoped); the 8-reason
reject matrix; the ingest-audience -> `wrong_audience` cross-surface fence; the
secret/token-never-logged redaction guardrail; the one-audit-event-per-request
assertion; and the backward-compat (auth-off) non-regression. Per ADR-0072 this
`--all-targets` invocation is the CI-only deep gate; the local pre-commit runs
only the fast `cargo test --workspace --lib --locked` subset, and `ci-watch.sh`
is the post-push safety net for the deep gates. DISTILL authors these tests;
DELIVER turns them green. No Gate 1 change.

### Gate 4 (cargo deny) — CONFIRMED: NO new external/license/advisory surface, NO new edge

1. **No new dependency edge.** All FOUR touched crates ALREADY declare
   `aegis = { path = "../aegis", version = "0.1.0" }` (non-wildcard PATH dep, the
   workspace cargo-deny rule) — they import `aegis::TenantId` today (ADR-0074
   Context). The `aegis::Validator` import this feature adds rides the SAME
   existing edge. `aegis` is already a workspace member at `0.1.0` in `Cargo.lock`.
2. **No new third-party crate enters the lockfile.** aegis's HS256 engine is
   `jsonwebtoken 9.3.1`, already present in `Cargo.lock` (single entry, pulled
   transitively by aegis) and already vetted by the existing Gate 4
   (`cargo deny --all-features check`, ci.yml:83-114) pass.
3. **Therefore Gate 4 is a NO-OP CONFIRMATION, not a new check.** No new external
   surface, no new license to allow, no new advisory to triage, no new edge to
   enforce. cargo-deny sees no delta from this feature.

### Gate 2 (public-api) + Gate 3 (semver) — CONFIRMED: do NOT fire, NO semver bump

1. **None of the four touched crates is enrolled.** Gate 2 (`cargo public-api`)
   and Gate 3 (`cargo semver-checks`) are enrolled for ONLY the four **graduated**
   packages — `otlp-conformance-harness`, `spark`, `sieve`, `codex`
   (`scripts/hooks/pre-push` line 54: `for pkg in otlp-conformance-harness spark
   sieve codex`). Neither `query-http-common` nor any of the three read APIs is in
   the loop, and `aegis` is not either.
2. **And there is no break to flag anyway.** Per ADR-0074 §"Public-API / semver
   posture": the new `query-http-common` function is additive `pub`; the router
   `Option<Arc<Validator>>` state change is breaking to IN-CRATE callers only (the
   three binaries, updated in lockstep); `aegis` is UNCHANGED (reused verbatim).
   So even if these crates WERE enrolled, Gate 2/Gate 3 would find no public diff.
3. **Therefore NO semver bump is needed.** All four crates stay pre-1.0. DELIVER
   must NOT bump any of the four `Cargo.toml` versions, nor aegis's. There is no
   public-api baseline to update for any of them. (Were a public type ever to leak
   in a future change, it would be semver-MINOR at most, pre-1.0, **NEVER 1.0.0** —
   Andrea's call.)
4. **Decision: do NOT enrol any of these crates into Gate 2/Gate 3 in this wave.**
   Graduating a crate into the public-surface lock is a separate, deliberate
   decision. Flagged, not actioned.

## gate-5-mutants jobs — EXISTED vs ADDED (explicit, as the brief requires)

| Crate | gate-5-mutants-<crate> | EXISTED or ADDED |
|---|---|---|
| query-http-common | gate-5-mutants-query-http-common (ci.yml:1868) | **EXISTED** |
| query-api | gate-5-mutants-query-api (ci.yml:1095) | **EXISTED** |
| log-query-api | gate-5-mutants-log-query-api (ci.yml:1182) | **EXISTED** |
| trace-query-api | gate-5-mutants-trace-query-api (ci.yml:1356) | **EXISTED** |

**All four EXISTED. ZERO added. No `.github/workflows/ci.yml` change. No commit
made by this wave.** (Per the brief: if no workflow change is needed, make NO
commit — the orchestrator commits the docs between waves.)

## Infrastructure Summary

- **New infrastructure**: none. No crate, no container, no service, no cloud
  resource, no IaC, no orchestration, no new always-running task.
- **New dependency edge**: none. All four touched crates already dep aegis
  (path, 0.1.0); the `Validator` import is on the same edge.
- **New external dependency**: none. jsonwebtoken 9.3.1 already in `Cargo.lock`,
  already vetted by Gate 4. No new external/license/advisory surface.
- **CI changes**: none. The five ADR-0005 gates are inherited unchanged; ALL
  FOUR relevant Gate 5 jobs already path-filter `--in-diff` onto their crate's
  modified files. No new job, no edit to an existing job, no commit.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for an
  internal multi-crate change, NOT deploy targets. See `environments.yaml`.
- **Deployment precondition (CONDITIONAL, operational)**: a read binary
  configured WITH auth requires a COMPLETE read-auth config (issuer + audience
  `kaleidoscope-query` + secret_file + catalogue) and the auth startup negative
  probe must pass, or it refuses to start. An UNCONFIGURED binary runs as today
  (env-tenant mode, additive opt-out). A PARTIAL config is the ONE
  refuse-to-start trap. Captured in `environments.yaml > deployment_assumptions`
  and C-DEVOPS-2 below.
- **Auth test environment**: in-suite-minted HS256 tokens driving the THREE real
  read binaries over the `Authorization` header against an EPHEMERAL bind, with
  valid/invalid/missing/wrong-audience variants — a TEST concern, no infra, no
  real IdP, no JWKS, no network at validation time. Recorded in
  `environments.yaml > auth_test_environment`.
- **Observability**: the aegis one-event-per-validate audit line on each read
  API's existing stderr stream (via the shared `init_tracing` subscriber) + the
  one shared-capability pre-validate `missing_claim` line + the
  health.startup.refused / config_validation_failed startup signals (existing
  conventions); no new metric, no new dashboard, no new stack. Redaction is
  structural.
- **Rollback**: `git revert` (trunk-based); the read APIs are stateless and the
  happy-path wire/response contracts are unchanged. CAVEAT: reverting an auth-ON
  deployment re-opens it to env-tenant-only resolution (drops per-request
  isolation) — a deliberate SECURITY decision; prefer fix-forward.

## KPI Instrumentation (outcome-kpis.md handoff)

No new telemetry stack — the six KPIs are measured by CORRELATING the existing
stderr JSON audit events with the per-request store read:

- **KPI-1 + KPI-4 (NORTH STAR — % of authenticated reads scoped to the token's
  tenant, isolation negative control passing)**: correlate `decision=allow`
  events (`tenant_id`, `subject`) with the scoped store read per request; the
  isolation positive+negative control (right tenant present, wrong tenant ABSENT,
  incl. trace lookup-by-id) is the per-delivery/CI assertion. Target 100%;
  baseline 0% (per-deployment tenancy today). **KPI-1/KPI-4 < 100% (any
  cross-tenant read, or any authenticated read not scoped to its token's tenant)
  is a CRITICAL alert.**
- **KPI-2 (100% of invalid/tokenless requests refused 401, nothing read)**: per
  rejected request, assert 401 + `WWW-Authenticate: Bearer` AND absence of a
  store read. **Any store read on a refused request is a CRITICAL guardrail
  alert.**
- **KPI-3 (every read denial reports a distinct aegis reason)**: tally the
  `reason`-field distribution across deny events (8 reasons; zero "unknown/other"
  bucket). Reason-distribution panel for Priya.
- **KPI-5 (backward compat)**: the existing read-API slice tests stay green
  byte-for-byte (auth off); the no-bearer-bypass assertion (auth on + missing
  token -> 401, NOT env-tenant).
- **KPI-6 (audience fence)**: tally `reason=wrong_audience` for ingest-audience
  tokens on the read path.
- **Redaction guarantee (hard guardrail)**: **any secret-bytes- OR raw-token-
  bytes-in-logs occurrence is a CRITICAL alert** (System Constraint 4). The
  guarantee is STRUCTURAL — composition holds `secret_file: PathBuf` (never the
  bytes); aegis opaque-Debugs the signing key; config errors name the file by
  path only; the 401 reason carries the aegis `reason()` class name, never the
  raw token; the existing query-http-common reason-redaction tests
  (lib.rs:403-416) extend to the bearer-derived 401s.

**Carried by the EXISTING surface (REUSED verbatim)**: the JSON-stderr subscriber
(`init_tracing`), the aegis one-event-per-validate decision line (allow/deny + the
8 reasons + the field contract), the structural redaction discipline, and the 401
error envelope.

**What DELIVER must WIRE (new per this feature)**: (a) the ONE pre-validate
`missing_claim` decision event in the shared capability (emitted in aegis's field
shape, since aegis never sees the empty-bearer case); (b) the `subject` value
(query_range/log_query/trace_query) on each read API's validate call; (c) the
auth STARTUP NEGATIVE PROBE + the health.startup.refused signal (DD4); (d) the
config_validation_failed refusal for a PARTIAL config (DD1). Baselines are 0%
per-request by construction — no baseline-measurement step needed.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change; no commit.** All four
  `gate-5-mutants-<crate>` jobs already exist and cover all modified files via
  `--in-diff` (everything lives under `crates/<crate>/`). DELIVER must NOT add a
  per-feature gate-5 job and must NOT add an aegis mutation job (aegis is reused
  verbatim; its gate-5 job short-circuits on an empty diff).
- **C-DEVOPS-2 — CONDITIONAL auth-config deployment precondition (the one
  operational item).** Read auth is OPTIONAL (additive). A read binary configured
  WITH auth requires a COMPLETE read-auth config (ISSUER + AUDIENCE
  `kaleidoscope-query` + SECRET_FILE + CATALOGUE, keyed off each binary's env
  prefix `KALEIDOSCOPE_[LOG_|TRACE_]QUERY_AUTH_*`) AND the auth startup negative
  probe must pass, or it refuses to start. An UNCONFIGURED binary runs as today
  (env-tenant mode). A PARTIAL config is the ONE refuse-to-start trap
  (config_validation_failed naming the missing field; no listener). There is NO
  `enabled=false` flag. SECRET HANDLING: HS256 secret BY FILE REFERENCE (never
  inline); operator MUST file-permission-restrict + MUST NEVER commit; config
  errors name the file BY PATH ONLY; no token/secret in any audit/deny/error line
  (structural per DD1). Dev/test uses a throwaway secret + one-tenant catalogue.
  Release notes must flag that an auth-configured read API will now reject
  tokenless/invalid callers and will NOT downgrade them to the env tenant (the
  intended security change).
- **C-DEVOPS-3 — NO new dependency edge / external dependency; cargo-deny (Gate
  4) satisfied.** All four crates already dep `aegis = { path = "../aegis",
  version = "0.1.0" }` (non-wildcard, already in Cargo.lock 0.1.0); the
  transitive jsonwebtoken 9.3.1 is already present and already vetted. NO new
  external/license/advisory surface. Gate 4 is a no-op confirmation. DELIVER must
  keep the aegis dep non-wildcard on all four crates.
- **C-DEVOPS-4 — NO public-API break, NO semver bump.** The new query-http-common
  function is additive `pub`; the router `Option<Arc<Validator>>` state change is
  breaking to in-crate callers only; aegis is UNCHANGED. None of the four crates
  is enrolled in Gate 2/Gate 3, AND there is no break to flag anyway. DELIVER must
  NOT bump any of the four `Cargo.toml` versions, nor aegis's; there is NO
  public-api baseline to update. NEVER 1.0.0 — Andrea's call.
- **C-DEVOPS-5 — Auth tests must be deterministic; the DEEP suite gates in CI,
  the FAST subset gates locally (ADR-0072).** In-suite-minted HS256 tokens +
  reject/accept + reason-string + store-read-present/absent + audit-line-count
  (exactly one) + 401-status + WWW-Authenticate + redaction (no secret/token
  bytes) assertions, NO wall-clock threshold — so neither the local hook nor CI
  flakes under overnight load (the p95-flake class does NOT apply; these are
  boolean/equality/exit-code assertions). The deep `--all-targets` auth-acceptance
  integration tests gate in CI (gate-1-test); the local pre-commit runs only the
  fast `--lib` subset; `ci-watch.sh` is the post-push safety net.
- **C-DEVOPS-6 — Falsifiability + non-regression are mandatory.** Each reject AC
  must produce HTTP 401 + `WWW-Authenticate: Bearer` with the exact aegis
  `reason`, the store NEVER queried, and EXACTLY ONE deny audit line. The
  NO-BEARER-BYPASS control (auth on + env tenant also set + no bearer -> 401, NOT
  env-scoped) must FAIL against any env-tenant fall-through. The isolation
  negative control (wrong tenant's data ABSENT, incl. trace lookup-by-id) is
  mandatory. The fail-closed-config controls (partial config / unreadable secret
  -> refuse to start naming the field/path, NO secret bytes) and the auth startup
  negative probe (known-bad token rejects before bind) are required. The ACCEPT +
  backward-compat controls (valid token reads byte-shape-identical; auth-off is
  today's behaviour) bound the regression to unauthenticated callers on
  auth-configured deployments.
- **C-DEVOPS-7 — Guardrails must stay green.** The existing read-API slice tests
  (auth off) must stay green byte-for-byte (KPI-5); the env-tenant happy path +
  response shape, the unset-tenant 401, zero secret/token bytes in any
  log/event/error/body (a CRITICAL guardrail), and no store read on a refused
  request (a CRITICAL guardrail) must not regress. Gate 5 must reach 100% kill on
  the modified files in all four crates (D9). pulse/lumen/ray are UNTOUCHED.
- **C-DEVOPS-8 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy is
  already pinned (D9); no permission re-ask needed (it is the project default).
- **C-DEVOPS-9 — The capability lands ONCE in query-http-common.** DELIVER must
  NOT triplicate the auth logic across the three read crates (ADR-0074 Option B
  rejected). The shared `resolve_request_tenant_or_refuse` is the single
  resolution point; the three APIs are thin wiring (one swapped handler call + the
  optional config + the startup probe each). The no-bearer-bypass property is one
  auditable branch in one shared function.

## Upstream Changes

**None expected.** DESIGN resolved DD1-DD6 into locked ACs for DISTILL; this
DEVOPS wave CONFIRMS (rather than corrects) the existing ADR-0005 CI contract
covers all four crates (incl. each crate's Gate 5 mutation job), the no-new-edge
/ cargo-deny posture, and the no-public-API-break / no-semver-bump posture — all
against the live source, the lockfile, and the workflow. No shared assumption
needed correcting; ADR-0074 and the brief already state these postures and the
CI/lockfile inspection agrees. No story re-scoping; no DISCUSS/DESIGN delta. No
`devops/upstream-changes.md` is created.

## Production Readiness (scoped to an internal multi-crate change + a conditional config precondition)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance tests defined (by DESIGN/DISTILL) for the accept + isolation
      (positive+negative, incl. trace lookup-by-id) + the no-token-401-before-
      store + the no-bearer-bypass + the 8-reason reject matrix + the
      wrong_audience fence + the redaction guardrail + one-audit-event-per-request
      + the backward-compat suite, driven through the three real binaries with
      in-suite-minted HS256 tokens; DISTILL authors them, DELIVER turns them green
      (KPI-1..6).
- [x] Mutation gate (Gate 5, 100% kill) auto-covers all modified files in ALL
      FOUR crates via the four existing `gate-5-mutants-<crate> --in-diff` jobs
      (D9); aegis short-circuits (out of scope).
- [x] cargo-deny (Gate 4) confirmed: no new dependency edge (all four crates
      already dep aegis path 0.1.0); jsonwebtoken already vetted; NO new
      external/license/advisory surface.
- [x] No-public-API-break / no-semver-bump confirmed against the live source, the
      lockfile, and the workflow (all four crates stay pre-1.0; aegis unchanged).
- [x] Auth decisions surfaced on existing channels (DD5): aegis's one-event-per-
      validate audit line via the shared init_tracing subscriber + the one
      shared-capability pre-validate `missing_claim` line + the
      health.startup.refused / config_validation_failed startup signals.
- [x] No new event family / metric / dashboard / observability stack; the six KPIs
      are correlations over the existing audit<->store-read event streams.
- [x] Redaction guarantee structural (PathBuf not bytes; opaque-Debug; path-only
      errors; reason = aegis class name); a dedicated AC + hard guardrail.
- [x] **CONDITIONAL operational precondition documented**: auth-on requires a
      complete config + a passing startup probe or refuses to start; partial
      config is the one refuse-to-start trap; absent config is the supported
      additive opt-out. The operator must provision + permission-restrict +
      never-commit the HS256 secret + the tenant catalogue (C-DEVOPS-2,
      `environments.yaml > deployment_assumptions`).
- [x] Rollback posture: `git revert`; the read APIs are stateless, happy-path
      wire/response contracts unchanged — but reverting an auth-ON deployment
      re-opens it to env-tenant-only resolution (a deliberate security decision;
      prefer fix-forward).
- [n/a] Canary / blue-green / rolling — no deployment surface (the change adds a
      conditional startup precondition but no rollout).
- [n/a] On-call / runbook — operators run / orchestrate the binaries; the aegis
      audit events + the health.startup.refused / config_validation_failed signals
      ARE the operator-facing signals. A fleet alert on KPI-1/KPI-4 /
      secret-or-token-in-logs / store-read-on-refusal is the standard DEVOPS
      instrumentation task (KPI Instrumentation above).

## Peer Review — Self-Review (reviewer not nested-invocable)

The `nw-platform-architect-reviewer` Agent could not be invoked as a nested
subagent from within this subagent context (the identical constraint was recorded
for the prior slim-DEVOPS features, e.g. `aegis-ingest-auth-v0/devops`). Per the
established slim-DEVOPS precedent, a structured self-review was conducted against
the reviewer's exact critique dimensions. The dispatch carries the nWave-order
reminder (no code/tests/CI exist at DEVOPS time — that absence is expected, not a
rejection reason).

### Dimension 1 — CI coverage complete for all four crates (incl. mutation)

| Claim | Evidence | Verdict |
|---|---|---|
| All four `gate-5-mutants-<crate>` jobs exist with correct package + glob | ci.yml:1868 (query-http-common), :1095 (query-api), :1182 (log-query-api), :1356 (trace-query-api); each `--package <crate>` + `git diff ... -- 'crates/<crate>/**'` grep-verified | ✓ verified |
| Gate 1 covers all four crates | `cargo test --workspace --all-targets --locked` (ci.yml:136-182) is workspace-wide | ✓ verified |
| Gate 4 covers the dep graph; no new edge | all four Cargo.toml already dep `aegis = { path = "../aegis", version = "0.1.0" }` (grep-verified); jsonwebtoken 9.3.1 single entry in Cargo.lock | ✓ verified |
| Gate 2/Gate 3 do NOT cover the four crates (consistent with no-break) | pre-push line 54 enrols only otlp-conformance-harness/spark/sieve/codex | ✓ verified |
| ALL FOUR gate-5 jobs EXISTED; ZERO added; no commit | grep of ci.yml; no edit made | ✓ verified |

**CI coverage is complete for all four crates across the five gates. No gap.**

### Dimension 2 — Environment inventory present

`environments.yaml` lists `clean` + `with-pre-commit` + `ci`, the auth-test
environment (in-suite HS256 tokens driving the three real binaries over the
`Authorization` header against an ephemeral bind, with valid/invalid/missing/
wrong-audience variants + the two load-bearing isolation/no-bypass controls + the
fail-closed-config controls + the auth startup probe), the conditional deployment
precondition, and the rollback posture. The ADR-0072 local-fast / CI-deep split is
captured (the deep auth-acceptance suite gates in CI; the local hook runs the
fast `--lib` subset; ci-watch.sh is the safety net). ✓ present and matches the
slim sibling precedent.

### Dimension 3 — Observability aligned to KPIs + the redaction guarantee

The six KPIs are each mapped to a concrete correlation over the existing stderr
JSON audit events (north star = KPI-1+KPI-4; leading = KPI-2/3/6; guardrail =
KPI-5 + the redaction/no-store-read hard guardrails), with the CRITICAL alert
thresholds carried from outcome-kpis.md (KPI-1/4 < 100%; secret-or-token-in-logs;
store-read-on-refusal). The redaction guarantee is documented as STRUCTURAL
(PathBuf not bytes; opaque-Debug; path-only errors; reason = aegis class name;
the existing reason-redaction tests extend). What is REUSED verbatim vs what
DELIVER must WIRE is split explicitly (the subscriber + the aegis audit event +
the structural redaction + the 401 envelope are reused; the pre-validate
`missing_claim` event + the `subject` value + the startup probe + the partial-
config refusal are new). ✓ complete and KPI-aligned.

### Dimension 4 — Rollback-first

`environments.yaml > rollback` and D6 + Infrastructure Summary state the rollback:
`git revert`, stateless query front-ends over the UNTOUCHED stores, happy-path
wire/response contracts unchanged, WITH the explicit caveat that reverting an
auth-ON deployment re-opens it to env-tenant-only resolution (drops per-request
isolation) so fix-forward is preferred. Rollback is present and honestly
caveated for a tenant-isolation feature. ✓

### Dimension 5 — Shift-left security / secret handling

The HS256 secret is handled structurally (by file reference, PathBuf not bytes,
opaque-Debug, path-only errors, no token/secret in any audit/deny/error line) +
operationally (permission-restrict, never-commit, rotation = replace + restart).
Fail-closed: partial config refuses to start (no `enabled=false` trap); the auth
startup negative probe proves the lock rejects before binding. The cross-surface
audience fence (`kaleidoscope-query` vs `kaleidoscope-ingest`) is documented. ✓

### Dimension 6 — DORA / delivery posture

Trunk-based, CI-as-feedback (no required checks, no enforce_admins — project
memory). The change is a moderate batch (the capability lands once in a shared
crate + three thin wirings), inherits the existing five-gate pipeline unchanged,
and adds no lead-time or change-failure surface beyond the intended behaviour
change (rejecting tokenless callers on auth-configured deployments). No DORA
regression; the feature improves the read tier's failure posture (per-request
isolation + fail-closed-before-store). ✓

### Dimension 7 — Handoff completeness (DISTILL / DELIVER)

Nine explicit Constraints (C-DEVOPS-1..9) hand DISTILL/DELIVER: no new CI job /
no commit; the conditional refuse-to-start deployment precondition + secret
handling; the no-new-edge / cargo-deny confirmation; the no-public-API-break /
no-semver-bump confirmation; deterministic auth tests with the ADR-0072
local-fast/CI-deep split; falsifiability + the no-bearer-bypass + non-regression;
guardrails-green; no CLAUDE.md change; the capability-lands-once constraint. The
KPI Instrumentation section hands the measurement plan with the
reused-vs-must-wire split. ✓ complete.

### Dimension 8 — No overstated readiness / simplest-solution

No new infrastructure, no new crate, no new CI job, no new observability stack, no
new dependency edge. Every "no new X" is justified by "an existing alternative
covers it" (the four existing gate-5 jobs; the existing Gate 4 deny pass; the
existing stderr audit stream + init_tracing subscriber; the already-present aegis
edge). No readiness is overstated: the wave CONFIRMS coverage and DOCUMENTS the
conditional precondition; it does not claim code/tests exist (they do not at
DEVOPS time — expected). The one honest residual (a revert re-opens an auth-ON
deployment) is disclosed, not hidden. ✓ passes the simplest-solution check.

### Verdict

**APPROVED_PENDING_INDEPENDENT_REVIEW** — 0 blocking issues (0 CRITICAL, 0 HIGH,
0 MEDIUM, 0 LOW).

The two artefacts correctly (a) confirm the existing ADR-0005 five-gate CI
contract covers all FOUR touched crates, with all FOUR `gate-5-mutants-<crate>`
jobs already present (ZERO added, no commit) and the workspace Gate 1 + Gate 4
covering all four; (b) confirm cargo-deny (Gate 4) is satisfied with NO new
dependency edge and NO new external/license/advisory surface (all four crates
already dep the in-workspace aegis path; jsonwebtoken already vetted); (c) confirm
NO public-API break and NO semver bump (none of the four crates enrolled in
Gate 2/3, no break to flag, stay pre-1.0, NEVER 1.0.0); (d) document the
CONDITIONAL auth-config deployment precondition (auth-on => complete-config-or-
refuse-to-start + startup probe; partial-config => the one refuse-to-start trap;
absent-config => the supported additive opt-out) with full secret-handling
guidance; and (e) confirm the KPI/redaction instrumentation (the six KPIs as
correlations over the existing audit<->store-read events; the structural
redaction guarantee; the reused-vs-must-wire split). An independent top-level
`nw-platform-architect-reviewer` run is recommended before DISTILL.

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (all four `gate-5-mutants-<crate>`
  jobs exist and are untouched; trunk-based, no required checks).
- Does not make any commit (no workflow change was needed; the orchestrator
  commits the docs between waves).
- Does not add an aegis mutation job (aegis is reused verbatim; its gate-5 job
  short-circuits on an empty diff).
- Does not enrol any of the four crates into Gate 2/Gate 3 (a separate graduation
  decision; flagged, not actioned).
- Does not write production code or the auth / isolation / fail-closed tests
  (crafter owns DELIVER; acceptance-designer owns the test specs in DISTILL).
- Does not provision the operator's HS256 secret file or tenant catalogue (a
  deployment-time operational responsibility; this wave only documents the
  conditional precondition).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned; no
  permission re-ask).
- Does not bump any `Cargo.toml` version (all four crates + aegis stay pre-1.0 —
  NO break, NO bump, NEVER 1.0.0).
- Does not change any aegis dep specifier to a wildcard (the path deps stay
  non-wildcard for cargo-deny on all four crates).
- Does not proceed into DISTILL.
