# Sieve v0 — Wave Decisions

Six scope decisions locked for Sieve v0. Each decision records the
recommended default, its rationale, and the alternative considered and
rejected. These decisions bound the DISCUSS package and the slice plan
that follows.

Sieve is Kaleidoscope's sampling and filtering processor. It sits in
the integration plane between Aperture (the OTLP receiver) and
downstream stages (Sluice or an external OTel-compatible backend). At
v0 it implements head-based probabilistic sampling on traces with
error-bias retention (100% of error-bearing traces kept, configurable
rate for non-error traces). Logs and metrics pass through unfiltered
at v0. PII-scrubbing and per-tenant rules are v1+.

Crate licence: AGPL-3.0-or-later (server-side platform component).

---

## Q1 — Library embedded in Aperture vs separate process binary?

**Decision (default chosen): library at v0.**

`crates/sieve` exposes a `Sampler` trait that Aperture's pipeline calls
before the sink. Roadmap C.4 says "Stage one is head-based
probabilistic sampling at Aperture"; a library shape matches that.
Process shape is deferred to v1+, when tail-sampling needs an
in-memory window across batches and a separate process becomes the
honest shape.

**Rejected alternative: separate process binary.** Adds a network hop
and a full OTLP-in / OTLP-out crate's worth of plumbing for no v0
user value. The walking skeleton needs to prove the sampling decision
itself, not the transport plumbing around it.

---

## Q2 — Sampling decision granularity?

**Decision (default chosen): trace-level.**

Sample whole traces (keep all spans of a trace, or none). The decision
is keyed on `trace_id`. This is the canonical OTel head-sampling
shape and preserves trace coherence — every span of a kept trace
arrives at the downstream stage; no kept trace is missing spans.

**Rejected alternative: span-level.** Loses trace coherence: some
spans of a trace kept, others dropped, producing broken traces in
downstream UIs. Operators chasing a latency spike would see
truncated waterfalls. Unacceptable for a sampling layer that claims
to preserve trace utility.

---

## Q3 — Error-bias rule definition?

**Decision (default chosen): `status.code == ERROR` defines an error
span; any error span makes the trace error-bearing; error-bearing
traces are retained at 100%; non-error traces are retained at the
configured probability.**

`status.code` is the canonical OTel signal for span outcome and is
language- and framework-agnostic: every OTel SDK sets it. This rule
is the simplest one that captures "keep what hurts" with no
framework-specific carve-outs.

**Rejected alternative: HTTP-status-aware bias (e.g. retain when
`http.response.status_code >= 500`).** Too flavour-specific. Sieve
sees traces from gRPC, messaging, database, and synthetic workloads;
HTTP semantic conventions are one shape among many. `status.code`
is the universal signal.

---

## Q4 — PII-scrubbing rules at v0?

**Decision (default chosen): defer to v1.**

v0 is sampling only. PII-scrubbing in CUE (per roadmap C.4) introduces
a CUE dependency, a rules vocabulary, and a fixture corpus. Pulling
that into a thin-slice walking skeleton is too much surface and
delays the moment Sieve becomes useful at all. Sampling alone is the
volume win operators are asking for.

**Rejected alternative: include scrubbing at v0.** Too much surface
for a walking skeleton. Risks shipping half-baked scrub rules that
operators come to depend on before the rule grammar is stable.

---

## Q5 — Sample rate configuration shape?

**Decision (default chosen): single global rate, configured via env
var `SIEVE_NON_ERROR_TRACE_RATE` (a float in `[0.0, 1.0]`), default
`0.1` (10%).**

A single global rate is the smallest control surface that demonstrates
the sampling story end-to-end. Env var wiring is consistent with
how Aperture configures itself today; no additional config-file
parser is needed at v0. Default of `0.1` mirrors common
head-sampling defaults across the OTel ecosystem.

**Rejected alternative: per-service or per-tenant rate.** Both need
context Sieve v0 doesn't have. There is no tenant catalogue at v0
(Aegis arrives later); per-service rates need a service-name
allowlist that has no clear v0 source of truth. Defer to v1.

---

## Q6 — Logs and metrics sampling at v0?

**Decision (default chosen): traces only; logs and metrics pass
through unfiltered.**

The volume problem v0 solves is trace volume — that is where the
spans-per-second pressure comes from in OTel pipelines. Logs and
metrics have different sampling shapes (severity-based filtering for
logs, aggregation-based reduction for metrics) and deserve their
own design pass in v1.

**Rejected alternative: drop unsampled logs and metrics on the
floor.** Operators need a working pipeline for all three signals or
v0 is not useful at all. Pass-through is the only honest v0
behaviour for signals Sieve does not yet sample.

---

## Q7 — Hash function for `trace_id`-keyed sampling determinism?

**Decision (default chosen): `xxh3_64` from the `xxhash-rust` crate.**

`trace_id` is a 128-bit identifier; the sampling decision needs to map
it to a uniform `[0.0, 1.0]` value the rate is compared against. xxh3
(64-bit) is the OTel community's converging choice for this kind of
fast, deterministic, well-distributed hash with no cryptographic
overhead. The `xxhash-rust` crate is dual-licensed BSL-1.0 / MIT;
permissive and on the workspace's allow list.

**Rejected alternative: Rust's standard `Hasher` trait
(`SipHasher`).** SipHasher is correct but slower; cryptographic
quality is not needed for sampling decisions; xxh3's measured
throughput is roughly an order of magnitude better at trace volumes.
The OTel collector's TailSamplingProcessor uses xxh3 for the same
purpose, which makes interop expectations easier.

---

## Q8 — Tracing-event verbosity for per-trace sampling decisions?

**Decision (default chosen): `DEBUG` for per-trace events; `INFO`
summary every 60 seconds on a tokio timer task.**

A live trace stream produces per-trace events at potentially thousands
per second. Logging each at INFO would either flood the operator's
log aggregator or force them to set a stricter filter, defeating the
observability story. DEBUG keeps per-trace decisions available for
operators who want them (`RUST_LOG=sieve=debug`) without polluting
the default log volume. An INFO-level summary every 60 seconds gives
the default operator the aggregate picture they care about: "sieve:
kept 412 traces (47 error-bearing, 365 sampled at 0.10 rate), dropped
3614 traces over the last 60s".

The summary tick is **part of the v0 contract** — without it, an
operator on default verbosity has no visibility into Sieve's
behaviour. Both the DEBUG per-trace events and the INFO 60-second
summary land in slice 06. The tick interval is 60 seconds in
production; the integration test infrastructure parameterises it down
to a smaller value (e.g. 100 ms) so CI can assert the summary fires
within a reasonable wall-clock budget. The "interval-agnostic" KPI
language in `outcome-kpis.md > KPI 5` ("100% of summary windows emit
exactly one INFO event") is intentional: the production interval and
the test interval differ, but the per-window invariant holds at both.

**Rejected alternative: INFO per-trace event.** Operationally
unworkable at the trace volumes Sieve targets (the entire reason
Sieve exists is to reduce the volume of trace data). The aggregate
summary at INFO is the right default verbosity for the operator who
has not opted into per-trace detail.

---

## Out of scope for v0

- Tail-based sampling (needs an in-memory window across batches)
- PII-scrubbing rules (deferred to v1)
- Per-tenant or per-service sample rates (deferred to v1)
- Log severity filtering and metric aggregation reduction (v1)
- Separate-process binary shape (v1+)
- Dynamic rate changes at runtime (v0 reads env var at startup)
