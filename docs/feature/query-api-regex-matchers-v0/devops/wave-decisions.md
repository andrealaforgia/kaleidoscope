# Wave Decisions - query-api-regex-matchers-v0 / DEVOPS

British English. No em dashes.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-22
- **Mode**: slim DEVOPS with one real supply-chain decision. This wave
  confirms that the existing CI contract already covers a service-internal
  logic change, and it carries out the one genuine DEVOPS task the DESIGN
  handoff flagged: the cargo-deny Gate 4 verification for the `regex` crate
  being promoted to a direct dependency. The decision to run a slim wave,
  and its shape, are Apex's own judgement from the DESIGN handoff, not
  pre-taken.

## Why this wave is slim (but not a no-op)

The feature is service-internal logic in an EXISTING service: it turns the
rejected `=~`/`!~` operators into real, fully-anchored regex label matchers
behind the same parser and `keep_row` filter, in three existing files of
`crates/query-api` (`selector.rs`, `matrix.rs`, `lib.rs`). There is no new
component, no new crate, no new public API (the `query_api::router`
signature is byte-identical, ADR-0046 Decision 5), no new HTTP route, and
no new deployment artefact. The query-api binary and the
`/api/v1/query_range` route already exist; this feature changes which
series the success arm carries and adds one new 400 arm.

Unlike a pure in-crate change, however, this feature promotes the `regex`
crate from a TRANSITIVE to a DIRECT dependency of `crates/query-api`, which
touches cargo-deny (ADR-0005 Gate 4). That is the one real DEVOPS decision
this wave verifies and records (A4 below). This follows the
`pulse-series-identity-v0` slim-DEVOPS precedent shape (two files:
environments.yaml, wave-decisions.md) and produces the same two; that
precedent had zero new dependencies, so its Gate 4 note was a one-line
"unaffected", whereas here Gate 4 needed a genuine evidence-backed
verification.

## Inputs read (in dependency order) - reads checklist

1. `CLAUDE.md` - paradigm (Rust idiomatic) and the per-feature mutation
   testing strategy at 100% kill rate (declared; not modified here). READ.
2. `../discuss/outcome-kpis.md` - the correctness north star plus KPIs 1-4
   (full-anchor, absent-label matrix, invalid-regex honesty, envelope
   guardrail) and the DEVOPS handoff (instrumentation carried, no new
   substrate, baselines at 0% / n/a). READ.
3. `../design/wave-decisions.md` - DESIGN Key Decisions, Reuse Analysis,
   and the explicit DEVOPS Handoff Annotation (new direct dependency
   `regex`; no new gate; no new external integration; instrumentation
   carried; no new probe). READ.
4. `docs/product/architecture/adr-0046-query-api-regex-label-matchers.md`
   - the companion ADR (Accepted): engine choice, anchoring, type shape,
   absent-as-empty matrix, unchanged router, and the explicit note that the
   deny.toml / Gate-4 verification is a DEVOPS task. READ.
5. `deny.toml` - Gate 4 config: the licence allow-list, `wildcards =
   "deny"`, `yanked = "deny"`, the RustSec advisory check. READ.
6. `crates/query-api/Cargo.toml` - confirmed `regex` is NOT yet a direct
   dependency (this feature adds it); confirmed the existing pin posture
   (tower-http, mutants pinned without wildcards for Gate 4). READ.
7. `Cargo.lock` - grepped `regex`, `regex-syntax`, `regex-automata`,
   `aho-corasick`, `memchr`: all present (see A4 evidence). READ.
8. `.github/workflows/ci.yml` - the existing five-gate workflow, read to
   CONFIRM (not modify) the gate scopes and the `gate-5-mutants-query-api`
   job (see "Verification against ci.yml"). READ.
9. `docs/feature/pulse-series-identity-v0/devops/{environments.yaml,
   wave-decisions.md}` - the slim-DEVOPS shape precedent for a
   library/internal feature. READ.

## Pre-wave decisions (carried in from project convention, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| P1 | `deployment_target` | None new (existing query-api binary + route; no new artefact) | DESIGN handoff + ADR-0046 |
| P2 | `container_orchestration` | N/A | service-internal logic only |
| P3 | `cicd_platform` | GitHub Actions (existing, unchanged) | ADR-0005 |
| P4 | `existing_infrastructure` | Yes (workspace + five-gate CI; `gate-5-mutants-query-api` already present, line 1036) | ci.yml |
| P5 | `git_branching_strategy` | Trunk-based, pure (main has no required-status-checks; CI is feedback, not a gate) | memory `project_kaleidoscope_pure_trunk_based` |
| P6 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md, ADR-0005 Gate 5 |

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] No new CI gate; ADR-0005's five gates inherited unchanged

The change touches three files of an already-gated crate plus one
dependency promotion. Each gate is satisfied by existing machinery:

- **Gate 1 (cargo test --workspace)**: runs the new/extended query-api
  acceptance assertions with zero workflow edit (auto-discovered under
  `crates/query-api/tests/`, driven through the existing handler via
  tower `oneshot`). This is the KPI collection surface (see mapping).
- **Gate 2 (cargo public-api)** and **Gate 3 (cargo semver-checks)**:
  scope to harness/spark/sieve/codex only; query-api is not in the locked
  set (verified below). No diff applies. The router signature is unchanged
  regardless (ADR-0046 Decision 5).
- **Gate 4 (cargo deny)**: the one gate this feature touches; verified in
  A4. No deny.toml change required.
- **Gate 5 (cargo mutants)**: covered by the existing
  `gate-5-mutants-query-api` job (A2 below).

No new or amended gate is warranted. No new CI workflow file is created;
no existing gate is added to, removed, or modified by this feature.

### [A2] Mutation testing: the existing `gate-5-mutants-query-api` job covers it; no workflow edit

**Options considered**:

1. Rely on the existing `gate-5-mutants-query-api` job (which already runs
   `cargo mutants --package query-api --in-diff` against
   `crates/query-api/**`).
2. Add a new file-scoped job pinned to the three modified files.
3. Reuse a different crate's job.

**Decision**: Option 1.

**Rationale**: `gate-5-mutants-query-api` already exists in `ci.yml` (line
1036) and runs the `--in-diff` cascade against `crates/query-api/**` (diff
built at line 1094, invocation at lines 1103-1104) with the
`origin/main -> HEAD~1 -> full` baseline, short-circuiting to a zero-second
exit on an empty diff. Because this feature touches
`crates/query-api/src/{selector.rs, matrix.rs, lib.rs}`, the diff filter
naturally limits mutation to exactly those files, which is precisely the
DESIGN-scoped mutation set. Option 2 would duplicate the existing job's
behaviour for no benefit and would require a workflow edit the feature does
not need. Option 3 loses per-package fail-fast isolation. The 100%
kill-rate gate (CLAUDE.md, ADR-0005 Gate 5) is enforced by the job's
non-zero exit on any surviving mutant.

**Primary mutation targets (per DESIGN / ADR-0046 Verification)**: the
full-anchor boundary (a mutant dropping the `^(?:...)$` wrapping so a
substring matches must DIE), the `Matches`/`NotMatches` negation, and the
invalid-vs-never-matching distinction (a 400 must not degrade to a 200
empty, nor the reverse). These all carry assertable behaviour, so mutation
is informative here, not a thin-shell case.

### [A3] No new public surface; the compiled `Regex` stays filter-side

`MatchOp` gains two variants and `LabelMatcher` is unchanged (raw pattern
in its existing `value` field); these are internal types. The compiled
`regex::Regex` lives in a filter-side value, never in the parsed types
(ADR-0046 Decision 3), because a compiled `Regex` is not `Eq`/`Hash`. The
`query_api::router` signature is byte-identical (ADR-0046 Decision 5), so
the public API of query-api is unchanged before and after the feature. This
keeps the door clean should query-api later graduate to Gates 2/3.

### [A4] cargo-deny Gate 4: VERIFIED, no deny.toml change required

This is the one real supply-chain decision the DESIGN handoff flagged.

**Question**: does promoting `regex` from a transitive to a DIRECT
dependency of `crates/query-api` introduce any new crate, new licence, new
advisory, or new yanked version that would change deny.toml or fail Gate 4?

**Evidence read in this wave** (`Cargo.lock` grep + `deny.toml` read):

| Crate | Version | Cargo.lock line | Licence (SPDX) | In deny.toml allow-list? |
|-------|---------|-----------------|----------------|--------------------------|
| `regex` | 1.12.3 | 1800 | MIT OR Apache-2.0 | Yes (MIT line 39, Apache-2.0 line 39/38) |
| `regex-automata` | 0.4.14 | 1812 | MIT OR Apache-2.0 | Yes |
| `regex-syntax` | 0.8.10 | 1823 | MIT OR Apache-2.0 | Yes |
| `aho-corasick` | 1.1.4 | 18 | MIT OR Unlicense (resolves to MIT) | Yes (MIT) |
| `memchr` | 2.8.0 | 1237 | MIT OR Unlicense (resolves to MIT) | Yes (MIT) |

(The `regex` family is uniformly dual MIT / Apache-2.0; `aho-corasick` and
`memchr`, from the same author, are MIT / Unlicense, and cargo-deny resolves
the dual choice to the allow-listed MIT side at confidence-threshold 0.8.)

**Reasoning**:

- **No new resolved crate.** All five crates are ALREADY in `Cargo.lock`,
  pulled in transitively, and Gate 4 currently passes workspace-wide.
  Adding a `regex = "1"`-style line to `crates/query-api/Cargo.toml` (at the
  version already resolved, 1.12.3) does not add a node to the resolved
  dependency graph; it only marks an already-present node as a direct
  dependency of one more crate. cargo-deny walks the resolved graph, which
  is unchanged.
- **No new licence.** Every licence in the table is already in deny.toml's
  `allow` list (lines 38-61). No BSL/SSPL/FSL/RSAL, no AGPL, nothing
  outside the permissive set. So `[licenses]` needs no edit.
- **No new advisory / no yanked version.** Gate 4 runs the RustSec
  advisory DB check (`[advisories]`, lines 91-94) and `yanked = "deny"`
  against the SAME resolved versions that already pass today. Promotion to
  direct does not change the resolved versions, so the advisory and yanked
  posture is unchanged.
- **Pin / wildcard posture.** Gate 4 sets `wildcards = "deny"` (line 84).
  The new `regex` entry MUST therefore be added without a bare wildcard
  (`"*"`); a caret requirement such as `regex = "1"` or `regex = "1.12"` is
  a SemVer requirement, not a wildcard, and satisfies the policy, matching
  the version already in the lock. This is a constraint recorded for the
  crafter at DELIVER (see "Constraints" below); it is NOT a deny.toml edit.
- **`multiple-versions = "allow"` (line 83).** Even if promotion somehow
  pulled a second version of any of these crates (it does not, since the
  version is pinned by the existing lock), the duplicate-version policy is
  already `allow`, so it could not fail Gate 4 on that axis either.

**Decision**: **No deny.toml change is required.** The reading shows no
genuine gap: no new crate, no new licence, no new advisory, no new yanked
version. deny.toml is left UNCHANGED by this wave (and was not edited). Had
the reading shown a gap (for example a regex transitive under a licence not
in the allow-list), the correct action would have been to RECORD the exact
needed allow-list addition here as a recommendation for the crafter rather
than to edit deny.toml in this wave; no such gap exists, so no
recommendation is needed.

**Residual DELIVER check**: the crafter should run `cargo deny check` once
locally after adding the direct dependency, purely as belt-and-braces
confirmation that the resolved graph is byte-identical; this wave's
static reading already establishes the expected result (pass, unchanged).

### [A5] Instrumentation carried; no new observability substrate

Per the DESIGN handoff and the KPI handoff: the existing per-query
`record_query` duration seam now spans the regex compile + match; the
matcher-count and kept/total-series ratio carried from the `=`/`!=` slice
extend to regex matchers. The KPI handoff asks for a regex-vs-exact
matcher-form count and an invalid-regex reject count (distinct from the
existing malformed-matcher count) IF the existing reject-form counter does
not already distinguish them; this is a small extension of the existing
seam, not a new substrate. There is no new probe (the new logic is pure and
in-process; ADR-0042 Decision 8 startup probe and its three-orthogonal-layer
enforcement are unchanged). No separate observability stack is designed.

### [A6] Alerting posture

Per the KPI handoff: per-query p95 > 500 ms on ubuntu-latest (the inherited
budget, now including regex compile + match), any contract-shape regression
(KPI 4), and any cross-tenant leak (inherited US-04 guardrail) remain hard
alerts on the existing seam. A spike in invalid-regex rejects is a SOFT
signal, not an alert (operator typos, not a service fault). For this
no-new-artefact feature the CI gates are also an alerting surface: a
correctness regression fails Gate 1 (test) or Gate 5 (mutants) at the next
push.

### [A7] No deployment/rollback procedure beyond fix-forward

The query-api binary and route already exist and gain no new artefact, so
there is nothing new to deploy and nothing new to roll back at the
deployment layer. The change is read-path filtering, not a storage format
change, so there is no data migration and no data-consequence rollback. The
project is pure trunk-based with no merge gate
(memory `project_kaleidoscope_pure_trunk_based`); recovery is fix-forward
on `main`. This satisfies the rollback-first principle for the actual risk
surface: the only "rollback" available and needed is a git revert of the
feature commit (which also reverts the one-line direct-dependency
promotion), and because no data format changes, a revert has no data
consequence.

## Verification against ci.yml (CONFIRM, not modify)

Read of `.github/workflows/ci.yml` in this wave confirmed:

| Claim | Verified location | Result |
|-------|-------------------|--------|
| Gate 2 (`cargo public-api`) scopes to harness/spark/sieve/codex; query-api excluded | lines 326-347 (`-p otlp-conformance-harness`, `-p spark`, `-p sieve`, `-p codex`) | CONFIRMED, query-api not present |
| Gate 3 (`cargo semver-checks`) scopes to the same four; query-api excluded | lines 420-433 (`--package` for the same four) | CONFIRMED, query-api not present |
| `gate-5-mutants-query-api` job exists and runs `cargo mutants --in-diff` | line 1036; diff built `git diff "$BASELINE" HEAD -- 'crates/query-api/**'` (line 1094); invocation `cargo mutants --package query-api --in-diff "$DIFF_FILE"` (lines 1103-1104) with `origin/main -> HEAD~1 -> full` cascade and empty-diff short-circuit | CONFIRMED present |

No workflow file was modified by this wave. No gate was added, removed, or
amended.

## KPI to gate mapping

All outcome KPIs (`../discuss/outcome-kpis.md`) are correctness indicators
collected by green acceptance tests under **Gate 1** (`cargo test
--workspace`) driving the existing `query_range` handler through the tower
`oneshot` pattern over a real Pulse tempdir. KPI 4 (envelope guardrail) is
collected through Prism's pinned validators in the same suite. The latency
guardrail rides the existing `record_query` seam (continuous in DEVOPS).
Gate 5 (`gate-5-mutants-query-api`) guards the test-suite strength behind
these assertions.

| KPI (from outcome-kpis.md) | Target | Gate | Collection |
|----------------------------|--------|------|------------|
| North star: exactly-Prometheus series or honest 400 | exact across full-anchor + absent-label matrices; 400 on invalid | Gate 1 | oneshot query_range over a real Pulse tempdir |
| KPI 1: full-anchor correctness | exact kept/excluded (substring does not match); 100% round-trip isPromSuccess | Gate 1 | per-arm assertions (prefix, substring-anchor, AND-composition, empty) + Prism contract test |
| KPI 2: absent-label matrix | each of 5 arms exactly right; 0 silently dropped / wrongly kept | Gate 1 | one scenario per arm + pure-predicate units on `matches` |
| KPI 3: invalid-regex honesty | 100% invalid = 400 status:error; 0 panics/500s; valid-but-never-matching stays 200 empty; DD6 redaction | Gate 1 | per-invalid-form 400 test; valid-but-empty 200 test; redaction test |
| KPI 4: envelope guardrail | 100% of success/empty/400 arms satisfy isPromSuccess/isPromError; 0 regressions | Gate 1 | Prism validators per arm |
| latency guardrail | per-query p95 <= 500 ms (now spanning regex compile + match) | continuous DEVOPS | existing record_query duration seam |
| cross-tenant guardrail (inherited US-04) | 0 leaks | Gate 1 + existing seam | inherited tenancy assertions; unchanged by this feature |
| test-suite strength behind the above | 100% mutant kill | Gate 5 | `gate-5-mutants-query-api` --in-diff over selector.rs, matrix.rs, lib.rs |

## Infrastructure summary

- **Deployment**: none new (existing query-api binary + route; no new
  artefact, no migration).
- **CI/CD**: GitHub Actions, ADR-0005 five gates, inherited unchanged.
  `gate-5-mutants-query-api` already present (line 1036); no new or amended
  job.
- **Branching**: pure trunk-based (project default, unchanged).
- **Mutation testing**: per-feature, 100% kill rate, scoped by `--in-diff`
  to `crates/query-api/src/{selector.rs, matrix.rs, lib.rs}`.
- **Supply chain (Gate 4)**: `regex` promoted to a direct dependency;
  VERIFIED no new resolved crate, no new licence, no new advisory, no
  yanked version; deny.toml UNCHANGED (A4).
- **External integrations**: none. The Prism contract boundary is
  unchanged (envelope unchanged, one new 400 arm already satisfies
  isPromError); the existing contract posture covers it. No new contract.
- **Observability**: instrumentation carried on the existing seam; no new
  substrate, no new probe (A5). Alerting posture unchanged (A6).
- **Public surface**: unchanged; router signature byte-identical (A3).

## Artefacts produced by this wave

| Artefact | Path |
|----------|------|
| Environment inventory (clean environment over the in-process substrate, no external services) | `docs/feature/query-api-regex-matchers-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/query-api-regex-matchers-v0/devops/wave-decisions.md` |

## Artefacts judged N/A (with reason)

| Skipped artefact | Reason |
|------------------|--------|
| `kpi-instrumentation.md` | The cinder/pulse library precedents judged this N/A and so does this feature. The KPIs are correctness indicators; the KPI to gate mapping is short and fully contained in the table above (every KPI maps to Gate 1 on the acceptance suite, plus Gate 5 for suite strength and the existing record_query seam for the latency guardrail). The instrumentation is a small CARRIED extension of an EXISTING seam (a regex-vs-exact matcher-form count and an invalid-regex reject count), not a new substrate to design; the DESIGN handoff and KPI handoff already specify it precisely. A separate file would only restate the mapping table and the one-line carried-counter note. |
| `ci-cd-pipeline.md` | This feature adds no job and edits no workflow; the existing `gate-5-mutants-query-api` covers mutation as-is and Gate 1 auto-discovers the acceptance suite. The "Verification against ci.yml" section above is the entire pipeline content for this feature; a separate addendum would be empty. Matches the pulse/cinder library precedent (both N/A). |
| `platform-architecture.md` | No platform infrastructure to architect (no cloud, no orchestration, no service mesh). Morgan's DESIGN docs are sufficient. |
| `observability-design.md` / `monitoring-alerting.md` | No new monitoring substrate; instrumentation rides the existing record_query seam (A5) and the alerting posture is carried unchanged (A6). |
| `infrastructure-integration.md` | No new external integration at runtime; the regex engine is an in-process library, not a network integration (ADR-0046 external-integration handoff). |
| `branching-strategy.md` | Pure trunk-based is the project default; no per-feature deviation (P5). |
| `deployment-strategy.md` / `rollback.md` | No new deployment artefact; recovery is fix-forward / git revert with no data consequence (A7). |

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Write the acceptance tests driving the EXISTING `query_api::router` through `tower::ServiceExt::oneshot` over a real Pulse store in a tempdir; cover full-anchor, the 5 absent-label arms, AND-composition, the invalid-regex 400 (with DD6 redaction), the valid-but-never-matching 200 empty, and the Prism envelope arms | The `clean` environment over the in-process substrate is the only environment to parametrise over (environments.yaml); no external services |
| At DISTILL | DO NOT edit `.github/workflows/ci.yml` | No new gate; Gate 1 auto-discovers the suite, `gate-5-mutants-query-api` already covers mutation (A1, A2) |
| At DISTILL | DO NOT add `query-api` to Gate 2 or Gate 3 | They scope to harness/spark/sieve/codex; query-api graduation is out of scope (A1) |
| At DELIVER | Add `regex` to `crates/query-api/Cargo.toml` as a SemVer requirement (for example `regex = "1"`), NOT a bare wildcard | Gate 4 sets `wildcards = "deny"`; match the 1.12.3 already in the lock (A4) |
| At DELIVER | DO NOT edit `deny.toml` | No new resolved crate, no new licence, no new advisory, no yanked version; Gate 4 passes unchanged (A4). Run `cargo deny check` once as belt-and-braces confirmation |
| At DELIVER | Keep the compiled `Regex` filter-side; do not store it in `MatchOp`/`LabelMatcher` | A compiled `Regex` is not `Eq`/`Hash`; the parsed types stay pure (A3, ADR-0046 Decision 3) |
| At DELIVER | The invalid-regex 400 must never echo the pattern, raw query, or a forwarded header | DD6 redaction (ADR-0046 Decision 3) |
| At DELIVER | Turn the modified files' mutants 100% killed before close, with the full-anchor boundary, the Matches/NotMatches negation, and the invalid-vs-never-matching distinction as primary targets | CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5 (A2) |
| At DELIVER | Extend the existing record_query / matcher-count instrumentation to distinguish regex-vs-exact matcher form and invalid-regex rejects, if not already distinguished | KPI handoff instrumentation note (A5) |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**What DISTILL receives**: the mandatory `environments.yaml` for Mandate 4
(the `clean` environment over the existing `query_range` handler driven via
`oneshot` against a real Pulse tempdir, no external services); the
confirmation that no CI edit is needed (A1, A2); the verified Gate 4
decision that `regex` as a direct dependency needs no deny.toml change
(A4); the constraints on the `regex` pin posture and the filter-side
compiled `Regex` (A3, A4); and the KPI to gate mapping above.

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt of
this wave's outputs.

## Contradictions with the DESIGN handoff

None. Every DEVOPS conclusion the DESIGN handoff anticipated was VERIFIED
against the live artefacts and held: `regex` and its tail are already in
`Cargo.lock` under allow-listed licences (Gate 4 needs no change);
`gate-5-mutants-query-api` already exists and covers the modified files; the
router signature is unchanged; no new external integration; instrumentation
is a carried extension of the existing seam. The DESIGN note that the Gate-4
run is a DEVOPS task (not a DESIGN verification) is exactly what this wave
discharged in A4. No DESIGN claim was contradicted or revised.
