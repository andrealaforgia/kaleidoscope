# ADR-0019 — `sieve` dependency pinning policy

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `@nw-solution-architect` (Morgan)
- **Feature**: `sieve` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

Sieve v0's runtime dependency tree is small and the licence policy is
strict: AGPL-3.0-or-later in the runtime closure is permitted (Sieve is
itself AGPL); proprietary or restrictive copyleft (GPL-2.0, LGPL,
SSPL) is forbidden.

DISCUSS Q7 locks `xxh3_64` from the `xxhash-rust` crate as the hash
function. DISCUSS does **not** lock the exact version pin, the feature
flags, or the `aperture` runtime/dev-dep posture. Sentinel's APPROVED
peer review and the slice-06 brief add three additional pinning
questions:

1. The `aperture` dependency posture: **runtime** (Sieve depends on
   Aperture's `OtlpSink` / `Probe` traits at runtime via the decorator)
   versus **dev** (only the slice tests need it). Spark's ADR-0011
   chose dev-dep because Spark is Apache-2.0 and AGPL is forbidden in
   Spark's runtime closure. Sieve is AGPL itself, so the constraint
   does not bind.
2. The `tokio` feature flag set: full runtime in dev, minimal in
   production?
3. The `tracing-subscriber` posture: dev-only (the slice-06 test
   captures events) or runtime (Sieve itself emits events but does not
   need to install a subscriber)?

This ADR closes those three questions and pins the runtime closure for
v0.

## Decision

### 1. `xxhash-rust` — exact-minor pin in v0

```toml
# crates/sieve/Cargo.toml
[dependencies]
xxhash-rust = { version = "=0.8", features = ["xxh3"] }
```

The `=0.8` operator pins to the 0.8 minor series (any 0.8.x patch is
acceptable; 0.9 is not). Rationale, with the same logic that drove
ADR-0013 §1 for `opentelemetry-otlp`:

- `xxhash-rust` is pre-1.0 (v0.8 at the time of this ADR). A minor
  bump can change algorithm constants or hash output (see the project
  changelog: 0.7 → 0.8 added const-eval support but did not change
  the hash; future minors might).
- Sieve's hash output is **observable behaviour**. Operators see the
  rate land on the same set of trace_ids run after run. A change in
  the underlying `xxh3_64` constants would silently shift which
  traces are kept. The exact-minor pin defends against that drift.
- Cargo.lock pins the exact patch transitively. Within the 0.8
  minor series, patch upgrades are absorbed without a public-behaviour
  change (the project commits to no algorithmic change within a
  minor).
- A future major upgrade is documented with a deprecation cycle: the
  Sieve crate's `CHANGELOG.md` entry will name the upstream change
  and the operator-facing impact (different traces kept run-over-run
  on the same fixture). This is a SemVer-major change on the Sieve
  crate.

### 1.1 Why `xxh3_64` and not `xxh64` or `xxh3_128`

The `xxhash-rust` crate exposes three variants under separate feature
flags: `xxh32`, `xxh64`, `xxh3` (which provides both `xxh3_64` and
`xxh3_128`). The DISCUSS Q7 locks `xxh3_64`:

- `xxh3_64` is the OTel collector's TailSamplingProcessor's choice for
  the same purpose (trace-id-keyed sampling). Aligning with that
  community precedent makes interop expectations easier.
- `xxh3_128` would give 128 bits of entropy — overkill for a
  rate-comparison decision and twice the storage cost in the future
  if Sieve ever caches decisions.
- `xxh64` is the previous-generation algorithm; `xxh3` is the
  current-generation one (better distribution at the same bit width).

The `xxh3` feature flag is the only feature Sieve enables. The
feature pulls in both `xxh3_64` and `xxh3_128`; using only the 64-bit
function is fine — the unused 128-bit code is dead code that the
Rust compiler trims.

### 2. `aperture` — runtime dependency

```toml
[dependencies]
aperture = { path = "../aperture", version = "0.1.0" }
```

Sieve depends on Aperture's public surface at runtime (the `OtlpSink`
trait, the `Probe` trait, the `SinkRecord` enum, the `SinkError` and
`ProbeError` types). The decorator `SamplingSink<S, N>` is generic
over `S: OtlpSink + Probe`; it must `impl OtlpSink + Probe` itself.
That requires the trait definitions to be in scope at compile time —
i.e. a runtime dep.

Spark cannot do this because Spark is Apache-2.0 and AGPL in Spark's
runtime closure would force Spark consumers (third-party application
code) to be AGPL too. Sieve is itself AGPL — its consumers (Aperture's
composition root, Sluice's future composition root, the slice tests)
are also AGPL. The licence is symmetric; no compliance issue.

`cargo deny check` Gate 4 already accepts AGPL-3.0-or-later in
Aperture's runtime closure (Aperture is AGPL); the same allow-rule
covers Sieve.

### 3. `async-trait` — runtime dependency, workspace-pinned

```toml
[dependencies]
async-trait = "0.1"
```

Sieve's `SamplingSink<S, N>` implements `aperture::ports::OtlpSink`,
which uses the `#[async_trait]` macro per ADR-0007 §"Decision". To
implement the trait, Sieve must depend on the same macro. The version
is workspace-permissive (caret 0.1) because `async-trait` is a stable
0.1 crate with no breaking changes since 0.1.50; the lockfile pins the
exact patch.

### 4. `tokio` — minimal feature set in production, full in tests

```toml
[dependencies]
tokio = { version = "1.40", features = ["macros", "rt", "sync", "time"] }

[dev-dependencies]
tokio = { version = "1.40", features = ["full", "test-util"] }
```

Sieve's runtime needs:

- `macros` for `#[tokio::main]` (not used by Sieve itself, but the
  `async-trait` proc-macro generates `Pin<Box<dyn Future>>` so a
  Tokio runtime is needed at the call site; Aperture's runtime is the
  one that drives Sieve, but Sieve's tests run without Aperture and
  need their own).
- `rt` (the basic runtime) and `sync` (oneshot channels for the
  timer-task cancellation per ADR-0020).
- `time` for `tokio::time::interval` (the periodic summary tick per
  ADR-0020).

The `dev-dependencies` `full` set includes `time`'s `pause()` /
`advance()` test utilities and `test-util` provides
`tokio::time::pause()` for the integration-test path that fires the
summary tick deterministically.

### 5. `tokio-util` — runtime dependency

```toml
[dependencies]
tokio-util = "0.7"
```

`tokio-util::sync::CancellationToken` is the cooperative-cancellation
primitive the timer task uses to wind down at `SamplingSink::drop`
(ADR-0020 §"Lifecycle"). Caret-0.7 pin matches the workspace's existing
posture (no other crate uses `tokio-util` yet; this is a new dep at
the workspace level). MSRV is 1.65; the workspace's `rust-version =
"1.88"` floor is well above that.

### 6. `tracing` — runtime dependency

```toml
[dependencies]
tracing = "0.1"
```

Sieve emits per-decision DEBUG events and the periodic INFO summary
via the `tracing` crate (per DISCUSS Q8 and slice-06's ACs). Caret-0.1
pin matches Aperture's existing posture. Sieve does NOT depend on
`tracing-subscriber`; that is a dev-dep only (see §8 below).

### 7. `thiserror` — runtime dependency

```toml
[dependencies]
thiserror = "2"
```

`SieveConfigError` derives `thiserror::Error`. Caret-2 pin; the
workspace's existing crates use `thiserror = "2"` for the same
pattern (ADR-0007 §"Decision" for SinkError, ADR-0012 for SparkError).

### 8. `opentelemetry-proto` — workspace-pinned

```toml
[dependencies]
opentelemetry-proto.workspace = true
```

Sieve's `TraceView` borrows `&'a opentelemetry_proto::tonic::trace::v1::Span`
from the upstream OTLP envelope. The workspace declares
`opentelemetry-proto = "=0.27.0"` per ADR-0003; Sieve inherits that
exact pin via `.workspace = true`. This guarantees Sieve sees the same
span shape Aperture and the harness see.

### 9. `tracing-subscriber` — dev-dep only

```toml
[dev-dependencies]
tracing-subscriber = { version = "0.3", default-features = false, features = ["fmt", "env-filter", "registry"] }
```

The slice-06 integration test installs a `tracing_subscriber::Registry`
to capture `target = "sieve"` events and assert the vocabulary.
Production Sieve does NOT install a subscriber — Aperture's binary
(or Spark's `init`, when Spark is in scope) owns the global subscriber.
Sieve emits events; the consumer collects them.

### Licence audit table (runtime closure)

| Crate | Version pin | Licence | Notes |
|-------|-------------|---------|-------|
| `aperture` | `=0.1.0` (path dep) | AGPL-3.0-or-later | symmetric with Sieve; allowed |
| `async-trait` | caret `0.1` | MIT OR Apache-2.0 | permissive |
| `opentelemetry-proto` | `=0.27.0` (workspace) | Apache-2.0 | permissive |
| `prost` (transitive via opentelemetry-proto) | caret `0.13` (workspace) | Apache-2.0 | permissive |
| `thiserror` | caret `2` | MIT OR Apache-2.0 | permissive |
| `tokio` | caret `1.40` | MIT | permissive |
| `tokio-util` | caret `0.7` | MIT | permissive |
| `tonic` (transitive via opentelemetry-proto) | caret (workspace) | MIT | permissive |
| `tracing` | caret `0.1` | MIT | permissive |
| `xxhash-rust` | `=0.8`, feature `xxh3` | BSL-1.0 OR MIT | permissive (BSL-1.0 is the Boost Software Licence; OSI-approved; permissive in spirit and letter — see https://www.boost.org/LICENSE_1_0.txt) |

The dev-dep closure adds `tracing-subscriber` (MIT) and `tokio`'s
`full` and `test-util` features (still MIT). No new licences enter the
runtime closure beyond AGPL-3.0-or-later (Sieve and Aperture; allowed)
and the four permissive families (MIT, Apache-2.0, BSL-1.0, MIT OR
Apache-2.0).

`cargo deny check` Gate 4 needs one new entry in `deny.toml`'s
`[licenses] allow` list:

```toml
# Boost Software Licence; permissive; OSI-approved. Used by
# `xxhash-rust` per ADR-0019 §1.
allow = [
    # ... existing entries ...
    "BSL-1.0",
]
```

(The exact `deny.toml` syntax is the platform-architect's territory
in DEVOPS; this ADR documents the licence choice and the rationale.)

### Why no `rand`

DISCUSS Q7 locks `xxh3_64(trace_id)` as the deterministic mapping.
Slice 03's brief mentions a possible `rand`-crate dep with an
injected probability source for tests; ADR-0018 §"`HeadSampler::sample`
mechanism" notes that the hash IS the probability source, so no `rand`
crate is needed. This keeps the runtime closure smaller and the hot
path simpler.

### Why no `serde`

Sieve does not parse or serialise structured config. The single env
var `SIEVE_NON_ERROR_TRACE_RATE` is parsed via `f64::from_str` from
the standard library; `SIEVE_SUMMARY_TICK_MS` (ADR-0020 §"Tick
interval") is parsed via `u64::from_str`. No structured config means
no `serde`; the runtime closure is correspondingly leaner.

## Alternatives Considered

### Option A — `=0.8` xxhash-rust pin + runtime aperture + workspace-permissive support deps (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Tight pin where pin matters (xxhash-rust, opentelemetry-proto via
  workspace), permissive elsewhere.
- AGPL-clean: aperture as runtime dep is fine because Sieve is itself
  AGPL.
- Explicit feature flags on tokio keep the runtime closure minimal.
- One new licence (BSL-1.0) added to `deny.toml`'s allow list,
  documented here.

**Cons**:
- `=0.8` pin means a 0.9 release of xxhash-rust forces a Sieve-side
  ADR amendment. Acceptable: the hash is observable behaviour, and a
  major upstream change should be a deliberate Sieve-side decision.

### Option B — `=0.8.13` xxhash-rust patch pin (mirroring harness ADR-0003's exact-patch pin on opentelemetry-proto)

**Pros**:
- Maximum reproducibility: even a patch upgrade requires a Sieve-side
  ADR amendment.

**Cons**:
- Patch upgrades within 0.8 are pure bug fixes (the upstream commits
  to no algorithmic change within a minor). Pinning to the patch
  forces Sieve to chase patches that have no semantic effect.
- Harness ADR-0003's exact-patch pin is right for the harness because
  the harness defends the wire format byte-for-byte against the OTel
  spec. Sieve's job is to *use* the hash, not to defend its byte
  output across the wire. The minor pin is the right shape here, the
  same way Spark's ADR-0013 §1 chose `=0.27` over `=0.27.0`.

**Rejected** for the same reason ADR-0013 §1 rejected exact-patch pin
for OTel SDK family: patch upgrades in pre-1.0 crates that commit to
no-algorithmic-change-within-minor are bug-fix absorption.

### Option C — `aperture` as dev-dep only + sieve exposes a generic `Sink` trait independent of Aperture

**Pros**:
- The `sieve` crate would have no runtime dependency on `aperture`,
  keeping the dependency edge invisible.
- A future non-Aperture consumer could depend on Sieve without
  pulling Aperture into its tree.

**Cons**:
- Speculative generality (same critique as ADR-0018 Option E).
  There is no second pipeline consumer at v0; there may never be one.
- Sieve would need its own parallel `Sink` trait with the same shape
  as Aperture's `OtlpSink`. The decorator would now wrap a
  `sieve::Sink` and adapter code in Aperture's composition root would
  bridge `aperture::OtlpSink` to `sieve::Sink`. Two extra adapter
  trait impls for no behavioural gain.

**Rejected** for premature abstraction.

### Option D — Tokio with the `full` feature in production

```toml
tokio = { version = "1.40", features = ["full"] }
```

**Pros**:
- One feature flag covers everything.

**Cons**:
- The `full` feature pulls in `process`, `signal`, `fs`, `net` —
  none of which Sieve uses. Compile time and binary size grow for
  no behavioural benefit.
- Aperture's existing `Cargo.toml` enables features explicitly; Sieve
  matches that posture for consistency.

**Rejected** for the unnecessary build cost.

## Consequences

### Positive

- Runtime closure is small (eight crates plus their transitives via
  `opentelemetry-proto`'s tree). Build time is short; binary size is
  small.
- Every licence in the runtime closure is permissive or symmetric
  AGPL. `cargo deny` Gate 4 has one new entry (`BSL-1.0`) and
  documented rationale.
- Pin policy mirrors ADR-0013 §1 (Spark's OTel SDK pin) for the
  same reason: pre-1.0 minor pin where the upstream commits to no
  algorithmic change within a minor.
- The `xxh3_64` choice aligns with the OTel collector's
  TailSamplingProcessor for interop expectations.

### Negative

- A 0.9 release of `xxhash-rust` requires a Sieve-side ADR amendment
  and a public-behaviour-changing release (different trace_ids may
  cross the rate boundary on the same fixture). This is the price of
  honest behaviour-stability documentation; the alternative is silent
  drift.
- The runtime `aperture` dep means Sieve cannot be reused outside an
  Aperture-bearing workspace. This is a v0 trade-off; Option C above
  documents the path to break the dependency if a v1 consumer
  emerges.

### Trade-off ATAM

This decision is a **sensitivity point** for **Compatibility —
Interoperability**: pinning `xxhash-rust` to a minor version means the
hash output is stable across patches but a major upgrade is a
behaviour-changing release. The trade is accepted because hash
stability across releases is the operator-visible contract Sieve
makes — operators reading the periodic INFO summary expect the same
trace_ids to be kept run-after-run.

It is a **sensitivity point** for **Maintainability — Modifiability**
in the positive direction: the small dependency closure plus
permissive licences make it cheap to add features (a future scrubbing
stage in v1, a tail-sampling window in v1+) without licence-policy
churn.

## Self-Application of Earned Trust (principle 12)

The pin policy is enforced by three mechanisms:

1. **Subtype check (compile-time)** — `cargo build --locked` (Gate 1)
   refuses to proceed if `Cargo.lock` would be modified. The
   exact-minor pin on `xxhash-rust` is locked at the manifest level;
   the lockfile pins the exact patch.
2. **Structural check (CI)** — `cargo deny check` (Gate 4) refuses
   any commit that introduces an unlisted licence in the runtime
   closure (the `BSL-1.0` allow entry is the only way `xxhash-rust`
   passes; a hypothetical future fork under a different licence would
   fail the gate).
3. **Behavioural check (CI)** — Slice 04's determinism test asserts
   that the same `trace_id` always yields the same `Decision` under
   the same `HeadSampler`. A silent algorithmic change in
   `xxhash-rust` between patches would not flip the per-trace
   decision (the algorithm constants do not change within a minor),
   but a hypothetical patch that did would be caught by the
   slice-03's distribution test (the kept-count would shift outside
   the ±3% band on the deterministic fixture).

A change that bypasses one layer is caught by another. The
hash-output stability is therefore enforced by the pin AND by the
fixture-based test; together they form the Earned-Trust contract for
the dependency.
