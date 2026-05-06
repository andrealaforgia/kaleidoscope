# ADR-0017 — `spark` logs-emission seam via `opentelemetry-appender-tracing`

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `nw-solution-architect` (Morgan, dispatched by Bea; ADR finalised by Bea after Morgan stalled mid-write)
- **Feature**: `spark` v0
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0011 (public API surface), ADR-0013 (dependency pinning)

## Context

The DISCUSS contract for Slice 05 (`logs and metrics symmetry`,
US-SP-05) presupposed that an application embedding Spark could emit
a log record through the symmetric three-signal API at the
`opentelemetry::global::*` namespace, mirroring the way `tracer_provider()`
and `meter_provider()` work for traces and metrics:

```rust
let logger = opentelemetry::global::logger_provider().logger("checkout-service");
logger.emit(LogRecord::builder()...build());
```

DISTILL surfaced (see `docs/feature/spark/distill/back-propagation.md`)
that the OpenTelemetry Rust SDK at the family-pinned `=0.27` (per
ADR-0013 §1) does **not** expose a global logger-provider getter or a
`set_logger_provider` global setter. The `opentelemetry::global` module
re-exports only `tracer_provider()` / `tracer()` /
`set_tracer_provider()`, and `meter_provider()` / `meter()` /
`set_meter_provider()`. The logs API exists at the trait level
(`opentelemetry::logs::LoggerProvider`, `Logger`, `LogRecord`) and at
the SDK level (`opentelemetry_sdk::logs::LoggerProvider`), but there
is no global plumbing.

This is an upstream limitation Spark v0 inherits. Resolving it
requires a v0 design choice on how an application emits logs through
Spark's configured `LoggerProvider`. DISTILL's back-propagation note
proposed four options (A1-A4); Bea, with Andrea's authorisation,
chose **Option A3**: adopt `opentelemetry-appender-tracing` as
Spark's runtime dependency for the v0 logs-emission path.

## Decision

### 1. Adopt `opentelemetry-appender-tracing` as a runtime dependency

The crate `opentelemetry-appender-tracing` is the OpenTelemetry
project's canonical bridge from the `tracing` ecosystem to the OTel
logs pipeline. It exposes
`OpenTelemetryTracingBridge::new(&LoggerProvider)`, which returns a
type that implements `tracing_subscriber::Layer`. Wired into the
application's `tracing-subscriber` layer stack, it forwards every
`tracing::*!` event as an OTel `LogRecord` through the configured
`LoggerProvider`.

**Version**: `=0.27` exact-minor pin. The
`opentelemetry-appender-tracing` crate's per-version
`[dependencies.opentelemetry]` field aligns the appender's minor
with the core's minor: at appender `0.27.0` the appender depends on
`opentelemetry 0.27`; at appender `0.28.1` the appender depends on
`opentelemetry 0.28`. When the core family is pinned at `=0.27` (per
ADR-0013), the matching appender release is `=0.27`, not `=0.28` as
this ADR originally claimed.

**Amendment, 2026-05-06 (Slice 05 DELIVER)**: the original wording
above asserted an "offset by one" release cadence. That was a
misreading of the upstream release matrix; the appender's minor
aligns with the core's minor. Pinning `=0.28` would have introduced
a duplicate `opentelemetry 0.28.0` in `Cargo.lock` alongside the
existing `0.27.1`. DELIVER verified the alignment by reading the
appender's per-version manifests on the registry and pinned `=0.27`
in `crates/spark/Cargo.toml`. See
`docs/feature/spark/deliver/back-propagation.md > Issue 4` for the
build-time evidence.

**Licence**: Apache-2.0 (verified at the upstream
`open-telemetry/opentelemetry-rust-contrib` repository's `LICENSE`
file). Consistent with Spark's permissive runtime supply chain
(LICENSING.md SDK class).

**Cargo.toml entry**:

```toml
[dependencies]
opentelemetry-appender-tracing = "=0.27"
```

Add it to the licence-audit table in ADR-0013 §3 in the next ADR-0013
revision (non-blocking note for Bea).

### 2. Application-side emission contract

Applications using Spark v0 emit logs via the `tracing` ecosystem.
The contract is:

- The application uses `tracing::info!`, `tracing::warn!`,
  `tracing::error!`, `tracing::debug!`, `tracing::trace!`, or
  structured equivalents, attached to any target the application owns.
- Spark's `init` configures the OTel `LoggerProvider`, builds an
  `OpenTelemetryTracingBridge` against it, and adds that bridge as a
  `tracing_subscriber::Layer`. The application's `tracing` events
  flow through the bridge into the OTel pipeline and out via OTLP.

Applications must NOT:

- Try to call `opentelemetry::global::logger_provider()` (does not
  exist at OTel `=0.27`).
- Try to call `opentelemetry::global::set_logger_provider()` (does
  not exist at OTel `=0.27`).
- Attempt to obtain Spark's internal `LoggerProvider` directly. The
  bridge is the only contracted emission path at v0.

### 3. The no-telemetry-on-telemetry invariant

Spark's own diagnostic events (`tracing::info!(target: "spark", ...)`,
`tracing::warn!(target: "spark", ...)`) MUST NOT flow through the
appender into the OTel pipeline Spark configured. Otherwise Spark
emits its shutdown event into its own export pipeline, which then
flushes that event during the same shutdown — a feedback loop that
violates DISCUSS D5.

The bridge configuration MUST exclude `target: "spark"` events. The
mechanism is the bridge's filter API or a `tracing_subscriber`
filter layer applied above the bridge:

```rust
let bridge = OpenTelemetryTracingBridge::new(&logger_provider)
    .with_filter(filter_fn(|metadata| metadata.target() != "spark"));
```

DELIVER (Crafty) implements the exact filter and wires it into the
init flow. The filter is verified by the
`tests/invariant_no_telemetry_on_telemetry.rs` binary, which is
extended at DELIVER time to assert that a `tracing::info!(target:
"spark", "marker")` event does NOT produce a corresponding
`LogRecord` at Aperture's RecordingSink.

### 4. The public surface stays at four items

Adopting the appender does NOT change Spark's public surface. The
bridge is wired internally during `init`. Applications import
`tracing::*!` macros from the standard `tracing` crate (which they
almost certainly already use), not from Spark. ADR-0011's four-item
public surface lock holds: `init`, `SparkConfig`, `SparkError`,
`SparkGuard`.

## Consequences

### Positive

- **DISCUSS contract intent preserved**. The semantic intent of
  US-SP-05 (logs flowing through Spark's configured `LoggerProvider`
  reach Aperture with the four house attributes intact) is preserved;
  only the literal emission API changes from
  `opentelemetry::global::logger_provider()` to `tracing::*!` macros.
- **Idiomatic for Rust applications in 2026**. The `tracing` crate is
  pervasive in the Rust ecosystem; almost any service-grade Rust
  application already uses it. Adopting the appender means Spark
  consumers do not need to learn a new logs-emission API; they keep
  using what they already use.
- **Public surface stays minimal**. Four items, ADR-0011 lock holds.
- **Forward-compatible**. When a future OTel SDK release adds the
  global logger-provider getter, Spark can offer that path as an
  additional emission option without removing the appender bridge.
  The bridge is non-breaking.
- **Three-signal symmetry KPI preserved at v0**. KPI 5 (logs +
  traces + metrics carry identical Resource) holds; the deferral
  flagged in DISTILL's back-propagation note is resolved.

### Negative

- **One additional runtime dependency**. The licence-audit table
  grows by one row. The transitive dependency tree may grow slightly
  (the appender depends on `tracing` and `tracing-core` at the
  versions matching the SDK's). `cargo deny check` is the structural
  guard.
- **The application must use a `tracing_subscriber`**. Most do
  already. If an application chooses to emit logs through some
  non-`tracing` path (e.g. directly through `log` or
  `slog`), Spark's appender does not route those. The `log` crate
  has a `tracing-log` bridge, so this is a documentation issue, not a
  technical one.
- **The no-telemetry-on-telemetry filter is a load-bearing detail**.
  Forgetting to filter `target: "spark"` would produce a feedback
  loop. The invariant test is the catch.

### Neutral

- **Bridge version coupling**. The appender-tracing crate's release
  cadence is loosely coupled to the core SDK's. When ADR-0013's
  family pin moves from `=0.27` to `=0.28`, the appender pin moves
  from `=0.28` to whatever the matching release is. The migration
  path mirrors ADR-0013's existing migration path; document the
  appender pin alongside the family pin in future ADR-0013 revisions.

## Alternatives considered

### Option A1: Spark exposes `pub fn logger_provider()`

Add a fifth item to Spark's public surface, returning a handle the
application uses to obtain a `Logger` directly. **Rejected**: this
expands the public surface beyond ADR-0011's four-item lock; requires
ADR-0011 to be superseded; and demands consumers learn an API path
that exists only at OTel `=0.27` and may obsolete itself when the SDK
adds the global getter. Adds permanent complexity to fix a temporary
upstream gap.

### Option A2: Test-only seam (`pub(crate) fn test_logger_provider`)

Expose a logger-provider seam visible only from inside the crate.
**Rejected**: integration tests live in `tests/` (separate Cargo
binaries), not inside `src/`. `pub(crate)` is invisible to them. A
`pub` getter gated by `#[cfg(test)]` would work but pollutes the
public-surface story for `cargo public-api` and is a workaround
masquerading as design.

### Option A4: Defer Slice 05 logs to v0.1

Wait for an upstream OTel SDK release that adds
`opentelemetry::global::logger_provider()`. **Rejected**: there is no
public timeline for this addition. Deferring means KPI 5
(three-signal symmetry at v0) becomes a v0.1 KPI, breaking the
DISCUSS-locked elephant-carpaccio that puts log-emission at Slice 05.
The cost of waiting is unbounded; the cost of A3 is bounded and
reversible.

## Verification

1. **`cargo deny check` (Gate 4)** confirms `opentelemetry-appender-tracing
   =0.28` is on the allow list and is Apache-2.0. The dep is added to
   the licence-audit table in DESIGN's `technology-choices.md` (DEVOPS
   updates the file in the same change).
2. **Slice 05's three `#[ignore]`'d tests** are un-ignored and rewritten
   to use `tracing::info!` (or equivalent) macros emitting through the
   Spark-configured appender. Test names stay identical (the BDD
   scenario function names were preserved verbatim by Scholar). The
   assertions stay identical (a `LogRecord` reaches Aperture's
   RecordingSink with the four house attributes on the Resource); only
   the emission mechanism changes.
3. **`tests/invariant_no_telemetry_on_telemetry.rs`** is extended at
   DELIVER time to assert that `tracing::info!(target: "spark",
   "marker")` does NOT produce a `LogRecord` at Aperture's
   RecordingSink. The filter excluding `target: "spark"` is the
   load-bearing implementation detail this invariant guards.
4. **`cargo public-api` (Gate 2)** confirms ADR-0011's four-item public
   surface is unchanged. Adopting the appender adds a runtime dep but
   does not add a public item.
5. **`cargo semver-checks` (Gate 3)** passes.
6. **`cargo mutants` (Gate 5)** runs against Spark's source including
   the bridge wiring; the kill-rate target is 100%.

## DISCUSS contract update required

DISCUSS US-SP-05 currently references the
`opentelemetry::global::logger_provider()` API. After ADR-0017 lands,
DISCUSS needs the following mechanical edits (Bea or Luna applies):

1. `discuss/user-stories.md > US-SP-05` — replace the
   `opentelemetry::global::logger_provider().logger("svc").emit(LogRecord::builder()...)`
   references with `tracing::info!` (or the equivalent `tracing` macro)
   per the new emission contract. Add a `## Changed Assumptions` entry
   documenting the move from the original (A0) to A3 (this ADR), with
   rationale and links.
2. `discuss/journey-spark.yaml` step 4 command + step 4 gherkin —
   same.
3. `discuss/journey-spark.feature` US-SP-05 scenarios — same.
4. `slices/slice-05-logs-and-metrics.md > Demo command` and
   `> Acceptance summary` — same.

The Slice 05 outcome (three-signal Resource symmetry) is unchanged.
The acceptance criteria's "Then" clauses are unchanged. Only the
"When" clause's emission API path changes.

See `docs/feature/spark/design/back-propagation-2.md` for the
precise edits Luna or Bea applies.
