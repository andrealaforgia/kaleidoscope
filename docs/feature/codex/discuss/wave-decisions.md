# Codex v0 — Wave Decisions

Locked scope decisions for the Codex schema-authority library at v0.
v0 ships as a single AGPL-3.0-or-later Rust crate (`crates/codex`)
exposing a `SchemaCatalogue` API that codifies the OTel semconv version
Kaleidoscope pins plus the Kaleidoscope-house attributes. Spark's
resource-attribute lint integrates via direct in-process API call.

CUE / Protobuf descriptor endpoints, FoundationDB-backed multi-version
storage, the gRPC service surface and HTML rendering all land at v1+.

## Q1 — Library inside the workspace, or separate gRPC service binary at v0?

**Decision**: Library at v0. New crate `crates/codex` published inside
the workspace under AGPL-3.0-or-later. Spark consumes Codex via direct
Rust API call.

**Rationale**: The v0 use case is Spark's resource-attribute lint at
`spark::init` time — an in-process call. A network round-trip to a
Codex daemon adds operational surface (separate binary, separate port,
FoundationDB dependency, mTLS via SPIFFE) without product value at v0.
Service shape becomes valuable when multi-version negotiation and
multi-tenant catalogues arrive; at v0 there is exactly one consumer
(Spark) and exactly one pinned semconv version.

**Rejected alternative**: gRPC service from v0. Carries deployment cost
ahead of demand, and forces premature commitment to wire-protocol shape
before v1+ feature pressure has shown what the protocol must carry.

## Q2 — Schema corpus shape at v0?

**Decision**: Hand-written Rust constants. A `static` slice of
`BlessedAttribute` records, generated from the upstream
`opentelemetry-semantic-conventions` crate version Kaleidoscope
already pins, plus the three Kaleidoscope-house attributes spelled
out by name. No CUE, no Protobuf descriptors, no Git-backed corpus.

**Rationale**: The v0 corpus is small (low hundreds of attributes) and
static (one pinned semconv version, three house attributes). Rust
constants give zero-cost lookup, compile-time verification of the
corpus shape, and no runtime dependencies. The validation power CUE
offers (constraints, defaults, type unions) has no v0 consumer because
v0 does not yet validate attribute *values*, only attribute *names*.

**Rejected alternative**: CUE-based corpus from v0. The CUE dependency
is heavy (toolchain, embedded runtime evaluation) and v0 does not
exercise CUE's strengths. Move to CUE when per-tenant extensions
arrive and the corpus stops being a static set.

## Q3 — OTel semconv version: single pinned, or multi-version?

**Decision**: Single pinned version, mirroring the harness's
`opentelemetry-proto` family pin (currently `=0.27`). Codex pins
`opentelemetry-semantic-conventions = "=0.27.x"` with the same exact
discipline that Aperture and Spark already use for the proto family.

**Rationale**: Multi-version negotiation arrives when multiple consumers
run on different SDK versions. At v0 Spark and Aperture are both pinned
to the same family, so Codex pinning to that family closes the loop.
Single-version is the smallest useful library that closes the lint loop
with Spark.

**Rejected alternative**: Multi-version from v0. Adds a version-
resolution surface and migration semantics that have no v0 consumer.
Multi-version is on the roadmap when FoundationDB-backed storage and a
service shape arrive together.

## Q4 — Tenant extensions at v0?

**Decision**: Defer to v1+. v0 ships only the global OTel semconv corpus
plus the three Kaleidoscope-house attributes (`tenant.id`,
`feature_flag.*`, `experiment.id`). No per-tenant overlay, no extension
API surface, no stub.

**Rationale**: The roadmap text "plus per-tenant schema extensions"
needs a tenant catalogue (Aegis) which does not exist at v0. Shipping
a stub extension API now bloats the surface without a consumer and
locks in a shape that the eventual Aegis integration may want to
revise. The closest blessed-match diagnostic in Q5 already gives Spark
developers a friendly path when they need a missing attribute — they
can request its inclusion in the global corpus.

**Rejected alternative**: Ship a stub tenant-extension API. Surface
bloat with no consumer. Better to design the extension shape against
real Aegis requirements than against a guess.

## Q5 — Lint diagnostics shape?

**Decision**: A `LintReport` value containing zero or more
`LintViolation` records. Each violation carries:

- `attribute_name: String` — the offending attribute as supplied
- `kind: ViolationKind` — `Unknown`, `Deprecated`, or `Misnamed`
- `nearest_blessed_match: Option<String>` — fuzzy suggestion when
  the unknown attribute is within Levenshtein distance ≤ 2 of a
  blessed name

Returned via `SchemaCatalogue::validate(resource_attributes) -> Result<(), LintReport>`.
`LintReport` implements `Display` for operator-friendly messages and
`std::error::Error` for ergonomic propagation.

**Rationale**: Structured detail is what Spark's developers need at
integration time. A single error type with one attribute name loses the
multi-violation case (a Resource with three typos surfaces three
violations in one pass, not three repeated init failures) and loses the
"did you mean" hint that turns a five-minute hunt into a five-second
fix. The `kind` enum lets Spark distinguish "attribute does not exist"
from "attribute exists but is deprecated" in future slices, even though
v0 only populates `Unknown`.

**Rejected alternative**: A single `LintError` enum variant carrying
just the offending attribute name. Loses the structured detail and
forces multiple init attempts to surface multiple violations.

## Q6 — Spark integration mechanism?

**Decision**: Spark adds a runtime dep on `codex` and calls
`SchemaCatalogue::validate(...)` inside `spark::init` after Resource
composition. On any `LintViolation`, behaviour is configurable:

- **Default (warn)**: emit a `tracing::warn!` event carrying the
  `LintReport`; `spark::init` returns `Ok`.
- **Opt-in (strict)**: `spark::init` returns
  `Err(SparkError::SchemaValidation(LintReport))`.

`SparkError` gains a new variant `SchemaValidation(LintReport)`.
The variant is added under the existing `#[non_exhaustive]` annotation
on `SparkError`, so the addition is non-breaking for downstream
matchers.

**Rationale**: In-process validation at init time catches the mistake
at the moment it would matter — when the SDK is composing the Resource
that will be attached to every span and metric for the lifetime of the
process. The default-warn posture means rolling Codex into Spark does
not break existing deployments; teams opt into strict mode when they
have finished cleaning up their Resource attributes. The non-exhaustive
SparkError shape means the new variant lands without a breaking-change
ADR.

**Rejected alternative**: A separate `cargo codex-lint` CLI tool.
Adds tooling burden (a binary to build, ship, document, version) and
shifts the check out of the path where the mistake is made. CI lint
tools get bypassed; in-process init checks do not.

## Cross-cutting commitments

- **Licence**: `crates/codex` is AGPL-3.0-or-later, matching the rest
  of the Kaleidoscope server-side surface.
- **MSRV**: tracks the workspace MSRV; no per-crate floor.
- **Pin discipline**: `opentelemetry-semantic-conventions = "=0.27.x"`
  exact-patch, mirroring Aperture's proto-family discipline.
- **Mutation testing**: Gate 5 (100% kill rate on modified files)
  applies to Codex from Slice 01.
- **House style**: British English in user-facing diagnostics, ADRs,
  and rustdoc.

---

## Q7 — Corpus regeneration ritual

**Decision (default chosen): checked-in generated Rust file at
`crates/codex/src/generated/{...}.rs`, regenerated by a maintainer
ritual when the semconv pin moves.**

The semconv corpus has hundreds of attribute names; codifying it as
checked-in Rust constants makes the diff visible in pull requests
when the pin moves. A maintainer running the regeneration script
sees the delta in the PR review; nothing changes silently in CI.
Build-time regeneration via `build.rs` would hide that delta and
slow incremental builds for every workspace consumer.

**Rejected alternative: `build.rs` script regenerating from the
upstream semconv repo at compile time.** Build-speed regression for
every developer; the PR diff visibility loss is the bigger cost.

---

## Q8 — Fuzzy-match dependency

**Decision (default chosen): in-tree Levenshtein implementation,
~30 lines, no external dependency.**

The corpus is small (a few hundred attributes). The algorithm is
straightforward. AGPL-3.0-or-later raises the bar on adding any new
runtime dependency that does not pay for itself; an in-tree
implementation pays once, in code Bea or the architect can read at
review time, and then stays inert.

**Rejected alternative: `strsim` crate (MIT) or similar.** The crate
is fine, but adds one more line to the licence audit table and one
more transitive dependency to the supply chain for a 30-line
algorithm.

---

## Q9 — Warn-mode tracing shape (default warn, opt-in strict)

**Decision (default chosen): a single `tracing::warn!` event per
`spark::init` call, carrying the full `LintReport` via its `Display`
impl.**

A multi-violation report renders as a single human-readable
multi-line message. Operators see one warn event per misconfigured
init, not one per offending attribute. This matches Sieve's "one
DEBUG event per decision plus one INFO summary per window" pattern:
the routine state is one event at default verbosity; per-detail
events are a future opt-in (DEBUG-level per-violation events would
fit naturally if a consumer asks for them).

**Rejected alternative: one `tracing::warn!` per `LintViolation`.**
Operationally noisy when a misconfiguration produces several
violations at once; dashboards counting "warn events" would
double-count a single bad init.

---

## Out of scope for v0

The roadmap C.1 describes the full Codex as a gRPC service over
FoundationDB serving CUE schemas, Protobuf descriptors, and HTML
renderings. v0 deliberately ships none of that:

- gRPC / HTTP service surface (v1+).
- FoundationDB-backed multi-version catalogue (v1+).
- CUE-based schema corpus (v1+).
- Per-tenant schema extensions (needs Aegis; v1+).
- HTML rendering of the spec (v1+).
- Aperture-side lint integration (a follow-up feature, not part of
  Codex v0).

v0 is the smallest useful library that closes the lint loop with
Spark.
