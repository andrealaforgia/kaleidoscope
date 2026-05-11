# Beacon v0 — story map

## Backbone (operator activities)

The Beacon v0 backbone follows the alerting operator's day:

```
Author rules → Validate → Deploy → Observe → Respond → Tune
  (Sasha)     (loader)  (--rules) (eval)    (Riley)   (Sasha)
```

Each activity is implemented by one or more user stories. The
walking skeleton is US-BE-01; the remainder of v0 grows the
catalogue, the evaluator's storm primitives, the integration
topology, and the SLO methodology in elephant-carpaccio slices.

## Slices (elephant carpaccio)

Each slice ships end-to-end in ≤ 1 day of crafter dispatch, with
a named learning hypothesis, production-shaped data (no synthetic
unless explicitly justified), and a dogfood moment.

| Slice | Story | Learning hypothesis (disproves X if it fails) |
|---|---|---|
| 01 | US-BE-01 | Disproves "Beacon can read from a Prometheus HTTP API end-to-end". |
| 02 | US-BE-02 | Disproves "the CUE schema scales beyond one rule without silent failures". |
| 03 | US-BE-03 | Disproves "grouping + inhibition collapse a 20-rule storm into one notification". |
| 04 | US-BE-04 | Disproves "the sink trait abstracts five different protocols cleanly". |
| 05 | US-BE-05 | Disproves "Google SRE MWMBR synthesises byte-equal to a hand-authored reference". |

## Slice taste tests

- ✔ Each slice ships end-to-end (load CUE → evaluate → emit), not a
  horizontal sliver.
- ✔ No slice ships > 4 new components; each slice grows the same
  module shape (loader, evaluator, sink, telemetry).
- ✔ Production-data shapes: real PromQL responses (recorded against
  a real Prometheus instance), not synthesised JSON; real CUE files
  authored in the team's voice.
- ✔ Each slice's learning hypothesis disproves a specific
  pre-commitment.
- ✔ No two slices are scale variants of each other.
- ✔ Dogfood moment per slice: at slice close, Sasha runs the
  binary against the team's existing Prometheus and confirms the
  new behaviour.

## Walking skeleton

US-BE-01 ships the minimum slice that connects every concentric
layer: CUE loader → rule evaluator → Prometheus HTTP fetch →
webhook sink. Every later slice grows ONE of those layers without
adding new ones until the v0 surface is complete.

## Prioritisation rationale

Order is US-BE-01 → 02 → 03 → 04 → 05. The justification per slice:

- **01 first**: highest learning leverage. If Beacon cannot complete
  one cycle against a real Prometheus, the entire feature is in
  doubt; we want to learn that as cheaply as possible.
- **02 second**: scaling the catalogue (the CUE schema) is the
  next-highest risk; a schema that does not scale to 35 rules is a
  rewrite, and we want to learn that before pouring effort into
  later slices.
- **03 third**: inhibition + grouping is the next-load-bearing
  primitive for incident response. Without it, Beacon is a one-rule
  toy.
- **04 fourth**: sink routing is mostly engineering, not learning;
  five adapters all consume the same incident shape. Lower
  uncertainty than 03; cheaper if we learn we got the trait wrong.
- **05 last**: SLO burn-rate is the most mathematically intricate
  but the most isolated — it produces synthesised PromQL that
  flows through the same evaluator (slice 01) and emits to the
  same sinks (slice 04). Maximum reuse of earlier-slice surface.
