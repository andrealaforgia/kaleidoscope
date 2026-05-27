# Definition of Ready Validation — query-http-common-v0

Self-validated by Luna (nw-product-owner) per the stretch nightly protocol;
peer review not invoked for this DISCUSS pass. All five stories
(US-01..US-05) are evaluated against the 9-item DoR gate. Per-story
verdicts follow the per-feature summary.

## Per-feature summary

| # | DoR item                                                                | Verdict | Evidence                                                                                                                                                  |
|---|-------------------------------------------------------------------------|---------|-----------------------------------------------------------------------------------------------------------------------------------------------------------|
| 1 | Problem statement clear, in domain language                              | PASS    | Every story's Problem section names a concrete pain point in maintainer language (one-place-edit, three-place-edit, byte-identical, pillar label, etc.). |
| 2 | User/persona with specific characteristics                              | PASS    | "The Kaleidoscope read-side maintainer (Andrea or a crafter agent)" with the specific concern named per story (cap tuning, parser format, envelope shape, fail-closed tenancy). |
| 3 | At least 3 domain examples with real data                               | PASS    | Each story carries 3 domain examples. Real data is the actual `MAX_RESULT_ROWS`, the actual cap reason strings, the actual pillar labels ("query", "log query", "trace query"), the actual file paths (`crates/query-api/src/lib.rs` etc.). |
| 4 | UAT scenarios in Given/When/Then (3-7 scenarios)                        | PASS    | Every story has 3 UAT scenarios. None has more than 7. Scenario titles are outcome-focused (e.g. "The error body bytes are unchanged on every existing error arm") not implementation-focused. |
| 5 | Acceptance criteria derived from UAT                                    | PASS    | Each story's Acceptance Criteria list maps 1:1 to its UAT scenarios with the same observable assertions (test counts, grep counts, byte-identical bodies, mutation kill rate). |
| 6 | Story right-sized (1-3 days, 3-7 scenarios)                             | PASS    | All five stories are sized ≤ 1 day in the story map; each has 3 UAT scenarios; the feature as a whole is 5 stories / 1 bounded context / ~5 days, within the elephant-carpaccio limits. |
| 7 | Technical notes identify constraints and dependencies                    | PASS    | Each story has a Technical Notes section naming the relevant constraints (signature choice on the parser, `seconds_to_nanos` staying per-crate, `Option<&str>` vs `&str`, AGPL licence, mutation gate). The System Constraints block at the top of `user-stories.md` names workspace-wide constraints. |
| 8 | Dependencies resolved or tracked                                        | PASS    | US-01 depends only on the workspace (no prior story). US-02, US-03, US-04 depend on US-01 (the new crate must exist). US-05 depends on all four. Dependencies are explicit in `story-map.md` priority rationale. |
| 9 | Outcome KPIs defined with measurable targets                            | PASS    | Four KPIs (K1..K4) defined in `outcome-kpis.md` with numeric targets, baselines, measurement methods, and owners. Each story's Outcome KPIs section names the segment / behaviour / target / measurement method. |

### Feature-level DoR status: PASSED

All 9 items pass with evidence. The feature is ready for DESIGN-wave
handoff to `nw-solution-architect`.

## Per-story DoR verdicts

| Story | 1 Problem | 2 Persona | 3 Examples | 4 UAT | 5 AC | 6 Size | 7 Tech notes | 8 Deps | 9 KPIs | Overall |
|-------|-----------|-----------|------------|-------|------|--------|--------------|--------|--------|---------|
| US-01 | PASS      | PASS      | PASS       | PASS  | PASS | PASS   | PASS         | PASS   | PASS   | PASSED  |
| US-02 | PASS      | PASS      | PASS       | PASS  | PASS | PASS   | PASS         | PASS   | PASS   | PASSED  |
| US-03 | PASS      | PASS      | PASS       | PASS  | PASS | PASS   | PASS         | PASS   | PASS   | PASSED  |
| US-04 | PASS      | PASS      | PASS       | PASS  | PASS | PASS   | PASS         | PASS   | PASS   | PASSED  |
| US-05 | PASS      | PASS      | PASS       | PASS  | PASS | PASS   | PASS         | PASS   | PASS   | PASSED  |

## Anti-pattern detection

- "Implement-X" anti-pattern: NOT PRESENT. Each story is framed as a
  maintainer pain point, not as "implement query-http-common".
- "Generic data" anti-pattern: NOT PRESENT. Examples use the actual cap
  values, the actual reason strings, the actual file paths.
- "Technical acceptance criteria" anti-pattern: REMEDIATED at AC writing
  time. AC are framed as observable outcomes (grep returns zero matches,
  test suite is green, body is byte-identical, mutation kill is 100%),
  not as implementation prescriptions ("use serde_json::json!", "use the
  Axum `FromRequestParts` trait", etc.). The story brief originally
  framed US-04 as a `FromRequestParts` extractor; this was REMEDIATED in
  the story body to reflect the actual code (an inline `match` block,
  not an extractor), with a note explaining the correction.
- "Oversized stories" anti-pattern: NOT PRESENT. Each story is ≤ 1 day
  with 3 UAT scenarios.
- "No examples" anti-pattern: NOT PRESENT. Each story has 3 domain
  examples with realistic narratives.

## Risk surfacing

| Risk                                                              | Probability | Impact | Mitigation                                                                                                  |
|-------------------------------------------------------------------|-------------|--------|-------------------------------------------------------------------------------------------------------------|
| Hidden divergence in one of the four extractions (parser signature, 401 reason text formatting) breaks byte identity | Low         | High   | K2 (byte-identical response bodies) is asserted by the existing acceptance suite, which already pins literal bytes |
| Mutation kill rate falls short of 100% on the new crate            | Medium      | Medium | K4 gates US-05; the per-arm inline tests in the consumer crates are migrated into `query-http-common` to preserve coverage |
| LOC counter under-counts because the grep pattern is too narrow    | Low         | Low    | The exact pattern list is recorded in the slice brief (DESIGN deliverable) and reviewed at slice landing    |
| Workspace MSRV drift if `query-http-common` introduces a new dep   | Low         | Low    | The new crate uses only existing workspace deps (`axum`, `serde`, `serde_json`, `aegis`); no new MSRV pressure |
| ADR-0054 hand-write versus deferral                               | Medium      | Low    | Flagged to DESIGN (`wave-decisions.md` flag 5); recommended SHIP a small ADR-0054 alongside this feature     |

Risks are surfaced here, NOT managed; downstream waves (DESIGN, DELIVER,
DISTILL) own mitigation execution.

## Handoff readiness

- DISCUSS deliverables present at
  `docs/feature/query-http-common-v0/discuss/`:
  - `user-stories.md`
  - `story-map.md`
  - `outcome-kpis.md`
  - `dor-validation.md` (this file)
  - `wave-decisions.md`
- DESIGN handoff: `nw-solution-architect` reads `wave-decisions.md`
  (Requirements Summary, Constraints Established, six flags) plus
  `user-stories.md` (System Constraints + five stories).
- DEVOPS handoff: `nw-platform-architect` reads `outcome-kpis.md` only
  (the only DEVOPS-relevant artefact). Gate-5 mutation job for the new
  crate is the expected addition.
- DISTILL handoff: `nw-acceptance-designer` reads the BDD scenarios
  embedded in `user-stories.md` (no separate journey YAML for this
  feature; Kaleidoscope is Rust, no journey artefacts).
