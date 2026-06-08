# Story Map: aperture-body-size-cap-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-07.
British English. No em dashes in body.

Companion to `user-stories.md` (US-01, US-02, US-03) and
`wave-decisions.md` (D1-D5; carpaccio cut-line). This map fixes the
backbone, marks the walking-skeleton posture, and slices the three
stories into releases with falsifiable learning hypotheses. It does not
restate the AC (they live per story in `user-stories.md`); it sequences
them.

## Persona and emotional arc (single persona, lightweight UX)

Priya, the platform operator, runs a live multi-tenant Kaleidoscope
ingest fleet exposed to partly-untrusted OTLP clients and scrapes
aperture's structured stderr into a log/alert pipeline. The arc is Problem
Relief: today a configured `max_recv_msg_size` does nothing (the knob is
parsed and ignored) and a single oversized body can OOM the shared gateway
with no telemetry naming it; after this feature an over-limit body is
rejected loudly before decode (one `body_too_large` event naming the limit
and the actual size), the cap is exact at the boundary, and an unset cap
leaves existing gateways untouched.

## Backbone (the ingest-path activity spine, left to right)

The horizontal sequence is the lifecycle of an OTLP body arriving at the
gateway, not a feature grouping. Each activity is a step a request moves
through.

```text
  BODY ARRIVES        SIZE IS CHECKED      BODY IS DECODED        OPERATOR LEARNS
  on an OTLP     ->   against the     ->   & validated &     ->  the cap fired
  endpoint            configured cap       routed to sink        (this feature:
  (HTTP buffered;     (today: NO check;    (today: any size      one body_too_large
   gRPC decoded)      the knob is          is decoded into       event names the
        |             ignored)             memory)               limit + size)
        |                  |                    |                      |
   out of scope       IN SCOPE             happy under-limit     IN SCOPE
   (framework         US-01 + US-02 +      path unchanged       US-01 (emit)
    ingress           US-03 (the gap:      (US-01 negative
    unchanged)         enforce the cap)     control, US-03 unset)
```

The first backbone cell (the body arriving via the framework) is
unchanged. The happy under-limit path stays exactly as today. The feature
owns the size-check cell (which does not exist today) and the operator-
learns cell (the `body_too_large` event, whose constant exists but has no
emitter).

### Backbone-to-story mapping

| Backbone activity | Owning story | Operator outcome |
|---|---|---|
| Size is checked against the configured cap | US-01 | an over-limit body is rejected before decode; one `body_too_large` event names the signal, limit, and size |
| The cap is exact at the boundary | US-02 | at-limit accepted, one-byte-over rejected; the limit is inclusive and config-driven |
| Unset = unchanged; both signals covered | US-03 | no behaviour change when unset; a set cap guards both logs and traces |

## Walking skeleton: No (brownfield)

`F-Skeleton = No` (recorded in `wave-decisions.md`). aperture exists and
is tagged `aperture/v0.1.0`: the config schema already PARSES
`max_recv_msg_size` (`config/mod.rs:481-485`), the `body_too_large` event
constant is DEFINED (`observability.rs:46`), the ingest entry points exist
(`app.rs:65-111`), and the refusal-shape precedent (the concurrency cap,
`backpressure.rs`) is in place. There is no greenfield end-to-end thread
to stand up. Instead the thinnest end-to-end slice is defined below.

### Thinnest end-to-end slice (the brownfield equivalent of a walking skeleton)

The thinnest slice that delivers a verifiable operator-visible behaviour
on its own is:

> A configured `max_recv_msg_size` is read into `Config` (D1), an
> over-limit body on ONE signal+transport (logs over HTTP) is rejected
> before the harness decodes it (US-01, D2 site), and aperture emits one
> `body_too_large` event naming the limit and the actual size, proven by
> ONE oversized-body acceptance test that FAILS against today's
> parsed-but-ignored field and an under-limit negative-control test that
> the cap does not disturb legitimate traffic.

That slice exercises the full vertical thread (config read -> size check
-> reject -> emit -> falsifiable test) for one arm. The remaining signals
and transports are then thin follow-ons over the same mechanism, plus the
boundary pins (US-02) and the unset-no-cap guarantee (US-03). The slice is
NOT "add the config accessor" or "wire the event constant" alone; those
are technical layers, not operator outcomes, and neither is demonstrable
to Priya by itself.

## Carpaccio slicing into releases

The default is one coherent slice carrying all three stories, because the
size check is identical for every signal and transport. The split below is
the pre-agreed fallback if DESIGN finds the enforcement-site ripple larger
than expected (`wave-decisions.md` carpaccio cut-line). Either way,
releases are sliced by operator outcome, not by technical layer.

### Release R0 (default): one coherent slice, both signals, both transports

| Field | Value |
|---|---|
| Scope | US-01 + US-02 + US-03, logs + traces, both transports, full mechanism (config read + enforcement site + `body_too_large` emit + reject) |
| Operator outcome | an over-limit body on any covered signal is rejected before decode and named; the boundary is exact; an unset cap is a no-op |
| Learning hypothesis | "One size check at the chosen enforcement site (D2), driven by one configured cap, is enough to guard logs and traces on both transports without disturbing legitimate or unset traffic." Falsified if the strong per-transport guard (D2) forces the spawn helpers / service builders to grow beyond the enumerated loci, or if the enforcement site cannot honestly report the actual size (D3). On falsification, split into R1 + R2 below. |
| Demonstrable in one session | yes (oversized-body test per signal + boundary tests + unset negative control) |

### Fallback split (if R0 falsified): R1 then R2, sliced along the signal seam

The full mechanism (config accessor + enforcement site D2 + `body_too_large`
emit C5 + boundary semantics US-02 + unset-no-cap US-03) ships in whichever
signal goes first, so the second signal is a thin follow-on.

#### R1 (carries the mechanism): the logs arm first

| Field | Value |
|---|---|
| Scope | US-01 + US-02 + US-03 for the logs path (both transports); introduces the config accessor, the enforcement site, the `body_too_large` emit, the reject, the boundary pins, the unset guarantee |
| Why logs first | logs is the slice-01 path the rest of aperture was built around (`app.rs:1-6`); the mechanism is most naturally proven there first |
| Operator outcome | oversized logs bodies are rejected and named on both transports; the boundary is exact; an unset cap is a no-op |
| Learning hypothesis | "The shared mechanism, proven on the logs arm, rejects a real over-limit body (test fails on today's accept-and-ignore), is exact at the boundary, and is a true no-op when unset (existing suites green)." |

#### R2 (thin follow-on): the traces arm

| Field | Value |
|---|---|
| Scope | apply the R1 mechanism to the traces path (`app.rs:94-111` / `/v1/traces` / gRPC `TraceService`); same size check, same event, same reject |
| Operator outcome | oversized traces bodies are rejected identically; both signals now covered (US-03 coverage fully satisfied) |
| Learning hypothesis | "With the mechanism already proven on logs, the traces arm is a same-shape wiring change with no new policy decisions; its own oversized-body test fails on today's behaviour and passes once wired." |

### Priority Rationale

Priority is by operator-outcome impact and dependency, not by feature
grouping.

1. **US-01 (enforce + emit) is the spine.** Without a size check that
   rejects an over-limit body and emits the event, there is no guard and
   nothing to alert on. It is the prerequisite for US-02 and US-03 (both
   depend on US-01's enforcement site). Highest priority.
2. **US-02 (exact boundary) makes the cap trustworthy.** A guard that
   exists but is fuzzy at the boundary loses legitimate telemetry or leaks
   the protection. The inclusive-limit boundary and the off-by-one
   mutation kill are what let Priya set the cap to a measured value.
   Second, because it pins the comparison US-01 introduces.
3. **US-03 (unset = unchanged; both signals) is the safe-rollout + full-
   coverage guarantee.** It protects every gateway that does NOT set the
   cap (the common case) from any behaviour change, and ensures a set cap
   covers both logs and traces, not a subset. Its unset negative control
   is a hard regression guard on the live gateway (C1): it must ship in
   the same release as US-01/US-02, never after, so the fix cannot break an
   unset deployment in production.
4. **If the split is taken, logs leads (R1).** It is the path the crate
   was built around; the traces follow-on (R2) is a thin same-shape change.

## Carpaccio taste tests

| Taste test | Verdict | Evidence |
|---|---|---|
| Is each release an end-to-end, demonstrable operator outcome (not a technical layer)? | PASS | R0, R1, R2 each reject a real over-limit body and emit the event, demonstrable by an oversized-body test in one session. The thinnest slice explicitly rejects "add the accessor" / "wire the constant" as non-outcomes. |
| Does each release deliver verifiable value on its own? | PASS | R0 covers both signals; R1 covers logs with the full mechanism + boundary + unset guarantee; R2 adds traces. Each is independently shippable and independently testable. |
| Is the highest-value work first? | PASS | US-01 (the actual guard) leads; the unset-no-cap safety and both-signals coverage ride in the same release so no half-guard ships. |
| Right-sized (1-3 days, 3-7 scenarios each)? | PASS | Three stories, 3 UAT scenarios each (9 total), one crate, one operator persona, no UI. Bounded loci (config accessor + one enforcement site + one event emit, applied to two signals). See Scope Assessment. |
| Does the regression guard ride with the change (no behaviour-change window)? | PASS | US-03's unset negative control + the existing slice-01..05 suites are required in the same release (Priority Rationale 3), so no release can change behaviour for an unset gateway. |
| Could a slice ship a half-truth (logs guarded, traces still uncapped)? | GUARDED | Only acceptable as R1 -> R2 where R1 is logs and R2 (traces) follows immediately; R0 avoids the half-truth by carrying both signals. Metrics is named for DESIGN (D4) so its coverage decision is explicit, never a silent omission. |

## Scope Assessment: PASS -- 3 stories, 1 bounded context (aperture crate), estimated 1-2 days

Oversized signals checked (none tripped):

- User stories: 3 (threshold > 10). PASS.
- Bounded contexts / modules: 1 crate, `aperture`; the touched modules
  (`config/mod.rs`, `app.rs` and/or `transport.rs`, and `observability.rs`
  only if the emit lands there) are all within it (threshold > 3 bounded
  contexts). PASS.
- Walking-skeleton integration points: WS = No (brownfield); the thinnest
  slice has one vertical thread per signal+transport (threshold > 5).
  PASS.
- Estimated effort: 1-2 days; one config accessor + builder setter + one
  enforcement site (the D2 choice) + one `body_too_large` emit, applied to
  two (or three, D4) signals (threshold > 2 weeks). PASS.
- Independent shippable outcomes: the signals could ship separately
  (R1/R2), but they share one mechanism and one persona; this is a
  controlled carpaccio split, not an oversized feature masquerading as one
  story. PASS.

No split required for size. The carpaccio split above is a DESIGN-ripple
contingency, not a size remedy. Proceeding without restructuring.

## Verified loci note (feeds D1/D2, confirmed on this branch)

- Config: `max_recv_msg_size: Option<u32>` parsed at `config/mod.rs:485`,
  never reaching `Config` (no field at `:46-58`, no accessor at
  `:193-194`); the parallel `max_concurrent_requests` plumbing
  (`:53,193-194,315-317,615-617`) is the template for the new accessor +
  builder setter.
- Event: `body_too_large` constant at `observability.rs:46`, no emitter.
- Enforcement candidate sites: app.rs `ingest_logs`/`ingest_traces` tops
  (`app.rs:65-82,94-111`, body already `&[u8]` in memory -- the weaker
  guard) vs the transport framework boundary (axum `Bytes` buffering at
  `transport.rs:473-477`; tonic decode + re-encode at `:865-869` -- the
  stronger guard). This is D2, the load-bearing OOM question DESIGN owns.
- Actual size already computed: `body.len()` / `bytes.len()` at the
  `request_received` emit sites (`transport.rs:524-529,871-876`).
