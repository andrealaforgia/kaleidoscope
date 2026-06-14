# Story Map - `experimentable-stack-v0`

> Feature: the rest of Milestone 1 of the consolidation roadmap (C2 one-command run story +
> C3 telemetry generator + sample data + C4 getting-started docs). Built on the DONE, CI-green C1
> consolidated runtime (`kaleidoscope` binary). British English, no em-dashes.

## User: Sam Okonkwo, newcomer/evaluator

Secondary users: Andrea (local experimenter), a contributor verifying a change.

## Goal: experience "one command, send, see" locally in minutes

Run one command, the consolidated runtime + Prism come up, push sample telemetry (or it is
auto-seeded), see a metric painted in Prism and query logs + traces, following honest
getting-started docs.

---

## Backbone (user activities, left to right)

| A. Bring the stack up | B. Get telemetry into it | C. See and query it | D. Learn how |
|---|---|---|---|
| One command up (US-01) | Push sample telemetry (US-04) | Prism honest empty/data state (US-02) | Getting-started docs (US-06) |
| Clean / idempotent / fails clearly (US-03) | Fresh stack not empty on first look (US-05) | (see-a-metric is the payoff of B+C) | Existing paths still work (US-07) |

Cross-cutting guardrail across all activities: **US-07 - additive, nothing pre-existing breaks**.

---

## Walking Skeleton (thinnest end-to-end slice across all activities)

The minimum that makes "one command, send, see" real end to end:

- **A**: `make up` brings the consolidated runtime + Prism up over a shared volume, one tenant,
  auth off (US-01).
- **B**: one command pushes a sample metric + log + trace (US-04), or the seed provides one
  (US-05).
- **C**: Prism, pointed at the runtime, paints `request_count`; the logs/traces endpoints return
  the sample rows (US-02 + US-04 payoff).
- **D**: a getting-started section walks a newcomer through exactly the above (US-06).

Every backbone activity is represented above the line. The skeleton is the C2+C3+C4 happy path with
the minimum of each.

---

## Release Slices (sliced by outcome, mapped to roadmap items)

### Slice 1 (roadmap C2): "a one-command stack a newcomer can reach"

- **Stories**: US-01 (one command up, reachable in browser), US-02 (Prism pointed at the runtime,
  honest empty/error states), US-03 (clean, idempotent, fails clearly).
- **Target outcome**: from "no run story exists" to "one command brings up a reachable stack with a
  loaded UI", reliably and legibly.
- **Outcome KPI**: one-command bring-up success rate to a loaded UI and answering endpoints (target
  100%); it is the precondition for time-to-first-telemetry-seen.
- **Rationale**: this is the spine of the feature and the gate for the rest. Until the stack comes
  up with one command, there is nothing to send telemetry to or document.

### Slice 2 (roadmap C3): "the stack is not empty - send and see all three signals"

- **Stories**: US-04 (one-command generator pushes metrics + logs + traces), US-05 (fresh stack not
  empty on first look / seed).
- **Target outcome**: from "empty stack after bring-up" to "sample telemetry present and all three
  signals queryable; a metric painted in Prism".
- **Outcome KPI**: signal coverage after the generator (3/3 queryable) and first-look-not-empty
  rate (target 100%); this slice is what actually moves the north-star
  time-to-first-telemetry-seen.
- **Rationale**: a reachable but empty stack does not deliver the "see" half of the outcome. This
  slice converts a running stack into a visible experiment.

### Slice 3 (roadmap C4): "a newcomer can follow it unaided, and nothing old breaks"

- **Stories**: US-06 (getting-started docs for the consolidated path, honest about the verification
  limit), US-07 (additive guardrail: existing binaries, Dockerfiles, and CLI demo still work).
- **Target outcome**: from "only the CLI demo is documented" to "a newcomer completes the loop
  unaided from honest docs, and existing users are not regressed".
- **Outcome KPI**: docs-followability (a cold reader reaches "see a metric" from the docs alone);
  0 regressions across the four binaries, three Dockerfiles, and the CLI demo.
- **Rationale**: the outcome is only met when a stranger can reproduce it from the docs; and the
  additive guarantee protects the current users while the new path becomes primary.

---

## Priority Rationale

Priority order: **Slice 1 (C2) > Slice 2 (C3) > Slice 3 (C4)**, walking skeleton first.

- **Slice 1 is first** because it is the walking skeleton and the gate: every other story needs a
  running, reachable stack. It also derisks the riskiest assumption of the feature (that the C1
  binary + Prism compose cleanly into one reachable stack with the existing relative-backend Prism
  config). Value high (unblocks everything), Urgency high (gate), Effort medium (compose + wrapper +
  Prism serving).
- **Slice 2 is second** because it is the highest-value outcome once a stack exists: it is the slice
  that actually moves the north-star (time-to-first-telemetry-seen). It depends on Slice 1. Value
  high (the "see" payoff), Urgency medium, Effort medium (generator choice is the main open
  question, flag F3).
- **Slice 3 is third** because docs document what Slices 1-2 deliver and therefore follow them, and
  the additive guardrail is a verification story that is cheapest to confirm last (after the
  additions exist). Value high (a stranger can reproduce; current users protected), Urgency medium,
  Effort low-medium.

Within slices: US-01 before US-02 before US-03 (you cannot point Prism at, or stress, a stack that
is not up); US-04 before US-05 (the seed reuses the generator's sample data); US-06 before US-07 is
arbitrary (independent), both close the feature.

Every story traces to an outcome KPI in `outcome-kpis.md`; there are no orphan stories.

---

## Scope Assessment: PASS

- **Stories**: 7 (US-01..US-07). Under the >10 oversize signal.
- **Bounded contexts / modules touched**: 3 - (1) compose + the make/just wrapper + a possible
  `Dockerfile.runtime` (devops/run-story); (2) the telemetry generator (CLI extension or a small
  new tool or an external generator wired in compose); (3) docs (README / `docs/`) plus the Prism
  serving wiring. At the >3 boundary but not over; the runtime itself is reused unchanged from C1.
- **Walking skeleton integration points**: bring-up, generator-to-ingest, Prism-to-query-router,
  docs. Under the >5 signal.
- **Independent user outcomes**: the three slices are the three roadmap items and are deliberately
  thin; they ship in sequence as one coherent Milestone-1 completion, not as separable features.
- **Verdict**: right-sized as a single feature with three thin slices. No split needed. The
  Elephant Carpaccio shape is already the three-slice plan above (one slice per roadmap item, each a
  working behaviour a newcomer can verify).
