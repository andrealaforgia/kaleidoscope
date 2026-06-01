# Story Map — gateway-tracing-subscriber-v0

## Backbone (operator activity: bring the gateway up and trust its logs)

```
Start the gateway  →  Confirm it is up & bound  →  Diagnose a refusal  →  Trust the stream
        │                      │                          │                      │
   US-04 (opt)             US-01                       US-02                  US-03
 pre-init eprintln    gateway_starting +          health.startup.refused   same shape as
                       listener_bound visible      visible before exit      aperture, no
                                                                            read-tier coupling
```

## Walking skeleton

Decision 2 = No (brownfield, isolated defect closure). There is no
greenfield walking skeleton. The minimum end-to-end slice that closes the
verifier's WIRE contract is **US-01 + US-02**: install the subscriber
early enough that both `gateway_starting` and `health.startup.refused`
render on stderr. `listener_bound` already renders (aperture emits it
inside spawn), so it rides along as a regression guard.

## Scope Assessment: PASS

- Stories: 3 core (US-01, US-02, US-03) + 1 optional (US-04). Well under
  the >10 oversized signal.
- Bounded contexts / crates touched: 1 (`kaleidoscope-gateway`). Under
  the >3 signal.
- Integration points: the aperture `spawn` seam (already wired) and the
  `probe_or_refuse` composition seam (already present). 0 new
  integration points.
- Independent shippable outcomes: a single coherent operability outcome
  (gateway lifecycle visible on stderr). No reason to split.

Right-sized: a single thin write-side slice. Estimated 1 day of crafter
dispatch (add one dep, move/insert the install point, optional pre-init
eprintln conversion, plus black-box tests).

## Priority Rationale

1. **US-01 + US-02 (ship together, highest impact).** These are the two
   events that close issue 005 for the gateway. The verifier's G01
   assertion needs both `gateway_starting` and `health.startup.refused`
   rendered. They share one root cause (subscriber installed too late /
   not at all in the gateway's own main) and one fix (install before
   main.rs line 102). Splitting them would be artificial: the same edit
   delivers both. Highest learning leverage too, because the install
   point that satisfies US-02 (must precede the refusal arm) is stricter
   than the one US-01 alone would need.
2. **US-03 (intrinsic to the same edit).** Choosing the aperture posture
   and forbidding the `query-http-common` edge is a property of how
   US-01/US-02 are implemented, not separate work. It is listed as its
   own story so the anti-coupling invariant is a first-class, testable
   constraint rather than a buried note.
3. **US-04 (optional, lowest priority).** Pre-subscriber failure
   formatting is a polish item that depends on the install point DESIGN
   chooses. If the install becomes the first statement of `main`, the
   pre-subscriber window shrinks to only `create_dir_all` and the store
   opens; DESIGN may fold US-04 into US-01/US-02 or defer it. It does not
   block issue 005 resolution.

## Dependencies

- US-02 depends on the install point preceding the `probe_or_refuse`
  failure arm (main.rs line 102), which is stricter than US-01's need.
  Resolve by choosing one install point that satisfies both. Tracked,
  not blocking.
- All stories depend on adding `tracing-subscriber` to the gateway's
  Cargo.toml (flag 1). Tracked, not blocking.
