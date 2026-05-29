# Story Map — gate-5-mutants-batch-v0

British English. No em dashes in body.

## User

Maintainers of the eight residual Kaleidoscope crates lacking
`gate-5-mutants-*` coverage in the pre-feature
`.github/workflows/ci.yml`: `aegis`, `augur`, `sluice`,
`beacon-server`, `cinder`, `loom`, `integration-suite`,
`kaleidoscope-gateway`.

## Goal

Land extensions to any of the eight crates with the same
mutation-resistance discipline that the 17 already-covered crates
enjoy. Receive an automatic mutation-test signal on every PR that
touches the crate's `src/`, not as a manual opt-in via
`cargo mutants --package <crate>`.

## Backbone

| 1. Identify 8 | 2. Replicate pattern | 3. Verify all 8 |
|---|---|---|
| 1.1 Read the residual enumeration in `gate-5-mutants-lumen-v0/discuss/story-map.md` lines 129 to 141 | 2.1 Choose sibling job to clone (`gate-5-mutants-lumen`, lines 1210 to 1295) | 3.1 `grep -c "^  gate-5-mutants-" .github/workflows/ci.yml` returns 25 |
| 1.2 Confirm pre-feature workflow has 17 `gate-5-mutants-*` jobs, none for the eight | 2.2 Choose placement (alphabetic insertion recommended; see FLAG 1) | 3.2 Synthetic empty-diff PR short-circuits all eight new jobs in seconds |
| 1.3 Confirm each crate's `package.name` from `crates/<dir>/Cargo.toml` (aegis, augur, sluice, beacon-server, cinder, loom, integration-suite, kaleidoscope-gateway) | 2.3 Substitute four tokens per crate: package, diff path, cache key, artefact name | 3.3 The 17 pre-existing jobs are byte-identical (no rename, no deletion, no script change); YAML still parses |

---

## Walking Skeleton

The walking skeleton IS US-01. The feature is a single-story batch
infrastructure slice; the skeleton is the minimum end-to-end edit
that delivers the outcome:

1. **Identify 8** (1.1, 1.2, 1.3): the DISCUSS wave records the
   target list and the package names in `wave-decisions.md`,
   `user-stories.md`, and this story-map.
2. **Replicate pattern** (2.1, 2.2, 2.3): the DESIGN wave specifies
   the eight YAML blocks (placement, `needs` graph, four token
   substitutions per crate).
3. **Verify all 8** (3.1, 3.2, 3.3): the DELIVER wave's commit lands
   the workflow edit; the post-commit CI run on the next PR is the
   first observable proof; a YAML-parser smoke confirms the file is
   still valid.

No vertical depth beyond this. No "Release 2". The slice ships in
one commit.

---

## Release 1 (this feature)

US-01: eight `gate-5-mutants-<crate>` jobs shipped.

- **Outcome**: a maintainer extending any of the eight residual
  crates receives an automatic mutation-test signal on the PR status
  panel for their crate.
- **Outcome KPI link**: K1 (eight jobs exist), K2 (each uses
  `--in-diff origin/main`), K3 (zero regression on the 17 siblings),
  K4 (total count 17 to 25), K5 (YAML still parses), K6 (zero
  regression on other CI jobs). See `outcome-kpis.md`.
- **Story count**: 1.
- **Demonstrable in single session**: yes; a `grep`, a YAML parse,
  and a `diff` close the loop.

---

## Slice rationale: why batch is the right size

The slice closes a CLOSED and FINITE residual gap. The eight target
crates are enumerated explicitly in
`gate-5-mutants-lumen-v0/discuss/story-map.md` lines 129 to 141. The
pattern is confirmed shipped twice on `main`:

- `gate-5-mutants-lumen` (commit d96a807, the immediate precedent at
  workflow lines 1210 to 1295)
- `gate-5-mutants-query-http-common` (commit a6175f1, sibling
  precedent at workflow lines 1809 to end)

Opening eight separate features would be ceremony without
information value. There is nothing more to learn about the shape of
the per-crate Gate 5 job than the two precedents have already taught.
Each of the eight new jobs is a token-substitution of the same
sibling block. The aggregate review surface (one YAML diff, eight
near-identical blocks) is smaller and more reviewable as a batch
than as eight serial PRs.

The "small focused features" project policy is honoured by the
finite scope (eight crates enumerated, not "all future gaps") and by
the single-story shape (US-01 is the only story). A hypothetical
future feature that added a ninth crate would be a separate feature
because it would have a different gap source (a new workspace crate,
or a new ADR-0005 gate), not a continuation of this one.

---

## Scope Assessment

**PASS** — 1 story, 1 bounded context (the CI workflow file),
estimated under 1 day of crafter effort (eight YAML blocks, four
token substitutions per block, one YAML-parser smoke, one synthetic
empty-diff PR observation).

The Elephant Carpaccio gate is satisfied: this is the thinnest
possible end-to-end slice that delivers the eight-crate outcome. The
batch is not oversized because:

- Stories: 1 (under the 10 cap).
- Bounded contexts: 1 (the CI workflow file; under the 3 cap).
- Integration points: 0 production-code integration; the eight new
  jobs are isomorphic clones of the sibling block (under the 5 cap).
- Effort estimate: under 1 day of crafter effort (well under the
  2-week threshold).
- Independent user outcomes: 1 (uniform Gate 5 coverage across the
  eight residual crates; the eight maintainer personas share the
  same outcome shape).

No splitting is warranted. Further splitting (one feature per crate)
would multiply ceremony cost by 8 without any incremental learning
or risk reduction; both precedents (`gate-5-mutants-lumen-v0` and
`gate-5-mutants-query-http-common-v0`) have already established the
per-crate Gate 5 pattern.

---

## Priority Rationale

The single story is the walking skeleton; no priority ordering is
required. The slice ships as a single workflow file edit producing
eight new job blocks.

The feature exists because the
`gate-5-mutants-lumen-v0/discuss/story-map.md` appendix at lines 129
to 141 surfaced eight residual crates without `gate-5-mutants-*`
coverage. The lumen feature itself chose to close only one crate
(its own scope); the appendix flagged the rest as
forward-looking maintenance. This feature is the operationalisation
of that forward-looking item.

Ordering rationale across the eight crates (informational only;
DESIGN may reorder per FLAG 1):

1. Crates with the largest production behaviour and most active
   evolution come first by readability concern (the maintainer
   scanning the file finds them quickly):
   - `aegis` (auth, RBAC, audit) is foundational; its evolution
     touches every downstream crate.
   - `augur` (anomaly observers) ships novel comparators; primary
     mutation surface.
   - `cinder` (lifecycle policy) has age comparators with boundary
     risk.
   - `sluice` (queue port) has bounded-capacity and FIFO predicates.
   - `loom` (Git-backed catalogues) has diff and plan ordering.
   - `beacon-server` (scheduler + HTTP client wiring) is the
     deployable form of `beacon`.
   - `kaleidoscope-gateway` (host composition) has small but real
     startup wiring.
   - `integration-suite` (test-only) ships last because it is the
     edge case (see FLAG 3 in `wave-decisions.md`).

The alphabetic insertion recommended in FLAG 1 produces a different
order at read time; both are acceptable. The priority rationale here
is the maintainer's mental order, not the file's textual order.

---

## Appendix: post-feature workspace coverage

This appendix records the expected post-feature state of
`gate-5-mutants-*` coverage. It is provided for the DESIGN and
DELIVER waves to verify against.

### Existing `gate-5-mutants-*` jobs (17, pre-feature)

Confirmed by `grep -n "^  gate-5-mutants-" .github/workflows/ci.yml`:

| # | Crate | Workflow line (pre-feature) |
|---|---|---|
| 1 | `harness` (alias for `otlp-conformance-harness`) | 453 |
| 2 | `aperture` | 503 |
| 3 | `spark` | 604 |
| 4 | `sieve` | 692 |
| 5 | `codex` | 777 |
| 6 | `self-observe` | 862 |
| 7 | `aperture-storage-sink` | 949 |
| 8 | `query-api` | 1036 |
| 9 | `log-query-api` | 1123 |
| 10 | `lumen` | 1210 |
| 11 | `trace-query-api` | 1297 |
| 12 | `pulse` | 1384 |
| 13 | `ray` | 1467 |
| 14 | `strata` | 1550 |
| 15 | `beacon` | 1635 |
| 16 | `kaleidoscope-cli` | 1723 |
| 17 | `query-http-common` | 1809 |

### New `gate-5-mutants-*` jobs (8, this feature)

| # | Crate dir | `package.name` | Notes |
|---|---|---|---|
| 1 | `crates/aegis/` | `aegis` | library |
| 2 | `crates/augur/` | `augur` | library |
| 3 | `crates/sluice/` | `sluice` | library |
| 4 | `crates/beacon-server/` | `beacon-server` | binary + lib |
| 5 | `crates/cinder/` | `cinder` | library |
| 6 | `crates/loom/` | `loom` | binary + lib |
| 7 | `crates/integration-suite/` | `integration-suite` | test-only; see FLAG 3 in `wave-decisions.md` |
| 8 | `crates/kaleidoscope-gateway/` | `kaleidoscope-gateway` | binary + lib |

### Post-feature totals

- Pre-feature `gate-5-mutants-*` count: 17.
- New jobs added by this feature: 8.
- Post-feature `gate-5-mutants-*` count: 25.
- Workspace crate count: 25.
- Per-crate Gate 5 coverage post-feature: 25 / 25 = 100%.
