<!-- markdownlint-disable MD024 -->

# User Stories: aperture-body-size-cap-v0

## Origin and Job Grounding

No DIVERGE artifacts exist for this feature
(`docs/feature/aperture-body-size-cap-v0/diverge/` is absent). Origin is
the **four-quadrants implementer assessment** for aperture
(`kaleidoscope-4-quadrants-theory/reports/aperture.md`, Q3 "DISCLOSED
omissions"): `max_recv_msg_size` is parsed for forward-compat but never
enforced (`config/mod.rs:474-485`) and the `body_too_large` event constant
exists with no emitter (`observability.rs:46`). This is genuine DoS-guard
value, not a lie being corrected: the knob is honestly disclosed as
unwired today, and this feature wires it. The job below is grounded in
that Q3 finding and the project's Earned-Trust posture
(`docs/product/architecture/brief.md` Principle 12). Absence of DIVERGE is
recorded as a risk in `wave-decisions.md`; it does not block, because the
gap and the fix direction are verified directly in code.

## The Operator Job (JTBD, resource-protection framing)

> **When** I run a live aperture ingest gateway exposed to OTLP clients I
> do not fully control, **I want** to cap the accepted body size so a
> single oversized payload is rejected loudly before it is decoded into
> memory, **so that** one huge request (accidental or hostile) cannot
> exhaust the collector's memory and take the gateway down for every
> tenant.

The current behaviour accepts a body of any size and hands it to the
validator/decoder. A configured `max_recv_msg_size` is silently ignored;
the `body_too_large` event never fires. An operator who sets the cap,
expecting protection, gets none: the disclosed-but-unwired knob.

## Verified Code Findings (confirming the four-quadrants read)

All confirmed by reading the source on this branch. Full table in
`wave-decisions.md` > Verified Code Findings. Summary:

| Claim | Verified location | Finding |
|---|---|---|
| `max_recv_msg_size` parsed but unused (DISCLOSED) | `config/mod.rs:474-485` | `Option<u32>` on `TransportArm`, `#[allow(dead_code)]`, "unused at v0". |
| No `max_recv_msg_size` on `Config`, no accessor | `config/mod.rs:46-58,193-194` | Parallel `max_concurrent_requests` field + accessor exist; the size cap has neither; the value never reaches `Config`. |
| `body_too_large` constant exists, no emitter | `observability.rs:46` | `pub const BODY_TOO_LARGE: &str = "body_too_large";` in the closed vocabulary; no call site emits it. |
| Ingest entry points sit before harness validate | `app.rs:65-82` (logs), `:94-111` (traces) | `validate_logs(body, framing)` is called immediately; a check would sit above it. But the body is already `&[u8]` in memory here. |
| HTTP body is buffered by axum before the handler | `transport.rs:473-477,579-583` | `body: Bytes` is the full body already read into memory. |
| gRPC frame is decoded by tonic, then re-encoded | `transport.rs:865-869,949-953` | tonic decodes to a typed request before the handler; the handler re-encodes to bytes. The body is decoded before any app.rs check. |
| Concurrency cap is the refusal-shape precedent | `backpressure.rs:89-140`, `transport.rs:485-490,791-801,842-850` | `concurrency_cap_hit` warn event naming the cap + observed value; HTTP 503 `Retry-After` / gRPC `RESOURCE_EXHAUSTED`. `body_too_large` should match this shape. |
| `request_received` already carries the byte count | `transport.rs:524-529,871-876` | `bytes = body.len()` / `bytes.len()` is the actual size the event must report. |
| CI invariant: single validator per signal | `app.rs:13-19` | A check in app.rs is an early return before `validate_*`, not a second validate call. |

## System Constraints

(Full text in `wave-decisions.md` > Constraints established. Pinned here
for the crafter and the reviewer.)

- **C1 -- aperture is LIVE** on `:4317`/`:4318`; a false reject drops real
  telemetry, so accept-at/under-limit and unset-no-cap are mandatory
  regression guards.
- **C2 -- Backward-compatible: unset means no cap.** `max_recv_msg_size`
  is `Option<u32>` (`config/mod.rs:485`); unset = existing behaviour.
- **C3 -- Reject BEFORE the harness validate/decode** (`app.rs:72,101`);
  DESIGN decides whether it also rejects before the framework buffer (the
  stronger guard, D2).
- **C4 -- Fail-closed and visible**: reject (not truncate, not drop, not
  accept), clear status (D5), exactly one `body_too_large` event naming
  the limit and the actual size.
- **C5 -- Use the existing `body_too_large` constant**
  (`observability.rs:46`), add an emitter; no new constant.
- **C6 -- Match the concurrency-cap refusal shape**
  (`backpressure.rs:116-140`, `transport.rs:791-801`).
- **C7 -- Cover logs AND traces; metrics named** (D4).
- **C8 -- In-process simulable** via a new `Config::builder` setter
  parallel to `max_concurrent_requests` (`config/mod.rs:315-317`) plus
  `testing::stderr_capture`.
- **C9 -- Mutation testing 100%** on the modified files (Gate 5): the
  boundary comparison, the unset-no-cap branch, the emit pinned.
- **C10 -- Rust idiomatic**: a `u32` threaded through config and handler
  state, not a new trait.
- **C11 -- Trunk-based, no CI gates** (MEMORY).
- **C12 -- Honour the single-validator-per-signal invariant**
  (`app.rs:13-19`).

---

## US-01: An oversized OTLP body is rejected before decode and named in a structured event

### Problem

Priya runs a multi-tenant Kaleidoscope ingest fleet. Her aperture
gateways accept OTLP from dozens of services she does not all control. She
sets `max_recv_msg_size` in her `[aperture.transport.grpc]` config
expecting it to protect the collector, because the schema accepts the key
and the docs mention a receive-size knob. It does nothing: the field is
parsed for forward-compat and ignored at v0 (`config/mod.rs:474-485`), so
a single 200 MB protobuf body from a misconfigured exporter is accepted
and decoded straight into the collector's memory. When it happens, Priya
sees the gateway's memory spike and the process get OOM-killed, taking
down ingest for every tenant on that instance, and there is nothing in the
structured stderr telling her a giant body arrived: the
`body_too_large` event constant exists (`observability.rs:46`) but nothing
emits it. Priya finds it impossible to protect her gateway from a single
huge payload, because the knob that promises that protection is wired to
nothing and the event that would name the offender never fires.

### Elevator Pitch

- **Before**: an operator sets `max_recv_msg_size` and gets no protection;
  an oversized OTLP body is accepted and decoded into memory; nothing on
  stderr names it. The knob is parsed-but-ignored.
- **After**: the operator-invocable surface is the running `aperture`
  binary's OTLP ingest endpoints (`POST /v1/logs`, `POST /v1/traces`, and
  the gRPC `LogsService.Export` / `TraceService.Export` methods); with
  `max_recv_msg_size` set, a body exceeding the cap is REJECTED before the
  harness decodes it, the client receives a clear reject (the exact status
  is DESIGN's D5 call), and aperture emits exactly one structured stderr
  line `event=body_too_large transport=... signal=logs limit=<bytes>
  size=<bytes>` in the same JSON shape as every other aperture event, so
  Priya's existing log scrape catches it and names the offending size.
- **Decision enabled**: Priya sees that an oversized body was rejected,
  with the limit and the actual size, and decides to chase the
  misconfigured exporter (or raise the cap) instead of discovering the
  problem only after an OOM kill takes the gateway down.

### Who

- Priya the platform operator | runs a live multi-tenant aperture ingest
  fleet exposed to partly-untrusted OTLP clients, scrapes structured
  stderr into a log/alert pipeline | motivated to cap the accepted body
  size so one huge payload cannot OOM the shared gateway, and to be told,
  in machine-parseable form, when a body is rejected for being too large,
  with the limit and the actual size.

### Solution

Wire the existing-but-dormant `max_recv_msg_size` knob to a real size
check on the ingest path. When a cap is configured and a body exceeds it,
aperture rejects the body BEFORE it is handed to the harness
(`validate_logs`/`validate_traces`) and emits exactly one
`event=body_too_large` line (warn level, matching the
`concurrency_cap_hit` precedent) carrying the transport, the signal, the
configured `limit`, and the actual `size`. The reject is fail-closed (the
body is never validated, decoded by the harness, or forwarded to the
sink). DESIGN owns the enforcement SITE (transport framework guard vs
app.rs early return, D2), the config surface (D1), the reported-size shape
(D3), and the reject status codes (D5); this story encodes the requirement
that an over-limit body is rejected before decode and named in one event.

### Domain Examples

#### 1: Happy Path / negative control -- an under-limit body is accepted unchanged

Priya sets `max_recv_msg_size = 4194304` (4 MiB) on her gateway. A normal
`payments-api` log export of 12 KB arrives on `POST /v1/logs`. It is well
under the cap, so aperture accepts it exactly as today: the harness
validates it, the record is forwarded to the sink, the client gets `200`,
and the usual `request_received` then `sink_accepted` events fire. NO
`body_too_large` line appears. The cap does not disturb legitimate
traffic.

#### 2: Error/Boundary -- an oversized logs body is rejected and named (HTTP)

Priya's cap is `4194304` (4 MiB). A misconfigured `bulk-importer` exporter
posts a 200 MB protobuf body to `POST /v1/logs`. aperture rejects it
before the harness decodes it: the client receives a clear too-large
reject (the exact HTTP status is DESIGN's D5 call, e.g. 413), the body is
never validated or forwarded to the sink, and aperture emits exactly one
`event=body_too_large transport=http_protobuf signal=logs limit=4194304
size=209715200` line on stderr at warn level, valid JSON in the same shape
every aperture event uses. Priya's scrape catches it and names the 200 MB
offender.

#### 3: Error/Boundary -- an oversized traces body is rejected and named (gRPC)

Priya's cap is `4194304` (4 MiB). A runaway `checkout-api` tracer sends a
180 MB `ExportTraceServiceRequest` to the gRPC `TraceService.Export`.
aperture rejects it before forwarding to the sink: the client receives a
clear too-large gRPC status (DESIGN's D5 call, e.g. `RESOURCE_EXHAUSTED`
matching the concurrency-cap precedent), and aperture emits exactly one
`event=body_too_large transport=grpc signal=traces limit=4194304
size=<bytes>` line. The traces arm surfaces identically to the logs arm;
only the `transport` and `signal` fields differ.

### UAT Scenarios (BDD)

#### Scenario: An under-limit body is accepted unchanged (negative control)

```gherkin
Given aperture is configured with a maximum receive body size of 4 MiB
And a logs export of 12 KB arrives on the OTLP ingest endpoint
When aperture processes the request
Then the body is accepted and forwarded to the sink
And the client receives a success response
And no body-too-large event appears on stderr
```

#### Scenario: An oversized logs body is rejected before decode and named on stderr

```gherkin
Given aperture is configured with a maximum receive body size of 4 MiB
And a logs body of 200 MB arrives on the OTLP logs ingest endpoint
When aperture processes the request
Then aperture rejects the body before it is validated or forwarded to the sink
And the client receives a clear too-large rejection
And aperture emits one structured body-too-large event naming the signal, the configured limit, and the actual size
And the event is emitted at warn level
```

#### Scenario: An oversized traces body is rejected before decode and named on stderr

```gherkin
Given aperture is configured with a maximum receive body size of 4 MiB
And a traces body of 180 MB arrives on the OTLP traces ingest endpoint
When aperture processes the request
Then aperture rejects the body before it is forwarded to the sink
And the client receives a clear too-large rejection
And aperture emits one structured body-too-large event naming the traces signal, the configured limit, and the actual size
```

### Acceptance Criteria

- [ ] With a cap set, an under-limit body is accepted and forwarded exactly as today, with NO `body_too_large` event (negative control, from scenario 1).
- [ ] With a cap set, an over-limit logs body is rejected before the harness validates or the sink is touched, the client gets a clear too-large reject, and exactly one `body_too_large` event names the signal, the limit, and the actual size, at warn level (from scenario 2).
- [ ] With a cap set, an over-limit traces body is rejected before the sink is touched, with the same reject + event shape, differing only in the `transport`/`signal` fields (from scenario 3).
- [ ] The event uses the existing `body_too_large` constant (`observability.rs:46`), in the same JSON shape and warn level as the `concurrency_cap_hit` precedent.

### Outcome KPIs

- **Who**: Priya's gateways and her log/alert pipeline, on the live
  multi-tenant ingest fleet.
- **Does what**: reject an over-limit OTLP body before the harness decodes
  it and emit one named `body_too_large` event, instead of accepting and
  decoding it silently.
- **By how much**: enforced size-cap sites move from 0 (today the knob is
  parsed and ignored) to the configured ingest paths (logs + traces, both
  transports); `body_too_large` emitters move from 0 to 1-per-rejection;
  the too-large AC is falsifiable in-suite (passes only when the body is
  rejected AND the event fires, fails against today's accept-and-ignore).
- **Measured by**: the oversized-body acceptance test (per signal, per
  transport) asserting the reject status and the captured `body_too_large`
  event with the correct `limit`/`size`; plus the under-limit negative
  control asserting no event.
- **Baseline**: today an oversized body is accepted and decoded; `cargo
  mutants` would show a surviving mutant on any size check because none
  exists; 0 `body_too_large` events ever fire.

### Technical Notes

- Depends on D1 (config surface), D2 (enforcement site -- THE load-bearing
  OOM question), D3 (reported-size shape), D5 (reject codes); all flagged
  for DESIGN in `wave-decisions.md`.
- Reuse the existing `body_too_large` constant (C5); match the
  `concurrency_cap_hit` event shape and the `refusal_message` reject-body
  shape (C6).
- Place any app.rs check as an early return before `validate_*` to honour
  the single-validator-per-signal CI invariant (C12).
- Test seam: a new `Config::builder().max_recv_msg_size(..)` setter
  (parallel to `max_concurrent_requests`, `config/mod.rs:315-317`) plus
  `testing::stderr_capture` (C8).

---

## US-02: The cap is exact at the boundary -- at-limit accepted, one byte over rejected

### Problem

A size cap that is fuzzy at the boundary is worse than none: if it rejects
bodies at or just under the limit, Priya loses legitimate telemetry and
distrusts the cap; if it accepts bodies just over the limit, the guard
leaks and the OOM exposure she set the cap to close stays open. Priya
needs the cap to mean exactly what it says: a body whose size equals the
limit is accepted (the limit is inclusive, the largest allowed body), and
a body one byte larger is rejected. Without a pinned boundary, a future
refactor could flip a `>` to a `>=` (or vice versa) and silently move the
line by one byte, and nobody would notice until a real body landed exactly
on it. Priya finds it impossible to reason about her cap, because "maximum
4 MiB" must not secretly mean "4 MiB minus one" or "4 MiB plus a bit".

### Elevator Pitch

- **Before**: the cap does not exist, so the boundary is undefined; once
  wired, an unpinned boundary could drift by a byte under refactoring and
  silently reject legitimate at-limit bodies or admit over-limit ones.
- **After**: on the running `aperture` binary's ingest endpoints, a body
  whose size exactly equals `max_recv_msg_size` is ACCEPTED (the limit is
  the largest allowed body, inclusive), and a body exactly one byte larger
  is REJECTED with the `body_too_large` event -- so Priya sends a body she
  has measured to be exactly at her 4 MiB limit and sees it accepted, and
  a body one byte over and sees the rejection, confirming the cap means
  precisely what it says.
- **Decision enabled**: Priya sets her cap knowing the exact largest body
  that will be admitted, and tunes it against her real payload sizes
  without fear of losing telemetry to an off-by-one.

### Who

- Priya the platform operator | tunes `max_recv_msg_size` against her
  observed payload-size distribution and needs the boundary to be exact |
  motivated to trust that a body at the limit is admitted and a body over
  the limit is rejected, with no silent one-byte drift.

### Solution

Define the limit as inclusive: a body of `size <= limit` is accepted, a
body of `size > limit` is rejected. Pin both edges of the boundary with
acceptance scenarios (at-limit accepted, at-limit-plus-one rejected) and
require the comparison to survive mutation testing (a `>` vs `>=` flip
must be killed, C9). DESIGN owns where the comparison lives (D2); this
story encodes the inclusive-limit semantics and the exact boundary
behaviour.

### Domain Examples

#### 1: Boundary -- a body exactly at the limit is accepted

Priya sets `max_recv_msg_size = 1048576` (1 MiB). A `payments-api` log
export whose encoded body is exactly 1048576 bytes arrives. The limit is
inclusive, so the body is accepted: the harness validates it, the record
is forwarded, the client gets success, and NO `body_too_large` event
fires. The largest allowed body is admitted.

#### 2: Boundary -- a body one byte over the limit is rejected

Priya's cap is `1048576` (1 MiB). A body of exactly 1048577 bytes (one
byte over) arrives. It exceeds the inclusive limit, so it is rejected
before decode, the client gets the too-large reject, and exactly one
`event=body_too_large limit=1048576 size=1048577` line fires. The single
extra byte is the difference between accept and reject.

#### 3: Edge Case -- a tiny cap rejects an ordinary body, proving the comparison is the limit, not a constant

Priya sets a deliberately tiny `max_recv_msg_size = 16` (16 bytes, a test
configuration) and sends an ordinary 12 KB log export. The body vastly
exceeds the 16-byte cap, so it is rejected with `limit=16
size=12288`. This proves the reject is driven by the CONFIGURED limit
value (not a hardcoded threshold): the same 12 KB body that was accepted
under the 4 MiB cap in US-01 is rejected under the 16-byte cap here.

### UAT Scenarios (BDD)

#### Scenario: A body exactly at the limit is accepted

```gherkin
Given aperture is configured with a maximum receive body size of exactly N bytes
And a body whose encoded size is exactly N bytes arrives on the ingest endpoint
When aperture processes the request
Then the body is accepted and forwarded to the sink
And no body-too-large event appears on stderr
```

#### Scenario: A body one byte over the limit is rejected

```gherkin
Given aperture is configured with a maximum receive body size of exactly N bytes
And a body whose encoded size is exactly N plus one bytes arrives on the ingest endpoint
When aperture processes the request
Then aperture rejects the body before it is forwarded to the sink
And aperture emits one body-too-large event reporting the limit as N and the size as N plus one
```

#### Scenario: The reject is driven by the configured limit, not a constant

```gherkin
Given aperture is configured with a very small maximum receive body size
And an ordinary body that would be accepted under a larger cap arrives
When aperture processes the request
Then aperture rejects the body
And the body-too-large event reports the small configured limit and the actual size
```

### Acceptance Criteria

- [ ] A body whose size exactly equals the configured limit is ACCEPTED and forwarded, with NO `body_too_large` event (inclusive limit, from scenario 1).
- [ ] A body exactly one byte over the limit is REJECTED with a `body_too_large` event reporting `limit=N size=N+1` (from scenario 2).
- [ ] The reject is driven by the configured limit value, not a hardcoded threshold: the same body accepted under a large cap is rejected under a tiny cap (from scenario 3).
- [ ] The boundary comparison survives mutation testing (a `>` vs `>=` flip is killed), per Gate 5 / C9.

### Outcome KPIs

- **Who**: Priya tuning `max_recv_msg_size` against her real payload
  sizes.
- **Does what**: gets an exact, inclusive boundary (at-limit accepted,
  over-limit rejected) so she can set the cap to a measured value without
  losing legitimate telemetry or leaking the guard.
- **By how much**: boundary ambiguity moves from undefined (no cap today)
  to exactly one byte (at-limit accepted, at-limit-plus-one rejected),
  pinned by two acceptance edges; off-by-one mutation survivors on the
  comparison move to 0 (100% kill on the boundary, Gate 5).
- **Measured by**: the at-limit-accepted and at-limit-plus-one-rejected
  acceptance tests; the tiny-cap test proving the limit is configurable;
  `cargo mutants` showing 0 survivors on the comparison line.
- **Baseline**: no cap exists today, so the boundary is undefined and
  unprotected; any future comparison would be unpinned without these
  scenarios.

### Technical Notes

- Depends on US-01 (the enforcement site and the event must exist before
  the boundary can be pinned on them).
- Inclusive-limit semantics (`size <= limit` accepted) is the requirement;
  DESIGN owns where the comparison lives (D2) and the integer type
  (`u32`/`u64`) against `body.len()` (`usize`).
- C9 / Gate 5: the boundary comparison and the unset-no-cap branch must
  each be killed by a test; the at-limit and at-limit-plus-one scenarios
  are the boundary kill, the tiny-cap scenario proves config-drivenness.

---

## US-03: When no cap is set, behaviour is unchanged; the guard covers both logs and traces

### Problem

This feature touches the LIVE ingest path of every aperture instance,
including the many that do not set `max_recv_msg_size` at all. If wiring
the cap changed behaviour for an unset config -- imposing a default cap,
or adding overhead, or rejecting bodies that flow fine today -- it would
break existing deployments that never asked for a cap. Priya runs a mix:
some gateways set the cap, most leave it unset and rely on upstream
limits. Equally, a cap that protected only the logs arm while leaving
traces uncapped would be a half-truth: the OOM exposure is identical on
both signals, and an operator who set the cap would reasonably assume it
covers all the telemetry her gateway accepts. Priya needs a hard
guarantee that an unset cap means today's exact behaviour (no cap, no
reject, no event), and that when the cap IS set it covers BOTH logs and
traces, not just one.

### Elevator Pitch

- **Before**: the cap does not exist, so unset is the only behaviour; once
  wired, a careless implementation could impose a default cap on unset
  configs (breaking existing deployments) or cover only one signal
  (leaving the other arm's OOM exposure open).
- **After**: on the running `aperture` binary, an instance with NO
  `max_recv_msg_size` configured behaves EXACTLY as today -- no size
  check, no reject, no `body_too_large` event, any body of any size
  flows as before -- so Priya's unset gateways are untouched; AND when the
  cap IS set, an oversized body is rejected on BOTH the logs and the traces
  endpoints identically, so Priya sees `body_too_large signal=logs` and
  `body_too_large signal=traces` from the same cap and knows both signals
  are guarded.
- **Decision enabled**: Priya rolls this out to her whole fleet without
  fear of breaking the unset gateways, and trusts that a cap she sets
  covers all the telemetry the gateway ingests, not just half of it.

### Who

- Priya the platform operator | runs a mixed fleet (some gateways cap,
  most leave it unset) and ingests both logs and traces on every instance
  | motivated to upgrade safely (unset = unchanged) and to have a set cap
  cover every signal, not a subset.

### Solution

Make the cap strictly opt-in: when `max_recv_msg_size` is `None`
(`config/mod.rs:485`, the unset case), the ingest path runs exactly as
today, with no size check, no reject, and no `body_too_large` event. When
the cap IS set, apply the identical check to both the logs and the traces
ingest paths (both transports). DESIGN decides whether metrics is included
in this slice or deferred as a disclosed follow-on (D4); this story
encodes the unset-no-cap backward-compatibility guarantee and the
logs-plus-traces coverage requirement.

### Domain Examples

#### 1: Happy Path / negative control -- an unset cap leaves behaviour unchanged

Priya's gateway has NO `max_recv_msg_size` in its config (the common
case). A 200 MB log body arrives -- exactly the body that would be
rejected if a cap were set. With no cap configured, aperture behaves as
today: it accepts the body and hands it to the harness (which may accept
or reject it on conformance grounds, unchanged), NO size check runs, and
NO `body_too_large` event fires. The unset gateway is byte-for-byte the
behaviour it had before this feature.

#### 2: Coverage -- the same cap rejects oversized logs AND oversized traces

Priya sets one `max_recv_msg_size = 4194304` (4 MiB). An oversized log
body to `/v1/logs` is rejected with `body_too_large signal=logs`, and an
oversized trace body to `/v1/traces` is rejected with `body_too_large
signal=traces`. The single configured cap guards both signals; neither arm
is left uncapped.

#### 3: Edge Case -- a zero or absent value is treated as no cap, not a zero-byte cap

Priya's config omits the key entirely (and, if DESIGN's D1 surface allows
it, a `0` is treated the same way per DESIGN's decision). aperture treats
this as "no cap": it does NOT reject every body as exceeding a zero-byte
limit. The absence of a cap never becomes an accidental reject-everything
configuration; only a positive configured limit imposes a check.

### UAT Scenarios (BDD)

#### Scenario: An unset cap leaves the ingest path unchanged (negative control)

```gherkin
Given aperture is configured with no maximum receive body size
And a very large logs body arrives on the ingest endpoint
When aperture processes the request
Then no size check is applied
And no body-too-large event appears on stderr
And the body is handled exactly as it was before this feature
```

#### Scenario: A single configured cap guards both logs and traces

```gherkin
Given aperture is configured with a maximum receive body size of 4 MiB
And an oversized logs body and an oversized traces body each arrive on their ingest endpoints
When aperture processes each request
Then the oversized logs body is rejected with a body-too-large event naming the logs signal
And the oversized traces body is rejected with a body-too-large event naming the traces signal
```

#### Scenario: An absent cap never becomes a reject-everything configuration

```gherkin
Given aperture is configured with no maximum receive body size
And an ordinary small body arrives on the ingest endpoint
When aperture processes the request
Then the body is accepted
And aperture does not treat the absence of a cap as a zero-byte limit
```

### Acceptance Criteria

- [ ] With no `max_recv_msg_size` configured, the ingest path runs exactly as before this feature: no size check, no reject, no `body_too_large` event, any body flows as today (negative control, from scenario 1).
- [ ] A single configured cap rejects oversized bodies on BOTH the logs and the traces ingest paths, with a `body_too_large` event naming the respective signal (from scenario 2).
- [ ] An absent cap is treated as "no cap", never as a zero-byte reject-everything limit (from scenario 3).
- [ ] The existing slice-01..05 acceptance suites stay green (regression guard on the live ingest path).

### Outcome KPIs

- **Who**: Priya rolling the cap out across a mixed fleet (some set, most
  unset), ingesting logs and traces on every instance.
- **Does what**: gets unset-means-unchanged safety (no behaviour change
  for gateways that do not set the cap) AND full-signal coverage (a set
  cap guards both logs and traces, not one).
- **By how much**: behaviour change for unset gateways is 0 (asserted by
  the negative control + the existing suites staying green); signal
  coverage moves from 0 enforced signals (today) to 2 (logs + traces),
  with metrics named for DESIGN (D4).
- **Measured by**: the unset-no-cap negative-control test asserting no
  size check and no event; the both-signals coverage test; the existing
  slice-01..05 acceptance suites staying green.
- **Baseline**: today every gateway is uncapped on every signal; there is
  no unset-vs-set distinction because the knob is ignored.

### Technical Notes

- Depends on US-01 (the enforcement mechanism) and shares its loci.
- C2: unset = `None` (`config/mod.rs:485`) = today's behaviour; the no-cap
  branch must be a true early no-op, pinned against a mutation (C9).
- D4 (metrics coverage) is DESIGN's call: include the third signal in the
  same slice (cheap, same shape) or defer with disclosure. This story
  requires logs + traces; it flags metrics so the decision is explicit,
  not a silent omission.
- The carpaccio cut-line (logs first, then traces) in `wave-decisions.md`
  is the fallback if DESIGN finds the enforcement ripple large; either way
  both signals ship before the feature is done.
