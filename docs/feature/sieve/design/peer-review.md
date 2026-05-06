# Peer review — Sieve v0 DESIGN

- **Date**: 2026-05-06
- **Reviewer**: `@nw-solution-architect-reviewer` (Atlas)
- **Wave**: DESIGN (Morgan, single iteration)
- **Artefact set**: `docs/feature/sieve/design/` plus ADRs 0018-0021
- **Verdict**: **APPROVED** — handoff to DISTILL
- **Critical issues**: 0
- **High issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

Morgan executed a disciplined DESIGN wave. All eight DISCUSS scope
locks (Q1-Q8) carry forward without amendment. All six implementation
decisions Sentinel flagged (D1-D6) are closed decisively in the four
ADRs (0018-0021) with rigorous trade-off analysis. The decorator
pattern (`SamplingSink<S, N>` over Aperture's existing `OtlpSink +
Probe`) is the single most consequential design choice and it is the
right one: zero Aperture surface change, canonical Rust pattern,
Earned-Trust invariant preserved through honest probe delegation,
DELIVER's wiring is a three-line edit in `crates/aperture/src/compose.rs`.

Zero back-propagation flags. The DISCUSS contract holds verbatim.

---

## DISCUSS contract fidelity

All eight DISCUSS scope decisions are locked in the DESIGN output:

| Decision | DISCUSS | DESIGN closure |
|---|---|---|
| Q1 library vs separate process | library | ADR-0021 §1 confirms decorator as library integration |
| Q2 trace-level vs span-level | trace-level | ADR-0018 `TraceView<'_>` signature |
| Q3 error-bias rule | `status.code == ERROR` | ADR-0018 `HeadSampler::sample` mechanism |
| Q4 PII-scrubbing | deferred to v1 | All four ADRs acknowledge v1+ scope |
| Q5 single global rate | env-var | ADR-0018 `HeadSampler::from_env` |
| Q6 logs/metrics passthrough | passthrough | ADR-0021 `SinkRecord` routing |
| Q7 xxh3_64 hash | xxh3_64 | ADR-0019 §1 exact-minor pin =0.8 |
| Q8 DEBUG + INFO 60s summary | locked | ADR-0020 §4-5 |

All six implementation decisions D1-D6 closed in the ADRs with
explicit Why / Why-not analysis.

---

## Per-ADR findings

### ADR-0018 — Public API and crate layout — APPROVED

`praise:` `Sampler::sample(&TraceView<'_>) -> Decision` is the right
shape. The borrowed view exposes `trace_id()` and `spans()` directly,
avoiding the `spans[0].trace_id` deref-per-call that the rejected
raw `&[Span]` alternative would force. The `__test_trace_view`
test seam follows Spark's `__reset_for_testing` precedent.

`praise:` Sealed `Decision { Keep, Drop }` plus a separate
`#[non_exhaustive] KeepReason` enum is the right shape. The decorator's
routing branch stays a clean two-arm match; observability metadata
lives on the tracing event, not the return value. Future samplers
(tail-sampling in v1) can extend `KeepReason` additively.

The five-option alternatives analysis (raw slice, metadata-carrying
Decision, before-accept hook on Aperture, sieve-core/sieve-aperture
split, the chosen decorator) is genuine. Each rejection cites a
substantive reason.

### ADR-0019 — Dependency pinning — APPROVED

`praise:` `xxhash-rust = "=0.8"` exact-minor pin justified by output
stability: a hash-algorithm change would shift the set of kept traces
on the same fixture, which is operator-visible. Mirrors Spark's
ADR-0013 pin policy.

`praise:` Aperture as runtime dep is symmetric (both AGPL); ADR-0019
§2 explains why this is allowed for Sieve and forbidden for Spark
(Apache supply chain). The contrast with Spark's posture is honest.

The BSL-1.0 licence audit is complete; the workspace `deny.toml`
needs one new allow entry, flagged for DEVOPS.

### ADR-0020 — Summary aggregator and timer task — APPROVED

`praise:` Three `AtomicU64` with Relaxed ordering is wait-free
without being clever. The cross-counter race (a record landing
between two `swap` calls during snapshot) is documented as
acceptable for the "approximate aggregate over a 60s window"
contract. Mutex / RwLock / DashMap alternatives are rejected on
substantive grounds (contention, wrong access pattern,
over-engineering for three statically-known counters).

The Sieve-owned timer task uses `tokio_util::sync::CancellationToken`
with sync-cancel + best-effort-join on Drop, mirroring Aperture's
`Handle::Drop` precedent. The `__test_summary_tick_now` test seam
fires the snapshot path synchronously, which works because Sieve
owns the timer (an Aperture-owned timer would force tests to drive
Aperture's runtime, awkward).

### ADR-0021 — Aperture integration — APPROVED

`praise:` The decorator preserves Aperture's contract and changes
nothing on Aperture's side. `accept` matches on `SinkRecord` and
routes Logs/Metrics straight to the inner sink (Q6 passthrough);
Traces run through the grouping → decision → counter-update →
forward pass. `probe()` delegates to the inner sink — honest because
Sieve has no external dependency to probe.

The DELIVER wiring is genuinely three lines:

```rust
let inner = build_inner_sink(config).await?;
let sampler = sieve::HeadSampler::from_env()?;
let decorated = sieve::SamplingSink::new(inner, sampler);
Ok(Arc::new(decorated) as Arc<dyn OtlpSink>)
```

`SinkRecord` reuse (no Sieve-local Signal wrapper) avoids two trait
impls and two memcpys per record on the hot path. The "no re-exports"
discipline from ADR-0011 applies; consumers reach `SinkRecord` via
`aperture::ports`.

---

## Cross-cutting checks

**ADR rigour**: All four ADRs follow the Context / Decision / Alternatives
/ Consequences structure. Every alternative is genuine; no strawmen.
Negative consequences and trade-offs are honestly faced.

**Public-API ergonomics**: Seven public items plus two doc-hidden
test seams. Tightest possible set delivering all six user stories.
No re-exports of upstream crates. Generic `SamplingSink<S, N>`
composes cleanly with both concrete (test) and erased
(`Arc<dyn OtlpSink>`) consumers.

**CI invariant alignment**: All five gates per ADR-0005 named in
the slice mapping. Platform-architect hand-offs are explicit:
new `BSL-1.0` allow entry in `deny.toml`, new
`gate-5-mutants-sieve.yml` workflow mirroring
`gate-5-mutants-aperture.yml`, Sieve crate addition to workspace
members, three-line composition-root edit in `compose.rs`.

**Scope discipline**: No ADR over-specifies v0.2+ concerns. Future
extensibility notes (e.g. ADR-0018's `#[non_exhaustive]` on
`KeepReason`) are architectural fact, not speculation. Speculative
generality (sieve-core/sieve-aperture split, Sieve-exposes-generic-Sink,
DashMap) is correctly rejected with named justifications.

**Antipattern scan**: None. ADRs avoid implementation-as-architecture
(pseudocode is annotated as such); type decisions are justified;
no premature optimisation; no speculative generality.

**Slice mapping**: Each of the six slices traces story → ADR → modules
→ CI gates → KPI cleanly. Mapping is acyclic and complete.

---

## Praise

`praise:` The decorator pattern is the right architectural choice.
The five rejected alternatives in ADR-0018 plus the four in ADR-0021
together demonstrate that Morgan thought through the design space
honestly. Zero Aperture surface change, canonical Rust idiom,
Earned-Trust invariant preserved, DELIVER cost minimal.

`praise:` The error-bias rule is operationally clear:
`status.code == ERROR` on any span makes the trace error-bearing;
error-bearing traces retained at 100%. Language- and framework-
agnostic. The rejected alternative (HTTP-status biasing) is too
flavour-specific.

`praise:` Dependency hygiene is disciplined. Every pin justified,
every feature minimal, the new BSL-1.0 licence audited and flagged
for the workspace policy. xxhash-rust's exact-minor pin is the right
trade-off (pin tight where output stability is operator-visible).

`praise:` The summary-aggregator concurrency model is wait-free on
the hot path without being clever. Three atomics, Relaxed ordering,
swap-based snapshot. The cross-counter race is documented as
semantically benign for the operator's "approximate aggregate" ask.

`praise:` Test seams follow the Spark precedent (`__` prefix +
`#[doc(hidden)]`). The convention is now established in the codebase
and reads as "stable across versions, but explicitly not part of
the consumer-facing contract".

---

## Notes for downstream waves

**For Scholar (DISTILL)**: read in order — wave-decisions →
slice-mapping → C4 L1/L2/L3 → technology-choices → ADRs 0018-0021 →
DISCUSS artefacts. The acceptance test surface is the seven public
items plus the two doc-hidden test seams. Strategy C "real local"
posture: real Aperture as a dependency (not just a dev-dep) because
the decorator pattern requires the trait definitions in scope.

**For Apex (DEVOPS)**: four hand-offs from slice-mapping §"Hand-off
boundary to platform-architect":

1. New `BSL-1.0` allow entry in workspace `deny.toml`.
2. New `gate-5-mutants-sieve.yml` workflow mirroring
   `gate-5-mutants-aperture.yml`.
3. Sieve crate addition to workspace members.
4. Sieve crate addition to Aperture's `Cargo.toml` as a runtime dep
   (AGPL-symmetric).

**For Crafty (DELIVER)**: the three-line wiring in `compose.rs` per
ADR-0021 §3. Non-breaking, additive. Six slices follow the order in
slice-mapping.md.

---

## Approval

**APPROVED** for handoff to DISTILL.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.
