# Story Map — gate-5-mutants-lumen-v0

British English. No em dashes in body.

## User

The Kaleidoscope `lumen` crate maintainer extending
`crates/lumen/src/predicate.rs` (and, by future implication, any
file under `crates/lumen/src/`).

## Goal

Land Predicate extensions with the same mutation-resistance
discipline that the read APIs (`log-query-api`, `query-api`,
`trace-query-api`) already enjoy. Receive an automatic mutation-test
signal on every PR that touches `crates/lumen/**`, not as a manual
opt-in via `cargo mutants --package lumen`.

## Backbone

| 1. Identify gap | 2. Replicate pattern | 3. Verify |
|---|---|---|
| 1.1 Read Apex's honest gap note in `log-body-text-search-v0/devops/wave-decisions.md` lines 56 to 89 | 2.1 Choose sibling job to clone (`gate-5-mutants-log-query-api`, lines 1123 to 1208) | 3.1 `grep "gate-5-mutants-lumen:" .github/workflows/ci.yml` returns one line |
| 1.2 Confirm pre-feature workflow has 16 `gate-5-mutants-*` jobs, none for lumen | 2.2 Choose placement (immediately after `gate-5-mutants-log-query-api`) | 3.2 Synthetic empty-diff PR short-circuits in seconds |
| 1.3 Confirm `lumen`'s `Cargo.toml` `package.name` is `lumen` | 2.3 Substitute four tokens: package, diff path, cache key, artefact name | 3.3 Sixteen pre-existing jobs unchanged (no rename, no deletion) |

---

## Walking Skeleton

The walking skeleton IS US-01. The feature is a single-story
infrastructure slice; the skeleton is the minimum end-to-end edit that
delivers the outcome:

1. **Identify gap** (1.1, 1.2, 1.3): the DISCUSS wave records this in
   `wave-decisions.md` and `outcome-kpis.md`.
2. **Replicate pattern** (2.1, 2.2, 2.3): the DESIGN wave specifies the
   YAML edit (placement, needs graph, four token substitutions).
3. **Verify** (3.1, 3.2, 3.3): the DELIVER wave's commit lands the
   workflow edit; the post-commit CI run on the next lumen-touching PR
   is the first observable proof.

No vertical depth beyond this. No "Release 2". The slice ships in one
commit.

---

## Release 1 (this feature)

US-01: `gate-5-mutants-lumen` job shipped.

- **Outcome**: a maintainer extending `Predicate` (or any other file
  under `crates/lumen/src/`) receives an automatic mutation-test
  signal on the PR status panel.
- **Outcome KPI link**: K1 (job exists), K2 (job exercises lumen
  diff correctly), K3 (zero regression), K4 (zero new dependency).
  See `outcome-kpis.md`.
- **Story count**: 1.
- **Demonstrable in single session**: yes; a `grep` and a synthetic
  PR closes the loop.

---

## Scope Assessment

**PASS** — 1 story, 1 bounded context (the CI workflow file), estimated
under 1 day of crafter effort (one YAML block append, four token
substitutions, one synthetic-PR smoke).

The Elephant Carpaccio gate is satisfied trivially: this is the
thinnest possible end-to-end slice that delivers the outcome. No
splitting is warranted, no further splitting is possible.

---

## Priority Rationale

The single story is the walking skeleton; no priority ordering is
required. The slice ships as a single workflow file edit.

The feature exists because Apex's honest gap note at
`docs/feature/log-body-text-search-v0/devops/wave-decisions.md` lines
56 to 89 surfaced a quietly-broken ADR-0005 Gate 5 invariant on the
`lumen` crate. The same DEVOPS wave authorised this slice as a
forward-looking item, NOT a blocker on the body-contains slice. The
slice is shipped now because:

1. The pattern is established (sixteen sibling jobs to clone from).
2. The fix is bounded (one workflow file, one job block).
3. The cost of waiting is real: every `Predicate` extension that
   lands between now and the fix carries silent mutation-coverage
   risk.

---

## Appendix: Other crates without a `gate-5-mutants-*` job (future maintenance)

This appendix lists the crates that lack a `gate-5-mutants-*` job in
the pre-feature `.github/workflows/ci.yml`. It is recorded as
forward-looking maintenance work. It is NOT promoted to a US-02 of
this feature. Each crate would be its own future thin slice, shaped
exactly like this one (clone sibling job, substitute tokens, verify).

The enumeration was performed by cross-referencing the workspace
`crates/*/Cargo.toml` glob (25 crates total) with the existing
`gate-5-mutants-*` jobs in `.github/workflows/ci.yml`.

### Existing `gate-5-mutants-*` jobs (16, pre-feature)

| # | Crate | Workflow line |
|---|---|---|
| 1 | `otlp-conformance-harness` (job alias `harness`) | 453 |
| 2 | `aperture` | 503 |
| 3 | `spark` | 604 |
| 4 | `sieve` | 692 |
| 5 | `codex` | 777 |
| 6 | `self-observe` | 862 |
| 7 | `aperture-storage-sink` | 949 |
| 8 | `query-api` | 1036 |
| 9 | `log-query-api` | 1123 |
| 10 | `trace-query-api` | 1210 |
| 11 | `pulse` | 1297 |
| 12 | `ray` | 1380 |
| 13 | `strata` | 1463 |
| 14 | `beacon` | 1548 |
| 15 | `kaleidoscope-cli` | 1636 |
| 16 | `query-http-common` | 1722 |

### Crates WITHOUT a `gate-5-mutants-*` job (9, pre-feature; 8 post-feature)

| # | Crate | Pre-feature status | This feature | Post-feature status |
|---|---|---|---|---|
| 1 | `lumen` | NO job | CLOSES gap (US-01) | HAS job |
| 2 | `aegis` | NO job | not in scope | NO job (future) |
| 3 | `augur` | NO job | not in scope | NO job (future) |
| 4 | `sluice` | NO job | not in scope | NO job (future) |
| 5 | `beacon-server` | NO job | not in scope | NO job (future) |
| 6 | `cinder` | NO job | not in scope | NO job (future) |
| 7 | `loom` | NO job | not in scope | NO job (future) |
| 8 | `integration-suite` | NO job | not in scope | NO job (future) |
| 9 | `kaleidoscope-gateway` | NO job | not in scope | NO job (future) |

### Note on `integration-suite`

`integration-suite` is a test-only crate; mutation-testing a
test-only crate is a different conversation (the mutations would
target the test harness itself). A future feature MAY decide that
this crate is excluded by policy rather than included with a job.
That decision is OUT of this feature's scope.

### Note on graduated vs ungraduated crates

The four graduated crates per ADR-0005 Gate 2 / Gate 3 locked set are
`otlp-conformance-harness`, `spark`, `sieve`, `codex`. All four
already have `gate-5-mutants-*` jobs. The gap is concentrated in
ungraduated and infrastructure crates. The same pattern (clone the
sibling job, substitute four tokens, verify with a synthetic PR)
applies uniformly to all eight remaining crates if a future
maintenance feature decides to close them.
