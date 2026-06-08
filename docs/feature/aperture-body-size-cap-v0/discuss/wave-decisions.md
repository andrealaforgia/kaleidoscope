# Wave Decisions: aperture-body-size-cap-v0 (DISCUSS)

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-07.
British English. No em dashes in body.

## Origin

The four-quadrants implementer assessment for aperture
(`kaleidoscope-4-quadrants-theory/reports/aperture.md`, Q3 "DISCLOSED
omissions"): `max_recv_msg_size` is parsed for forward-compat but never
enforced, and the `body_too_large` event constant exists with no emitter.
This is the next item in the unwired-disclosed-knob family. The plumbing
is half present and honestly DISCLOSED as unwired; this feature wires it.
It is genuine DoS-guard value (an oversized OTLP body is decoded into
memory today with no cap), not a lie being corrected.

No DIVERGE artifacts exist for this feature
(`docs/feature/aperture-body-size-cap-v0/diverge/` is absent). The job
below is grounded in the four-quadrants Q3 finding and the project's
Earned-Trust posture (`brief.md` Principle 12). Absence of DIVERGE is
recorded as a risk; it does not block, because the gap and the fix
direction are verified directly in code on this branch.

## The Operator Job (JTBD, resource-protection framing)

> **When** I run a live aperture ingest gateway exposed to OTLP clients I
> do not fully control, **I want** to cap the accepted body size so a
> single oversized payload is rejected loudly before it is decoded into
> memory, **so that** one huge request (accidental or hostile) cannot
> exhaust the collector's memory and take the gateway down for every
> tenant.

The current behaviour accepts a body of any size and hands it straight to
the validator/decoder: on the HTTP arm axum has already buffered the full
`Bytes` body before the handler runs (`transport.rs:477`), and on the
gRPC arm tonic has already decoded the frame into a typed request which
the handler then re-encodes to bytes (`transport.rs:865-869`). Either way
the bytes are in memory before any size check could run today, because no
size check exists. A configured `max_recv_msg_size` is silently ignored
(`config/mod.rs:474-485`, "parsed for forward-compat... unused at v0").
The `body_too_large` event constant (`observability.rs:46`) has no
emitter. So an operator who sets the cap, expecting protection, gets none:
the disclosed-but-unwired knob.

## Feature framing decisions (DISCUSS, decided)

| ID | Decision | Rationale |
|---|---|---|
| F-Type | **Backend** (gateway ingest-path resource guard + observability) | No UI surface; the operator touchpoints are the TOML config knob `max_recv_msg_size`, the rejection the OTLP client receives, and the structured `body_too_large` stderr event. |
| F-Skeleton | **Walking Skeleton = No** (brownfield) | aperture exists and is tagged `aperture/v0.1.0`. The config schema parses the field, the event constant is defined, the ingest entry points and the refusal-shape precedent (the concurrency cap) all exist. This feature wires an existing-but-dormant knob; it is not a greenfield bootstrap. |
| F-UX | **UX research = Lightweight** | Single persona (an operator protecting a shared gateway plus the OTLP client that gets rejected). Emotional arc is Problem Relief (a configured cap that does nothing -> a cap that actually rejects oversized bodies and says so). No journey-visual / journey-yaml artifacts produced: backend feature with no screen flow, matching the sibling `aperture-serve-loop-error-surfacing-v0`. The config knob, the rejection, and the event shape are the only operator surfaces and are captured in the AC. |
| F-JTBD | **The cap-the-body-size operator job** (above) | Grounded in the four-quadrants Q3 DISCLOSED-omission finding + the Earned-Trust posture. F-JTBD = No (per the run authorisation) so no standalone job-analysis artifact is produced; the job is stated here and carried into the stories. |
| F-Slicing | **Single coherent slice, US-01..US-03, covering BOTH logs and traces** | The size check is identical for every signal; logs and traces share one mechanism. One slice carries both. The carpaccio cut-line, if DESIGN finds the enforcement-site ripple large, is logs-first then traces. See "Carpaccio cut-line" below. Metrics is named as a coverage question for DESIGN (see D4). |

## Verified Code Findings (confirming the four-quadrants read)

All confirmed by reading the source on this branch.

| Claim | Verified location | Finding |
|---|---|---|
| `max_recv_msg_size` is parsed but unused (DISCLOSED) | `crates/aperture/src/config/mod.rs:474-485` | `max_recv_msg_size: Option<u32>` on `TransportArm`, with `#[allow(dead_code)]` and the comment "parsed for forward-compat... unused at v0 -- Slice 05's concurrency limiter is the only backpressure surface lit up". |
| There is NO `max_recv_msg_size` on `Config` and no accessor | `crates/aperture/src/config/mod.rs:46-58` (struct), `:193-194` (the parallel `max_concurrent_requests` accessor) | `Config` has `max_concurrent_requests: u32` with a `max_concurrent_requests()` accessor; there is no `max_recv_msg_size` field or accessor. The raw value never reaches `Config`. |
| `into_config` reads concurrency from gRPC only, ignores HTTP at v0 | `crates/aperture/src/config/mod.rs:608-617` | "ADR-0008 declares the field per-transport but ADR-0010 / Slice 05 takes a single cap at v0. We honour the gRPC value... the HTTP value (if distinct) is ignored at v0 by design." The size cap faces the SAME single-vs-per-arm choice (D1). |
| `body_too_large` event constant exists with NO emitter | `crates/aperture/src/observability.rs:46` | `pub const BODY_TOO_LARGE: &str = "body_too_large";` in the closed `#[allow(dead_code)]` vocabulary. No call site emits it (the only emitters today are `request_received`, `sink_accepted`, `concurrency_cap_hit`, `unsupported_media_type`, and the listener/drain family). |
| The ingest entry points sit BEFORE validate/decode in app.rs | `crates/aperture/src/app.rs:65-82` (`ingest_logs`), `:94-111` (`ingest_traces`) | `pub async fn ingest_logs(body: &[u8], transport, tenant, sink)` calls `validate_logs(body, framing)` immediately; the size check would sit at the top of these fns, before `validate_logs`/`validate_traces`. BUT the body is already `&[u8]` in memory here (see the transport findings below). |
| app.rs ingest fns do NOT emit events (transport + sink do) | `crates/aperture/src/app.rs:62-64` (doc), `:65-82` | "This function does not emit events." The transport adapter emits `request_received` before calling; the sink emits `sink_accepted`. So a `body_too_large` emit added in app.rs would be the FIRST event app.rs emits, OR the check belongs in transport (D2/D3). |
| HTTP arm: the full body is BUFFERED by axum before the handler runs | `crates/aperture/src/transport.rs:473-477` (`handle_logs(State, HeaderMap, body: Bytes)`), `:579-583` (`handle_traces`) | axum extracts `body: Bytes` (the entire request body, already read into memory) before the handler body executes. A check inside the handler or in app.rs runs AFTER the bytes are buffered. The strongest guard would reject before the full buffer is read (axum `DefaultBodyLimit` / a length-limited extractor) -- a DESIGN decision (D3). |
| gRPC arm: tonic DECODES the frame, then the handler RE-ENCODES to bytes | `crates/aperture/src/transport.rs:865-869` (`req.encode_to_vec()`), mirrored at `:949-953` (traces) | tonic decodes the protobuf frame into a typed `ExportLogsServiceRequest` BEFORE the handler runs; the handler then re-encodes it to `bytes` to feed the harness. So by the time an app.rs `&[u8]` check could run on gRPC, the body has ALREADY been decoded into a typed value. The strongest gRPC guard is tonic's `max_decoding_message_size` on the service, set from the cap -- a DESIGN decision (D3). |
| There is an established refusal-shape precedent: the concurrency cap | `crates/aperture/src/backpressure.rs:89-140`, `transport.rs:485-490,791-801` (`refuse_http`), `:842-850` (gRPC `Status::resource_exhausted`) | The concurrency cap emits `event=concurrency_cap_hit transport=.. cap=.. in_flight_at_refusal=..` (warn), and refuses with HTTP 503 + `Retry-After: 1` / gRPC `RESOURCE_EXHAUSTED`, body naming the cap (`refusal_message`). `body_too_large` should match this shape (warn event naming the limit + actual size; a clear reject status). The reject STATUS for too-large is a DESIGN choice (D5): HTTP 413 vs 400; gRPC `RESOURCE_EXHAUSTED` vs `INVALID_ARGUMENT`. |
| The CI invariant: single validator per signal | `crates/aperture/src/app.rs:13-19` | `ingest_logs`/`ingest_traces`/`ingest_metrics` are the ONLY call sites of `validate_logs`/`validate_traces`/`validate_metrics`; an xtask AST gate enforces this. A size check placed in app.rs must sit BEFORE `validate_*` (an early return), not add a second validate call. A check placed in transport (before app.rs is called) does not touch this invariant. |
| `request_received` already carries the body byte count | `crates/aperture/src/transport.rs:524-529` (HTTP logs), `:871-876` (gRPC logs), mirrored for traces | The handlers already log `bytes = body.len()` / `bytes.len()`. The actual size the `body_too_large` event must report is already computed at these sites. |

## Verified actual-size / cap availability (who knows the size, who knows the cap)

| Fact | Location | Implication |
|---|---|---|
| HTTP handler knows the actual size as `body.len()` | `transport.rs:528,626,722` | The actual-size field for `body_too_large` is `body.len()` on the HTTP arm. |
| gRPC handler knows the actual size as `bytes.len()` AFTER re-encode | `transport.rs:869,953` | On gRPC the re-encoded length is known only after decode; the on-the-wire frame size is known earlier only to tonic's decoder. This is the crux of D3. |
| The cap is NOT yet plumbed to the handlers | `compose.rs:183-186` (limiter wiring), no `max_recv_msg_size` accessor | The concurrency limiter is constructed in `compose::spawn` and cloned into each `HttpState` / gRPC service impl. The size cap would follow the SAME wiring path: read from `Config`, thread into the handler state. DESIGN owns the exact carrier (a `u32` on `HttpState` / the service impls, or a tonic builder call). |

## Decisions FLAGGED for DESIGN (the heart of the feature)

DISCUSS encodes the REQUIREMENT (reject an oversized body before decode,
emit `body_too_large` with the limit and the actual size, accept at/under
the limit, no cap when unset, cover logs and traces, fail-closed and
visible). DESIGN (`nw-solution-architect`) owns the exact mechanism.

### D1 — Single collapsed cap vs genuinely per-transport-arm (config surface)

- **What**: the raw schema parses `max_recv_msg_size` per arm
  (`[aperture.transport.grpc]` and `[aperture.transport.http]`,
  `config/mod.rs:481-485`), but `Config` has no field yet. Concurrency
  took a single value from the gRPC arm and ignored HTTP at v0
  (`config/mod.rs:608-617`). DESIGN decides: mirror that (single
  collapsed cap, honour gRPC, accept-and-ignore HTTP) or surface a
  genuine per-arm cap on `Config`.
- **Luna's lean** (non-binding): mirror the concurrency precedent for v0
  consistency (single cap, honour the gRPC key, accept both silently), so
  the two backpressure knobs behave identically and the schema-test
  surface stays small. A per-arm cap is defensible because the two arms
  buffer differently (D3) but adds surface. DESIGN confirms and, if it
  diverges from the concurrency precedent, documents why.

### D2 — Where the check sits: transport (before buffering, ideal) vs app.rs (after the bytes are in memory) -- THE LOAD-BEARING OOM QUESTION

- **What**: app.rs `ingest_logs`/`ingest_traces` receive `body: &[u8]`
  that is ALREADY fully in memory (HTTP: axum buffered `Bytes`; gRPC:
  tonic decoded then the handler re-encoded). A check at the top of the
  app.rs ingest fns is the simplest seam and satisfies "reject before
  validate/decode-by-the-harness", but it runs AFTER the bytes are already
  buffered (HTTP) or already decoded (gRPC), so it is the WEAKER
  protection: the memory has already been spent on reading/decoding the
  oversized payload. The STRONGER protection rejects before the full body
  is read into memory.
- **Why it is flagged, not decided here**: this is the crux of the
  DoS-guard value. DISCUSS requires the reject to be "before validate /
  decode" and "fail-closed and visible", and explicitly NOTES that
  checking AFTER the bytes are buffered is weaker protection. DESIGN
  decides the actual enforcement site and MUST state the protection
  strength it achieves (does it prevent the oversized allocation, or only
  prevent the downstream decode/validate?).
- **Luna's lean** (non-binding): the strongest honest guard is
  per-transport at the framework boundary -- on HTTP an axum
  `DefaultBodyLimit` / length-limited body extractor (rejects before the
  full body is buffered, using `Content-Length` and/or a streaming cap);
  on gRPC tonic's `max_decoding_message_size` on the service builder
  (rejects before the frame is decoded). An app.rs `&[u8]` length check is
  acceptable as a v0 first cut ONLY if DESIGN documents that it is the
  weaker "guard the decode, not the allocation" variant and the AC are
  written to that honest claim. DESIGN picks; the chosen strength becomes
  a locked, honestly-worded AC.

### D3 — The actual-size and limit reported in the event, given the enforcement site

- **What**: `body_too_large` must carry the configured limit and the
  actual size. The actual size that is knowable depends on D2's site:
  `body.len()` (HTTP, post-buffer), `bytes.len()` (gRPC, post-re-encode),
  the `Content-Length` header (HTTP, pre-buffer), or the wire frame size
  (gRPC, known to tonic's decoder). If the framework rejects before the
  handler runs (the strong guard), aperture may only know the declared or
  truncated size, not the exact byte count.
- **Why it is flagged**: the event's honesty depends on what the
  enforcement site can truthfully report. DISCUSS requires the event to
  carry "the configured limit and the actual size"; DESIGN reconciles
  this with D2 (if the strong guard means only the declared
  `Content-Length` is known, the field must be named/worded honestly,
  e.g. `declared_size` vs `actual_size`).

### D4 — Metrics coverage (logs + traces required; metrics named)

- **What**: DISCUSS scopes the cap to logs and traces (the run brief).
  The third signal, metrics (`ingest_metrics`, `app.rs:124-141`;
  `/v1/metrics` + the gRPC `MetricsService`), has the identical ingest
  shape. The same OOM exposure exists on it.
- **Why it is flagged**: a guard that protects logs and traces but leaves
  metrics uncapped is a half-truth on the resource-protection job. DISCUSS
  requires logs + traces; DESIGN decides whether to extend the same
  mechanism to metrics in this slice (cheap, same shape) or defer it as a
  named thin follow-on. If deferred, the gap MUST be disclosed (not left
  silent), to avoid re-creating the exact disclosed-omission pattern this
  feature closes.
- **Luna's lean** (non-binding): include metrics in the same slice if the
  enforcement site is transport-level (it is then free, all three arms
  share the seam). If app.rs-level, all three ingest fns get the same
  early-return guard. Either way the marginal cost of the third signal is
  near zero; excluding it re-creates a disclosed omission.

### D5 — The reject status code / gRPC status for too-large

- **What**: the HTTP reject status (413 Payload Too Large -- the precise
  semantic -- vs 400 Bad Request) and the gRPC status
  (`RESOURCE_EXHAUSTED`, matching the concurrency cap's
  resource-protection framing, vs `INVALID_ARGUMENT`/`OUT_OF_RANGE`).
- **Why it is flagged**: it is a protocol-visible contract the OTLP
  client reacts to. DISCUSS requires the reject to be "clear" and
  "fail-closed and visible"; DESIGN picks the exact codes and the body
  text (matching the `refusal_message` precedent shape from the
  concurrency cap, naming the limit and the actual size). The chosen codes
  become locked ACs for DISTILL.

## Carpaccio cut-line

If DESIGN finds the enforcement-site ripple larger than expected (e.g. the
per-transport framework guards in D2 touch the spawn helpers and the
service builders more than a single app.rs early-return would), the slice
splits along the signal seam:

- **Half A (logs)**: enforce the cap on the logs ingest path (both
  transports); introduce the config accessor, the enforcement site, the
  `body_too_large` emit, the reject.
- **Half B (traces)**: apply the same mechanism to the traces path.

The mechanism is shared, so one slice is the default. If a split is
needed, whichever ships first carries the full mechanism (config wiring +
enforcement site + event emit + reject shape) so the second signal is a
thin follow-on. Logs is the natural first half (it is the slice-01 path
the rest of aperture was built around).

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| **No DIVERGE artifacts** -- JTBD not validated through a DIVERGE wave | Medium | Low | The job is grounded in the four-quadrants Q3 DISCLOSED-omission finding + the Earned-Trust posture (`brief.md` Principle 12) and verified directly in code. The gap (parsed-but-unused knob, emitter-less event) and the fix direction are unambiguous. Recorded here; does not block. |
| **The check lands AFTER the bytes are already in memory** (D2) -- the weak-guard trap | High (the simplest seam is the weak one) | High | An app.rs `&[u8]` length check runs after axum has buffered the body and after tonic has decoded the frame, so it guards the decode/validate but NOT the allocation -- weaker DoS protection than an operator setting a cap would assume. DESIGN MUST state the protection strength achieved and word the AC honestly; a weak v0 guard is acceptable ONLY if disclosed, never sold as preventing the oversized allocation. This is the Earned-Trust crux of the feature (D2). |
| **aperture is the LIVE ingest gateway** (real blast radius) | High (it is live) | High | aperture is tagged `v0.1.0` and is the live OTLP gateway on `:4317`/`:4318`. The cap must NOT reject legitimate at/under-limit bodies (a false reject drops real telemetry) and MUST be a no-op when unset (backward-compatible). The accept-at-limit and unset-no-cap ACs are the regression guards; the existing slice-01..05 acceptance suites must stay green. |
| **A too-large test that passes on the unwired knob** (the false-confidence trap) | Medium | High | The acceptance test must make the unenforced path OBSERVABLY wrong: with a cap set and an oversized body, today the body is ACCEPTED (200 / gRPC Ok) and NO `body_too_large` event is emitted. The test passes only when the oversized body is REJECTED and the event IS emitted with the limit + size. DISTILL must not inherit a test that passes against today's parsed-but-ignored field. |
| **Off-by-one at the boundary** (exactly-at-limit) | Medium | Medium | The at-limit body must be ACCEPTED and the at-limit-plus-one-byte body REJECTED. The boundary AC (US-02) pins this; a `>` vs `>=` mutation must be killed (Gate 5). |
| **Metrics left silently uncapped** (D4) | Medium | Medium | If DESIGN defers metrics, the gap MUST be disclosed (comment + wave note), not left silent, to avoid re-creating the exact disclosed-omission pattern this feature closes. Luna's lean is to include all three signals since the marginal cost is near zero. |
| **Enforcement-site ripple touches the spawn helpers / service builders** (D2/D3) | Low | Medium | If the strong per-transport guard touches `spawn_grpc`/`spawn_http` and the tonic service builders more than a single app.rs early-return, the carpaccio cut-line (logs then traces) is the pre-defined split. DESIGN confirms the ripple and the public-API impact (likely internal; any leak is semver-MINOR, pre-1.0). **NEVER 1.0.0.** |

## Constraints established

- **C1 -- aperture is LIVE.** The live OTLP gateway on `:4317`/`:4318`
  (`brief.md`; `lib.rs`). The cap is a runtime resource guard on a live
  ingest path; a false reject drops real telemetry, so accept-at/under-
  limit and unset-no-cap are mandatory regression guards.
- **C2 -- Backward-compatible: unset means no cap.** When
  `max_recv_msg_size` is unset, behaviour is UNCHANGED (no size check, no
  reject, no `body_too_large`). The field is `Option<u32>`
  (`config/mod.rs:485`) and the no-cap path is the existing behaviour. The
  unset-no-cap AC (US-03) is the regression guard.
- **C3 -- Reject BEFORE the harness validate/decode.** The size check
  runs before `validate_logs`/`validate_traces` (`app.rs:72,101`); an
  oversized body is never handed to the harness. DESIGN decides whether it
  also runs before the framework buffer/decode (the stronger guard, D2).
- **C4 -- Fail-closed and visible.** An over-limit body is REJECTED (not
  truncated, not silently dropped, not accepted), with a clear reject
  status (D5) and exactly one `body_too_large` event naming the limit and
  the actual size. The operator can see why the body was rejected.
- **C5 -- Use the existing `body_too_large` constant, add an emitter.**
  The constant already exists (`observability.rs:46`); this feature adds
  the call site(s) that emit it, in the same JSON shape and warn-level as
  the sibling `concurrency_cap_hit` event. No new constant needed.
- **C6 -- Match the concurrency-cap refusal shape.** The reject body and
  the event fields mirror the established backpressure precedent
  (`backpressure.rs:116-140`, `transport.rs:791-801`): a warn event
  naming the cap and the observed value, a refuse response naming the cap.
- **C7 -- Cover logs AND traces; metrics named (D4).** The cap applies to
  both logs and traces ingest in this slice; metrics is a DESIGN coverage
  decision that, if deferred, must be disclosed.
- **C8 -- In-process simulable.** The oversized-body reject is falsifiable
  in-suite by posting an over-limit body to `/v1/logs` (HTTP) and calling
  the gRPC `export` with an over-limit request, with a cap set via
  `Config::builder` (needs a new builder setter, parallel to
  `max_concurrent_requests`, `config/mod.rs:315-317`), asserting the
  reject status and the captured `body_too_large` event via
  `testing::stderr_capture`.
- **C9 -- Mutation testing 100%** on the modified files (`app.rs` and/or
  `transport.rs`, `config/mod.rs`, and `observability.rs` if touched) per
  ADR-0005 Gate 5 / CLAUDE.md. The boundary comparison (`>` vs `>=`), the
  unset-no-cap branch, and the emit must each be pinned.
- **C10 -- Rust idiomatic** per CLAUDE.md: data + free functions; the cap
  is a `u32` threaded through config and the handler state, not a new
  trait. No `dyn Trait` where a plain value suffices.
- **C11 -- Pure trunk-based, no CI gates** (MEMORY). CI is feedback, not a
  merge gate.
- **C12 -- Honour the CI single-validator-per-signal invariant**
  (`app.rs:13-19`). A size check placed in app.rs is an early return
  BEFORE `validate_*`, not a second validate call. A transport-level
  check does not touch the invariant.

## Notes for downstream waves

- **DESIGN** (`nw-solution-architect`): own D1-D5. Decide the config
  surface (D1: single collapsed vs per-arm), and crucially the
  ENFORCEMENT SITE (D2: transport framework guard vs app.rs early return)
  and STATE the protection strength it achieves honestly; reconcile the
  reported size/limit with that site (D3); decide metrics coverage (D4)
  and disclose any deferral; pick the reject codes (D5). Add the
  `Config::max_recv_msg_size` field + accessor + builder setter (parallel
  to `max_concurrent_requests`). Produce an ADR if the enforcement-site
  choice warrants one (a tonic `max_decoding_message_size` / axum
  `DefaultBodyLimit` policy is ADR-worthy). Confirm no public-API leak;
  any leak is semver-MINOR, pre-1.0, **NEVER 1.0.0.**
- **DISTILL** (`nw-acceptance-designer`): the BDD scenarios in
  `user-stories.md` are the source. The D2 protection strength, the D3
  reported-size shape, the D4 metrics decision, and the D5 reject codes
  DESIGN picks become locked ACs. Do NOT inherit a too-large test that
  passes against today's parsed-but-ignored field; the test must fail when
  the body is accepted and pass only when it is rejected with the event.
- **DELIVER** (`nw-software-crafter`): only the crafter writes
  `crates/*/src/`. Wire the config accessor + the enforcement site + the
  `body_too_large` emit + the reject. 100% mutation kill on the modified
  files (Gate 5), with the boundary comparison and the unset-no-cap branch
  pinned. Keep the existing slice-01..05 acceptance suites green.
