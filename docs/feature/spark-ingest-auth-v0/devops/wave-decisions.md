# Wave Decisions — spark-ingest-auth-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-06
- **Mode**: Autonomous overnight run. **SLIM** wave — an INTERNAL,
  single-crate change to the EXISTING `spark` SDK crate; NO new crate, NO new
  dependency, NO deploy surface, NO new infrastructure. **The real difference
  from the recent cinder/aperture slim-DEVOPS waves: `spark` IS one of the
  four packages GRADUATED into Gate 2 (cargo public-api) and Gate 3 (cargo
  semver-checks), so the additive public method `with_bearer_token` HAS a real
  CI consequence DELIVER must action.**
- **Inputs read**: `design/wave-decisions.md` (DD1-DD5 resolved; the
  public-api/semver consequence section),
  `docs/product/architecture/adr-0069-spark-ingest-auth.md` (incl. its
  "Public-API / semver posture" section + DEVOPS-flag),
  `discuss/outcome-kpis.md` (KPI-1..4 + DEVOPS handoff "no new metric, no new
  dashboard"), ADR-0005 (the five workspace gates),
  `.github/workflows/ci.yml` (Gate 2 :248/332-349, Gate 3 :356/419-436, Gate 4
  :84, Gate 1, gate-5-mutants-spark :606-692), `scripts/hooks/{pre-commit,
  pre-push}` (pre-push Gate 2/Gate 3 loop lines 54/77), `crates/spark/
  Cargo.toml` (version = "0.1.0"), `Cargo.lock` (tonic + percent-encoding
  already present), `CLAUDE.md` / MEMORY, and the prior slim-DEVOPS shape
  (`aperture-serve-loop-error-surfacing-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | DD1-DD5 resolved (one `build_auth_metadata` helper + apply-shim to all three exporters; `with_bearer_token` + precedence; `BearerToken` redacting newtype; OTLP_HEADERS authorization-only percent-decode; silent-but-documented no-token); the explicit "Public-API / SemVer consequence" section naming Gate 2/3 + the minor bump; Reuse = REUSE+EXTEND+thin-CREATE, no new dep | − none; DESIGN hands DEVOPS the mutation scope (the modified spark files) and the public-api/semver consequence, both confirmed below |
| ADR-0069 | the mechanism (`.with_metadata(MetadataMap)`, DD1); the SECRET-never-logged posture (DD3, structural redaction); the test seam (authenticated-aperture E2E + unit helper assertion + never-log grep + no-token non-regression); §"Public-API / semver posture (REAL DIFFERENCE — flag for DELIVER)" stating Gate 2 must regenerate/accept the baseline and Gate 3 needs a MINOR bump (pre-1.0, NEVER 1.0.0) | − ADR-0069 says "regenerate/accept the public-api baseline" generically; this wave PINS the mechanism on THIS repo (checkout-diff, not a snapshot file) so DELIVER knows exactly what "regenerate" means — see CI Contract Gate 2 below |
| `discuss/outcome-kpis.md` | KPI-1 (authenticated three-signal export ACCEPTED, North Star, E01-E04 GREEN), KPI-2 (env-var path), KPI-3 (zero token leak, guardrail/defect gate), KPI-4 (no-auth path unchanged, guardrail); DEVOPS handoff: NO new metric / dashboard / alert — KPI-1/2 ride the existing aperture audit, KPI-3/4 are CI test gates | − none; the handoff explicitly states no platform-level data collection is new |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all run on every push to main | − spark IS in the Gate 2/Gate 3 graduated set (UNLIKE aperture); that is the load-bearing fact this wave confirms and acts on |
| `.github/workflows/ci.yml` | `gate-5-mutants-spark` (:606-692) exists, `--in-diff` path-filtered on `crates/spark/**`; Gate 2 `cargo public-api -p spark` (:332-335, :347); Gate 3 `cargo semver-checks --package spark --baseline-rev origin/main` (:425-427); Gate 4 (:84); Gate 1 | − Gate 2 uses `--diff-git-checkouts origin/main HEAD ... --deny=added` — there is NO snapshot baseline FILE; the baseline IS the git origin/main checkout (pinned below so DELIVER does not hunt for a `.txt`) |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = Gate 4 + Gate 1 (the local mirror); pre-push = Gate 2/Gate 3 for the 4 graduated pkgs | − pre-push lines 54 & 77 INCLUDE spark in the loop, so DELIVER's push WILL exercise Gate 2/Gate 3 for spark LOCALLY — the bump + additive surface must be in the commit before push (CONTRAST aperture, which was absent from the loop) |
| `crates/spark/Cargo.toml` | `version = "0.1.0"`; the runtime deps (opentelemetry family `=0.27`, transitive tonic) | − the bump target is `0.2.0` (MINOR), pinned below |
| `Cargo.lock` | `tonic` (line 2554) and `percent-encoding` (line 1528) already present | − confirms NO new external dependency (Gate 4 unaffected); DELIVER may reuse percent-encoding for DD4 |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and no CI-config change is made by
this wave.** The feature modifies the existing source files inside the single
crate `spark` (`src/config.rs` — the `bearer_token` field + `with_bearer_token`
method; `src/init.rs` — the `WithTonicConfig` import, `build_auth_metadata`,
the per-signal apply-shim, the three `.with_metadata(map.clone())` calls,
`resolve_bearer_token`, the `OTEL_EXPORTER_OTLP_HEADERS` parser; and the
`BearerToken` redacting newtype wherever DELIVER places it; `src/observability.rs`
is EXPECTED to stay UNCHANGED — DD3 keeps `emit_init_succeeded`'s closed
vocabulary untouched). spark already owns a path-filtered `gate-5-mutants-spark`
`--in-diff` job that mutates exactly its changed lines automatically.

**The load-bearing difference (vs aperture/cinder slim-DEVOPS): `spark` IS
GRADUATED into Gate 2 and Gate 3.** Adding the public method
`SparkConfig::with_bearer_token` is an additive public-API change. Therefore
DELIVER MUST (1) accept/regenerate the spark public-api surface and (2) bump
spark's MINOR version `0.1.0 -> 0.2.0` (pre-1.0, NEVER 1.0.0) in the SAME
commit. Both are detailed in CI Contract > Gate 2 / Gate 3 below.

**Confirmed against the live source**: `crates/spark/Cargo.toml` is
`version = "0.1.0"`. `tonic` (Cargo.lock:2554) and `percent-encoding`
(Cargo.lock:1528) are already in the lockfile — NO new external dependency.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production code, NO tests, and NO CI-config
changes exist yet for this feature. That absence is the EXPECTED and CORRECT
state — it is not a finding. This wave's job is to CONFIRM the existing
ADR-0005 CI contract covers the feature, CAPTURE the public-api/semver
consequence precisely, and produce `environments.yaml` + this file; review
THAT, not the non-existence of code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers the change and that the local pre-push hook will surface
the Gate 2/Gate 3 consequence to DELIVER before the push.

## Decision summary (D1-D9, all existing / inherited — brownfield, NOT a deploy)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | spark is an embeddable Apache-2.0 SDK library linked into integrator applications. Kaleidoscope deploys nothing for this feature. No deploy step is added or required. |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface added by this wave. The SDK gains an optional gRPC metadata header on exports it already makes. |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged by this wave. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the modified spark files automatically (Gate 5 via the existing `gate-5-mutants-spark --in-diff` job). Gate 2/Gate 3 DO cover spark (it is graduated) — and they fire on the additive method (the real consequence, CI Contract below). No new gate, no CI edit. |
| D5 | Observability | **Existing — NO new metric / dashboard / stack** | Per the outcome-kpis.md DEVOPS handoff and ADR-0068's "no new metric, no new dashboard". KPI-1/2 (accepted authenticated export) ride the EXISTING aperture audit stream; KPI-3/4 are CI TEST GATES (KPI-3 a hard defect gate anchored by Gate 5 100% kill). The token is structurally redacted (DD3); `emit_init_succeeded`'s closed vocabulary is UNCHANGED. No fleet alert (a future separate feature if ever wanted). |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; spark is a stateless SDK (no WAL/snapshot/on-disk format), and the wire change is an OPTIONAL extra `authorization` metadata header present only when a token is configured. A revert restores the prior no-knob behaviour with no data or wire-format consideration (semver-revert note in environments.yaml > rollback). |
| D7 | Continuous learning | **N/A** | No live telemetry loop owned by this feature; the KPIs are the aperture-audit accept signal (KPI-1/2) + CI test gates (KPI-3/4) — the project's raw-observation idiom. |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. NB the Gate 2 `--deny=added` checkout-diff means a feature-branch run reports the addition (expected); it clears once the additive commit is on main (CI Contract > Gate 2). |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5 / CLAUDE.md)** | Already pinned in CLAUDE.md ("This project uses per-feature mutation testing ... Kill rate gate: 100%"). Mutation scope = the modified spark files (`config.rs`, `init.rs`, the `BearerToken` newtype location; `observability.rs` is expected UNCHANGED, so it contributes no mutants). Covered by the existing `gate-5-mutants-spark --in-diff` job. **No CLAUDE.md change needed.** |

## CI Contract — confirmation and findings

### Gate 5 (mutants, 100% kill) — CONFIRMED, no new job

| Touched path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|--------------|------------------------|---------------------|-------------|----------|
| `crates/spark/src/config.rs` | the private `bearer_token: Option<BearerToken>` field (defaulted `None` in `for_service`); the additive `with_bearer_token(impl Into<String>) -> Self` builder method | `gate-5-mutants-spark` | 606-692 | ✓ `--in-diff` on `crates/spark/**` |
| `crates/spark/src/init.rs` | add `WithTonicConfig` to the `use` (:45); `build_auth_metadata(&SparkConfig) -> Option<MetadataMap>`; the per-signal apply-shim; the three `.with_metadata(map.clone())` calls on the span/log/metric builders (:282-352); `resolve_bearer_token` (the precedence chain); the `OTEL_EXPORTER_OTLP_HEADERS` parser (case-insensitive `authorization`, percent-decode, fail-fast on malformed) | `gate-5-mutants-spark` | 606-692 | ✓ same job |
| `BearerToken` newtype (DELIVER places it — likely `config.rs` or a small new module under `crates/spark/src/`) | the ~10-line redacting wrapper: redacting `Debug`, no value-`Display`, one `pub(crate)` accessor | `gate-5-mutants-spark` | 606-692 | ✓ same job (any file under `crates/spark/**`) |
| `crates/spark/src/observability.rs` | **expected UNCHANGED** (DD3 — closed vocabulary holds; no token joins the `target="spark"` surface). If DELIVER does touch it, the same `--in-diff` job auto-covers it. | `gate-5-mutants-spark` | 606-692 | ✓ same job |

The job runs `cargo mutants --package spark --in-diff "$DIFF_FILE"` against
`git diff "$BASELINE" HEAD -- 'crates/spark/**'` (baseline cascade
`origin/main` -> `HEAD~1` -> full; ci.yml:656-684; an empty spark diff
short-circuits to a zero-second exit, ci.yml:666-668). The `--in-diff` filter
means the job mutates ONLY the lines this feature changes — a mutant that
makes `build_auth_metadata` always return `None` (un-authenticating every
export), drops a `.with_metadata` call on one of the three signals (the
partial-wire failure mode), weakens the precedence resolution, collapses the
percent-decode, or un-redacts the `BearerToken` `Debug` must be killed by the
accept / all-three / env-path / never-log tests (KPI-1/2/3, Gate 5 100% kill).
**No per-feature wiring, no new gate-5 job.** spark was already enrolled in the
per-crate `--in-diff` model, so this feature inherits gating for free.

### Gate 2 (public-api) + Gate 3 (semver) — THE REAL DIFFERENCE: spark IS graduated; DELIVER must act

**This is the load-bearing finding of the wave.** spark is graduated into both
gates, and this feature adds a public method — so unlike the recent
aperture/cinder slim-DEVOPS waves (where Gate 2/Gate 3 did not fire), here they
DO, and DELIVER has two mandatory actions.

1. **spark IS enrolled.** Gate 2 (`gate-2-public-api`, ci.yml:248) runs
   `cargo public-api ... -p spark ...` (ci.yml:332-335, and the no-baseline
   branch :347). Gate 3 (`gate-3-semver`, ci.yml:356) runs
   `cargo semver-checks --package spark --baseline-rev origin/main`
   (ci.yml:425-427). spark is one of the four graduated packages
   (`otlp-conformance-harness`, **spark**, `sieve`, `codex`). The local
   pre-push hook mirrors exactly that set and INCLUDES spark
   (`scripts/hooks/pre-push` line 54 Gate 2, line 77 Gate 3:
   `for pkg in otlp-conformance-harness spark sieve codex`).

2. **The addition is a genuine public-surface change.**
   `SparkConfig::with_bearer_token(impl Into<String>) -> Self` is a new `pub`
   method on the `#[non_exhaustive]` `SparkConfig`. `BearerToken` and the
   `bearer_token` field stay `pub(crate)`/private — they do NOT enter the
   public surface; the builder method does. So Gate 2 WILL see exactly one
   added public item, and Gate 3 classifies it as MINOR (additive on a
   `#[non_exhaustive]` struct).

3. **Gate 2 — how the baseline works on THIS repo (DELIVER read this):**
   there is **NO snapshot baseline FILE** for spark (no `public-api.txt`, no
   `tests/public-api/` snapshot — searched; none exists). Gate 2 computes the
   diff with **`cargo public-api --diff-git-checkouts origin/main HEAD -p spark
   --deny=added --deny=changed --deny=removed`** (ci.yml:332-335; pre-push
   lines 55-58). **The baseline IS the git `origin/main` checkout itself.**
   Consequences:
   - "Regenerate / accept the public-api baseline" (ADR-0069's wording) is
     satisfied on THIS repo by the additive commit LANDING ON MAIN. There is
     NO snapshot file to edit. Once the DELIVER commit (adding
     `with_bearer_token`) is on `origin/main`, the next push diffs against the
     new main (which already contains the method) -> EMPTY diff -> Gate 2
     green.
   - Because the gate uses `--deny=added`, a PR / feature-branch run that diffs
     the branch against `origin/main` WILL report the added method and FAIL on
     the branch. **That failure is EXPECTED** — it is the intended signal that
     a public-surface addition occurred — and it CLEARS once the commit is on
     main. On pure trunk-based direct-to-main (project memory: no required
     status checks; CI is feedback, not a gate), the additive commit lands and
     the next push is green. DELIVER must not treat the branch-side Gate 2
     "added" report as a defect; it is the contract working as designed for an
     intended additive change.
   - The local pre-push hook runs the same `--diff-git-checkouts origin/main
     HEAD ... --deny=added` for spark, so it WILL flag the addition at push
     time. This is expected; it is the local mirror of the same intended-add
     signal.

4. **Gate 3 — VERSION BUMP REQUIRED (mandatory, in the same commit):**
   `cargo semver-checks --package spark --baseline-rev origin/main`
   classifies a new public method on a `#[non_exhaustive]` struct as **MINOR**
   (additive, non-breaking) and requires the version to reflect it. **DELIVER
   MUST bump `crates/spark/Cargo.toml`:**
   - **current: `0.1.0`**
   - **target: `0.2.0`** (MINOR — pre-1.0; **NEVER 1.0.0** — Andrea's call;
     CLAUDE.md / MEMORY `semver_one_zero_is_andreas_call`).
   The bump must be in the SAME commit as the additive method, so Gate 3's
   `--baseline-rev origin/main` comparison sees `0.1.0 (main) -> 0.2.0 (HEAD)`
   for an additive change and passes. The local pre-push semver step currently
   only WARNS on spark (pre-push lines 78-81 downgrade it to a note on a
   no-published-baseline crate), but CI Gate 3 is the authoritative comparison
   — so the bump is non-optional regardless of the local downgrade.

5. **No other crate's public surface changes; Gate 4 unaffected.** Only
   spark's surface grows. No new external dependency (tonic + percent-encoding
   already in Cargo.lock), so `cargo deny` (Gate 4) is a no-op confirmation.

**CONTRAST with the prior slim-DEVOPS waves**: aperture-serve-loop-error-
surfacing-v0 was NOT enrolled in Gate 2/Gate 3 and had no public-API break, so
its DEVOPS wave recorded "Gate 2/Gate 3 do NOT fire, NO semver bump, aperture
stays 0.1.0". **spark is the opposite**: enrolled AND an intended additive
public change, so Gate 2 fires (and clears on merge) AND a MINOR bump is
mandatory. This is the "real CI consequence" the brief flagged.

### Gates 1 and 4 — CONFIRMED unchanged

- **Gate 1 (`cargo test --workspace --all-targets --locked`)** runs the auth
  acceptance tests (E2E accept through the aegis-authenticated aperture, the
  unit `build_auth_metadata` all-three assertion, the env-path test, the
  never-log grep) and the no-token non-regression (slice_01..slice_07 + the
  two invariant suites), identically in the local pre-commit hook and CI
  (DISTILL authors the specs; DELIVER turns them green). No change.
- **Gate 4 (`cargo deny --all-features check`, ci.yml:84)** — no new
  dependency is introduced (tonic `MetadataMap`/`MetadataValue` arrive via the
  existing `opentelemetry_otlp -> tonic` chain; percent-encoding is already in
  Cargo.lock), so Gate 4 is a no-op confirmation. The Apache-2.0 licence
  containment of spark is unaffected (no new runtime dep, no licence change).

## Infrastructure Summary

- **New infrastructure**: none. No crate, no container, no service, no cloud
  resource, no IaC, no orchestration.
- **New dependency**: none. tonic `MetadataMap`/`MetadataValue` via the
  existing `opentelemetry_otlp -> tonic` chain (Cargo.lock:2554);
  percent-decode reuses the existing `percent-encoding` crate (Cargo.lock:1528)
  at DELIVER's discretion. Gate 4 unaffected.
- **CI changes**: none. The five ADR-0005 gates are inherited unchanged; the
  single relevant Gate 5 job (`gate-5-mutants-spark`, ci.yml:606-692) already
  path-filters `--in-diff` onto the modified spark files. No new job, no edit
  to an existing job.
- **THE public-api / semver consequence (the real difference — DELIVER acts):**
  1. **Gate 2 baseline = the git `origin/main` checkout** (checkout-diff, NOT a
     snapshot file). The additive `with_bearer_token` method is accepted into
     the surface by the DELIVER commit LANDING ON MAIN; there is NO `*.txt`
     baseline file to regenerate. A branch-side `--deny=added` failure is
     EXPECTED and clears on merge. **Exact mechanism path:
     `cargo public-api --diff-git-checkouts origin/main HEAD -p spark
     --deny=added --deny=changed --deny=removed` (ci.yml:332-335; pre-push
     lines 55-58).** There is no on-disk baseline artefact for DELIVER to edit.
  2. **Gate 3 minor version bump: `crates/spark/Cargo.toml` `0.1.0 -> 0.2.0`**
     (MINOR, additive; pre-1.0; NEVER 1.0.0), in the SAME commit as the method.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for an
  internal single-crate change, NOT deploy targets. See `environments.yaml`.
- **Auth test environment**: a real in-process aegis-authenticated aperture +
  recording sink + an in-suite HS256-minted token (reuses ADR-0068 F5) — a
  TEST concern, no infra, no real network peer, no secret store. Recorded in
  `environments.yaml > auth_test_environment`.
- **Observability**: NONE new. No metric, no dashboard, no stack, no alert.
  KPI-1/2 ride the existing aperture audit; KPI-3/4 are CI test gates (KPI-3 a
  hard defect gate anchored by Gate 5 100% kill). The token is structurally
  redacted (DD3); `emit_init_succeeded`'s closed vocabulary is unchanged.
- **Rollback**: `git revert` (trunk-based); spark is a stateless SDK, the wire
  change is an OPTIONAL `authorization` header present only when a token is
  configured, so a revert restores the prior no-knob behaviour cleanly. (A
  revert would remove a public method — a Gate 2 `--deny=removed` / Gate 3
  MAJOR signal — acceptable pre-1.0; see environments.yaml > rollback >
  semver_revert_note.)

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** The existing
  `gate-5-mutants-spark` job covers the modified spark files via `--in-diff`.
  DELIVER must NOT add a per-feature gate-5 job and must NOT edit ci.yml.

- **C-DEVOPS-2 — REGENERATE / ACCEPT the spark public-api baseline (the way
  THIS repo does it).** spark IS graduated into Gate 2. The baseline is the
  git `origin/main` checkout (checkout-diff via `cargo public-api
  --diff-git-checkouts origin/main HEAD -p spark --deny=added --deny=changed
  --deny=removed`, ci.yml:332-335; pre-push lines 55-58). **There is NO
  snapshot baseline FILE to edit** — the additive `with_bearer_token` method is
  accepted into the surface by the DELIVER commit LANDING ON MAIN. A branch-side
  `--deny=added` failure is EXPECTED and CLEARS on merge; DELIVER must not treat
  it as a defect. (Path DELIVER must know: there is no `crates/spark/tests/
  public-api/*.txt` or any snapshot artefact — the "baseline" is `origin/main`
  itself.)

- **C-DEVOPS-3 — BUMP spark's MINOR version `0.1.0 -> 0.2.0` (mandatory, same
  commit).** spark IS graduated into Gate 3. The additive public method on a
  `#[non_exhaustive]` struct is MINOR. DELIVER MUST bump
  `crates/spark/Cargo.toml` `version = "0.1.0"` to `version = "0.2.0"` in the
  same commit as the method. **Pre-1.0; NEVER 1.0.0** (Andrea's call;
  CLAUDE.md / MEMORY). Without the bump, Gate 3's `--baseline-rev origin/main`
  comparison fails on an unversioned additive change.

- **C-DEVOPS-4 — NO new dependency; Gate 4 is a no-op confirmation.** tonic
  `MetadataMap`/`MetadataValue` come via the existing `opentelemetry_otlp ->
  tonic` chain (Cargo.lock:2554); the DD4 percent-decode reuses the existing
  `percent-encoding` crate (Cargo.lock:1528). DELIVER must NOT add a new
  external crate. If DELIVER chooses a dependency-free hand-rolled decode
  instead, that is fine too — but DO NOT add a new external dep.

- **C-DEVOPS-5 — The token is a SECRET; the never-log invariant is a HARD CI
  defect gate (KPI-3).** Structural redaction via the `BearerToken` newtype
  (DD3); `emit_init_succeeded`'s closed vocabulary (observability.rs:53-70)
  must stay UNCHANGED. The never-log grep test asserts 0 occurrences of the
  configured token across every spark log / `Debug` / error surface; a single
  occurrence is a defect. Gate 5 must reach 100% kill on the redaction branch
  (a mutant that un-redacts the `BearerToken` `Debug` must be killed).

- **C-DEVOPS-6 — The all-three-signals property is structural and
  mutation-anchored (KPI-1).** The single `build_auth_metadata` helper +
  single call site + `MetadataMap` cloned into all three exporter builders;
  the unit assertion checks the apply-shim attaches to span/log/metric builder
  types identically. Gate 5 must kill a mutant that drops `.with_metadata` on
  any one signal (the partial-wire failure mode).

- **C-DEVOPS-7 — Non-regression guardrail (KPI-4).** No token ->
  `build_auth_metadata` returns `None` -> no `.with_metadata` call -> the
  no-token exporter build is byte-unchanged -> slice_01..slice_07 + the two
  invariant suites stay green. DELIVER must keep this control passing.

- **C-DEVOPS-8 — Tests must be deterministic and run in BOTH the local
  pre-commit hook AND CI Gate 1.** The auth accept/deny, all-three unit
  assertion, env-path, and never-log tests are boolean accept/deny + tenant-tag
  equality + substring-count assertions, NO wall-clock thresholds — so the hook
  does not flake under overnight load (the p95-flake class does NOT apply).

- **C-DEVOPS-9 — The push order is load-bearing because spark IS graduated.**
  DELIVER's commit must carry BOTH the additive method AND the `0.2.0` bump
  BEFORE `git push`, or the local pre-push Gate 2/Gate 3 loop (which includes
  spark) fails. The bump and the method are inseparable in the commit.

- **C-DEVOPS-10 — No CLAUDE.md change.** Per-feature 100%-kill mutation
  strategy is already pinned (D9); the Mutation Testing Strategy section
  already states "per-feature ... Kill rate gate: 100% (per ADR-0005 Gate 5)".

## Upstream Changes

**None.** No new crate, no new dependency, no new env var beyond honouring the
standard `OTEL_EXPORTER_OTLP_HEADERS`, no change to aegis/aperture (reuses
ADR-0068 verbatim as the gateway counterpart), no infra change, no
DISCUSS/DESIGN delta, no story re-scoping. This DEVOPS wave CONFIRMS the
existing ADR-0005 CI contract covers the feature and CAPTURES the spark-specific
Gate 2/Gate 3 public-api/semver consequence that ADR-0069 already flagged
(pinning the exact repo mechanism — checkout-diff, not a snapshot file — and
the exact bump `0.1.0 -> 0.2.0`). No shared assumption needed correcting:
ADR-0069 and the design wave-decisions already state the consequence and CI
inspection agrees.

## Production Readiness (scoped to an internal, stateless-SDK additive change)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance tests defined for the authenticated-aperture E2E accept/deny
      (KPI-1), the all-three unit assertion (KPI-1), the env-var path + precedence
      (KPI-2), the never-log grep (KPI-3), and the no-token non-regression
      (KPI-4) via the in-process authenticated-aperture + in-suite HS256 mint;
      DISTILL authors them, DELIVER turns them green.
- [x] Mutation gate (Gate 5, 100% kill) auto-covers the modified spark files
      via the existing `gate-5-mutants-spark --in-diff` job (D9).
- [x] Public-API / semver consequence captured: Gate 2 baseline = origin/main
      checkout (no snapshot file; commit-on-main accepts the additive method);
      Gate 3 MINOR bump `0.1.0 -> 0.2.0` mandatory in the same commit (NEVER
      1.0.0).
- [x] No new dependency (Gate 4 no-op); Apache-2.0 licence containment intact.
- [x] Secret posture: token structurally redacted (DD3); never-log a hard CI
      defect gate (KPI-3); closed vocabulary unchanged.
- [x] No new metric / dashboard / observability stack (outcome-kpis.md DEVOPS
      handoff); KPI-1/2 ride the existing aperture audit, KPI-3/4 are CI gates.
- [x] Rollback posture: `git revert`; spark is a stateless SDK, the wire delta
      is an optional auth header, so a revert is clean (pre-1.0 semver-revert
      note recorded).
- [n/a] Canary / blue-green / rolling — no deployment surface.
- [n/a] On-call / runbook — spark is an embedded SDK; the operator-facing
      "you forgot the token" signal is the gateway's `missing_claim` (ADR-0068),
      not a spark runtime surface.

## Peer Review

See "Self-Review" below. The `nw-platform-architect-reviewer` Agent could not
be invoked as a nested subagent from within this subagent context (the
identical constraint was recorded for the prior slim-DEVOPS features, e.g.
`aperture-serve-loop-error-surfacing-v0/devops/wave-decisions.md > Peer
Review`). Per the established slim-DEVOPS precedent on this project, a
structured self-review was conducted against the reviewer's exact dimensions
(external validity -> evidence-based findings -> severity-driven -> DORA ->
handoff completeness), carrying the nWave-order reminder (no code/tests/CI
exist at DEVOPS time — that absence is expected, not a rejection reason).
Verdict: **APPROVED_PENDING_INDEPENDENT_REVIEW**, 0 blocking issues. An
independent top-level `nw-platform-architect-reviewer` run is recommended
before DISTILL.

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (the existing
  `gate-5-mutants-spark` job is untouched; trunk-based, no required checks).
- Does not edit `.github/workflows/ci.yml`, `scripts/hooks/*`, or any
  Cargo.toml (DELIVER bumps `crates/spark/Cargo.toml` `0.1.0 -> 0.2.0`, not
  this wave).
- Does not write production code or the auth / never-log / non-regression tests
  (crafter owns DELIVER; acceptance-designer owns the test specs in DISTILL).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not add a dependency (tonic + percent-encoding already in Cargo.lock).
- Does not proceed into DISTILL.
