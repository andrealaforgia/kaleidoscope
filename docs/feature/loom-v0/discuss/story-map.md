# Loom v0 — story map

## Backbone (operator activities)

```
Author → Validate → Plan → Apply → (operator deploys) → Audit
  (Sasha)  (CI hook)  (PR review)  (CI merge)              (Riley)
```

Each activity is one user story. The walking skeleton is US-LO-01;
the remainder grows the verbs (plan, apply, pre-commit integration)
in elephant-carpaccio slices, each end-to-end in ≤ 1 day.

## Slices (elephant carpaccio)

| Slice | Story | Learning hypothesis (disproves X if it fails) |
|---|---|---|
| 01 | US-LO-01 | Disproves "Loom can wrap Beacon's loader as a CLI without re-implementing parsing". |
| 02 | US-LO-02 | Disproves "the plan output is operator-readable AND machine-parseable simultaneously". |
| 03 | US-LO-03 | Disproves "atomic write + idempotency hold under realistic catalogue churn". |
| 04 | US-LO-04 | Disproves "Loom's diagnostic format integrates with standard CI tooling (grep, PR-comment posting)". |

## Slice taste tests

- ✔ Each slice ships end-to-end (CLI invocation → operator-readable
  output → exit code).
- ✔ Slice 01 reuses Beacon's loader as a runtime dep — no
  re-implementation, no schema duplication.
- ✔ No slice depends on a new abstraction that has to be invented
  first.
- ✔ Each slice's learning hypothesis disproves a specific
  pre-commitment.
- ✔ Production-data shapes: real TOML files authored in the team's
  voice; the 50-rule corpus mirrors `acme-observability`'s shape.
- ✔ Dogfood moment per slice: at slice close, Sasha runs the
  binary against the team's actual Git repository.

## Walking skeleton

US-LO-01 ships the minimum that connects every concentric layer:
CLI argument parsing → Beacon's loader → exit code mapping. Every
later slice grows ONE of those layers (planner, applier, JSON
output) without adding new ones.

## Prioritisation rationale

Order is US-LO-01 → 02 → 03 → 04. Justification per slice:

- **01 first**: validates the integration with Beacon's loader is
  workable. If `beacon::load_rules` cannot be invoked from a
  separate crate, the entire feature is in doubt.
- **02 second**: the planner is the highest-value operator-visible
  surface. Without it Loom is just a wrapper around `beacon
  validate-toml`; with it Loom is the change-control surface.
- **03 third**: the applier is structurally simpler than the
  planner (it's mostly atomic file operations) but is the
  highest-stakes operationally. Done last among the core features.
- **04 last**: CI integration is a packaging concern; the work is
  small and depends on slices 01-03 being stable.
