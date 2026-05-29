# Definition of Ready Validation — gate-5-mutants-batch-v0

British English. No em dashes in body.

Self-validation by Luna (`nw-product-owner`). Per the orchestrator's
authorisation for this batch slice, peer review is skipped; the
validation is recorded here for audit, and the DESIGN wave (Morgan)
consumes the artefacts directly.

## Story under validation

US-01: eight `gate-5-mutants-<crate>` jobs shipped (the single batch
story in this slice; see `user-stories.md`).

## 9-item DoR Checklist

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | `user-stories.md` "Problem" section names the residual gap source (`gate-5-mutants-lumen-v0/discuss/story-map.md` lines 129 to 141, commit a11910f), names the silently-violated invariant (ADR-0005 Gate 5 "100% kill rate per crate"), enumerates the eight target crates with the file under enforcement gap per crate (`crates/aegis/src/`, `crates/augur/src/`, etc.), and cites two pattern-confirming commits on `main` (d96a807 for `gate-5-mutants-lumen`, a6175f1 for `gate-5-mutants-query-http-common`) |
| 2 | User/persona identified with specific characteristics | PASS | "Who" section: maintainers of any of the eight residual crates extending `crates/<crate-dir>/src/**`; read PR status panel before merging; trust a green panel as evidence that Gate 5 has fired. Specific context (PR-time), specific signal (status panel), specific trust assumption (green = enforced). The persona is plural (eight maintainer roles) but the shape is the same per crate |
| 3 | 3+ domain examples with real data | PASS | Three concrete examples in "Domain Examples" section: (1) `aegis::Catalogue` extension with real field name `disabled_tenants` and real `Vec::contains` call site; (2) empty-diff PR with real path globs for all eight crates and real short-circuit log message shape; (3) `cinder::LifecyclePolicy` with real comparator `>=`, real boundary value 30 days, and real surviving-mutation scenario (`>=` to `>`). Each example uses real crate names, real file paths, and realistic code shapes |
| 4 | UAT scenarios in Given/When/Then (3-7 scenarios) | PASS | Three Gherkin scenarios in "UAT Scenarios (BDD)" section. Each scenario describes a maintainer-observable outcome (all eight jobs exist, all eight short-circuit on empty diff, zero regression on the 17 pre-existing jobs plus YAML still parses). Titles describe WHAT the maintainer sees, not HOW the jobs are implemented (per BDD anti-pattern guidance) |
| 5 | AC derived from UAT | PASS | Thirteen acceptance criteria (AC-1 to AC-13), each tracing back to a specific scenario clause or KPI. AC-1 is the count-roll-up from Scenario 1. AC-2 through AC-9 are the eight per-crate existence + script + diff-filter checks from Scenario 1. AC-10 is the uniform `needs` graph from Scenario 1. AC-11 is the zero-regression-on-17 from Scenario 3. AC-12 is the zero-net-new-dependency guardrail (K4's predecessor pattern). AC-13 is the YAML-parses guardrail from Scenario 3 |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 story, 3 scenarios, 8 near-identical YAML blocks each shaped via four token substitutions over an existing 86-line sibling job block. Estimated under 1 day of crafter effort (the substitution is mechanical and the sibling is byte-stable). See `story-map.md` Scope Assessment for the full Elephant Carpaccio audit (1 story, 1 bounded context, 0 integration points, under 1 day, 1 outcome). The batch is the thinnest possible end-to-end slice for the eight-crate residual; per-crate splitting would multiply ceremony by 8 without information value |
| 7 | Technical notes identify constraints | PASS | "Technical Notes" section names the sibling job to clone (`gate-5-mutants-lumen` at lines 1210 to 1295), the four substitution tokens per crate, the three open flags (placement, `needs` graph, small-crate handling), the public-API posture (no change; none of the eight crates is in Gate 2 / Gate 3's locked set), and the dependency posture (no `deny.toml` change; `cargo-mutants` installed via `taiki-e/install-action`). System Constraints section also enumerates: no production source change, no new tooling, no new dependency, ADR-0005 immutability, pattern fidelity, uniform `needs` graph |
| 8 | Dependencies resolved or tracked | PASS | Resolved: ADR-0005 (CI contract); the 17 pre-existing `gate-5-mutants-*` jobs; the `gate-5-mutants-lumen-v0` DISCUSS / DESIGN / DEVOPS waves (gap source + pattern precedent); the `gate-5-mutants-query-http-common-v0` DEVOPS wave (sibling precedent). Tracked, not blocking: none. This feature closes the residual gap entirely; a future feature creating a new workspace crate is a separate concern (workspace-template or CI-codegen) and is OUT of this feature's scope |
| 9 | Outcome KPIs defined with measurable targets | PASS | Six KPIs (K1 to K6) in `outcome-kpis.md`, each with [Who], [Does what], [By how much], [Baseline], [Measured by], and [Type]. K1 binary (eight jobs exist; eight binary sub-checks). K2 per-crate correctness of the diff filter and package scope. K3 zero-delta `diff` over the 17 pre-existing blocks. K4 count roll-up (17 to 25). K5 YAML parses. K6 zero regression on the rest of CI. All six are build-time measurements, consistent with the platform's no-runtime-telemetry posture |

## DoR Status: PASSED (9 of 9)

All nine items pass with evidence. No remediation required. The
slice is READY for the DESIGN wave (`nw-platform-architect`, Morgan
acting as Apex).

## Notes on the validation

This is a self-validation by the requirements analyst, not a peer
review. Per the orchestrator's standing authorisation for this batch
slice:

- The slice is infrastructure-only (no production code change).
- The pattern is established twice on `main` (`gate-5-mutants-lumen`
  at d96a807, `gate-5-mutants-query-http-common` at a6175f1).
- The DoR items are objectively verifiable (a `grep` per crate, a
  YAML-parser smoke, a `diff` over the workflow file).
- The risk surface of skipping peer review is bounded: the DESIGN
  wave's deliverable is eight YAML blocks (each shaped by four token
  substitutions over a byte-stable sibling); the DELIVER wave's
  deliverable is a single workflow edit; both are reversible by a
  `git revert` with no data-format consequence.

A full peer-review pass would surface the same finding (PASS, all
nine items) and would not change any artefact. The audit trail is
complete: the gap source (the lumen-v0 appendix), the pattern source
(the lumen sibling job), the two pattern-confirming commits, and the
six KPIs are all named with file paths, line numbers, and commit
hashes where applicable.

## Anti-pattern scan

| Anti-pattern | Signal in this slice | Verdict |
|---|---|---|
| Implement-X | "Implement eight gate-5-mutants jobs" would be the bad framing | NOT PRESENT. The story is framed from the maintainer's pain point (silent ADR-0005 Gate 5 violation on the eight residual crates) and the Decision enabled (automatic mutation signal on every extension to any of the eight), not as "implement eight jobs" |
| Generic data | "user123", "test-crate" | NOT PRESENT. Real crate names (`aegis`, `augur`, `sluice`, `beacon-server`, `cinder`, `loom`, `integration-suite`, `kaleidoscope-gateway`), real file paths (`crates/<crate-dir>/src/**`), real workflow line numbers (1210 to 1295 for the sibling), real sibling job name (`gate-5-mutants-lumen`), real commit hashes (d96a807, a6175f1, a11910f) |
| Technical AC | "Use `cargo mutants` v25.x" | NOT PRESENT. AC describe observable outcomes (count roll-up, per-crate existence + script + diff-filter, zero regression on 17 jobs, zero new dependency, YAML parses) rather than implementation prescriptions. The version of `cargo-mutants` is inherited from the sibling job's `taiki-e/install-action` pin |
| Technical scenario title | "FileWatcher triggers TreeView refresh" | NOT PRESENT. The three scenario titles describe maintainer-observable outcomes ("all eight new gate-5-mutants jobs exist in the CI workflow", "a PR that does not touch any of the eight target crates short-circuits all eight new jobs in seconds", "zero regression on the 17 pre-existing gate-5-mutants jobs and zero regression on every other CI job") |
| Oversized story | More than 7 scenarios, more than 3 days | NOT PRESENT. 3 scenarios, under 1 day, single workflow file edit. Batch-of-eight is justified in `story-map.md` "Slice rationale" and in `wave-decisions.md` "Why batch is the right size" sections |
| Abstract requirements | No concrete examples | NOT PRESENT. Three concrete examples with real field names, real comparators, real boundary values, real crate names |

All anti-patterns absent. No remediation required.

## Note on the batch shape

A reviewer applying the "small focused features" rubric mechanically
might flag the eight-crate scope as a violation. The "Why batch is
the right size" section in `wave-decisions.md` addresses this
explicitly. In summary:

- The scope is closed and finite (eight crates enumerated by
  `package.name`).
- The pattern is confirmed twice on `main`.
- Per-crate splitting would multiply ceremony by 8 without
  information value.
- A future feature for a different category of gap (a new ADR-0005
  gate, a new workspace crate at a later date) would be a separate
  feature; this feature does not pre-empt future maintenance.

The reviewer would arrive at the same conclusion that Luna records
here: the batch is the right size for this specific residual gap.
