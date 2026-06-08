# ADR-0073 — Aperture body-size cap: transport-boundary enforcement, honest protection strength, and event survival

- **Status**: Accepted
- **Date**: 2026-06-07
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture-body-size-cap-v0`
- **Sibling precedent**: ADR-0010 (per-transport concurrency cap: the refusal-shape and event-shape template this ADR matches). ADR-0006 (tonic/axum transport stack). ADR-0008 (configuration schema: the `max_recv_msg_size` field this ADR finally wires). ADR-0009 (closed event vocabulary: the `body_too_large` constant this ADR finally emits).
- **Earned-Trust note (Principle 12)**: this ADR is the swallowed-resource-exhaustion sibling of ADR-0066 (serve-loop) and the cinder/sluice WAL fixes. The four-quadrants Q3 report flagged `max_recv_msg_size` as a DISCLOSED-but-unwired knob: an operator who sets it, expecting OOM protection, gets none. The load-bearing honesty requirement here is that the chosen enforcement site MUST deliver the protection the operator assumes (reject *before* the oversized body is buffered/decoded into memory), and the acceptance criteria MUST be worded to the protection strength actually achieved, never overstated.
- **Supersedes**: none.
- **Superseded by**: none.

## Context

Aperture v0 is the **live** OTLP ingest gateway (`tonic` gRPC on `:4317`, `axum` HTTP/protobuf on `:4318`, tagged `aperture/v0.1.0`). Today an oversized OTLP body is accepted and decoded into memory with no cap:

| Arm | Where the body lands in memory today | Locus |
|---|---|---|
| HTTP | axum extracts the **entire** request body as `body: Bytes` (already buffered into memory) BEFORE `handle_logs`/`handle_traces`/`handle_metrics` runs. | `transport.rs:473-477,579-583,676-680` |
| gRPC | tonic **decodes** the protobuf frame into a typed `ExportLogsServiceRequest` BEFORE the handler runs; the handler then `req.encode_to_vec()` re-encodes it to bytes for the harness. | `transport.rs:865-869,949-953,1026-1030` |

DISCUSS (`docs/feature/aperture-body-size-cap-v0/discuss/`) verified all loci on this branch and flagged five DESIGN-owned decisions (D1-D5). The crux is **D2**: where the size check sits, and the **honest protection strength** it achieves.

The disclosed-but-unwired state:
- `max_recv_msg_size: Option<u32>` is parsed per transport arm (`config/mod.rs:481-485`, `#[allow(dead_code)]`, "unused at v0") but never reaches `Config` (no field at `:46-58`, no accessor at `:193-194`).
- `BODY_TOO_LARGE` event constant exists (`observability.rs:46`) with **no emitter**.

The concurrency cap (ADR-0010) is the established refusal-shape precedent: a `warn`-level `concurrency_cap_hit` event naming the cap + observed value, and a deterministic refusal (HTTP 503 + `Retry-After: 1`, gRPC `RESOURCE_EXHAUSTED`).

## The load-bearing honesty problem (D2)

The simplest seam is an `&[u8]` length check at the top of `app::ingest_logs`/`ingest_traces` (`app.rs:65-82,94-111`). It is **the weaker guard**: it runs AFTER axum has buffered the full `Bytes` (HTTP) and AFTER tonic has decoded the frame (gRPC), so the oversized allocation has **already happened**. A guard that fires only after the 200 MB body is fully in memory does not prevent the OOM the operator set the cap to prevent. The whole point of this feature is an OOM/DoS guard, so the app.rs-only check would be largely theatre and would force the AC to claim protection the placement does not deliver — exactly the disclosed-omission pattern this feature exists to close.

The **stronger, honest guard** rejects before the body is buffered/decoded, at the framework boundary:
- **HTTP**: `axum::extract::DefaultBodyLimit::max(n)` applied as a route/router layer. axum/hyper enforces the limit against the `Content-Length` header when present and otherwise during streaming, so an over-limit body is rejected before the full `Bytes` is assembled. The allocation is never spent.
- **gRPC**: `max_decoding_message_size(n)` on each generated service server (`LogsServiceServer::new(svc).max_decoding_message_size(n)`, likewise Trace/Metrics). tonic rejects the frame inside the codec, before the typed request is decoded and before the handler's `into_inner()` runs. The decode allocation is never spent.

**The trade this creates (the honesty crux):** when the framework rejects at the boundary, **aperture's handler code never runs**, so the handler cannot emit `body_too_large` with the exact `limit`/`size`, and the framework's default rejection (axum 413 with a generic body; tonic `RESOURCE_EXHAUSTED` from the codec) does not match aperture's event vocabulary or refusal-body shape. We cannot have BOTH the strongest protection AND a handler-emitted event on the *same* code path, because the strong protection is precisely "your code does not run on an over-limit body".

This ADR resolves that trade explicitly rather than letting the AC overstate.

## Decision

**Primary protection at the transport boundary; the event survives via a thin custom rejection seam at each boundary; an app.rs `&[u8]` early-return is a defence-in-depth secondary ONLY where it adds value.**

### D2 — Enforcement site (chosen): transport-boundary guard, event via custom rejection

1. **HTTP (primary + event together).** Apply the size limit at the axum boundary AND emit the event there, using a custom length-limited body extraction rather than the bare `DefaultBodyLimit` layer:
   - The handlers stop extracting `body: Bytes` directly. Instead each handler takes the request and reads the body through a length-checked path that consults the configured cap (threaded onto `HttpState`). The cap is enforced against `Content-Length` first (reject before reading a single byte when the declared length exceeds the cap) and against the actual streamed length as a backstop (so a lying/absent `Content-Length` cannot bypass it).
   - On over-limit, the handler returns the canonical refusal (D5: **413 Payload Too Large**, body naming the limit + size, mirroring the `refusal_message` shape) AND emits exactly one `event=body_too_large transport=http_protobuf signal=<signal> limit=<bytes> size=<bytes>` at `warn` level (D3 size shape below). The harness `validate_*` is never called; the sink is never touched.
   - This keeps the protection strong (the over-limit body is rejected before the full buffer is assembled — for the dominant `Content-Length`-present case, before ANY body byte is read) AND keeps the event truthful. `DefaultBodyLimit` is NOT used as the sole guard precisely because its silent 413 cannot carry the event; the custom length-checked read is the seam that gives both.
2. **gRPC (primary at the codec; event at the codec-error surface).** Set `max_decoding_message_size(cap)` on each generated service server in `spawn_grpc`. This is the real OOM guard: tonic refuses the frame in the codec, before decode, before the handler. Because the handler does not run on an over-limit frame, the `body_too_large` event for gRPC is emitted from the **codec-error surface**, not the handler: a thin layer (or the server's decode-error mapping) recognises the `max_decoding_message_size` rejection and emits one `event=body_too_large transport=grpc signal=<signal> limit=<bytes> size=<declared-or-frame-bytes>` at `warn`, then surfaces the D5 status (`RESOURCE_EXHAUSTED`). If the precise on-the-wire frame size is not recoverable at that surface, the event reports the **configured limit** and the **size tonic observed** (the decoder knows the length it refused at); the field is named honestly (D3).
3. **app.rs `&[u8]` early-return (defence-in-depth, secondary).** A `size > limit` early-return is ALSO added at the top of `ingest_logs`/`ingest_traces` (before `validate_*`, honouring the single-validator-per-signal invariant, `app.rs:13-19`). It is **not** the primary OOM guard (the body is already in memory by the time it runs) and is documented as such. Its value is: (a) a belt-and-braces guard if a future transport change ever lets a body past the boundary guard; (b) a single, transport-agnostic place the boundary emitters can delegate the event construction to, so the `body_too_large` field shape is built once. It MUST NOT be sold as preventing the allocation.

### Honest protection-strength statement (the locked claim the AC must use)

| Arm | Primary guard | What it prevents | Residual weakness (disclosed) |
|---|---|---|---|
| HTTP (`Content-Length` present) | axum-boundary length check vs `Content-Length` | the body is rejected **before any body byte is read into memory**. Full OOM protection. | none for the declared-length case. |
| HTTP (`Content-Length` absent/lying) | streamed-length backstop | the read is aborted once the cap is exceeded; at most ~`limit` bytes (plus framing) are buffered before rejection, NOT the full oversized body. | up to ~one cap's worth of bytes may be read before the abort — bounded, not unbounded; this is the honest strength, and the AC says "rejected before the full body is buffered", NOT "before any byte". |
| gRPC | tonic `max_decoding_message_size` | the frame is refused in the codec **before decode**; the typed request is never allocated. Full OOM protection against the decode allocation. | tonic still reads the length-delimited frame header; the refusal is at the configured size, so at most ~`limit` bytes are accepted before refusal — bounded. |
| Both (app.rs secondary) | `&[u8]` length early-return | guards the harness decode/validate, NOT the allocation. | the body is already in memory here; this is explicitly the weaker, secondary guard. |

**The AC MUST be worded to "rejected before the harness decodes/validates it AND before the full oversized body is buffered/decoded into memory" — which the transport-boundary guard delivers — and MUST NOT claim "before any byte is read" except for the `Content-Length`-present HTTP case.** This is the Earned-Trust requirement: the claim equals the placement.

### D1 — Config surface (chosen): single collapsed cap, honour the gRPC arm, accept both keys

Mirror the concurrency precedent (`config/mod.rs:608-617`): add a single `max_recv_msg_size: Option<u32>` to `Config` with a `pub(crate) fn max_recv_msg_size(&self) -> Option<u32>` accessor and a `Config::builder().max_recv_msg_size(u32)` setter (parallel to `max_concurrent_requests`, `config/mod.rs:315-317`). `into_config` reads the value from the **gRPC arm** when set and accepts (silently, at v0) a distinct HTTP value, exactly as concurrency does. Single cap shared by both transports keeps the two backpressure knobs behaving identically and the schema-test surface small. A genuine per-arm cap is defensible (the arms buffer differently) but adds surface for no v0 operator need; if the post-deserialise validator (Slice 08 line) later warns on a divergent pair, the size cap rides that same warning for free. `Option` is load-bearing: **`None` (unset) = no cap = today's exact behaviour** (C2). A `0` value, if reachable, is treated as "no cap" not "zero-byte cap" (US-03 scenario 3) — decided at the accessor/parse boundary, never as a reject-everything limit.

### D3 — Reported size/limit shape (chosen)

The event carries `limit=<configured bytes>` always. For `size`:
- **HTTP**: report the actual rejected size where known. For the `Content-Length`-present case, `size` = the declared `Content-Length` (named so it is honest that it is the declared length the gateway refused on, not a fully-read length). For the streamed-backstop case, `size` = the byte count at which the read was aborted (`>= limit`). The field is `size` in both cases; the body text / field doc states it is the size aperture observed at the point of rejection.
- **gRPC**: `size` = the length tonic's decoder refused at (the frame length it observed exceeded the cap). If tonic only exposes "exceeded N", report that observed value; never fabricate a precise byte count the surface does not have.

The honesty rule: **report what the rejection surface truthfully knows; name the field for what it is; never invent a precise `size` the placement cannot observe.** This reconciles DISCUSS's "carry the limit and the actual size" with the strong-guard reality (D3 flag).

### D4 — Metrics coverage (chosen): include all three signals in this slice

The cap applies to **logs, traces, AND metrics** in this slice. The enforcement is transport-level (the axum layer/extractor and the tonic `max_decoding_message_size` cover all routes/services at once) and the app.rs secondary is a per-fn early-return on all three `ingest_*`. The marginal cost of the third signal (metrics) is near zero, and excluding it would re-create the exact disclosed-omission pattern this feature closes (a guard that protects logs+traces but leaves the identical `ingest_metrics` OOM exposure open). DISCUSS scoped logs+traces and named metrics for an explicit DESIGN decision; the decision is **include metrics**, not defer. No silent gap.

### D5 — Reject status codes (chosen)

- **HTTP: `413 Payload Too Large`** (not 400). 413 is the precise semantic for an entity exceeding the server's size limit; OTLP SDKs that distinguish 4xx classes get the correct signal. The body names the limit + size (mirroring the `refusal_message` shape).
- **gRPC: `RESOURCE_EXHAUSTED`** (status 8, not `INVALID_ARGUMENT`/`OUT_OF_RANGE`). This matches the concurrency cap's resource-protection framing (ADR-0010) and is tonic's native status for `max_decoding_message_size` rejection, so the boundary guard and our event agree on the status. The `grpc-message` names the limit + size.

These codes become locked AC for DISTILL.

## Alternatives Considered

### Option A — Transport-boundary guard + custom rejection seam for the event (RECOMMENDED, accepted)

**Pros**: delivers the real OOM protection (reject before buffer/decode); keeps the `body_too_large` event truthful and in the closed vocabulary; uses the canonical axum (`DefaultBodyLimit`/length-checked extraction) and tonic (`max_decoding_message_size`) primitives, both stable in the pinned `axum 0.7` / `tonic 0.12`; the protection-strength claim equals the placement (Earned-Trust honest).

**Cons**: the event is emitted from a different surface than the handler's other events (a custom HTTP rejection path + a gRPC codec-error surface), which is more wiring than a single app.rs early-return. Accepted: the extra wiring is the price of having BOTH strong protection AND the event, and it is bounded (one HTTP extraction seam + one gRPC layer/error-map + the shared event constructor).

### Option B — app.rs `&[u8]` early-return ONLY (the simplest seam)

**Pros**: smallest change; one early-return per `ingest_*`; trivially emits the event with the exact `body.len()`.

**Cons**: **runs after the body is already in memory** (axum buffered `Bytes`; tonic decoded the frame), so it does NOT prevent the oversized allocation — it guards the harness decode/validate only. The operator sets the cap to prevent OOM; this placement does not deliver that. Adopting it as the primary guard would force the AC to overstate protection (the disclosed-omission pattern this feature closes). **Rejected as the primary guard**; retained ONLY as the disclosed defence-in-depth secondary (D2 item 3).

### Option C — Bare `DefaultBodyLimit` (HTTP) + bare `max_decoding_message_size` (gRPC), no custom event

**Pros**: the least code; the frameworks do all the work; strongest protection out of the box.

**Cons**: the frameworks reject **before our code runs**, so `body_too_large` **never fires** — the constant stays emitter-less, and the operator gets a generic 413 / `RESOURCE_EXHAUSTED` with no structured stderr line naming the offending size. That leaves half the feature undone (KPI-2: one event per rejection, naming limit+size) and re-creates the emitter-less-constant disclosed omission on the event side. **Rejected**: protection without the event is half the operator job.

### Option D — Genuine per-arm cap on `Config` (D1 alternative)

**Pros**: honest to the two arms buffering differently; each arm could be tuned independently.

**Cons**: diverges from the concurrency precedent for no v0 operator need; doubles the config-test surface; the two arms' OOM exposure is symmetric enough that a single shared cap is the right v0 simplification. **Rejected** for v0; revisitable if operators report needing divergent per-arm caps (Phase-2 gate).

### Option E — Reject with HTTP 400 / gRPC `INVALID_ARGUMENT` (D5 alternative)

**Cons**: 400/`INVALID_ARGUMENT` say "your request is malformed", which is false — the request is well-formed, just too large. 413/`RESOURCE_EXHAUSTED` say "too big for my resource limit", which is true and matches the concurrency-cap resource-protection framing. **Rejected** for semantic incorrectness.

## Consequences

### Positive
- The disclosed-but-unwired `max_recv_msg_size` knob now delivers the OOM protection an operator assumes: an over-limit body is rejected before it is buffered/decoded into memory (the strong guard), on all three signals and both transports.
- The emitter-less `body_too_large` constant now fires exactly once per rejection, naming the limit and the observed size, in the same `warn`-level JSON shape as `concurrency_cap_hit` — the operator's existing scrape catches it.
- Unset = byte-for-byte today's behaviour (C2); the mixed fleet (most gateways unset) is untouched.
- The protection-strength claim equals the placement (Earned-Trust): the AC says "before the full body is buffered/decoded", which is exactly what the boundary guard delivers, with the `Content-Length`-present case being the stronger "before any byte".

### Negative
- The event is emitted from two surfaces (HTTP custom-rejection path, gRPC codec-error surface) rather than one app.rs line — more wiring and two mutation loci, not one. Mitigated by a single shared event-constructor the secondary app.rs guard also calls.
- The gRPC `size` field may report the size tonic observed at refusal rather than an exact full-body byte count, because the strong guard refuses before full decode. This is disclosed in the field doc and the AC; it is the honest consequence of refusing early. An operator who needs the exact byte count would have to let the body decode (the weaker guard) — a worse trade.
- A streamed HTTP request with absent/lying `Content-Length` is bounded to ~one cap's worth of bytes before the abort, not zero — the honest residual (disclosed in the protection-strength table).

### Trade-off ATAM
- **Sensitivity point** for **Performance Efficiency — Resource utilisation** (the cap is the per-request memory ceiling on the ingest path; with ADR-0010's `cap × body × transport` product, this `max_recv_msg_size` is the `body` term operators multiply for pod sizing) and for **Reliability — Fault tolerance** (deterministic reject-before-OOM is the load-bearing property).
- **Trade-off point**: protection strength vs event fidelity. The strongest guard (refuse before our code runs) costs exact-size fidelity in the event; we recover the event via a custom rejection seam and accept the disclosed `size`-fidelity limit on gRPC. The alternative (weaker app.rs guard) buys exact `body.len()` at the cost of the OOM protection the feature exists to deliver — the wrong trade.

### Public-API / semver
The `Config` field + accessor are `pub(crate)` (mirroring `max_concurrent_requests`); the builder setter is `pub` on the builder but only widens an existing builder. `HttpState` and the gRPC service impls are crate-private. **Confirmed INTERNAL ripple**; Gate 2/3 do not fire (aperture is not in the public-API set). Any leak would be semver-MINOR, pre-1.0, **NEVER 1.0.0** (Andrea's call).

### Phase-1 revisit gates
- If operators report needing divergent per-arm caps, revisit D1 (Option D).
- If the gRPC `size`-fidelity limit (D3) proves operationally insufficient, revisit whether a bounded pre-decode length read can recover a more precise frame size without re-introducing the OOM.
- If a future transport change (HTTP/2 server push, gRPC-web) changes where the body lands, re-verify the boundary guard still fires before buffering and that the app.rs secondary still backstops.
