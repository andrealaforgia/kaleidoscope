# Slice 05 — Backpressure (concurrency cap, deterministic refusal) — completion summary

> **Wave**: DELIVER (`nw-software-crafter` / Crafty).
> **Date**: 2026-05-04.
> **Slice**: 05 — per-transport concurrency cap with deterministic refusal.
> **Companion brief**: [`../slices/slice-05-backpressure.md`](../slices/slice-05-backpressure.md).
> **Companion story**: US-AP-07.
> **Companion ADR**: [`../../product/architecture/adr-0010-aperture-backpressure-policy.md`](../../product/architecture/adr-0010-aperture-backpressure-policy.md).

---

## Headline

Each transport (gRPC and HTTP/protobuf) now carries an independent
`tokio::sync::Semaphore` sized from `Config::max_concurrent_requests`
(default 1024 per transport, locked by ADR-0010). Permits are
acquired with `try_acquire_owned()` BEFORE the harness sees the body
and dropped on response sent — so the sink hand-off-and-await counts
as in-flight, exactly per the contract. Saturation produces immediate
refusal: gRPC `RESOURCE_EXHAUSTED` with `grpc-message` naming the
cap; HTTP 503 with `Retry-After: 1` and a body naming the cap. Every
refusal emits the closed-vocabulary `event=concurrency_cap_hit` warn
line carrying `transport`, `cap`, and `in_flight_at_refusal` fields.

The locked DISCUSS Q4 contract — cap, refuse, never block, never
queue, never drop silently — holds at v0.

## What turned GREEN

| Test binary | Tests passing |
|---|---:|
| `tests/slice_05_backpressure.rs` | **10/10** |
| `tests/slice_04_metrics.rs` | **9/9** (no regressions) |
| `tests/slice_03_traces.rs` | **10/10** (no regressions) |
| `tests/slice_02_http_protobuf_and_readiness.rs` | **15/15** (no regressions) |
| `tests/slice_01_walking_skeleton.rs` | **13/13** (no regressions) |
| `tests/invariant_single_validator.rs` | **1/1** |
| `tests/invariant_no_telemetry_on_telemetry.rs` | **3/3** |
| `src/lib.rs` (lib unit tests) | **48/48** (38 pre-existing + 8 backpressure module + 2 config) |
| **Slice 05 active total** | **107/107** |

The 10 acceptance tests in `slice_05_backpressure.rs` cover (per
Mandate Single-Then-Per-Fact):

- **gRPC refusal shape (4)**: 5th concurrent request at cap=4
  receives `Code::ResourceExhausted`; the `grpc-message` names the
  cap (`cap of 4`); the refusal emits an `event=concurrency_cap_hit`
  warn-level stderr line; the same event carries `transport=grpc`.
- **HTTP refusal shape (4)**: 5th concurrent POST at cap=4 receives
  HTTP 503; the response carries the `Retry-After: 1` header; the
  body names the cap; the refusal event carries
  `transport=http_protobuf`.
- **Cross-transport independence (1)**:
  `saturated_grpc_does_not_block_http_requests` — a saturated gRPC
  listener does not affect HTTP requests, defending ADR-0010's
  "independent semaphore per transport" decision.
- **Refusal-not-drop property (1)**:
  `every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance`
  — fires 10 simultaneous requests at cap=2, asserts every response
  is either 200 (sink-accepted) or 503 (cap-refused). No silent
  drop, no connection close, no timeout. This is the
  `@property`-tagged UAT from `journey-aperture.feature` (DISCUSS
  D5).

The 8 new backpressure-module unit tests cover:

- `cap_transport_grpc_renders_as_grpc` and
  `cap_transport_http_protobuf_renders_as_http_protobuf` pin the
  closed-vocabulary `transport` field strings against an
  `Ok(Default::default())` mutation.
- `refusal_message_for_grpc_names_the_cap_and_transport` and the
  HTTP equivalent pin the operator-facing diagnostic shape (`cap of
  N` substring + `transport=...` substring) so the gRPC
  `grpc-message` and HTTP body cannot silently degrade.
- `limiter_with_cap_two_permits_first_two_acquires`,
  `limiter_refuses_third_acquire_when_cap_is_two`,
  `limiter_recovers_capacity_when_a_permit_is_dropped`, and
  `limiter_cap_reports_the_configured_value` pin the limiter's
  contract: cap-many simultaneous permits, refusal at cap+1,
  capacity restored on permit drop, cap reads back the configured
  value. The drop-restoration test is what defends "permit released
  on response sent" semantics in unit isolation.

The 2 new `config` unit tests pin the `max_concurrent_requests`
setter and the locked default of 1024 (see Mutation testing section).

## Production code added or modified

| File | Net change | What it does |
|---|---:|---|
| `src/backpressure.rs` | **+191 (new)** | New `backpressure` module. `CapTransport` enum (closed two-variant set, `as_str` rendering for the structured-log `transport` field), `ConcurrencyLimiter` (`Arc<Semaphore>` + cap + transport, `try_acquire` returning an `OwnedSemaphorePermit` or `Err(())` after emitting the warn line), `refusal_message` (operator-facing diagnostic), `emit_cap_hit_event` (private helper for the closed-vocabulary stderr line). 8 focused unit tests. |
| `src/transport.rs` | **+102 / 0** | Each gRPC `*ServiceImpl::export` (3 handlers) acquires a permit at the very top of the function via `self.limiter.try_acquire()`; on `Err`, returns `Status::resource_exhausted(refusal_message(...))` immediately. Each HTTP `handle_*` (3 handlers) does the same; on `Err`, calls a new `refuse_http(cap)` helper that builds a `(503, [Retry-After: 1], body)` response. The `HttpState` struct gains a `limiter: ConcurrencyLimiter` field. |
| `src/compose.rs` | **+12 / 0** | The composition root constructs two independent `ConcurrencyLimiter` instances (one per transport, both sized from `config.max_concurrent_requests()`) and passes each into the corresponding spawn function. |
| `src/config/mod.rs` | **+41 / 1** | `max_concurrent_requests` field's `#[allow(dead_code)]` removed (now load-bearing). New `Config::max_concurrent_requests()` accessor (`pub(crate)`). 2 unit tests pin the setter against `Ok(Default::default())` mutation and the locked default of 1024. |
| `src/lib.rs` | **+1 / 0** | New `mod backpressure;` declaration. |
| **Slice 05 net delta** | **+346 / 1** | (`git diff dc0bb4d..HEAD -- crates/aperture/src/`) |

Modules NOT touched (still placeholders for future slices):
`src/error.rs` (Slice 07/08 lands the rich `ApertureError` enum),
`src/shutdown.rs` (Slice 08 lands the drain orchestrator that will
read `Semaphore::available_permits()` to compute the in-flight
count), `src/sinks.rs`, `src/ports/mod.rs`, `src/main.rs`,
`src/readiness.rs`. No `// SCAFFOLD: true` markers were carried by
any module Slice 05 touched.

## Commits

| Hash | Subject |
|---|---|
| `7e6c8dd` | `feat(aperture): Slice 05 — per-transport concurrency cap with deterministic refusal` |
| `ba6f624` | `test(aperture): pin max_concurrent_requests setter and default` |
| (this doc) | `docs(aperture): DELIVER slice-05-completion summary` |

The first commit lands the GREEN feature outcome (10 RED acceptance
tests → GREEN, plus 8 new backpressure unit tests). The second
commit pins the `max_concurrent_requests` setter against an
`Ok(Default::default())` mutation deterministically, replacing a 28 s
timeout-based kill with a 0.01 s assertion-based kill.

## Integration shape chosen

ADR-0010 expresses a preference for `tower::Layer` middleware. Slice
05 chose **per-handler `try_acquire_owned`** instead. Reasons:

1. **Smallest churn against the existing transport.rs.** The gRPC
   service impls and HTTP handlers are already shaped for explicit
   per-call work (re-encoding for the harness, content-type checks).
   Adding three lines at the top of each is symmetric across all six
   handlers and makes the "permit acquired before validate, dropped
   on response sent" contract physically visible in the source.
2. **No new transitive dependency surface.** A `tower::Layer` shared
   between tonic 0.12 (which uses `tower 0.4`) and axum 0.7 (which
   uses `tower 0.5`) requires either a careful generic shape over
   `tower-layer 0.3` and `tower-service 0.3` only, or two distinct
   layer impls. Both options are viable but neither is shorter than
   the per-handler form.
3. **Identical observable semantics.** The contract specifies *when*
   the permit is acquired and *when* it is released, not *which Rust
   abstraction* mediates them. The semaphore IS the gating point;
   the per-handler form makes the gating explicit at every call
   site, which is what ADR-0010's "permit acquired before validate"
   clause requires.

ADR-0010 explicitly permits this choice ("interceptor / middleware /
extractor: whichever shape ADR-0010 prefers and the existing
transport.rs accommodates with the least churn"). The slice
contract repeats the permission. The chosen shape is the smallest
deliberate departure from ADR-0010's recommended pattern, with the
trade-off documented here.

If a future slice (Slice 08's drain orchestrator) finds it more
natural to compute "in-flight count" from a tower-Layer-managed
semaphore than from the per-handler form, the migration is local
(replace each handler's `_permit = ...` with the layer's wrapping)
and does not change the contract.

## Mutation testing

Per ADR-0005 Gate 5, the target is 100% kill rate on Slice 05
touched files. Run command (scoped to Slice 05 territory, restricted
to the green-by-design test set so the baseline passes):

```text
cargo mutants --package aperture --no-shuffle --jobs 2 \
  --file crates/aperture/src/backpressure.rs \
  --file crates/aperture/src/transport.rs \
  --file crates/aperture/src/compose.rs \
  --cargo-test-arg "--lib" \
  --cargo-test-arg "--test=slice_01_walking_skeleton" \
  --cargo-test-arg "--test=slice_02_http_protobuf_and_readiness" \
  --cargo-test-arg "--test=slice_03_traces" \
  --cargo-test-arg "--test=slice_04_metrics" \
  --cargo-test-arg "--test=slice_05_backpressure" \
  --cargo-test-arg "--test=invariant_single_validator" \
  --cargo-test-arg "--test=invariant_no_telemetry_on_telemetry"
```

Result on Slice 05 territory (`backpressure.rs`, `transport.rs`,
`compose.rs`) after `ba6f624`:

| Metric | Count |
|---|---:|
| Mutants generated | 39 |
| Caught | **19** |
| Missed | **0** |
| Unviable (mutation produces non-compiling code) | 20 |
| **Kill rate** | **100% (19 / 19 viable)** |

A separate run that includes `config/mod.rs` finds 60 mutants total
with 7 missed; all 7 are pre-existing scaffold setters in Slice
06/07/08 territory (`forwarding_sink`, `forwarding_timeout`,
`tls_enabled`, `spiffe_enabled`, `drain_deadline`) plus
`Config::builder` and the `ConfigError::Display` impl. These were
already MISSED at the Slice 04 boundary by design — their
behaviour-asserting tests land with their owning slices. Slice 05
closed the one config-level mutation it owned (`max_concurrent_requests`
setter, previously a 28 s timeout, now a deterministic kill).

The baseline build was 13 s with 5 s test; the auto-set test
timeout was 28 s.

## Architectural observations

- The `single_validator_per_signal` invariant continues to hold:
  `validate_logs`, `validate_traces`, `validate_metrics` each have
  exactly one call site, and the cap acquisition is OUTSIDE that
  call site (acquired in the handler, released after the
  `ingest_*` future resolves). The semaphore is a wrapper around
  the validator, not a second call site.
- The closed v0 event vocabulary continues to hold: the
  `concurrency_cap_hit` constant was already declared in
  `observability::event` (DESIGN ADR-0009 reserved it); Slice 05
  added the call site that fires it. No new event names were
  introduced.
- The `available_permits()` primitive Slice 05 wires through the
  semaphores is the same primitive Slice 08's drain orchestrator
  will read to compute "in-flight count = cap - available_permits".
  No second source of truth is created.
- The `ConcurrencyLimiter` newtype around `Arc<Semaphore>` keeps the
  raw tokio primitive out of the call sites — the transport handlers
  see a `try_acquire()` returning `Result<OwnedSemaphorePermit, ()>`
  rather than a tokio-specific `TryAcquireError` enum. This shrinks
  the surface a future maintainer reading `transport.rs` has to
  reason about, and keeps the operator-facing diagnostic (`refusal_message`)
  alongside the limiter that emits it.
- The choice to NOT include the `in_flight_at_refusal` field as a
  best-effort `available_permits()` snapshot was deliberate: at the
  moment of refusal, every permit IS outstanding (that is the
  definition of refusal), so the value equals the cap. Reporting
  `cap` and `in_flight_at_refusal=cap` is equivalent to reporting
  `cap` alone, but the field name is operationally meaningful (a
  future per-tenant cap, Aegis Phase 2, would surface the same
  field with a different value). The slice contract's "drop the
  field if it's not cheap" clause therefore does not apply.

## Genuine forks discovered

**One.** ADR-0010 prefers `tower::Layer` middleware for the
integration shape; Slice 05 chose per-handler `try_acquire_owned`
for the reasons documented above. ADR-0010 permits the choice in its
Alternatives Considered section; the slice contract repeats the
permission. No back-propagation needed — the trade-off is local to
this slice's implementation, observable in the same `transport.rs`
file, and reversible if a future slice discovers a tower-layer-shaped
need (e.g. Slice 08's drain orchestrator's `available_permits()`
call site).

The "known unknown" the slice brief flags — default value of
`max_concurrent_requests` per transport — was locked at 1024 by
DISCUSS / ADR-0010 in advance; DELIVER honoured the lock and pinned
it with `default_max_concurrent_requests_is_one_thousand_twenty_four`.

## Quality gates at commit time

- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean
- `cargo test --workspace --exclude aperture --all-targets --locked`:
  73 harness tests pass (matches DEVOPS A2 graduation contract)
- `cargo test --package aperture` (slices 01, 02, 03, 04, 05 + lib
  unit + invariant tests): all green (107 tests)
- Pre-commit hook: not modified; `--exclude aperture` graduation
  stays per Apex's wave-decisions A2

## Out of scope (left RED for subsequent slices)

- `tests/slice_06_forwarding_sink.rs` — Slice 06 (real downstream)
- `tests/slice_07_tls_schema_knob.rs` — Slice 07 (figment loader +
  TLS schema)
- `tests/slice_08_graceful_shutdown.rs` — Slice 08 (drain
  orchestrator + `Draining` state; will reuse the per-transport
  semaphores Slice 05 wired)

These remain RED-by-design until DELIVER advances each slice in
turn.

## Handoff to next slice

Slice 06 (ForwardingSink) is the natural next cycle. The seams Slice
06 will fill, with Slice 05's groundwork visible:

1. `SinkKind::Forwarding` is currently a hard error in
   `compose::wire_sink`; Slice 06 lands the `ForwardingSink`
   construction that exchanges the error for a real reqwest-driven
   downstream.
2. `Config::forwarding_endpoint` and `Config::forwarding_timeout`
   become load-bearing (today their `#[allow(dead_code)]` survives;
   Slice 06's behaviour-asserting tests will close their pending
   mutation surface).
3. The cap-and-refuse path is signal-agnostic: Slice 06's
   `ForwardingSink` arm of `SinkRecord::Logs / Traces / Metrics`
   inherits the per-transport cap with no new wiring — the permit
   is held for the full handler lifetime, which already includes
   the sink hand-off-and-await.
4. The `event=sink_failed` and `event=sink_accepted` events Slice 06
   adds will fire from inside the `ForwardingSink`'s own code; the
   `event=concurrency_cap_hit` line Slice 05 introduced fires
   strictly OUTSIDE the sink hand-off (before the harness call).
   The two events do not interleave and the structured-log
   timestamp ordering reflects the actual flow.

Slice 05's per-transport-semaphore primitive is also the load-bearing
mechanism Slice 08's drain orchestrator will reuse (per ADR-0010 §
Drain interaction). The orchestrator will read
`Semaphore::available_permits()` per transport to compute "in-flight
count = cap - available_permits"; the semaphores Slice 05 wired in
`compose::spawn` are the values it will read. No second source of
truth is created.
