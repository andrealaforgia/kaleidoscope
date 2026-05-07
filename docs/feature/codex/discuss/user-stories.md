# Codex v0 — user stories

Six LeanUX user stories with mandatory Elevator Pitches per the nWave
DISCUSS template. Personas drawn from `acme-observability`, the same
fictional team Sieve and Spark have been built for.

The principal user is **Sasha, a platform engineer** integrating
Spark into a service. Sasha's job is to wire telemetry without
pushing a misconfigured Resource into production. Secondary user is
**Riley, an SRE** triaging an incident; Riley relies on every
emitted attribute being recognised by downstream tooling, and a
misnamed attribute is exactly the kind of silent failure that costs
debugging time at three in the morning.

System constraints (apply to every story):

1. Library, not service. Codex v0 is a Rust crate Spark consumes via
   direct API call; not a gRPC daemon (per Q1).
2. AGPL-3.0-or-later. Same rationale as Aperture and Sieve per
   `LICENSING.md`.
3. Schema corpus is hand-written Rust constants generated from
   upstream `opentelemetry-semantic-conventions =0.27` (per Q2).
4. Single semconv version pinned (no multi-version negotiation at v0
   per Q3).
5. No per-tenant overlays at v0 (per Q4); the catalogue is global
   plus the three Kaleidoscope-house attributes.
6. `LintReport` carries one or more `LintViolation`s, each with
   attribute name, `ViolationKind`, and `nearest_blessed_match` (per
   Q5).
7. Spark integration: `codex` runtime dep on Spark; `spark::init`
   calls `SchemaCatalogue::validate(...)` after Resource composition;
   default-warn / opt-in-strict (per Q6).
8. `feature_flag.{key}` honours the prefix-and-arbitrary-suffix
   shape per Spark's existing convention (per Slice 03).
9. Single `tracing::warn!` event per init in warn mode, carrying the
   full report via Display (per Q9).
10. Public surface keeps the lint loop minimal: `SchemaCatalogue`,
    `BlessedAttribute`, `LintReport`, `LintViolation`,
    `ViolationKind`. No re-exports of upstream types.

---

## US-CO-01 — Walking skeleton: catalogue exists and validates a canonical pair

### Elevator Pitch

- **Before**: there is no Codex code; Spark's resource-attribute lint
  checks only `service.name`; an operator typing `tenat.id` instead
  of `tenant.id` ships the typo all the way to Aperture's recording
  sink.
- **After**: run `cargo test -p codex --test slice_01_walking_skeleton` →
  sees a GREEN result that proves `SchemaCatalogue::new()` returns a
  catalogue containing at least one OTel attribute (`service.name`)
  and one Kaleidoscope-house attribute (`tenant.id`); `validate(&[
  ("service.name", "payments-api"), ("tenant.id", "acme-prod") ])`
  returns `Ok(())`.
- **Decision enabled**: Sasha confirms Codex's contract has shape and
  the team can build the rest of the catalogue plus the lint paths
  on top.

### Problem

A schema authority that nobody has run end-to-end is conjecture. The
walking skeleton is the smallest unit of evidence that the catalogue
type compiles, the validate call compiles, and the happy path
returns `Ok(())`.

### Who

- Sasha, platform engineer: needs proof of concept before approving
  further slices.

### Solution

Define a `SchemaCatalogue` struct with a `new()` constructor that
returns a catalogue containing two seeded entries (`service.name`,
`tenant.id`) and a `validate(attrs)` method returning
`Result<(), LintReport>`. Walking-skeleton test asserts the success
path on a canonical attribute pair.

### Domain examples

1. **Acme prod canonical**: Sasha calls `SchemaCatalogue::new()` and
   `validate(&[("service.name", "payments-api"), ("tenant.id",
   "acme-prod")])`. Returns `Ok(())`.
2. **Empty input**: `validate(&[])` returns `Ok(())` (zero violations
   on zero input is the boundary case).

### UAT scenarios (BDD)

#### Scenario: A canonical attribute pair validates clean

```
Given a SchemaCatalogue::new()
And the attribute pair [("service.name", "payments-api"), ("tenant.id", "acme-prod")]
When validate is called
Then the result is Ok(())
```

#### Scenario: An empty attribute set validates clean

```
Given a SchemaCatalogue::new()
And no attributes
When validate is called
Then the result is Ok(())
```

### Acceptance Criteria

- [ ] `crates/codex/Cargo.toml` declares the workspace member with `license = "AGPL-3.0-or-later"`.
- [ ] `SchemaCatalogue` is a `pub struct` with a `pub fn new() -> Self` constructor.
- [ ] `validate(&self, attrs: &[(&str, &str)]) -> Result<(), LintReport>` is the public entry point.
- [ ] At least `service.name` and `tenant.id` are blessed in the v0 catalogue.

### Outcome KPIs

- 100% of canonical-attribute-pair fixtures yield `Ok(())` (CI invariant).
- Walking-skeleton test wall time under 1 second.

### Technical Notes

DESIGN-wave decisions: exact shape of `BlessedAttribute` (a struct or
an enum?); whether the catalogue is `&'static` or owned; whether
`validate` takes `&[(&str, &str)]` or accepts an `IntoIterator` for
ergonomic flexibility.

---

## US-CO-02 — Full OTel semconv 0.27 corpus is blessed

### Elevator Pitch

- **Before**: the catalogue contains two seed attributes; an
  application emitting standard OTel resource attributes (e.g.
  `host.name`, `process.pid`, `telemetry.sdk.language`) trips the
  lint as if they were typos.
- **After**: `cargo test -p codex --test slice_02_otel_semconv_corpus`
  → sees a GREEN test asserting that every resource attribute in
  upstream `opentelemetry-semantic-conventions =0.27` validates
  clean; the catalogue is complete against the pinned semconv.
- **Decision enabled**: Sasha trusts that wiring an off-the-shelf
  OTel SDK against Codex does not produce false positives.

### Problem

Operators integrate against the OTel semantic conventions all the
time. A lint that flags every standard attribute as "unknown" is
worse than no lint; it pollutes signal with noise and trains
developers to ignore Codex output.

### Who

Sasha, platform engineer: needs the lint to be accurate against
mainstream OTel usage.

### Solution

Generate `crates/codex/src/generated/semconv_0_27.rs` from the
upstream `opentelemetry-semantic-conventions` crate via a maintainer
ritual (script + checked-in artefact per Q7). The generated file
contains a `static` slice of `BlessedAttribute` records; the
catalogue's `new()` constructor seeds itself from this slice.

### Domain examples

1. **Standard OTel application**: `validate(&[("service.name",
   "payments-api"), ("host.name", "node-01"), ("process.pid",
   "12345"), ("telemetry.sdk.language", "rust")])` returns `Ok(())`.
2. **Mixed standard + house**: standard OTel attributes plus
   Kaleidoscope-house `tenant.id` validate together.

### UAT scenarios (BDD)

#### Scenario: A complete OTel semconv resource attribute set validates clean

```
Given a SchemaCatalogue::new()
And a fixture set of every OTel semconv 0.27 resource attribute
When validate is called
Then the result is Ok(())
```

#### Scenario: A mixed standard + house attribute set validates clean

```
Given a SchemaCatalogue::new()
And the attributes [("service.name", "..."), ("host.name", "..."), ("tenant.id", "...")]
When validate is called
Then the result is Ok(())
```

### Acceptance Criteria

- [ ] The generated `semconv_0_27.rs` file is checked into the repo and visible in PR diffs when the semconv pin moves.
- [ ] Every resource-class attribute in upstream `opentelemetry-semantic-conventions =0.27` is blessed.
- [ ] A fixture exercising the full upstream corpus passes `validate`.

### Outcome KPIs

- 100% of upstream OTel semconv 0.27 resource attributes blessed (CI invariant).

### Technical Notes

DESIGN: the regeneration script's exact shape (a small Rust binary?
A bash + perl one-liner?). The script is operator infrastructure,
not part of the public crate surface.

---

## US-CO-03 — Kaleidoscope-house attributes are first-class

### Elevator Pitch

- **Before**: `tenant.id` is hardcoded; `experiment.id` and
  `feature_flag.{key}` are not in the catalogue at all.
- **After**: `cargo test -p codex --test slice_03_house_attributes`
  → sees Spark's three house attributes blessed in the catalogue;
  `feature_flag.{key}` validates for any non-empty `{key}` suffix
  (e.g. `feature_flag.checkout-v2`, `feature_flag.dark-mode`).
- **Decision enabled**: Sasha emits Spark's full Resource through
  Codex and gets `Ok(())`.

### Problem

Spark's three house attributes (`tenant.id`, `feature_flag.{key}`,
`experiment.id`) need to round-trip cleanly through Codex's lint.
The `feature_flag.{key}` shape is special: any non-empty suffix is
valid, since `{key}` is application-supplied.

### Who

Sasha, platform engineer: needs Spark's emitted Resource to validate
without false positives.

### Solution

Add the three house attributes to the catalogue. `tenant.id` and
`experiment.id` are exact-match. `feature_flag.{key}` is a prefix
rule: any attribute name starting with `feature_flag.` followed by
a non-empty suffix is blessed.

### Domain examples

1. **Spark canonical Resource**: `validate(&[("service.name", ...),
   ("tenant.id", ...), ("feature_flag.checkout-v2", "on"),
   ("experiment.id", "exp-2026-Q2-pricing")])` returns `Ok(())`.
2. **Multiple feature flags**: two `feature_flag.*` entries both
   pass.

### UAT scenarios (BDD)

#### Scenario: All three house attributes validate clean

```
Given a SchemaCatalogue::new()
And the attributes [("tenant.id", "acme-prod"), ("feature_flag.checkout-v2", "on"), ("experiment.id", "exp-Q2")]
When validate is called
Then the result is Ok(())
```

#### Scenario: A feature_flag prefix with arbitrary suffix validates clean

```
Given a SchemaCatalogue::new()
And the attribute ("feature_flag.dark-mode", "off")
When validate is called
Then the result is Ok(())
```

#### Scenario: A feature_flag prefix with empty suffix is rejected

```
Given a SchemaCatalogue::new()
And the attribute ("feature_flag.", "on")
When validate is called
Then the result is Err with a LintReport containing one violation for "feature_flag."
```

### Acceptance Criteria

- [ ] `tenant.id` and `experiment.id` blessed as exact-match attributes.
- [ ] `feature_flag.{suffix}` accepts any non-empty suffix.
- [ ] An empty `feature_flag.` (no suffix) is rejected.

### Outcome KPIs

- 100% of Spark canonical Resources validate clean (CI invariant).

### Technical Notes

DESIGN: prefix matching shape — a `BlessedAttribute::ExactMatch(name)`
and `BlessedAttribute::Prefix(name)` enum, or a single struct with a
`MatchKind` field? Pick the simplest that the catalogue iteration
loop can express cleanly.

---

## US-CO-04 — Unknown attributes produce structured violations

### Elevator Pitch

- **Before**: Spark only checks `service.name`; an operator typing
  `tenat.id` (typo) ships the broken attribute to Aperture, where
  downstream tools either drop it or, worse, render it as a separate
  legitimate column.
- **After**: `cargo test -p codex --test slice_04_unknown_attribute_lint`
  → sees `validate(&[("tenat.id", "acme-prod")])` return
  `Err(LintReport)` whose single `LintViolation` carries
  `name = "tenat.id"`, `kind = ViolationKind::Unknown`, and
  `nearest_blessed_match = None` (slice 05 fills in the suggestion).
- **Decision enabled**: Sasha sees the misconfiguration at lint time
  rather than chasing a missing column in Riley's dashboard.

### Problem

Typos and unrecognised attributes ship silently today. The lint must
surface them with enough structured detail that the developer fixes
the offending name without guessing.

### Who

Sasha, platform engineer.

### Solution

When `validate` finds an attribute not in the catalogue, build a
`LintViolation { name, kind: Unknown, nearest_blessed_match: None }`
and add it to a `LintReport`. Multiple unknown attributes produce
multiple violations in one report. The report's `Display` impl
renders one human-readable line per violation.

### Domain examples

1. **Typo on `tenant.id`**: `validate(&[("tenat.id", "acme-prod")])`
   → `Err(LintReport)` with one violation.
2. **Two typos at once**: `validate(&[("tenat.id", "..."),
   ("svc.name", "...")])` → `Err(LintReport)` with two violations.
3. **Attribute name not in any allowed namespace**: `validate(&[
   ("acme.totally-custom", "x")])` → `Err(LintReport)` with one
   violation.

### UAT scenarios (BDD)

#### Scenario: A single unknown attribute produces one LintViolation

```
Given a SchemaCatalogue::new()
And the attribute ("tenat.id", "acme-prod")
When validate is called
Then the result is Err(LintReport)
And the report contains exactly one LintViolation
And the violation's name is "tenat.id"
And the violation's kind is ViolationKind::Unknown
```

#### Scenario: Multiple unknown attributes produce multiple violations

```
Given a SchemaCatalogue::new()
And the attributes [("tenat.id", "..."), ("svc.name", "...")]
When validate is called
Then the result is Err(LintReport)
And the report contains exactly two LintViolations
```

#### Scenario: The LintReport's Display impl renders human-readable text

```
Given a LintReport with two LintViolations
When the report is rendered via Display
Then the output names each offending attribute
And the output mentions ViolationKind for each
```

### Acceptance Criteria

- [ ] `LintReport` is a `pub struct` exposing `violations()` (slice or iterator).
- [ ] `LintViolation` is a `pub struct` with `name`, `kind`, `nearest_blessed_match`.
- [ ] `ViolationKind` is a `pub enum`, non-exhaustive, with at least an `Unknown` variant.
- [ ] `validate` returns `Err(LintReport)` containing one violation per offending attribute.
- [ ] `LintReport::Display` renders human-readable output naming each violation.

### Outcome KPIs

- 100% of unknown-attribute fixtures yield exactly one violation per offending attribute (CI invariant).

### Technical Notes

DESIGN: should `validate` short-circuit on the first violation, or
collect all? Collect all — operators want one round-trip to know
all the problems. Short-circuit semantics can be a future opt-in.

---

## US-CO-05 — Fuzzy "did you mean" suggestions

### Elevator Pitch

- **Before**: an unknown attribute violation tells Sasha "tenat.id"
  is unrecognised but not what the developer probably meant.
- **After**: `cargo test -p codex --test slice_05_fuzzy_suggestions`
  → sees that `validate(&[("tenat.id", "...")])` returns a
  `LintReport` whose single violation has `nearest_blessed_match =
  Some("tenant.id")`; common typos like `svc.name` → `Some("service.name")`
  surface their nearest match.
- **Decision enabled**: Sasha reads the violation message, sees "did
  you mean tenant.id?", and corrects the typo without guessing.

### Problem

Operators making typos need a "did you mean" pointer. Without it the
lint is a stricter version of "unknown" without a recovery path.

### Who

Sasha, platform engineer.

### Solution

Compute Levenshtein distance from the offending attribute name to
every blessed attribute name in the catalogue (in-tree
implementation per Q8). When the minimum distance is ≤ 2 (a typical
"close typo" threshold), populate `nearest_blessed_match =
Some(closest_name)`. When the minimum distance is > 2, leave
`nearest_blessed_match = None`.

### Domain examples

1. **`tenat.id` → `tenant.id`**: distance 1 (insert `n`); suggested.
2. **`svc.name` → `service.name`**: distance 4; not suggested
   (above threshold). Operator sees the violation but no suggestion.
3. **`feature_flag.checkout-v2` typo: `feature-flag.checkout-v2`** →
   distance 1 (substitute `_` for `-`); the suggestion logic notes
   the prefix family.

### UAT scenarios (BDD)

#### Scenario: A close typo produces a suggestion

```
Given a SchemaCatalogue::new()
And the attribute ("tenat.id", "acme-prod")
When validate is called
Then the result is Err(LintReport)
And the violation's nearest_blessed_match is Some("tenant.id")
```

#### Scenario: A far-distance attribute produces no suggestion

```
Given a SchemaCatalogue::new()
And the attribute ("acme.totally-custom", "x")
When validate is called
Then the result is Err(LintReport)
And the violation's nearest_blessed_match is None
```

#### Scenario: A feature_flag prefix typo suggests the prefix family

```
Given a SchemaCatalogue::new()
And the attribute ("feature-flag.checkout-v2", "on")
When validate is called
Then the result is Err(LintReport)
And the violation's nearest_blessed_match is Some("feature_flag.{key}") or similar
```

### Acceptance Criteria

- [ ] `nearest_blessed_match` is populated when min Levenshtein distance ≤ 2.
- [ ] `nearest_blessed_match` is `None` when min distance > 2.
- [ ] The Levenshtein implementation is in-tree (no `strsim` or similar dep).
- [ ] Common typo fixtures (`tenat.id`, `service.nme`) surface the right suggestion.

### Outcome KPIs

- 100% of close-typo fixtures (distance ≤ 2) yield a populated `nearest_blessed_match` (CI invariant).
- Levenshtein computation completes in under 10 ms for a corpus of ~400 entries on typical hardware.

### Technical Notes

DESIGN: prefix-family suggestion shape (e.g. `feature_flag.{key}`)
needs a string template to render. The suggestion text could be the
literal prefix-pattern or the offending attribute with the corrected
prefix. Pick the friendlier one.

---

## US-CO-06 — Spark integrates Codex as the resource-attribute lint

### Elevator Pitch

- **Before**: Spark's `init` checks only `service.name`; a typo on
  `tenant.id` ships through to Aperture without warning.
- **After**: Spark's `init` calls `SchemaCatalogue::validate(...)`
  after Resource composition. In the default `warn` mode, a
  `LintReport` produces one `tracing::warn!(target = "spark"...)`
  event carrying the report's full text. In opt-in `strict` mode,
  init returns `Err(SparkError::SchemaValidation(LintReport))` and
  the application's startup fails fast.
- **Decision enabled**: Sasha gets immediate feedback at integration
  time; Riley reads the warn line in the operator log aggregator
  and never sees a typo'd attribute slip through to her dashboard.

### Problem

The lint is only useful if Spark consumes it. Without integration,
Codex is a library that nobody calls.

### Who

Sasha (developer integration), Riley (incident triage).

### Solution

Add `codex` as a runtime dependency of `crates/spark`. Extend
`SparkConfig` with a `with_strict_schema_lint(bool)` builder method
(default `false`, i.e. warn mode). In `spark::init`, after Resource
composition, call `SchemaCatalogue::validate` against the assembled
attributes. On `Err(report)` in warn mode, emit one
`tracing::warn!` event with the report's `Display` rendering. In
strict mode, return `Err(SparkError::SchemaValidation(report))`. The
new `SparkError` variant is additive on the `#[non_exhaustive]` enum
and does not break consumers.

### Domain examples

1. **Acme prod (warn mode default)**: Sasha calls
   `SparkConfig::for_service(...).with_tenant_id(...)
   .with_feature_flags([("checkout-v2", "on")])` — happens to
   misname `tenant.id` as `tenat.id` somewhere in the pipeline.
   `init` returns `Ok(SparkGuard)`. Stderr shows one `WARN spark:
   schema validation failed: violation tenat.id (Unknown; did you
   mean tenant.id?)`. Riley sees it; Sasha fixes it.
2. **Acme prod (strict mode)**: Sasha enables
   `with_strict_schema_lint(true)` for CI integration tests. The
   same misnamed attribute now causes `init` to return
   `Err(SparkError::SchemaValidation(report))` and the test fails
   fast.
3. **Clean canonical Resource**: every attribute is blessed; warn
   mode emits no warn event; strict mode returns `Ok(SparkGuard)`.

### UAT scenarios (BDD)

#### Scenario: Warn mode emits one warn event per misconfigured init

```
Given a SparkConfig with the default warn mode
And a Resource carrying one unknown attribute ("tenat.id", "...")
When spark::init is called
Then the result is Ok(SparkGuard)
And exactly one tracing::warn! event with target="spark" is captured
And the warn event message names the offending attribute
```

#### Scenario: Strict mode rejects the init

```
Given a SparkConfig with .with_strict_schema_lint(true)
And a Resource carrying one unknown attribute ("tenat.id", "...")
When spark::init is called
Then the result is Err(SparkError::SchemaValidation(report))
And the report contains exactly one LintViolation
```

#### Scenario: A clean Resource validates without warning

```
Given a SparkConfig with the default warn mode
And a canonical Resource (every attribute blessed)
When spark::init is called
Then the result is Ok(SparkGuard)
And no tracing::warn! event with target="spark" mentioning schema validation is captured
```

### Acceptance Criteria

- [ ] Spark's `Cargo.toml` adds `codex` as a runtime dep.
- [ ] `SparkConfig::with_strict_schema_lint(bool)` builder method.
- [ ] `SparkError::SchemaValidation(LintReport)` variant (non-breaking on `#[non_exhaustive]`).
- [ ] Warn mode emits one `tracing::warn!` event per misconfigured init.
- [ ] Strict mode returns `Err(SparkError::SchemaValidation(...))` and never reaches `Ok(SparkGuard)`.
- [ ] A canonical Resource validates clean (no warn event in warn mode; `Ok` in strict mode).

### Outcome KPIs

- 100% of unknown-attribute Resources surface in `tracing::warn!` events at default verbosity (CI invariant).
- 100% of strict-mode misconfigured inits return `Err` (CI invariant).
- The new variant is additive: existing Spark consumers compile without change.

### Technical Notes

DESIGN-wave decisions: ADR-0012 (Spark error type) needs a small
additive amendment for the new variant. ADR-0013 (Spark dep
pinning) gains a row for `codex` in the licence-audit table (AGPL
runtime dep, symmetric with Aperture's case for Sieve). Spark v0
ships at v0.1.0; the new variant is an additive minor candidate
(would land at v0.2.0 if Spark ever goes there). For Codex v0
delivery, Spark's amendment lands as a post-DELIVER correction
note, mirroring how Aperture's `--config` wiring landed at the
slice-08 completion summary.

---

## Out of scope (explicitly not user stories at v0)

- gRPC service surface, Protobuf descriptor endpoint, HTML
  rendering — full Codex per roadmap C.1; v1+.
- FoundationDB-backed multi-version catalogue — v1+.
- CUE-based schema corpus — v1+ (when the catalogue's complexity
  outgrows Rust constants).
- Per-tenant schema extensions — needs Aegis; v1+.
- Aperture-side lint integration — a follow-up feature, not part of
  Codex v0 (per Q-out-of-scope).
- Multi-version semconv negotiation — v1+ when consumers run on
  different SDK families.
