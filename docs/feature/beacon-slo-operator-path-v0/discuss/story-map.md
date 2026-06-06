# Story Map — beacon-slo-operator-path-v0

British English throughout, no em dashes.

## User: Priya Nadkarni, SRE on the payments platform team

## Goal: declare an SLO in a beacon rule file and get correct multi-window multi-burn-rate alerting, without hand-writing the four threshold rules, with malformed SLOs refused at load

## Backbone

The operator's end-to-end activity sequence, left to right:

| Author SLO | Load and validate | Synthesise and merge | Evaluate and alert | Tune live |
|------------|-------------------|----------------------|--------------------|-----------|
| Write `[[slo]]` table in a TOML rule file (service, target, queries, budget, sinks) | Loader parses the SLO; validates target in `(0,1)` and budget == 30d | `synthesise_slo` expands one SLO into four MWMBR rules; merge into catalogue | beacon-server evaluates synthesised rules alongside hand-authored rules; page on fast burn, ticket on slow burn | Edit the SLO and `kill -HUP`; reload applies atomically or refuses |

## Ribs (tasks under each activity)

| Author SLO | Load and validate | Synthesise and merge | Evaluate and alert | Tune live |
|------------|-------------------|----------------------|--------------------|-----------|
| Declare service + good/total queries (US-01) | Parse `[[slo]]` into `Slo` (US-01) | Expand to four MWMBR rules (US-01) | Load four rules at startup (US-01) | Valid SLO edit reloads atomically (US-05) |
| Set target availability (US-01/US-02) | Reject target not in `(0,1)` with clear message (US-02) | Merge with hand-authored rules (US-04) | Page on fast burn, ticket on slow burn (US-01) | Malformed SLO edit refused, previous kept (US-05) |
| Set 30d budget (US-01/US-03) | Reject non-30d budget; make doc claim true (US-03) | Surface name collisions (US-04) | Existing rules-only path unchanged (US-04) | Surviving firing rule keeps `since`, no re-page (US-05) |
| Declare sinks (US-01) | Keep `deny_unknown_fields` "did you mean" (US-01, FLAG-1) | Deterministic synthesis (US-01) | | |

---

### Walking Skeleton

This is a BROWNFIELD feature (Walking Skeleton = No, per wave-decisions
DEC-2). The engine (`synthesise_slo`, `MWMBR_TABLE`), the loader, the
catalogue, the evaluator, and the SIGHUP reload path all already exist and
are green. There is no thin-end-to-end-skeleton-to-stand-up; the work is
WIRING an existing correct engine to the existing operator surface.

The thinnest reachable slice (the analogue of a walking skeleton here) is
**US-01**: one `[[slo]]` table parses, validates, synthesises four rules,
and loads end-to-end. This is the minimum that takes the SLO engine from
0% operator-reachable to reachable, and every other story builds on it.

### Slice 1 (Walking slice): the headline made real

- **Stories**: US-01 (an SLO declared in the file synthesises and loads).
- **Target outcome**: SLO operator-reachability from 0% to 100%; a declared
  SLO yields four correct loaded rules with zero hand-authored MWMBR rules.
- **Rationale**: this is the headline four-quadrants Q3 gap closed. It is
  independently demonstrable (start the server, see `rules_loaded=4`, watch
  a burn page) and every later slice depends on it.

### Slice 2 (Safety): malformed SLOs cannot reach evaluation

- **Stories**: US-02 (target not in `(0,1)` refused), US-03 (non-30d budget
  refused, doc claim made true).
- **Target outcome**: degenerate always-fire rules reaching evaluation to 0;
  out-of-range targets and non-30d budgets caught at load to 100%; false doc
  claims in the SLO area from 1 to 0.
- **Rationale**: beacon-server is operator-critical; a degenerate
  always-fire SLO is its own dishonesty. The refusal negatives are exactly
  what make the headline (Slice 1) trustworthy. US-03 also pays off the
  honesty debt (slo.rs:49-51).

### Slice 3 (Coexistence and live tuning)

- **Stories**: US-04 (synthesised rules coexist with hand-authored rules),
  US-05 (an SLO edit hot-reloads under SIGHUP).
- **Target outcome**: hand-authored rules silently dropped by SLO adoption
  to 0; SLO edits requiring a restart to 0; bad SLO edits going partially
  live to 0.
- **Rationale**: real operators have an existing rule catalogue and tune on
  a live daemon. Coexistence guards their existing coverage; reload
  consistency (ADR-0063) lets them adopt and tune SLOs the same way they
  already tune rules. Depends on Slices 1 and 2.

---

## Priority Rationale

Priority by outcome impact and dependency, using Value x Urgency / Effort
with the walking-slice-first tie-break.

| Priority | Story | Slice | Value | Urgency | Effort | Outcome link | Depends on |
|----------|-------|-------|-------|---------|--------|--------------|------------|
| 1 | US-01 | 1 | 5 | 5 | 2 | Reachability 0% to 100% | none (reuses engine + loader) |
| 2 | US-02 | 2 | 5 | 4 | 1 | Always-fire rules to 0 | US-01 |
| 3 | US-03 | 2 | 4 | 4 | 1 | False doc claims 1 to 0 | US-01 |
| 4 | US-04 | 3 | 4 | 3 | 2 | Silent shadowing to 0 | US-01 |
| 5 | US-05 | 3 | 4 | 3 | 2 | Restart-free tuning; no partial apply | US-01, US-02, US-03, US-04 |

- **US-01 first** (P1): it is the walking slice. Nothing else is reachable
  or demonstrable until the SLO can be declared and loaded at all. Highest
  value (the whole point), high urgency (the verifier's B06 depends on it),
  low effort (pure wiring of an existing engine).
- **US-02 then US-03** (P2, P3): the validation negatives. They are the
  riskiest-assumption-next: the moment US-01 makes the path reachable, an
  unvalidated SLO is a loaded gun (always-fire). US-02 ranks above US-03
  because a target typo (always-fire) is more dangerous than a budget typo
  (wrong-window thresholds), though both are P2-class. US-03 additionally
  pays the honesty debt (slo.rs:49-51), so it is bundled in the same safety
  slice.
- **US-04 then US-05** (P4, P5): coexistence and live tuning. Lower urgency
  because a fresh operator can use SLOs without them, but essential for real
  adoption. US-05 is last because it depends on all four prior stories (the
  reload re-runs the whole load+validate+synthesise+merge path) and reuses
  the ADR-0063 mechanism, so it is the cheapest of the dependents once the
  others land.

---

## Scope Assessment: PASS — 5 stories, 1 bounded context (beacon), estimated 2-4 days

Oversized signals checked (need 2+ to be oversized; this feature trips
none):

- User stories: 5 (at the right-sized boundary, not over).
- Bounded contexts / modules: 1 (the beacon crate plus its server binary;
  one module group: loader + slo + the reload orchestrator). Not >3.
- Walking-skeleton integration points: not applicable (brownfield); the
  wiring touches three existing seams (loader `FileShape`, the catalogue
  `Vec<Rule>`, the reload re-read), all within beacon. Not >5.
- Estimated effort: 2-4 days of wiring (no new engine logic; reuse
  `synthesise_slo`, the loader, the catalogue, the ADR-0063 reload). Not
  >2 weeks.
- Independent shippable outcomes: the three slices ship in sequence and
  depend on each other; this is one coherent feature, not several
  independent features that should split.

Verdict: right-sized. No split needed. Each of the five stories is 1-3 days
with 3-7 scenarios and is independently demonstrable.
