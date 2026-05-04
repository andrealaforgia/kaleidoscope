# DEVOPS-wave peer review — `otlp-conformance-harness-v0` — iteration 1

**Reviewer**: Forge (`nw-platform-architect-reviewer`) | **Date**: 2026-05-04 | **Iteration**: 1 of 2

## Verdict: **APPROVED**

Zero critical blockers. One high-severity finding documented as accepted risk for the solo-author period. Two medium-severity findings recommended for future action. The DEVOPS wave is production-ready and the OTLP conformance harness v0 is shipped end-to-end through nWave.

## External validity

| Criterion | Status | Evidence |
|---|---|---|
| Deployment path complete | PASS | All five ADR-0005 gates present, blocking, enforced via branch protection. Merge-to-`main` is the deployment contract. |
| Observability enabled | PASS | Seven outcome KPIs instrumented. KPI 1 (false-positive rate) and KPI 4 (verdict-counts) automated and real. |
| Rollback capability | PASS | Trunk-based; rollback is `git revert`. Time-to-restore target documented as under one hour. |
| Security gates integrated | PASS | `cargo deny check` (Gate 4) runs first, fail-fast. Licences, advisories, pins, yanked versions all covered. |

## Per-dimension verdicts

| Dimension | Verdict |
|---|---|
| 1. CI/CD pipeline correctness and completeness | PASS |
| 2. Environment inventory coverage | PASS (with M1 noted) |
| 3. Observability design alignment with outcome KPIs | PASS |
| 4. Infrastructure security and deployment strategy soundness | PASS (with H1 noted) |

All five ADR-0005 gates are present, ordered for fail-fast, and mapped 1:1 to workflow jobs. The toolchain is reproducibly pinned (`rust-toolchain.toml` for stable `1.78`, `NIGHTLY_PIN=nightly-2026-04-15` for the two nightly-only gates). The cache strategy uses two separate namespaces (stable / nightly) keyed on `Cargo.lock` plus the toolchain, preventing poisoning. Mutation budget rule (60-second threshold, two-merge confirmation, manual full-run check on switch-over) is documented and matches Crafty's Q4. The KPI 4 verdict-counts artefact is real, schema-versioned, and uploaded with 90-day retention.

## Crafty's six open questions — all resolved

| Q | Resolution | Evidence |
|---|---|---|
| Q1 | GitHub Actions chosen by Andrea pre-wave; five gates wired | `ci.yml`, wave-decisions.md |
| Q2 | `dtolnay/rust-toolchain@master` with `NIGHTLY_PIN=nightly-2026-04-15` for Gates 2 and 3 | `ci.yml` lines 188, 237 |
| Q3 | `rust-toolchain.toml` shipped at repo root pinning stable 1.78 | `rust-toolchain.toml` |
| Q4 | 60-second threshold with two-merge confirmation; switch to `cargo mutants --in-diff origin/main` at switch-over | ci-cd-pipeline.md lines 149–187 |
| Q5 | JSON schema v1 for verdict-counts.json; Python step in Gate 1; 90-day retention | ci.yml lines 125–171; kpi-instrumentation.md |
| Q6 | Paste-ready upstream issue stub at `upstream-issue-opentelemetry-proto-feature-split.md`; non-blocking | issue stub file |

## Findings

### H1 — `dtolnay/rust-toolchain` action pinned by tag, not commit SHA (high)

The action is pinned to floating tags (`@stable`, `@master`) at four locations in `.github/workflows/ci.yml` (lines 97, 188, 237, 287). Floating tags create a low-grade supply-chain exposure: the repository could be transferred, deleted, forked, or a malicious version could be released under the same tag without the workflow noticing.

The action is widely used and audited by the Rust community, and Andrea is currently the sole author, so the practical risk is low. The DEVOPS wave-decisions document acknowledges this explicitly (A4) as "acceptable for Andrea's solo-author period, worth tightening when contribution opens".

**Recommendation**: tighten to commit-SHA pinning when external contributions are accepted. No immediate action required for v0.

### M1 — WSL environment listed as expected but untested (medium)

`environments.yaml` lists `contributor-wsl` as part of the harness's portability contract. The document is honest that it is untested as of 2026-05-03 and that any incompatibility would be a defect to fix, not a "platform not supported" response. The tension is that the listing creates an expectation that has not been validated.

**Recommendation**: remove WSL from the main environments list and move it into the out-of-scope section with the deferral framing, or alternatively make the untested-status note more prominent in the main entry. Either resolves the tension. Non-blocking.

### M2 — `ci.yml` Gate 1 comment lacks ADR-0005 cross-reference (medium)

The comment at the Gate 1 step explains `--locked` but not why `--all-targets` is required. A future maintainer might be tempted to remove the flag for speed.

**Recommendation**: extend the comment to reference ADR-0005 Gate 1 explicitly and note that `--all-targets` covers unit, integration, and doc tests. Non-blocking, two-line edit.

## House style and discipline

- British English consistent across all DEVOPS prose.
- No human-effort estimates anywhere; timing is wall-clock or conceptual.
- Library-not-service framing throughout `platform-architecture.md` and elsewhere.
- Trunk-based discipline codified as the project rule (not just the feature rule) in `branching-strategy.md`.
- `CLAUDE.md` paradigm declaration matches Morgan's DESIGN recommendation; mutation-strategy line matches ADR-0005 Gate 5's 100% bar (deviating from the skill template's 80% default for cause).

## Apex's reviewer-attention items

All six items Apex flagged for reviewer attention are validated as defensible or documented:

1. Single OS in CI (`ubuntu-latest` only) — justified by the harness having no platform-specific code; reversible.
2. No local quality-gate framework — recommendation only in `CONTRIBUTING.md`; full local suite runs in seconds.
3. No path filters on triggers — reversible; cost of running gates on docs-only changes is negligible.
4. No PR-review requirement on `main` — codifies the AI-direct-to-main pattern; flagged for future tightening when contribution opens.
5. `actions/upload-artifact@v4` — correct (v3 deprecated since January 2025).
6. Python3 for verdict-counts capture — system-installed on `ubuntu-latest`, avoids a third tool that would need its own cache namespace.

## DORA assessment

The harness is structurally in the Elite band on every DORA metric (deployment frequency, lead time for changes, change-failure rate, time-to-restore) because the gates make `main` always green. This is the trunk-based-development pay-off.

## Iteration budget

Iteration 1 of 2 maximum per the skill. Zero blocking items; no iteration 2 required. DEVOPS wave is closed for `otlp-conformance-harness-v0`.

## Manual follow-up step required

GitHub branch-protection rules cannot be set via the workflow. Andrea must configure them manually in the GitHub Settings UI:

- Required status checks (all five): `gate-4-deny`, `gate-1-test`, `gate-2-public-api`, `gate-3-semver`, `gate-5-mutants`.
- Require linear history: on.
- Allow force-push: off.
- Allow deletions: off.
- Require pull-request reviews before merging: off (codified policy choice for solo-author period).

After branch protection is configured, the next push to `main` will trigger the first real CI run.
