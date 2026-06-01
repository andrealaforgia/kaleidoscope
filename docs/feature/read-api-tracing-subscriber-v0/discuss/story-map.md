# Story Map — read-api-tracing-subscriber-v0

## Backbone (operator's startup-visibility activity)

```
Adopt aperture's       Apply the posture          Verify the events
subscriber posture  -> to the 3 read binaries  -> are visible on stderr
(the reference)        (query / log / trace)      (black-box capture)
```

| Activity                         | Tasks under it                                   |
|----------------------------------|--------------------------------------------------|
| Adopt aperture's posture         | US-05 (one pattern: JSON-to-stderr + EnvFilter)  |
| Apply to the 3 read binaries     | US-01, US-02 (log), US-03 (query), US-04 (trace) |
| Verify events on stderr          | AC of every story; harness grep of events        |
| (optional) Pre-init via eprintln | US-06                                            |

The backbone runs left to right: the aperture posture is the reference,
each read binary adopts it, and each adoption is proven by capturing
stderr and grepping the structured events.

## Walking Skeleton

**US-01 (log-query-api startup lifecycle visible on stderr).**

This is the minimum end-to-end slice that proves the whole approach: pick
one read binary, add the `tracing-subscriber` dependency, install the
aperture-posture subscriber as the first action in `main`, and confirm via
captured stderr that `log_query_api_starting` and `listener_bound` render.
Once US-01 works, US-02/03/04 are mechanical repetitions of the same
pattern (US-02 on the same binary's refusal path, US-03/04 on the other
two binaries), and US-05 falls out as the cross-cutting uniformity check.
The EDD-verifier explicitly captured the log-query-api gap (LQ02/LQ03), so
proving it on log-query-api first directly addresses confirmed evidence.

## Slicing by Outcome

Although the brief frames this as a single read-tier slice for uniformity,
the work is internally ordered by operator outcome, not by technical layer:

- **Slice A (walking skeleton, US-01):** prove startup visibility on one
  binary. Outcome: an operator can confirm log-query-api is up.
- **Slice B (US-02):** add refusal visibility on the same binary. Outcome:
  an operator learns WHY log-query-api refused to start.
- **Slice C (US-03, US-04):** replicate A+B on query-api and
  trace-query-api. Outcome: same visibility across the metrics and traces
  read APIs. (trace-query-api is the verifier's directly-confirmed TQ01
  case.)
- **Slice D (US-05):** confirm all four binaries share one subscriber
  configuration. Outcome: one format and one filter across the read tier.
- **Slice E (US-06, optional):** pre-init `eprintln!` parity with aperture.
  Outcome: even earliest-stage failures reach stderr.

All slices ship together as one delivery for read-tier uniformity, but the
ordering above is the safe build sequence: A proves the mechanism, B adds
the highest-value operator signal (the refusal reason), C generalises, D
locks uniformity, E closes the pre-init corner.

## Priority Rationale

1. **US-01 first** — walking skeleton; proves the subscriber mechanism
   end-to-end on the binary the verifier already exercised. Lowest risk,
   highest learning leverage: if the JSON-to-stderr capture does not work
   here, every other story is blocked, so failing fast here is cheapest.
2. **US-02 next** — highest operator value. The fail-closed refusal reason
   is what unblocks the EDD-verifier's issue-005 tightening (asserting the
   structured `health.startup.refused` instead of the bare `Err`). Depends
   on US-01's subscriber being installed on the same binary.
3. **US-03, US-04** — generalise the proven pattern to the other two read
   binaries. Mechanical once US-01/US-02 land; trace-query-api (US-04) is
   the verifier's directly-confirmed TQ01 gap so it must not be skipped.
4. **US-05** — cross-cutting uniformity check; naturally satisfied if
   US-01..04 all reuse one init expression (ideally a shared helper).
   Validated last because it asserts the consistency of the prior four.
5. **US-06 (optional)** — pre-init `eprintln!` parity. Lowest priority: the
   pre-init failure surface in the read binaries is small, and the core
   issue-005 fix (structured events on stderr) does not depend on it. Ship
   it for full aperture parity if cheap; defer if it threatens the slice
   size.

Dependencies: US-02 depends on US-01 (same binary, subscriber must exist
first). US-05 depends on US-01..04 (asserts their consistency). The
`tracing-subscriber` dependency addition is a prerequisite for all stories
and is shared across the three crates.

## Scope Assessment: PASS

6 stories (one optional), 3 crates/modules touched (query-api,
log-query-api, trace-query-api; plus an optional shared helper in the
existing query-http-common crate), 0 new integration points (stderr is an
existing process surface), no HTTP contract change. Estimated 1 day of
crafter work: the change is a near-identical few-line edit repeated across
three `main.rs` files plus three `Cargo.toml` dependency additions, with
black-box stderr-capture acceptance tests. Well within the Elephant
Carpaccio bound. Right-sized; no split required.
