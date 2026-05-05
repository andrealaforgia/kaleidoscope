# Slice 06 — ForwardingSink (downstream OTLP write) — completion summary

> **Wave**: DELIVER (`nw-software-crafter` / Crafty).
> **Date**: 2026-05-04.
> **Slice**: 06 — `ForwardingSink` posting to a configured OTel-compatible
> downstream, with two-stage Earned-Trust probe.
> **Companion brief**: [`../slices/slice-06-forwarding-sink.md`](../slices/slice-06-forwarding-sink.md).
> **Companion story**: US-AP-08.
> **Companion ADRs**:
> [`../../product/architecture/adr-0007-otlpsink-trait-design.md`](../../product/architecture/adr-0007-otlpsink-trait-design.md),
> [`../../product/architecture/adr-0009-aperture-observability-strategy.md`](../../product/architecture/adr-0009-aperture-observability-strategy.md).

---

## Headline

After this slice Aperture is **production-useful**: an operator who
configures `[sink] kind = "forwarding"` against any OTel-compatible
HTTP/protobuf backend (Loki, Tempo, Mimir, an OTel Collector) sees
their accepted records land downstream, with full Earned-Trust
enforcement at startup.

The `ForwardingSink` is a real `reqwest`-driven OTLP/HTTP/protobuf
producer. Each accepted `SinkRecord` is encoded via
`prost::Message::encode_to_vec` (the typed payload identity guarantee
from `otlp-conformance-harness-v0` US-04 AC 2 makes the round-trip
byte-equivalent to what the upstream SDK put on the wire) and POSTed
to `<endpoint>/v1/{logs|traces|metrics}` with
`Content-Type: application/x-protobuf`. The composition root replaces
the test-injected sink with a freshly-constructed `ForwardingSink`
when `config.sink_kind == Forwarding` and runs the Earned-Trust probe
against the real downstream before any listener binds. A failed probe
emits `event=health.startup.refused` and propagates as
`Err(ApertureError)` from `aperture::spawn`.

The probe is the **two-stage** shape ADR-0007 / ADR-0010 promise:

1. **OPTIONS preflight** to `<endpoint>/v1/logs`. RFC 9110 specifies
   `204 No Content` as the canonical preflight-OK response; an
   OTel-compatible downstream that genuinely supports OTLP/HTTP
   returns 204. Aperture treats 204 as the **only** short-circuit. Any
   other 2xx falls through to stage 2 — this is what catches the
   catalogued v0 substrate lie (200 OPTIONS / 503 POST). 404/405 fall
   through too: an OTel-compatible downstream is allowed not to
   implement OPTIONS.
2. **Degraded POST** of a zero-records `ExportLogsServiceRequest`. 2xx
   → success; non-2xx → `ProbeError::Refused { status }`; transport
   error → `Unreachable` or `Timeout`.

The behavioural-layer enforcement of this probe contract — the gold
test runner ADR-0010 layer 3 demands — is `tests/probe_gold_runner.rs`.
A maintainer who replaces `ForwardingSink::probe()` with `Ok(())`
(verified by mutation simulation during cycle 4) is caught by 4 of
5 gold-test scenarios.

## What turned GREEN

| Test binary | Tests passing |
|---|---:|
| `tests/slice_06_forwarding_sink.rs` | **11/11** |
| `tests/probe_gold_runner.rs` (new) | **5/5** |
| `tests/invariant_no_telemetry_on_telemetry.rs` | **5/5** (was 3; +2 substantive) |
| `tests/slice_05_backpressure.rs` | **10/10** (no regressions) |
| `tests/slice_04_metrics.rs` | **9/9** (no regressions) |
| `tests/slice_03_traces.rs` | **10/10** (no regressions) |
| `tests/slice_02_http_protobuf_and_readiness.rs` | **15/15** (no regressions) |
| `tests/slice_01_walking_skeleton.rs` | **13/13** (no regressions) |
| `tests/invariant_single_validator.rs` | **1/1** |
| `src/lib.rs` (lib unit tests) | **60/60** (48 pre-existing + 12 sinks tests) |
| **Slice 06 active total** | **139/139** |

The 11 acceptance tests in `slice_06_forwarding_sink.rs` cover, per
the slice contract:

- **Probe success and short-circuit (2)**:
  `forwarding_sink_probe_succeeds_against_options_responder` (OPTIONS=204
  short-circuits the probe) and
  `forwarding_sink_probe_falls_back_to_post_when_options_returns_405`
  (OPTIONS=405 falls through to a POST that succeeds).
- **Catalogued substrate lie (2)**:
  `forwarding_sink_probe_refuses_startup_when_downstream_lies_with_503_on_post`
  pins the 200-OPTIONS / 503-POST scenario must refuse;
  `forwarding_sink_probe_failure_emits_health_startup_refused_event`
  pins the closed-vocabulary failure event.
- **Happy-path forwarding (3)**:
  `customer_exports_one_log_record_and_downstream_receives_protobuf_post`
  (the wiremock receives exactly one POST on `/v1/logs`),
  `forwarding_sink_accepted_event_includes_downstream_endpoint`
  (`sink=forwarding`, `downstream=<url>` in stderr), and
  `forwarding_sink_accepted_event_includes_downstream_latency_ms_field`
  (the new `downstream_latency_ms` field).
- **Failure-mode mapping (4)**:
  - 5xx-from-downstream → upstream `Code::Unavailable` +
    `event=sink_failed`;
  - connection refused → probe refuses startup;
  - downstream timeout → upstream `Code::Unavailable`;
  - the same scenario emits `event=sink_failed` at error level.

The 5 new gold-test scenarios in `tests/probe_gold_runner.rs` (cycle 4)
form the ADR-0010 layer-3 enforcement: they assert *wire traffic*
against a wiremock server, so a `Probe { Ok(()) }` body is caught by
the request-count check, not just by the result-status check. The
mutation simulation written into the cycle-4 commit message verifies
the catch.

The 2 new substantive scenarios in
`invariant_no_telemetry_on_telemetry.rs` close the load-bearing claim
"ForwardingSink is the only outbound network Aperture originates":

- `stub_sink_export_does_not_reach_an_unrelated_loopback_listener` —
  starts a stub-sink Aperture, drives one export, asserts an unrelated
  wiremock observed zero requests.
- `forwarding_sink_export_only_reaches_the_configured_downstream` —
  starts a forwarding-sink Aperture against a "reachable" wiremock,
  drives one export, asserts the reachable server received exactly one
  POST on `/v1/logs` and an unrelated wiremock observed zero
  requests, AND that no foreign paths were touched on the reachable
  server.

The DEVOPS-owned network-namespace fixture remains the load-bearing
gate; these tests are the application-surface corroboration that runs
unconditionally.

The 12 new lib unit tests in `src/sinks.rs` (cycle 6) close the
mutation gap on the pure helpers (`signal_name_for`,
`encode_for_forwarding`, `empty_export_logs_service_request_bytes`,
`url_for`).

## Production code added or modified

| File | Net change | What it does |
|---|---:|---|
| `src/sinks.rs` | **+596 / 0** | New `ForwardingSink` struct with `OtlpSink` and `Probe` impls. Two-stage probe (`probe_options` + `probe_degraded_post`) with per-stage 2 s budget independent of `forwarding_timeout`. `accept` encodes the typed record and POSTs; on 2xx emits `event=sink_accepted` with `sink=forwarding`, `downstream`, `downstream_latency_ms`, and the per-signal count field; on non-2xx or transport error emits `event=sink_failed`. 12 focused unit tests on the pure helpers. |
| `src/compose.rs` | **+50 / 12** | `wire_sink` and `spawn` now both honour `SinkKind::Forwarding` by constructing a real `ForwardingSink` and running its probe via the new `probe_or_refuse<P: Probe>` helper. The `SinkKind::Stub` arm preserves the legacy test-path behaviour. |
| `src/config/mod.rs` | **+15 / 2** | `forwarding_endpoint` and `forwarding_timeout` fields lose their `#[allow(dead_code)]` (now load-bearing). New `Config::forwarding_endpoint()` and `Config::forwarding_timeout()` accessors (`pub(crate)`). |
| `src/testing.rs` | **+27 / 3** | New `forwarding_sink_probe_for_gold_test(endpoint, timeout) -> Arc<dyn Probe>` factory: the seam the gold-test (`tests/probe_gold_runner.rs`) enters through. |
| `Cargo.toml` | **+11 / 0** | New `reqwest` production dependency (`default-features = false, features = ["http2"]` — TLS deliberately disabled, Slice 07 owns the `tls.enabled=true` reservation). New `[[test]] probe_gold_runner` entry. |
| **Slice 06 production-tree net delta** | **+688 / 17** | (`git diff 84f0cdc..HEAD -- crates/aperture/src/ crates/aperture/Cargo.toml`) |
| `tests/probe_gold_runner.rs` | **+200 (new)** | The behavioural-layer Earned-Trust gold-test. |
| `tests/invariant_no_telemetry_on_telemetry.rs` | **+130 / 1** | Slice 06 substance — see above. |

Modules NOT touched (still placeholders for future slices):
`src/error.rs` (Slice 07/08 lands the rich `ApertureError` enum),
`src/shutdown.rs` (Slice 08), `src/main.rs`, `src/readiness.rs`,
`src/transport.rs`, `src/backpressure.rs`, `src/app.rs`,
`src/ports/mod.rs`. The `single_validator_per_signal` invariant
continues to hold by construction — `ForwardingSink::accept` does
not call `validate_*` (validation stays in `app::ingest_*`).

## Commits

| Hash | Subject |
|---|---|
| `97f834e` | `feat(aperture): Slice 06 cycle 1 — ForwardingSink scaffold and sink_kind-driven wiring` |
| `ee71af7` | `feat(aperture): Slice 06 cycle 2 — two-stage Earned-Trust probe for ForwardingSink` |
| `dc61886` | `feat(aperture): Slice 06 cycle 3 — ForwardingSink accept path closes the slice` |
| `61baf65` | `test(aperture): Slice 06 cycle 4 — probe gold-test runner (ADR-0010 layer 3)` |
| `4941dba` | `test(aperture): Slice 06 cycle 5 — no_telemetry_on_telemetry invariant gains real substance` |
| `338be80` | `test(aperture): Slice 06 cycle 6 — pin pure helpers in sinks.rs against mutation` |
| (this doc) | `docs(aperture): DELIVER slice-06-completion summary` |

Six atomic GREEN commits, each pushed to `main` immediately. The
slice was run under the per-cycle commit + push discipline the
orchestrator instructions called for; if interruption had occurred
mid-slice, the next dispatch could have resumed from the
last-committed cycle.

## Probe semantics — the choice that diverges from the design template

The design template in `component-design.md > Probe` short-circuits
on `r.status().is_success() || r.status() == 204`. The acceptance
test `forwarding_sink_probe_refuses_startup_when_downstream_lies_with_503_on_post`
is incompatible with that condition: it sets OPTIONS=200 and
POST=503, expecting the probe to refuse — which requires the probe
to POST even after OPTIONS succeeds with 200.

The slice contract is the source of truth: "the test now asserts
that within a single accept-path operation, the only outbound network
call from Aperture is to the configured ForwardingSink endpoint". The
test contract that wins is "OPTIONS=204 short-circuits; any other
status (including 200) requires the POST stage to verify the
downstream is genuinely OTLP-compatible". RFC 9110's choice of 204
(No Content) as the canonical preflight-OK response supports this
asymmetry: 204 is a formal "yes, I understood the preflight question";
200 is a generic "yes" that does not necessarily mean "yes I speak
OTLP/HTTP".

The implementation lands in `classify_options_response`:

```rust
if status.as_u16() == 204 {
    return ProbeStageOutcome::Succeeded;          // formal preflight OK
}
if status.is_success() || matches!(status.as_u16(), 404 | 405) {
    return ProbeStageOutcome::FallThrough;        // verify with POST
}
ProbeStageOutcome::Failed(ProbeError::Refused { ... })
```

This is the single deliberate departure from the design template; the
slice brief permits it explicitly via the test contract. ADR-0007 is
unaffected — the trait shape and the dual-trait enforcement scheme are
unchanged.

## Mutation testing

Per ADR-0005 Gate 5, the target is 100% kill rate on Slice 06 touched
files. Run command (scoped to Slice 06 territory; restricted to the
green-by-design test set so the baseline passes):

```text
cargo mutants --package aperture --no-shuffle --jobs 2 \
  --file crates/aperture/src/sinks.rs \
  --file crates/aperture/src/compose.rs \
  --file crates/aperture/src/testing.rs \
  --file crates/aperture/src/config/mod.rs \
  --cargo-test-arg "--lib" \
  --cargo-test-arg "--test=slice_01_walking_skeleton" \
  --cargo-test-arg "--test=slice_02_http_protobuf_and_readiness" \
  --cargo-test-arg "--test=slice_03_traces" \
  --cargo-test-arg "--test=slice_04_metrics" \
  --cargo-test-arg "--test=slice_05_backpressure" \
  --cargo-test-arg "--test=slice_06_forwarding_sink" \
  --cargo-test-arg "--test=probe_gold_runner" \
  --cargo-test-arg "--test=invariant_no_telemetry_on_telemetry"
```

Per-file results after `338be80`:

| File | Mutants | Caught | Missed | Unviable | Notes |
|---|---:|---:|---:|---:|---|
| `src/sinks.rs` | 38 | 17 | 1 | 20 | 1 equivalent mutant on `empty_export_logs_service_request_bytes` |
| `src/compose.rs` | 7 | 7 | 0 | 0 | 100% kill |
| `src/testing.rs` | 15 | 0 | 0 | 15 | only unviable mutations (factory + capture seam) |
| `src/config/mod.rs` | 24 | 13 | 5 | 6 | all 5 missed are pre-existing slice 01/07/08 territory |
| **Slice 06 footprint** | **84** | **37** | **6** | **41** | **100% kill on slice-introduced mutations** |

The 1 missed mutation on `sinks.rs` is the equivalent mutant
`replace empty_export_logs_service_request_bytes -> Vec<u8> with vec![]`:
by the proto3 wire format an `ExportLogsServiceRequest` with no
`resource_logs` serialises to the empty byte sequence — exactly
`vec![]`. The two encodings are observationally indistinguishable.
The function body is documented inline so future maintainers see why
cargo-mutants leaves the entry MISSED and so a future change that
adds non-trivial probe-body content (auth tokens, schema negotiation)
will resume catching the mutation.

The 5 missed mutations on `src/config/mod.rs` are all pre-existing
slice 01/07/08 territory, identical to the pre-slice-06 baseline:
- `Config::builder` and `ConfigError::Display::fmt` (slice 01-era);
- `drain_deadline` setter (slice 08 territory);
- `tls_enabled` and `spiffe_enabled` setters (slice 07 territory).

Slice 06's two new accessors (`forwarding_endpoint`,
`forwarding_timeout`) and the two new `[forwarding_sink]` setter
mutations are killed by the slice 06 acceptance tests; slice 06
introduces zero new misses on `config/mod.rs`.

The baseline build was 13 s with 6 s test; the auto-set test timeout
was 32 s.

## Architectural observations

- **`Probe` is a separate trait, not a default method on `OtlpSink`.**
  ADR-0007 locks the dual-trait shape so the structural-layer xtask
  AST walk can verify every `impl OtlpSink` is matched by a real
  `impl Probe`. The implementation lands here without the Phase-1
  `dyn`-upcast trick: when `config.sink_kind == Forwarding`,
  `compose::spawn` constructs the concrete `ForwardingSink` value,
  runs `probe()` against it directly, then erases to `Arc<dyn OtlpSink>`.
  No `Arc<dyn Probe>` storage is needed in the hot path.
- **Per-stage probe budget independent of `forwarding_timeout`.** The
  probe's per-stage timeout is a fixed 2 s (`PROBE_STAGE_TIMEOUT`) so
  a misconfigured `[forwarding_sink] timeout_ms = 50` cannot starve
  the probe and produce a false-positive `Timeout` that hides a
  slower-but-correct startup. The `accept` path uses the configured
  `forwarding_timeout` because that IS the operator's request budget.
- **No retry, no circuit breaker.** The OTel SDK retries upstream;
  Aperture refuses fast (per ADR-0007 alternatives Considered "no
  double retry"). A 5xx, a connection refused, and a timeout all
  collapse to `gRPC UNAVAILABLE` / `HTTP 503` upstream, with the
  downstream's specific status surfaced only in the `event=sink_failed`
  stderr line.
- **Plaintext at v0.** The `reqwest` production dep is built with
  `default-features = false, features = ["http2"]` — TLS is
  deliberately off. Slice 07's `tls.enabled=true` knob is rejected by
  the config validator; an operator who needs TLS must wait for Aegis
  (Phase 2). The dev-dependency reqwest copy keeps `rustls-tls` for
  the integration tests' own reachability, with no leakage into the
  production tree.
- **Sink selection in `compose::spawn` swaps the test-path sink.**
  When `config.sink_kind == Forwarding` the passed
  `Arc<dyn OtlpSink>` is REPLACED by a freshly-constructed
  `ForwardingSink`. This mirrors the binary path's `wire_sink` and
  keeps the integration tests honest: a slice 06 test that passes
  `RecordingSink` but configures forwarding sees the real
  `ForwardingSink` wired through, not the recorder. The
  pre-slice-06 tests that rely on `RecordingSink` still work because
  they never set `forwarding_sink(...)` on the config builder, so
  `sink_kind` defaults to `Stub` and the passed sink is honoured.

## Genuine forks discovered

**One.** The probe semantics divergence documented under "Probe
semantics" above. The design template's
`r.status().is_success() || r.status() == 204` short-circuit
contradicts the slice contract's catalogued-substrate-lie scenario;
the slice contract wins. The implementation lands in
`classify_options_response` with a dedicated 204 branch and an
explicit fall-through for every other 2xx. ADR-0007 is unchanged
(the trait shape is unaffected); the design doc's pseudocode is
the only artefact that needs a future amendment, which DELIVER
notes here for the next ADR refresh.

The "known unknowns" the slice brief flags — outbound transport
choice (HTTP/protobuf vs gRPC) and default downstream timeout —
were locked at DISCUSS / ADR time: HTTP/protobuf, 5 s default. DELIVER
honoured both locks.

## Quality gates at commit time

- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean
- `cargo test --workspace --exclude aperture --all-targets --locked`:
  73 harness tests pass (matches DEVOPS A2 graduation contract)
- `cargo test --package aperture` (slices 01–06 + lib unit + invariant
  + gold-test): all green (139 tests)
- Pre-commit hook: not modified; `--exclude aperture` graduation
  stays per Apex's wave-decisions A2

## Out of scope (left RED for subsequent slices)

- `tests/slice_07_tls_schema_knob.rs` — Slice 07 (figment loader +
  `tls.enabled=true` warn-but-continue + `auth.spiffe.enabled=true`
  warn-but-continue)
- `tests/slice_08_graceful_shutdown.rs` — Slice 08 (drain orchestrator
  + `Draining` readiness state + deadline semantics)

These remain RED-by-design until DELIVER advances each slice in turn.

## Handoff to next slice

Slice 07 (TLS schema knob + figment loader) is the natural next cycle.
The seams Slice 07 will fill, with Slice 06's groundwork visible:

1. `Config::from_toml_path` and `Config::from_toml_str` are currently
   stubs returning `Err(ConfigError("not implemented until Slice 07"))`.
   Slice 07 lands the figment-driven TOML loader.
2. `Config::tls_enabled` and `Config::spiffe_enabled` setters are
   load-bearing today (the field exists, the setter mutates), but no
   warn-line is emitted when they flip to `true`. Slice 07 lands the
   `event=tls_not_supported_in_v0` warn line.
3. The `forwarding_sink.endpoint` validator: today
   `Config::builder().forwarding_sink("")` builds without error
   because the validator runs at config-load time, which Slice 07
   owns. Slice 07's loader rejects `kind = "forwarding"` with an
   empty `endpoint`. (The slice 06 tests work around this by always
   passing a `wiremock` URI; the integration tests are not affected.)

Slice 06's `ForwardingSink` is also the load-bearing target Slice 08's
drain orchestrator will reuse: the in-flight count Slice 08 reads from
the per-transport semaphores includes the time spent in
`ForwardingSink::accept`'s POST, so the drain deadline genuinely
covers downstream latency.

## Recommendation on the next dispatch

Slice 07 is the natural next slice: it's much smaller than Slice 06
(no new external dependency, no new outbound network surface, just
the figment loader and two warn-line emissions). A single dispatch
should land Slice 07 plus a Slice 06/07 stitching pass that closes
the `Config::tls_enabled` / `Config::spiffe_enabled` mutation surface
that Slice 06 left as pre-existing.

Whether to combine Slice 07 with Slice 08 (graceful shutdown drain) in
the same dispatch depends on appetite: Slice 08 is the most
substantive remaining slice after Slice 06, and combining it with
Slice 07 is reasonable given Slice 07's small footprint. A separate
dispatch for Slice 08 also works.

The orchestrator may choose either path; both honour the per-cycle
commit + push discipline.
