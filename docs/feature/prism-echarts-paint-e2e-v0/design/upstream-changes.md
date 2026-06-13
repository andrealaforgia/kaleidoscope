# Upstream changes (DESIGN → DISCUSS back-propagation)

Author: Morgan (nw-solution-architect). Wave: DESIGN. Date: 2026-06-13.
British English. No em dashes in body.

This file records refinements DESIGN makes to DISCUSS assumptions, so the
chain stays honest. None of the items below change a requirement; they
disambiguate within the already-stated scope boundary.

## UC-1 — the slice-01 main test's embedded `< 1000 ms` wall-clock assertion is OUT OF SCOPE (latency family)

**DISCUSS state.** The Scope boundary in `discuss/wave-decisions.md`
enumerates the OUT-OF-SCOPE perf family as "the two p95 perf-KPI blocks and
the operator-time guardrail inside `slice-01-*.spec.ts` (latency p95 < 2s /
< 800ms)". The walking-skeleton **main** test block (US-PR-01 / US-PE-01) is
listed as IN SCOPE.

**The disambiguation.** That main block's pseudocode carries a **fourth**
embedded timing assertion not named in the DISCUSS list:

```
// slice-01-walking-skeleton.spec.ts:65-66
// const t1 = Date.now();
// expect(t1 - t0).toBeLessThan(1000);
```

This `< 1000 ms` wall-clock gate is a latency KPI of the same family as the
p95 blocks and is subject to the same overnight wall-clock flake (MEMORY
`p95_wallclock_flakes_overnight`).

**DESIGN decision.** The in-scope slice-01 paint body keeps the genuine paint
assertions (the `data-prism-chart-painted="true"` wait, the non-blank canvas
pixel probe, the URL and chrome assertions) and **drops the embedded
`< 1000 ms` wall-clock line**. Latency belongs to the OUT-OF-SCOPE perf
family (deferred with the p95 blocks, per ADR-0058/0070 CI-gating treatment).

**Why this is not a requirement change.** DISCUSS already excludes the
latency KPIs from this feature; this only clarifies that the embedded line is
part of that same excluded family. The paint requirement (US-PE-01) is
asserted in full and unchanged. No AC is weakened: the feature still proves
the chart genuinely paints; it simply does not also assert how fast.

**Action for DISCUSS (optional, non-blocking).** If desired, add the
`< 1000 ms` embedded line to the Scope boundary's perf-family enumeration so
the four timing assertions are listed together. DESIGN proceeds on the
clarified reading regardless.

## Refinements within DESIGN-owned decisions (recorded, not back-propagated)

These are inside the latitude DISCUSS explicitly granted DESIGN (D2, D5);
they are not changes to DISCUSS and need no DISCUSS action. Recorded here for
traceability.

- **D2 technique**: DESIGN chose DOM `<canvas>` `getImageData` pixel-sampling
  over Luna's non-binding `chart.getDataURL()` lean, to avoid a production
  `window`-instance test seam (C1 minimal-surface). D2 explicitly delegated
  the technique to DESIGN/DISTILL.
- **D5 perf handling**: DESIGN chose `test.fixme` (not a file split) for the
  perf and out-of-story blocks. D5 explicitly delegated "split the file or
  fixme the blocks" to DESIGN.
</content>
