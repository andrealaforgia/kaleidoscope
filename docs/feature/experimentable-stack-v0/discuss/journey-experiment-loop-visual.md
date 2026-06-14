# Journey - the newcomer's "one command, send, see" loop

> Companion to `user-stories.md`. The journey a newcomer (Sam) travels from a fresh clone to seeing
> telemetry, built on the DONE C1 consolidated runtime. Lightweight UX depth (infrastructure
> feature, no human-in-the-loop UI to research beyond Prism's first-look states). British English,
> no em-dashes.

## The loop

```
[Trigger:            ] -> [ A. Bring up   ] -> [ B. Send      ] -> [ C. See / query ] -> [ D. Learn / repeat ]
 "I cloned the repo,       make up             make demo          open Prism            getting-started docs
  does it work?"           (US-01/02/03)       (US-04/05)         see request_count     (US-06); existing
                                                                  curl logs/traces      paths intact (US-07)
   Feels: curious,         Sees: stack up,     Sees: "sent N      Sees: a line in       Feels: convinced,
   slightly sceptical      Prism loads,        samples", all      Prism; rows from      able to reproduce
   ("another platform")    honest empty        3 signals          the logs/traces       and explain it
                           state               accepted           queries
   Artifacts: ???          tenant=acme,        request_count,     /api/v1/query_range   README section,
                           pillar volume,      declined-checkout  /api/v1/logs          minimal config
                           ports 9090/91/92    log, trace id      /api/v1/traces        honesty note
```

## Emotional arc (Problem Relief -> Discovery Joy)

| Phase | Target emotion | Design lever | Failure to avoid |
|---|---|---|---|
| Trigger (clone) | Curious, mildly sceptical | A single obvious command in the README | A wall of manual steps; only a CLI demo |
| A. Bring up | Confident it started | One command; endpoints answer; Prism loads | A half-up stack; a blank or erroring Prism |
| (A failure) | Supported, not blamed | Clear named error on port conflict; idempotent re-up | Silent half-up; a cryptic compose error |
| B. Send | Anticipating the payoff | One generator command; clear "sent" feedback; clear failure if the stack is down | A generator that hangs or fails silently |
| C. See | Delighted (first success) | A real metric painted in Prism; logs/traces rows returned; non-empty first look (seed) | An empty Prism that reads as "broken" |
| D. Learn / repeat | Convinced, in control | Honest docs that reproduce the loop; existing paths preserved | Docs that overpromise; broken old paths |

The peak is C (seeing `request_count` painted), the moment a sceptical evaluator becomes a user.
The seed (US-05) guarantees C delivers a small win even before the user discovers the generator.

## Honesty buffer

The one negative-to-positive transition that needs an explicit buffer is the verification claim:
the docs state plainly that the browser experience is verified by bringing it up (not by a
browser-driven CI test), so the reader's trust is earned, not assumed (W6).

## Shared artifacts surfaced by the journey

Every `${value}` the journey shows (tenant `acme`, the pillar volume, ports 9090/9091/9092, Prism's
`/api/v1` backend, the sample metric/log/trace names, the bring-up command) is tracked with a single
source of truth in `shared-artifacts-registry.md`. The load-bearing ones are the tenant agreement
and the Prism backend agreement: if either drifts, the user reaches phase C and sees nothing.

## Integration checkpoints

- A -> B: the generator targets the same ingest ports the run story published, for the same tenant.
- B -> C: the metric the generator pushed (`request_count`) is the metric the docs tell the user to
  query and the metric Prism charts; one vocabulary end to end.
- C: Prism's backend resolves to the runtime's metrics router (same-origin recommended, F4).
- D: the docs quote the exact wrapper commands and the minimal config, and nothing the docs rely on
  has broken the pre-existing paths (US-07).
