# Wave Decisions: aperture-body-size-cap-v0 (DESIGN)

Author: Morgan (nw-solution-architect). Wave: DESIGN. Date: 2026-06-07.
Mode: PROPOSE (autonomous overnight run). British English. No em dashes.

Companion to the DISCUSS artifacts (`../discuss/`), to **ADR-0073**
(`docs/product/architecture/adr-0073-aperture-body-size-cap.md`), and to
the `## Application Architecture — aperture-body-size-cap-v0` section of
`docs/product/architecture/brief.md`. This file records the DESIGN
decisions (DD1..), the MANDATORY Reuse Analysis, the For-Acceptance-Designer
hand-off at the honest protection strength, and the self-review verdict.

DISCUSS handed DESIGN five open decisions (D1-D5), with D2 (the enforcement
site + its honest protection strength) flagged load-bearing. All five are
resolved below; the full rationale + five rejected alternatives live in
ADR-0073.

## Resolved decisions (DD1..DD5)

### DD1 (resolves DISCUSS D2) — Enforcement site: transport boundary, NOT app.rs

**Decision**: enforce the cap at the **transport framework boundary** (the
strong guard), with an app.rs `&[u8]` early-return kept ONLY as a disclosed
defence-in-depth secondary.

- **HTTP**: reject at the axum boundary, against `Content-Length` first
  (reject before reading a byte when the declared length exceeds the cap)
  and against the streamed length as a backstop (so an absent/lying
  `Content-Length` cannot bypass). The three handlers stop extracting the
  bare `body: Bytes` and read through a length-checked path consulting the
  cap on `HttpState`. **The bare `axum::extract::DefaultBodyLimit` layer is
  NOT used alone** because its silent 413 cannot carry the `body_too_large`
  event; the custom length-checked read is the seam that yields BOTH strong
  protection AND the event.
- **gRPC**: set `max_decoding_message_size(cap)` on each generated service
  server in `spawn_grpc` (`LogsServiceServer::new(svc).max_decoding_message_size(n)`,
  likewise Trace/Metrics). tonic refuses the frame in the codec, before
  decode, before the handler. The event is emitted from the codec-error
  surface (a thin layer / decode-error map), not the handler, because the
  handler does not run on an over-limit frame.
- **app.rs secondary**: a `size > limit` early-return at the top of
  `ingest_logs`/`ingest_traces`/`ingest_metrics`, BEFORE `validate_*`
  (honours the single-validator-per-signal invariant, `app.rs:13-19`). It
  guards the harness decode/validate, NOT the allocation; it is documented
  as the weaker, secondary guard and MUST NOT be sold as the OOM guard.

**Why not app.rs-only (the simplest seam)**: it runs after axum buffered
the full `Bytes` (`transport.rs:473-477`) and after tonic decoded the frame
(`transport.rs:865-869`), so the oversized allocation has already happened.
A guard that fires after the 200 MB body is in memory does not prevent the
OOM the operator set the cap to prevent. Adopting it as primary would force
the AC to overstate protection (the disclosed-omission pattern this feature
closes). See ADR-0073 Option B (rejected as primary, retained as secondary).

### DD1a (the honesty crux) — Honest protection strength achieved

The strong guard means **aperture's handler does not run on an over-limit
body**, so the protection and the handler-emitted event cannot share one
code path. We recover the event via a custom rejection seam at each
boundary. The locked, honest strength:

| Arm | Strength achieved | Disclosed residual |
|---|---|---|
| HTTP, `Content-Length` present | rejected **before any body byte is read** | none |
| HTTP, `Content-Length` absent/lying | rejected before the **full** body is buffered; <= ~one cap of bytes read before abort | bounded ~`limit` bytes, not zero |
| gRPC | frame refused **in the codec before decode**; typed request never allocated | tonic reads the frame header up to the cap before refusing |
| app.rs secondary | guards decode/validate only | body already in memory; weaker by design |

**AC wording rule (Earned-Trust, back-propagated to DISTILL)**: the AC say
"rejected before the harness decodes/validates it AND before the full
oversized body is buffered/decoded into memory". They MUST NOT claim "before
any byte is read" EXCEPT for the `Content-Length`-present HTTP case. The
claim equals the placement. See `upstream-changes.md` for the precise
back-propagation against the DISCUSS AC wording.

### DD2 (resolves DISCUSS D1) — Config surface: single collapsed cap

Mirror the concurrency precedent (`config/mod.rs:608-617`). Add
`Config.max_recv_msg_size: Option<u32>` (`pub(crate)`), a `pub(crate) fn
max_recv_msg_size(&self) -> Option<u32>` accessor, and a
`Config::builder().max_recv_msg_size(u32)` setter (parallel to
`max_concurrent_requests`, `:315-317`). `into_config` honours the gRPC arm
when set, accepts a distinct HTTP value silently at v0. **`None` (unset) =
no cap = today's exact behaviour** (C2). A `0`, if reachable, is treated as
"no cap" at the accessor/parse boundary, never a zero-byte reject-everything
limit (US-03 scenario 3). A genuine per-arm cap was rejected for v0 (adds
surface for no operator need; the two arms' OOM exposure is symmetric);
revisitable later.

### DD3 (resolves DISCUSS D3) — Reported size/limit shape

`limit=<configured bytes>` always. `size` = what the rejection surface
truthfully knows:
- HTTP `Content-Length` present: the declared `Content-Length`.
- HTTP streamed backstop: the byte count at which the read aborted (`>= limit`).
- gRPC: the length tonic's decoder refused at (the observed frame length
  exceeding the cap).

**Never fabricate a precise `size` the placement cannot observe.** The field
is `size`; its doc/body text names it for what it is (the size aperture
observed at the point of rejection). This reconciles DISCUSS's "carry the
limit and the actual size" with the strong-guard reality.

### DD4 (resolves DISCUSS D4) — Metrics coverage: include all three signals

The cap applies to **logs, traces, AND metrics** in this slice. The
boundary guard covers all routes (`/v1/{logs,traces,metrics}`) and all three
gRPC services at once; the app.rs secondary is a per-fn early-return on all
three `ingest_*`. The marginal cost of metrics is near zero, and excluding
it would re-create the identical-`ingest_metrics` OOM exposure as a fresh
disclosed omission. **Decision = include metrics, not defer.** No silent gap.

A runtime rejection **counter** (per tenant/signal) is named by DISCUSS as a
useful future fleet metric. This feature does NOT add it; the `body_too_large`
event stream is the v0 surface. The deferral is **disclosed** (here + brief
DEVOPS handoff), not a vacuous metrics claim. A future
`aperture-body-too-large-metric-v0` may add the counter.

### DD5 (resolves DISCUSS D5) — Reject codes

- **HTTP 413 Payload Too Large** (not 400): the request is well-formed, just
  too large; 413 is the precise semantic. Body names limit + size.
- **gRPC `RESOURCE_EXHAUSTED`** (status 8, not `INVALID_ARGUMENT`): matches
  the ADR-0010 concurrency-cap resource-protection framing and is tonic's
  native `max_decoding_message_size` status, so the boundary guard and the
  event agree. `grpc-message` names limit + size.

Both mirror the `refusal_message` shape (`backpressure.rs:130-140`).

## MANDATORY Reuse Analysis

Every existing-machinery reuse is preferred over a new component; every
CREATE-NEW item is justified by "no existing alternative".

| Existing machinery | Path | Decision | Justification |
|---|---|---|---|
| `max_recv_msg_size: Option<u32>` parsed per arm | `config/mod.rs:481-485` | **REUSE the parse** | already deserialises; this feature wires it through to `Config`. |
| `max_concurrent_requests` field + accessor + builder setter + `into_config` honour-gRPC | `config/mod.rs:53,193-194,315-317,608-617` | **MIRROR** | the established single-collapsed-cap template; identical shape. |
| `BODY_TOO_LARGE` constant | `observability.rs:46` | **REUSE** | the constant exists; add the emitter. No new constant (C5). |
| `concurrency_cap_hit` warn-event field shape + `refusal_message` body | `backpressure.rs:116-140`, `transport.rs:791-801,842-850` | **MIRROR** | warn-level JSON + cap-naming refusal body (C6). |
| `CapTransport {Grpc, HttpProtobuf}` + `as_str()` | `backpressure.rs:28-43` | **REUSE** | the `transport` field value for the event. |
| limiter threaded onto `HttpState` + service impls via `compose::spawn` | `transport.rs:349-354`, `compose.rs:183-186` | **MIRROR the wiring path** | thread the `Option<u32>` cap the same way the limiter is threaded. |
| `body.len()`/`bytes.len()` at `request_received` | `transport.rs:524-529,871-876` | **REUSE** | actual-size source for the secondary path where the body is in hand. |
| single-validator-per-signal invariant | `app.rs:13-19,65-111` | **HONOUR** | the secondary guard is an early return BEFORE `validate_*` (C12). |
| axum `DefaultBodyLimit` / tonic `max_decoding_message_size` | `axum 0.7` / `tonic 0.12` (`Cargo.toml:45,51`) | **REUSE the framework primitives** | wrapped with the custom rejection seam so the event survives. |
| `testing::stderr_capture` + `Config::builder` | `observability.rs`, `config/mod.rs:315-317` | **REUSE** | the in-process falsifiable test seam (C8). |
| **HTTP length-checked body-read seam** (replaces bare `Bytes`) | NEW in `transport.rs` | **CREATE (justified)** | no existing seam reads the body within a configured cap AND emits the event; `DefaultBodyLimit` alone loses the event (ADR-0073 Option C rejected). |
| **gRPC codec-error event surface** | NEW in `transport.rs` | **CREATE (justified)** | tonic refuses the frame before the handler; no existing surface emits `body_too_large` for the codec refusal. |
| **shared `body_too_large` event-constructor** | NEW (small) | **CREATE (justified)** | two emit surfaces (HTTP + gRPC) plus the secondary; one constructor builds the field shape once. |
| **app.rs `&[u8]` secondary early-return** | NEW in `app.rs` | **CREATE (justified, disclosed-secondary)** | defence-in-depth + a single delegation point; explicitly NOT the primary guard. |

**Net new surface (all INTERNAL)**: one `Config` field + accessor + setter;
one HTTP length-checked read seam; one gRPC `max_decoding_message_size` call
per service + codec-error event surface; one shared event-constructor; one
app.rs secondary early-return per `ingest_*`. No new crate, dependency,
event constant, or public type. Confirmed INTERNAL: Gate 2/3 do not fire;
any leak is semver-MINOR, pre-1.0, **NEVER 1.0.0**.

## For Acceptance Designer

**Driving ports** (black-box; never reach into private fns):

1. **HTTP** `POST /v1/logs` / `/v1/traces` / `/v1/metrics`
   (`application/x-protobuf`) on the running binary / in-process axum
   listener.
2. **gRPC** `LogsService.Export` / `TraceService.Export` /
   `MetricsService.Export`.
3. **structured stderr** via `testing::stderr_capture`.
4. **the recording sink** (empty on reject; record lands on accept).

Cap set in-suite via `Config::builder().max_recv_msg_size(n)` (new setter,
DD2).

**Exactly what each AC must assert (at the honest protection strength)**:

- **US-01 reject-and-name** (HTTP logs sc.2; gRPC traces sc.3): with a cap
  set, an over-limit body is **rejected before the harness validates and
  before the full body is buffered/decoded into memory** (HTTP **413**;
  gRPC **`RESOURCE_EXHAUSTED`**); the sink is **untouched**; exactly **one**
  `event=body_too_large transport=<http_protobuf|grpc> signal=<signal>
  limit=<bytes> size=<bytes>` at **warn**. **Word it "before the full body is
  buffered/decoded", NOT "before any byte"** (the `Content-Length`-present
  HTTP case MAY assert the stronger "before any body byte is read"). MUST
  FAIL on today's accept-and-ignore (200 / gRPC Ok, no event).
- **US-01 negative control** (sc.1): a 12 KB body under a 4 MiB cap is
  accepted exactly as today (validate -> sink -> success), **NO** event.
- **US-02 boundary** (sc.1/2/3): at-limit **ACCEPTED** (inclusive), no
  event; at-limit-plus-one **REJECTED** with `limit=N size=N+1`; tiny cap
  (`16`) rejects an ordinary 12 KB body (`limit=16 size=12288`), proving the
  reject is config-driven, not a constant. The `>`/`>=` boundary mutation
  must be killed (Gate 5). Exercise the edges so the boundary observes the
  size faithfully (e.g. exact `Content-Length`); the AC asserts the
  inclusive-limit BEHAVIOUR, DELIVER owns the `u32`/`u64`-vs-`usize`
  reconciliation.
- **US-03 unset = unchanged** (sc.1): with NO cap configured, the path runs
  **exactly as before** (no check, no reject, no event), asserted on a body
  that WOULD be rejected under a set cap, plus the slice-01..05 suites
  staying green. A `0` is "no cap", never a zero-byte limit (sc.3).
- **US-03 all signals** (sc.2 + D4): a single cap rejects oversized **logs,
  traces, AND metrics**, each with the correct `signal`. Metrics IS in this
  slice.

**Falsifiability (do NOT inherit a test that passes on the unwired knob)**:
every reject/boundary AC MUST fail against today's parsed-but-ignored field
(body accepted, no event) and pass ONLY when rejected + emitted with correct
`limit`/`size`. The unset control MUST assert no event + no reject on a body
that would be rejected under a set cap. The Earned-Trust probe for this
driven boundary IS the oversized-body acceptance test exercising the real
allocation/decode path.

**Negative controls (guardrails, must stay green)**: under-limit accept;
at-limit accept; unset unchanged; existing slice-01..05 acceptance suites.

## Does the `body_too_large` event survive at the chosen placement?

**Yes, on both arms, by design** — but NOT from the handler (the handler
does not run on an over-limit body at the strong guard). It survives via:
- **HTTP**: the custom length-checked body-read seam emits-and-rejects at the
  axum boundary (replacing the bare `Bytes` extraction; the bare
  `DefaultBodyLimit` layer would have lost it).
- **gRPC**: the codec-error surface recognises the `max_decoding_message_size`
  refusal and emits before surfacing `RESOURCE_EXHAUSTED`.
- a shared event-constructor builds the field shape once; the disclosed
  app.rs secondary also calls it.

The honest trade is in the `size` field fidelity (DD3): the strong guard
refuses before full read/decode, so `size` is the value the rejection
surface observed (declared `Content-Length` / abort count / refused frame
length), not necessarily a fully-read byte count. This is disclosed in the
field doc and the AC; it is the price of refusing before the OOM. The
alternative (let the body fully decode for an exact `body.len()`) is the
weaker guard and the wrong trade.

## Self-review verdict (reviewer not nested-invocable; self-review per project precedent)

Reviewed against the solution-architect critique dimensions
(`nw-sa-critique-dimensions`):

| Dimension | Check | Verdict |
|---|---|---|
| Reuse analysis present | mandatory table above; every CREATE-NEW justified | PASS |
| Every CREATE-NEW justified | 4 new items, each "no existing alternative" stated | PASS |
| C4 diagrams present | sequence/component view in brief; L1/L2 reuse stated, L3 below threshold (matches precedents) | PASS |
| ADR alternatives documented | ADR-0073 carries 5 alternatives (A accepted, B/C/D/E rejected with rationale) | PASS |
| No overstated claims | the honest protection-strength table is the load-bearing artefact; AC wording rule locks the claim to the placement; gRPC `size`-fidelity limit disclosed | PASS |
| Honest protection-strength stated | DD1a + ADR-0073 table; back-propagated to DISTILL via `upstream-changes.md` | PASS |
| Architectural bias (resume-driven, tech-preference, latest-tech) | no new crate, no new dependency, no trendy tech; mirrors established concurrency precedent + canonical axum/tonic primitives | PASS (none detected) |
| ADR quality (context, alternatives, consequences) | ADR-0073 has all three + ATAM + revisit gates | PASS |
| Completeness (perf/reliability/observability) | resource-utilisation + fault-tolerance ATAM; the event is the observability surface; metrics-counter deferral disclosed not vacuous | PASS |
| Feasibility / testability | in-process falsifiable via `Config::builder` + `stderr_capture`; ports-and-adapters boundary cleanly testable | PASS |
| Priority validation | the largest exposure (unbounded ingest allocation) IS the target; simpler app.rs-only alternative considered and rejected with rationale; data-grounded in verified loci | PASS |
| Solution-neutrality vs DISCUSS | DESIGN owns the mechanism (D1-D5); DISCUSS owned the requirement; no DISCUSS requirement contradicted (one AC-wording back-prop, recorded) | PASS |

**Verdict: APPROVED (self-review).** No critical/high issues. One MEDIUM
note carried as a disclosed consequence (gRPC `size`-field fidelity, DD3) —
disclosed in ADR-0073 Consequences, the brief, and the AC wording, not a
silent gap. One back-propagation to DISTILL (AC wording at the honest
strength) recorded in `upstream-changes.md`.

## Constraints inherited (DISCUSS C1-C12) — all honoured

C1 (live gateway: false-reject + unset regression guards) -> US-01 neg
control + US-02 at-limit + US-03 unset. C2 (unset=no cap) -> DD2 `None`
branch. C3 (reject before harness validate/decode) -> DD1 boundary guard
(stronger than C3 requires) + secondary before `validate_*`. C4 (fail-closed
+ visible) -> 413/`RESOURCE_EXHAUSTED` + one warn event. C5 (reuse the
constant) -> Reuse table. C6 (match concurrency-cap shape) -> DD5 +
`refusal_message` mirror. C7 (logs+traces; metrics named) -> DD4 includes
all three. C8 (in-process simulable) -> builder setter + stderr_capture.
C9 (100% mutation) -> DEVOPS handoff mutation scope. C10 (Rust idiomatic:
`u32` threaded, no new trait) -> DD2. C11 (trunk-based, no CI gate) -> noted.
C12 (single-validator invariant) -> secondary is an early return before
`validate_*`.
