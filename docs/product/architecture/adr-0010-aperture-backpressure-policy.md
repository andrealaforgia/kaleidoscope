# ADR-0010 — Aperture backpressure policy: per-transport semaphore, deterministic refusal, no queue

- **Status**: Accepted
- **Date**: 2026-05-04
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

DISCUSS Q4 locks the v0 backpressure policy:
- Configurable `max_concurrent_requests` per transport.
- Once reached: gRPC `RESOURCE_EXHAUSTED` (status 8) / HTTP 503 with `Retry-After: 1`.
- **No internal queue** (Sluice's job, Phase 7).
- **No block** (violates OTel SDK contract).
- **No silent drop** (anti-pattern).

DISCUSS D5 (refusal-not-drop) is `@property`-tagged in `journey-aperture.feature`: every cap-exceeded request carries a deterministic refusal status; every refusal is a structured stderr line; zero silent drops.

DISCUSS D7 locks the mechanism at the contract level: per-transport semaphore. Default capacity 1024 per transport. Permit acquired on connection accept (gRPC) or request begin (HTTP); released on response sent. The sink hand-off-and-await counts as in-flight.

DISCUSS D7 also documents the memory-bound NFR: worst-case in-flight memory is `max_concurrent_requests × max_recv_msg_size × number_of_transports`. With v0 defaults: 1024 × 4 MiB × 2 = 8 GiB. Operators sizing pods MUST compute this product before setting `resources.limits.memory`.

DISCUSS US-AP-07 specifies the UAT shape:
- gRPC refusal: `grpc-status: 8`, message names cap.
- HTTP refusal: HTTP 503, `Retry-After: 1` header, body names cap.
- Caps independent per transport.
- One `event=concurrency_cap_hit` warn stderr line per refusal, with `transport` and `cap`.

What DESIGN must lock:
1. The exact semaphore implementation (`tokio::sync::Semaphore` vs alternatives).
2. The exact permit-acquire shape (`try_acquire` non-blocking vs `acquire_owned` etc.).
3. The exact integration with tonic's interceptor / axum's middleware.
4. The exact response-construction call sites.
5. The interaction with US-AP-09's drain (in-flight permits at SIGTERM).

## Decision

- **Semaphore implementation**: `tokio::sync::Semaphore`, one instance per transport, wrapped in `Arc<Semaphore>`.
- **Permit acquisition shape**: `Semaphore::try_acquire_owned()` — non-blocking, returns immediately with `Err(TryAcquireError::NoPermits)` on saturation. **No `acquire().await`**; that would block, violating DISCUSS Q4.
- **gRPC integration**: a `tower::Layer` middleware in front of the tonic Server. The middleware wraps each request in a `try_acquire_owned()` call; on `Err`, it returns a `tonic::Status::resource_exhausted("aperture: gRPC concurrency cap of {cap} reached on transport=grpc")` and emits `event=concurrency_cap_hit transport=grpc cap=N` stderr line.
- **HTTP integration**: a `tower::Layer` middleware in the axum stack (registered via `Router::layer(...)`). Same `try_acquire_owned()` shape; on `Err`, returns `(StatusCode::SERVICE_UNAVAILABLE, [("Retry-After", "1")], "aperture: HTTP concurrency cap of {cap} reached on transport=http_protobuf")` and emits the warn line.
- **Permit lifetime**: from middleware-acquired (request begins) until response is sent. The acquired `OwnedSemaphorePermit` is dropped at end-of-request automatically; the sink's hand-off-and-await therefore counts as in-flight.
- **Drain interaction**: `shutdown::orchestrate` reads `Semaphore::available_permits()` per transport to compute "in-flight count" = `cap - available_permits`. Drain awaits both semaphores reaching `available_permits == cap` (no permits issued); on deadline elapse, the residual is the dropped count.

## Alternatives Considered

### Option A — `tokio::sync::Semaphore` per transport, tower middleware, try_acquire_owned (RECOMMENDED, accepted)

**Pros**:
- `tokio::sync::Semaphore` is the canonical Rust async semaphore. Mature, well-understood, fast.
- `try_acquire_owned()` matches the locked contract (no wait, immediate refusal on saturation).
- `tower::Layer` is the canonical middleware shape for both tonic and axum; one Layer implementation works for both, parameterised by the per-transport semaphore Arc.
- `OwnedSemaphorePermit` carries a lifetime-bound to the request future; auto-drop at request end means correct release without manual book-keeping.
- `Semaphore::available_permits()` is the standard way to compute "in-flight" for the drain logic; matches DISCUSS D7's contract.

**Cons**:
- Tower's middleware composition adds a small per-request cost (one `Box::new` for the inner future). Acceptable: the cost is sub-microsecond and the alternative is hand-rolling middleware.

### Option B — `tower::limit::ConcurrencyLimitLayer`

**Pros**:
- Built-in tower middleware; would-be-zero-code.

**Cons**:
- `ConcurrencyLimitLayer`'s default behaviour is to **wait** for a permit (using tower's `poll_ready` ratcheting). To make it refuse-not-wait requires layering it inside a `tower::limit::rate::RateLimitLayer` or wrapping with a custom combinator that maps `Pending` to a synthesised refusal; that defeats the layer's value.
- The contract is "no wait, no queue, no block". A library intended for queue-style backpressure is the wrong shape.

**Rejected** because the contract requires immediate refusal; this layer is built for the opposite.

### Option C — A dedicated `arc-swap::ArcSwap<u64>` counter with manual increment/decrement

**Pros**:
- Dependency-free of `tokio::sync::Semaphore`.

**Cons**:
- Re-implements a semaphore; introduces races; loses the `try_acquire` atomicity primitive.
- Resume-driven development pattern.

**Rejected** outright.

### Option D — `dashmap` or `papaya` map of `peer -> in-flight count`, refusal when peer exceeds cap

**Pros**:
- Per-peer refusal would defend against single-tenant DoS.

**Cons**:
- DISCUSS Q4 says "per transport", not "per peer". Per-peer is multi-tenancy, which is Aegis's domain (Phase 2).
- Would require tracking peer identity, which is a richer requirement than v0 needs.

**Rejected** for being scope creep.

### Option E — Single global semaphore (not per-transport)

**Pros**:
- Smaller code; one Arc to clone.

**Cons**:
- DISCUSS Q4 explicitly says "per transport". A single global cap means a saturated gRPC fleet could starve the HTTP listener.
- DISCUSS US-AP-07 Scenario "Caps are independent per transport" is the test that defends this.

**Rejected** by the locked DISCUSS contract.

## Consequences

### Positive
- Deterministic refusal under overload: no queue, no block, no silent drop. The `@property`-tagged UAT in `journey-aperture.feature` defends this invariant.
- Caps are independent per transport: a hot HTTP path does not starve gRPC.
- The drain logic (US-AP-09) reads `Semaphore::available_permits()` to compute in-flight count; this is the same primitive, used twice. No second source of truth.
- The middleware shape (`tower::Layer`) is the canonical Rust pattern; future maintainers reading the code see something familiar.
- The default cap (1024 per transport) is operator-tunable; the memory-bound NFR is documented for operator pod sizing.

### Negative
- The default-1024 cap × default-4-MiB-body × 2-transports = 8 GiB worst-case in-flight memory footprint. Operators MUST compute this product before setting `resources.limits.memory`. Documented in the config schema header (component-design.md) and in the operational runbook DEVOPS will write.
- A single misbehaving SDK client opening many concurrent connections can saturate one transport's cap. Mitigation: SDKs respect `RESOURCE_EXHAUSTED` and back off; operator scales replicas; per-peer caps come at Aegis (Phase 2). Acceptable at v0.

### Trade-off ATAM

**Sensitivity point** for **Performance Efficiency — Resource utilisation** (the cap × body × transport product is the operator-facing memory ceiling) and for **Reliability — Fault tolerance** (deterministic refusal-not-drop is the load-bearing property).

**Trade-off point**: Reliability vs Operational Simplicity. The cap is a hard wall; operators with bursty workloads will see `concurrency_cap_hit` events under burst. The alternative would be an internal queue (Sluice's job, Phase 7) which trades latency under burst for lower refusal rate. DISCUSS Q4 deliberately picks deterministic refusal at v0; queueing arrives at Sluice when the operational scenario demands it.

### Phase-1 revisit gates

- If a load test (KPI 5) shows the gRPC and HTTP transports contending on the shared Tokio runtime under burst, revisit splitting them onto separate runtimes (one process, two `Runtime` instances). DESIGN-time hypothesis: not needed at v0; the runtime work-steals; profiling will show.
- If operators consistently report needing per-peer caps before Aegis, revisit. Hypothesis: they will not; Aegis Phase 2 is close enough.
- If `tower::Layer`'s implementation ergonomics surface a footgun (e.g. `poll_ready` not playing well with `try_acquire`), revisit. Hypothesis: the canonical pattern works; many production Rust services do this.
- If the OTel SDK's default backoff on `RESOURCE_EXHAUSTED` surfaces a reset-and-retry storm, document the operator runbook entry. Hypothesis: SDK retries are well-spaced; not a problem at v0 traffic levels.
