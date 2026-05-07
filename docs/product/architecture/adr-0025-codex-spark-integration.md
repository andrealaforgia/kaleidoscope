# ADR-0025 — Codex–Spark integration: lint hook, warn-mode tracing, opt-in strict

- **Status**: Accepted (Spark side landed at commit `866423e`,
  2026-05-07)
- **Date**: 2026-05-07
- **Author**: `nw-solution-architect` (Morgan, dispatched by Bea;
  ADR finalised by Bea after Morgan stalled mid-write)
- **Feature**: `codex` v0
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0011 (Spark public API), ADR-0012 (Spark error
  type, gains a post-DELIVER amendment for the new variant), ADR-0013
  (Spark dependency pinning, gains a post-DELIVER amendment for the
  Codex runtime dep), ADR-0022 (Codex public API)

## Context

DISCUSS Q6 locked the Codex–Spark integration mechanism: Spark
takes Codex as a runtime dep, calls `SchemaCatalogue::validate(...)`
inside `spark::init`, and surfaces violations either as a
`tracing::warn!` event (default) or as `Err(SparkError::SchemaValidation(report))`
(opt-in via `SparkConfig::with_strict_schema_lint(true)`).

DISCUSS Q9 locked the warn-event shape: a single
`tracing::warn!(target = "spark", ...)` event per misconfigured
init, carrying the full `LintReport` via its `Display` rendering.

This ADR locks the integration's exact shape: where in `spark::init`
the call lives, how the catalogue is constructed (lazily, once per
process), how the `SparkError` variant is added, and how the warn
event is structured.

The Codex–Spark integration is the first real exercise of Spark's
`#[non_exhaustive]` discipline on `SparkError`. Spark v0.1.0
shipped with the marker; this ADR adds a variant additively.

## Decision

### 1. Spark's Cargo.toml gains `codex` as a runtime dep

```toml
# crates/spark/Cargo.toml

[dependencies]
codex = { path = "../codex", version = "0.1" }
# (rest of Spark's existing runtime deps unchanged)
```

The dep is path-resolved with a version pin. Codex is `AGPL-3.0-or-later`;
Spark is `Apache-2.0`. The licence asymmetry is acceptable for the
same reason Sieve takes Aperture as a runtime dep (ADR-0019 §2):
both crates ship as part of a single Kaleidoscope deployment, and
the AGPL on the platform side is structural rather than viral
across the SDK. Spark's AGPL closure is bounded to Codex; downstream
consumers of Spark (closed-source applications) do not inherit AGPL
because they consume Spark, not Codex.

### 2. Lazy catalogue construction

```rust
// crates/spark/src/init.rs

use std::sync::OnceLock;
use codex::SchemaCatalogue;

static CATALOGUE: OnceLock<SchemaCatalogue> = OnceLock::new();

fn catalogue() -> &'static SchemaCatalogue {
    CATALOGUE.get_or_init(SchemaCatalogue::new)
}
```

The static corpus inside `SchemaCatalogue::new()` is large enough
that rebuilding it on every `spark::init` call would add measurable
boot-time overhead. `OnceLock` builds it once per process, which
matches Spark's single-init invariant (ADR-0015) anyway.

### 3. The lint hook in `spark::init`

```rust
// In spark::init, after Resource composition but before
// TracerProvider / LoggerProvider / MeterProvider construction:

let resource_attrs: Vec<(&str, &str)> = compose_resource_attributes(&config);

if let Err(report) = catalogue().validate(&resource_attrs) {
    if config.strict_schema_lint {
        return Err(SparkError::SchemaValidation(report));
    } else {
        tracing::warn!(
            target = "spark",
            "schema validation failed:\n{}",
            report,
        );
    }
}
```

The hook runs **before** any OTel SDK type is constructed, so a
schema violation surfaces before any spans or metrics are
attributed to a malformed Resource. This matches the
fail-fast-at-init posture Spark already takes for
`MissingRequiredAttribute` and `InvalidEndpoint` (ADR-0012's
existing variants).

The warn event uses Display rendering (`{}`) inline in the message
body, not as a structured field. The full report text is the
operator-readable diagnostic; downstream log aggregators consume the
message body, and operators do not need to decode a structured
field to read which attributes are wrong.

### 4. The new `SparkError::SchemaValidation` variant

```rust
// crates/spark/src/error.rs

#[non_exhaustive]
pub enum SparkError {
    // existing variants:
    MissingRequiredAttribute { name: &'static str },
    InvalidEndpoint { endpoint: String, reason: String },
    ExporterInitFailed { source: Option<Box<dyn std::error::Error + Send + Sync>> },
    GlobalAlreadyInitialised,

    // new variant added at Codex Slice 06 DELIVER:
    SchemaValidation(codex::LintReport),
}
```

The variant is added under the existing `#[non_exhaustive]` annotation
that ADR-0012 locked. By Rust's semver rules, adding a variant under
`#[non_exhaustive]` is non-breaking. The `LintReport` type is
re-exported from `codex` (or, if Spark prefers a stricter
encapsulation, wrapped in a Spark-side newtype — DELIVER's call).

ADR-0012 itself gains a post-DELIVER amendment note documenting
this addition, mirroring the pattern from Slice 06 of Spark's
DELIVER (where the appender-tracing wiring landed) and Slice 06 of
Sieve's DELIVER (where the slice-by-slice mutation pattern landed).

### 5. The `with_strict_schema_lint(bool)` builder method

```rust
// crates/spark/src/config.rs

impl SparkConfig {
    /// Configure strict-mode schema lint.
    ///
    /// Default: `false` (warn mode). A `LintReport` from Codex is
    /// emitted as a single `tracing::warn!(target = "spark")` event;
    /// `spark::init` returns `Ok(SparkGuard)`.
    ///
    /// Strict mode (`true`): a `LintReport` from Codex causes
    /// `spark::init` to return `Err(SparkError::SchemaValidation(report))`.
    /// Useful for CI integration tests where a misconfiguration
    /// should fail-fast.
    pub fn with_strict_schema_lint(mut self, strict: bool) -> Self {
        self.strict_schema_lint = strict;
        self
    }
}
```

Default `false` is the operationally safe choice: operators rolling
out Codex into existing Spark deployments do not see new init
failures. Strict mode is opt-in for CI / pre-production environments.

### 6. The `LintReport` `Display` contract

The Display rendering is the operator-readable text. From
ADR-0022 §4 (Codex public API):

```text
schema validation failed:
  - tenat.id (Unknown; did you mean tenant.id?)
  - svc.name (Unknown; no close match)
```

One line per violation, each naming the offending attribute, the
violation kind, and the nearest blessed match (when populated). The
warn event's message body is exactly this text.

## Alternatives considered

### Option A (rejected): One `tracing::warn!` per `LintViolation`

Slice 06's brief originally recommended this shape (Bea has
corrected the brief to align with Q9). The arguments for per-violation
events: structured fields per attribute, easy count-by-violation
dashboards. The arguments against (and the reason Q9 rejected it):
operationally noisy; a single misconfigured init with three typos
produces three warn events, three log-aggregator hits, three
notification-system pings; dashboards counting "warn events"
double-count a single bad init; the operator's mental model of
"one bad init" maps cleanly to "one warn event".

The Q9 decision is the locked posture. This ADR is consistent with
it.

### Option B (rejected): The lint runs after OTel SDK construction

Some implementations lint the Resource after the providers are
built. The argument: the providers carry the Resource, so the
lint is a property of the live providers. The argument against (and
the reason this ADR rejects it): in strict mode, the Err return
must happen before any OTel SDK type is constructed, otherwise the
SDK is already wired and `init` has side effects that strict mode's
Err contract should never produce. Linting before construction is
the safer fail-fast posture.

### Option C (rejected): Codex emits the warn event directly

Codex calls `tracing::warn!` on Err and returns `Ok(())` always.
The argument: encapsulation; Spark does not need to own the warn
shape. The argument against: Codex stays emit-free at v0 (ADR-0024
§3); the consumer decides how to surface the report. Future
consumers (Aperture's lint integration in v1+, possibly Sluice's
schema validation later) may want different behaviours; locking the
warn shape inside Codex would force those consumers to filter or
re-route.

### Option D (rejected): The strict-vs-warn knob is an env var

`SIEVE_NON_ERROR_TRACE_RATE` is an env var (Sieve ADR-0021); why
not the same here? The argument against: schema-validation strict
mode is a deployment-environment decision (CI strict, prod warn)
that operators encode in their deployment config or their build
config, not in their runtime env. The builder method is the right
shape for "this property of the Spark instance, set at construction
time". Env var override could land in v1+ if operators ask.

## Consequences

### Positive

- **`#[non_exhaustive]` proves itself**: the new variant lands
  additive on Spark's existing error type without breaking
  consumers. This is the first real validation that the discipline
  ADR-0012 locked at Spark v0 works as intended.
- **Default warn means safe rollout**: operators upgrading Spark to
  consume Codex see warn events for any pre-existing
  misconfiguration but no new init failures. The ergonomic onboarding
  matches the discipline's spirit ("catch typos early, but do not
  weaponise the catch as a breaking change").
- **Lazy `OnceLock` keeps boot time bounded**: the static corpus is
  built once per process. Spark's single-init invariant (ADR-0015)
  composes cleanly: the lazy init happens during the first
  `spark::init`, before the OTel global providers are set.
- **Strict mode is opt-in CI affordance**: `with_strict_schema_lint(true)`
  in CI integration tests catches misconfigurations as
  test failures, not as warn-only events that scroll past in CI
  output.

### Negative

- **Spark's runtime closure grows by one AGPL crate**: `cargo deny`
  audits a new entry. The licence allow-list already includes AGPL
  for Aperture (Sieve consumes it) so no policy change is required;
  but the audit table extends.
- **Spark's Cargo.toml gains a path-resolved dep**: future Cargo.lock
  churn when Codex moves between versions. The exact-minor pin per
  ADR-0024's pattern keeps drift bounded.
- **Codex evolution becomes a Spark concern**: when Codex v1 ships
  (with the gRPC daemon shape per the original roadmap C.1), Spark
  needs to update; either continue the runtime-dep posture or
  switch to the daemon. v1 ADR (when it exists) settles this.

### Trade-off summary

The integration is small (one runtime dep, one variant addition,
one builder method, one OnceLock, one warn event) but
load-bearing: it is the first cross-feature Spark touch since
Spark v0.1.0 graduated. The non-breaking discipline and the
default-warn posture together protect the existing Spark consumer
base from any disruption.

## Verification

- `cargo public-api -p spark`: the new SparkError variant is the
  only addition. Under `#[non_exhaustive]` this is non-breaking;
  Gate 2 confirms.
- `cargo semver-checks -p spark`: confirms additive-only changes.
- Slice 06 integration test asserts the warn event arrives in warn
  mode and the Err return arrives in strict mode.
- Slice 06 mutation tests cover the strict-vs-warn branch in
  `init.rs` and the new variant in `error.rs`. Gate 5 100% kill
  rate target.
- The DELIVER commit lands a post-DELIVER amendment note on
  ADR-0012 and ADR-0013 documenting the change, mirroring the
  pattern from Spark's appender-tracing landing at Slice 06.
