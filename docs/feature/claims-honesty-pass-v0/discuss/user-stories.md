<!-- markdownlint-disable MD024 -->

# User Stories — claims-honesty-pass-v0

The job every story below traces to (N:1):

> **JTBD**: "When I read what Kaleidoscope claims — in a README, a crate doc, a
> codename, a Cargo.toml description — the claim matches what the code actually
> does, or is clearly marked as future/roadmap."

Persona for all stories: **Devin Okafor**, a senior platform engineer at a mid-
size SaaS (Northwind Logistics) evaluating Kaleidoscope to replace a five-figure
Datadog bill. Devin reads claims, then opens the code to verify them. The moment
a claim overstates the code, Devin's trust in the *whole* project drops — which
for an honesty-thesis project is the worst possible failure.

## System Constraints (cross-cutting)

- **The per-crate `lib.rs` already-honest wording is the canonical truth.** Every
  correction aligns the louder surface (README, Cargo.toml, test headers) TO the
  quiet honest `lib.rs`, not to a freshly-invented phrasing.
- **Touch ONLY markers proven to sit over GREEN code.** Genuinely-RED /
  `#[ignore]`d in-flight scaffolds (crash-durability, pagination, body-regex,
  tls-config-reject, tracing-subscriber) describe a true current state and MUST
  NOT be touched.
- **Solution-neutral.** Stories state the honest claim and the testable guard,
  not the edit mechanics. For the two document-vs-implement stories, the
  document-vs-implement decision is DESIGN's, not DISCUSS's.
- **Acceptance shape** (for DISTILL): a prose-honesty correction is verified by a
  guard that the false string is ABSENT and the corrected claim is PRESENT
  (grep/doc-lint); for the code-touch items, a test asserts the real behaviour
  the doc now describes.
- **Trunk-based, CI-is-feedback** (project memory): no CI gate blocks a doc-only
  change; the guard tests are the regression net.
- **Mutation**: doc-only slices have nothing to mutate; only the two code-touch
  items (if DESIGN picks "implement") carry the per-feature mutation obligation.

---

## US-01: The README headline table stops overstating four capabilities

### Problem

Devin opens the README and reads the "Components at a glance" table first — it is
the project's brand. Four rows claim capabilities the code does not have: Loom is
"Dashboards-as-code" (it reads TOML rule files and handles zero dashboards),
Spark is "Auto-instrumentation SDKs" (it is a manual-init OTel wrapper that
auto-instruments nothing), Strata is "Continuous profiling" (it is a passive
profile sink with no scheduler), and Cinder is a "cold-tier coordinator" (it
stores tier metadata on local disk with no object-storage code). Each crate's own
`lib.rs` already tells the truth — but the table Devin reads first does not. For
an honesty-thesis project, the headline table overstating four crates is the
sharpest possible self-inflicted wound.

### Who

- Devin Okafor | senior platform engineer evaluating adoption | reads the README
  table first, then spot-checks `lib.rs` and catches the mismatch immediately.

### Elevator Pitch

- **Before**: The README "Components at a glance" table tells Devin that Spark
  auto-instruments, Strata continuously profiles, Cinder coordinates a cold tier,
  and Loom is dashboards-as-code — none of which the v0/v1 code does.
- **After**: Reading the same table in the rendered `README.md`, Devin sees
  Spark as "manual-init OTel SDK wrapper", Strata as "profile storage (passive
  sink)", Cinder as "local tier-metadata coordinator", Loom as "rule-catalogue
  change control (TOML)" — each with the over-the-horizon capability marked
  future/v1+.
- **Decision enabled**: Devin trusts the table as a true capability map and
  scopes the Datadog-replacement evaluation to what actually ships today.

### Solution

Correct the four component rows (README lines 171, 179, 180, 185) and the
cost-table profiling line (line 213) so each role string matches the crate's
already-honest `lib.rs`, with the future capability explicitly future-tensed.

### Domain Examples

#### 1: Happy Path — Devin reads the corrected Spark row

Devin reads `README.md` line 171 and sees "Manual-init OTel SDK wrapper
(auto-instrumentation: v0.2/v1)". Devin opens `spark/src/lib.rs:1-17`, reads
"thin wrapper around the upstream opentelemetry … crates", and the two match.
Trust preserved.

#### 2: Edge Case — Devin reads the corrected Strata row AND the cost table

Devin reads the Strata row (line 179) now reading "Profile storage; continuous
scraping is future" AND the cost-table line (213) now reading "Profiling is
included as first-party storage (continuous scraping: roadmap)". Both surfaces
agree with `strata/src/lib.rs:17` ("first-party profile storage engine … Library
only at v0"). No single-surface drift.

#### 3: Error/Boundary — Devin checks the row NOT in scope (durability)

Devin reads the README `Status` section's durability claim (lines 89-95) and
finds it TRUE (all seven `FileBacked*` stores fsync + atomic snapshot after
`store-fsync-durability-v0`). This story does not touch it; the correction does
not over-reach into already-true claims.

### UAT Scenarios (BDD)

#### Scenario: The Spark row names manual init, not auto-instrumentation

Given Devin is reading the rendered `README.md` "Components at a glance" table
When Devin reads the Spark row
Then the role text describes a manual-init OTel SDK wrapper
And the phrase "Auto-instrumentation SDKs" is absent as a present-tense claim
And auto-instrumentation is marked as a future (v0.2/v1) capability

#### Scenario: The Strata row and the cost table agree that profiling is passive

Given Devin is reading the README
When Devin reads both the Strata component row and the cost-model profiling line
Then both describe profile storage / a passive sink, not continuous profiling
And "continuous" is marked as future on both surfaces

#### Scenario: The Cinder row names local tier metadata, not a cold-tier coordinator

Given Devin is reading the Cinder component row
When Devin reads the role text
Then it describes a local tier-metadata coordinator
And the object-storage (S3/OpenDAL/Iceberg) cold tier is marked as a future (v2) capability

#### Scenario: The Loom row names TOML change control, not dashboards-as-code

Given Devin is reading the Loom component row
When Devin reads the role text
Then it describes change control over operator TOML catalogues
And dashboards-as-code is marked as a future (v1+) capability

#### Scenario: Each corrected row matches its crate's lib.rs

Given the four corrected rows and the four crates' `lib.rs` doc headers
When Devin cross-reads any corrected row against its `lib.rs`
Then the present-tense capability claimed in the README is also claimed in the `lib.rs`
And neither surface overstates the other

### Acceptance Criteria

- [ ] README Spark row contains no present-tense "auto-instrumentation" claim;
  auto-instrumentation is future-tensed. (scenario 1)
- [ ] README Strata row + cost-table line both describe passive profile storage;
  "continuous" is future-tensed on both. (scenario 2)
- [ ] README Cinder row describes local tier-metadata; object-storage cold tier
  is future-tensed (v2). (scenario 3)
- [ ] README Loom row describes TOML rule-catalogue change control; dashboards
  are future-tensed (v1+). (scenario 4)
- [ ] Each corrected row's present-tense claim is consistent with the
  corresponding crate `lib.rs`. (scenario 5)

### Outcome KPIs

- **Who**: evaluators reading the README "Components at a glance" table.
- **Does what**: encounter zero present-tense capability claims that the v0/v1
  code does not back.
- **By how much**: 4 of 4 overstated rows corrected; 0 residual present-tense
  overstatements in the table.
- **Measured by**: doc-lint/grep guard asserting the four false phrases are
  absent and the four corrected phrases present, run in the guard suite.
- **Baseline**: 4 overstated rows + 1 overstated cost-table line today.

### Technical Notes

- README only. The roadmap (C.6) is already correctly future-tense; do not touch
  it. Aligns to `lib.rs` of loom/spark/strata/cinder.

---

## US-02: Codex stops declaring itself an unbuilt DISTILL stub

### Problem

Devin opens `codex` to evaluate the schema-authority crate. The `lib.rs` says
"Fully implemented and green" — but `Cargo.toml` (lines 17-24) declares a
"DISTILL-state stub" where "every acceptance test under `tests/` panics with
`unimplemented!()`", and all five `tests/slice_0*.rs` headers plus
`tests/common/mod.rs` repeat "Tests panic on `unimplemented!()` until DELIVER
lands…". All five slices are real and green; the `slice_04` test asserts a live
`Err` path. A delivered crate calling itself unbuilt is stale prose that makes
Devin distrust the build-status claims everywhere.

### Who

- Devin Okafor | evaluator | reads `Cargo.toml` and test headers to gauge how
  finished a crate is, and is misled into thinking codex is a stub.

### Elevator Pitch

- **Before**: `codex/Cargo.toml` and every `codex/tests/*.rs` header tell Devin
  the crate is a DISTILL stub whose tests panic with `unimplemented!()`.
- **After**: Reading the same `Cargo.toml` comment block and test headers, Devin
  sees them describe a delivered, green crate whose tests exercise live behaviour.
- **Decision enabled**: Devin counts codex as a finished, dependable component in
  the adoption assessment instead of discounting it as unbuilt.

### Solution

Correct the stale stub declarations in `codex/Cargo.toml` (lines 17-24), the five
`tests/slice_0*.rs` headers, and `tests/common/mod.rs` (lines 14-16) to reflect
the delivered, green state that `lib.rs:43-48` already states.

### Domain Examples

#### 1: Happy Path — Devin reads the corrected Cargo.toml block

Devin reads `codex/Cargo.toml` and finds the comment block now states the crate
is delivered and the `tests/` exercise the live `validate` path, matching
`lib.rs` ("Fully implemented and green"). Consistent.

#### 2: Edge Case — Devin reads the slice_04 test header then the test body

Devin reads `tests/slice_04_unknown_attribute_lint.rs`: the header now says the
test asserts the live `Err`/`Display` path (not "panics on `unimplemented!()`"),
and the body indeed asserts `result.is_err()` against real code. Header and body
agree.

#### 3: Error/Boundary — Devin checks no genuinely-RED codex test exists

Devin greps codex tests for active `unimplemented!`/`#[ignore]` and finds none —
confirming every corrected header was stale-over-green, not a touched in-flight
marker.

### UAT Scenarios (BDD)

#### Scenario: The Cargo.toml no longer declares codex a stub

Given Devin is reading `codex/Cargo.toml`
When Devin reads the crate-status comment block
Then it describes a delivered, green crate
And it contains no claim that the acceptance tests panic with `unimplemented!()`

#### Scenario: Each slice test header describes live behaviour

Given Devin is reading any of the five `codex/tests/slice_0*.rs` headers
When Devin reads the header's status note
Then it describes the live behaviour the test asserts
And it contains no "Tests panic on `unimplemented!()` until DELIVER" claim

#### Scenario: The corrected headers match the green test bodies

Given a corrected test header and its test body
When Devin cross-reads them
Then the header's described behaviour matches what the body asserts against live code

#### Scenario: No genuinely-RED codex test was altered

Given the codex test suite
When the correction is applied
Then only headers sitting over GREEN, passing tests were changed
And no `#[ignore]`d or actively-RED codex test had its meaning altered

### Acceptance Criteria

- [ ] `codex/Cargo.toml` contains no "DISTILL-state stub" / "panics with
  `unimplemented!()`" declaration. (scenario 1)
- [ ] All five `codex/tests/slice_0*.rs` headers + `tests/common/mod.rs` contain
  no "panic on `unimplemented!()` until DELIVER" claim. (scenario 2)
- [ ] Each corrected header matches its green test body. (scenario 3)
- [ ] Only stale-over-green headers changed; no in-flight RED test altered.
  (scenario 4)

### Outcome KPIs

- **Who**: evaluators reading codex `Cargo.toml` / test headers.
- **Does what**: read a status that matches the delivered green code.
- **By how much**: 0 of 7 codex doc surfaces (1 Cargo.toml block + 5 headers + 1
  common) still declare a stub; was 7 of 7.
- **Measured by**: grep guard asserting absence of the stub phrases in the seven
  loci; codex suite stays green.
- **Baseline**: 7 stale stub declarations over green code.

### Technical Notes

- Doc/comment only. Depends on nothing; codex is delivered. Guardrail: confirm
  green before editing each header.

---

## US-03: Two delivered crates stop claiming their bodies are unimplemented scaffolds

### Problem

Devin reads `query-http-common` and `trace-query-api` — both fully delivered,
green crates. But `query-http-common/src/lib.rs:30-42` still says "DISTILL
scaffold — DELIVER fills the bodies … All free functions are
`unimplemented!("__SCAFFOLD__ … RED")`" directly above fully-live function bodies
(each of which already says "DELIVER state: implemented"). And
`trace-query-api/src/lib.rs:207-232` still calls `handle_traces_by_id` an
"`unimplemented!` scaffold" above its real implementation. The doc comment
contradicts the code two lines below it.

### Who

- Devin Okafor | evaluator reading source docs | trusts the prominent
  module/handler doc comment over the buried per-fn note, and concludes the read
  side is half-built.

### Elevator Pitch

- **Before**: The module doc of `query-http-common/src/lib.rs` and the handler
  doc of `trace-query-api/src/lib.rs` tell Devin the bodies are
  `unimplemented!` `__SCAFFOLD__` RED stubs.
- **After**: Reading the same module/handler docs, Devin sees them describe live,
  implemented functions — matching the bodies and the per-fn "implemented" notes.
- **Decision enabled**: Devin treats the read-side query crates as delivered and
  proceeds to evaluate their behaviour instead of writing them off as scaffolds.

### Solution

Remove/correct the stale scaffold doc comments that sit over GREEN bodies:
`query-http-common/src/lib.rs:30-42` (module-level) and
`trace-query-api/src/lib.rs:207-209,228-232` (handler-level). Leave every
genuinely-RED / `#[ignore]`d scaffold elsewhere untouched.

### Domain Examples

#### 1: Happy Path — Devin reads the corrected query-http-common module doc

Devin reads `query-http-common/src/lib.rs` module doc and sees it describe four
delivered helpers (`parse_time_range`, `resolve_tenant_or_refuse`,
`error_response`, `init_tracing`) — matching both the bodies and each fn's
"DELIVER state: implemented" note.

#### 2: Edge Case — Devin reads the corrected trace-query-api handler doc

Devin reads the `handle_traces_by_id` doc and sees it describe the live
resolve→parse→get_trace→cap→serialise orchestration, matching the body at lines
233-292. No "`unimplemented!` scaffold" claim remains.

#### 3: Error/Boundary — Devin verifies an in-flight RED scaffold was NOT touched

Devin greps for the still-legitimate `__SCAFFOLD__` markers in
`log-query-api/tests/slice_05_body_regex.rs` and the `*_crash_durability` tests
and finds them intact — confirming the correction touched only stale-over-green
comments, not honest in-flight markers.

### UAT Scenarios (BDD)

#### Scenario: The query-http-common module doc describes implemented helpers

Given Devin is reading `query-http-common/src/lib.rs` module documentation
When Devin reads the public-surface summary
Then it describes implemented helper functions
And it contains no claim that the free functions are `unimplemented!`
`__SCAFFOLD__` RED stubs

#### Scenario: The trace-query-api handler doc describes a live handler

Given Devin is reading the `handle_traces_by_id` doc comment
When Devin reads its status note
Then it describes the implemented lookup-by-id orchestration
And it contains no "`unimplemented!` scaffold" claim

#### Scenario: The corrected docs match the live bodies

Given the corrected module/handler docs and their function bodies
When Devin cross-reads them
Then the documented behaviour matches the implemented behaviour

#### Scenario: Genuinely in-flight scaffolds are left intact

Given the workspace's other `__SCAFFOLD__` / `#[ignore]`d RED markers
When the correction is applied
Then no marker sitting over genuinely-RED or `#[ignore]`d in-flight code is altered
And only comments sitting over GREEN, passing code were changed

### Acceptance Criteria

- [ ] `query-http-common/src/lib.rs` module doc contains no "DISTILL scaffold /
  all free functions are `unimplemented!`" claim. (scenario 1)
- [ ] `trace-query-api/src/lib.rs` handler doc contains no "`unimplemented!`
  scaffold" claim for `handle_traces_by_id`. (scenario 2)
- [ ] Corrected docs match the live bodies. (scenario 3)
- [ ] No genuinely-RED / `#[ignore]`d scaffold marker was altered. (scenario 4)

### Outcome KPIs

- **Who**: evaluators reading the read-side query crates' source docs.
- **Does what**: read module/handler docs that match the delivered green bodies.
- **By how much**: 2 of 2 stale-over-green scaffold doc blocks corrected; 0
  genuinely-RED markers touched.
- **Measured by**: grep guard asserting the stale scaffold phrasing is absent in
  the two loci AND present in the explicitly-listed in-flight loci (the guard
  proves the correction did not over-reach).
- **Baseline**: 2 stale scaffold doc blocks over green code.

### Technical Notes

- Doc-comment only. Hard guardrail: the inventory in `wave-decisions.md`
  enumerates exactly which markers are stale-over-green (touch) vs in-flight
  (leave). The guard test asserts BOTH directions.

---

## US-04: The conformance harness stops claiming semantic wire-spec validation

### Problem

Devin reads `otlp-conformance-harness` — the crate whose whole purpose is to be
the project's honesty anchor for OTLP conformance. Its `lib.rs:1-7`, `README.md:3-4`,
and `Cargo.toml:11` all say it "validates byte sequences against the OpenTelemetry
OTLP **wire specification**". But `validate.rs` + `decode.rs` show the validation
is structural/decode-level only: non-empty, first wire tag references the resource
field, decodes as the asserted prost type, with a signal-mismatch fallback. There
is NO semantic validation — no trace_id/span_id length, no timestamp, no
attribute, no semantic-convention checks. Worse, the harness `README.md:8-16`
still says "implementation intentionally absent / every `validate_*` returns
`unimplemented!()`" over fully-live code. A conformance harness overstating its
own conformance is the project thesis failing in miniature.

### Who

- Devin Okafor | evaluator who will trust telemetry to this harness | reads "wire
  specification" as "semantically conformant" and over-relies on it.

### Elevator Pitch

- **Before**: The harness `lib.rs`/`README`/`Cargo.toml` tell Devin it validates
  against the OTLP wire specification, and the README says it is unimplemented.
- **After**: Reading the same surfaces, Devin sees "structural decode-level
  validation" with the semantic checks it does NOT perform named explicitly, and
  a status that says implemented-and-green.
- **Decision enabled**: Devin knows the harness proves "this decodes as the
  asserted OTLP type", not "this is semantically conformant", and plans
  semantic validation accordingly.

### Solution

Correct the harness validation-depth claim in `lib.rs:1-7`, `README.md:3-4`, and
`Cargo.toml:11` to "structural decode-level validation", naming the semantic
checks it does not do; and correct the stale-over-green status block in
`README.md:8-16`.

### Domain Examples

#### 1: Happy Path — Devin reads the corrected harness lib.rs header

Devin reads `harness/src/lib.rs` and sees "validates that a byte sequence decodes
structurally as the asserted OTLP signal type (non-empty, resource-field-first,
prost-decodable) — NOT a semantic conformance check". Devin reads `decode.rs` and
confirms exactly that.

#### 2: Edge Case — Devin reads the corrected README status

Devin reads `harness/README.md` and sees the status block now says the three
validators are implemented and green (matching `lib.rs:17-22`), with the corpus
runner described — no "implementation intentionally absent / `unimplemented!()`".

#### 3: Error/Boundary — Devin sends a structurally-valid but semantically-bogus body

Devin constructs an `ExportTraceServiceRequest` with a 4-byte `trace_id` (invalid
per the OTLP/W3C 16-byte rule). The harness ACCEPTS it (decodes fine). Because the
corrected docs say "structural decode-level, not semantic", Devin is not
surprised — the doc predicted exactly this boundary.

### UAT Scenarios (BDD)

#### Scenario: The harness describes structural decode-level validation, not wire-spec conformance

Given Devin is reading the harness `lib.rs` / `README.md` / `Cargo.toml` description
When Devin reads what the harness validates
Then it states structural decode-level validation (non-empty, resource-field-first, prost-decodable as the asserted type)
And it does not claim semantic OTLP-wire-specification conformance
And the semantic checks it does NOT perform are named

#### Scenario: The harness README status reflects the delivered green code

Given Devin is reading the harness `README.md` status section
When Devin reads the implementation-status text
Then it describes implemented, green validators
And it contains no claim that implementation is absent or that `validate_*` returns `unimplemented!()`

#### Scenario: A structurally-valid, semantically-invalid body is accepted as the docs now predict

Given a trace export request that decodes cleanly but carries a 4-byte trace_id
When Devin validates it with `validate_traces` under HTTP framing
Then the harness accepts it (structural decode succeeds)
And the corrected documentation already states semantic checks are out of scope, so the behaviour matches the claim

#### Scenario: The corrected claim matches decode.rs

Given the corrected validation-depth claim and the `decode.rs` implementation
When Devin cross-reads them
Then every validation step the doc names is present in `decode.rs`
And no validation step the doc names is absent from the code

### Acceptance Criteria

- [ ] Harness `lib.rs`/`README`/`Cargo.toml` describe structural decode-level
  validation and name the absent semantic checks; no "wire specification"
  conformance overclaim remains. (scenario 1)
- [ ] Harness `README` status describes implemented green validators; no
  "unimplemented" claim. (scenario 2)
- [ ] A structurally-valid, semantically-invalid body is accepted, matching the
  corrected doc. (scenario 3)
- [ ] The corrected claim matches `decode.rs` step-for-step. (scenario 4)

### Outcome KPIs

- **Who**: evaluators / downstream crates trusting the conformance harness.
- **Does what**: understand the harness proves structural decode, not semantic
  conformance.
- **By how much**: 3 of 3 depth-claim loci corrected + 1 stale status block
  corrected; 0 residual "wire specification" semantic overclaim.
- **Measured by**: grep guard (false phrase absent, corrected phrase present) +
  one acceptance test asserting a semantically-invalid-but-structurally-valid
  body is accepted (pinning the documented boundary).
- **Baseline**: 3 depth overclaims + 1 stale status block.

### Technical Notes

- Pure prose for the depth + status correction. Does NOT change validation
  behaviour. The `GrpcProtobuf` framing question is split into US-06 (the
  document-vs-implement decision). Pairs with US-06 in one PR if convenient.

---

## US-05: The read endpoint's stepped-grid implication matches reality (DESIGN flag #1)

### Problem

Devin reads that Kaleidoscope serves a "Prometheus-compatible
`/api/v1/query_range`" endpoint (README lines 104-108). Prometheus' contract
re-samples results onto a stepped grid defined by the `step` parameter. But
`query-api/src/lib.rs:143-146` accepts `step`, then silently ignores it, returning
raw native-timestamp points. The in-code field doc is ALREADY honest ("`step`
accepted and ignored at v0; raw points, no re-stepping"); the residual
overstatement is the README's "Prometheus-compatible" framing. A verifier is
independently building a black-box (two `step` values → identical output) that
will expose the gap. The honesty fix must at least make the claim match reality.

### Who

- Devin Okafor | evaluator who will point Grafana/Prometheus tooling at the
  endpoint | expects `step` to re-sample and is surprised when two `step` values
  return identical points.

### Elevator Pitch

- **Before**: The README brands `/api/v1/query_range` "Prometheus-compatible",
  implying `step` re-samples onto a grid; in fact two different `step` values
  return byte-identical raw points.
- **After (DESIGN: document)**: The README states the endpoint serves raw points
  and that `step` is accepted-but-not-yet-honoured at v0; a black-box guard pins
  that two `step` values return identical output, matching the corrected claim.
- **After (DESIGN: implement)**: `step` re-samples onto a Prometheus grid; two
  `step` values return different, correctly-stepped output, matching the
  "Prometheus-compatible" claim.
- **Decision enabled**: Devin knows whether to expect stepped re-sampling before
  wiring dashboards, and is never silently handed un-stepped data under a
  stepped-grid promise.

### Solution

DESIGN decides **document** (qualify the README "Prometheus-compatible" framing
to state `step` is accepted-but-not-honoured at v0, raw points returned) vs
**implement** (build the Prometheus stepped grid). DISCUSS recommends DOCUMENT
(keeps the feature a true honesty pass). Either way the claim must match the
behaviour the verifier's black-box probes.

### Domain Examples

#### 1: Happy Path (document) — Devin reads the qualified README

Devin reads the corrected README and sees "`/api/v1/query_range` returns raw
stored points; the Prometheus `step` parameter is accepted for compatibility but
not yet honoured at v0 (no grid re-sampling)". Devin's expectation is set
correctly before querying.

#### 2: Edge Case — Devin queries with step=15s then step=60s

Devin queries `service="checkout"` over the same window with `step=15s`, then
`step=60s`. Under DOCUMENT, the two responses are byte-identical raw points — and
the corrected doc predicted exactly this. Under IMPLEMENT, the two differ on a
correct grid — matching a then-unqualified "Prometheus-compatible" claim.

#### 3: Error/Boundary — Devin omits step entirely

Devin queries with no `step`. The endpoint returns the same raw points it returns
with any `step` (under DOCUMENT). The corrected doc's "accepted but not honoured"
wording covers the omitted-parameter case too.

### UAT Scenarios (BDD)

#### Scenario: The endpoint's stepped-grid claim matches its behaviour

Given Devin is reading the README description of `/api/v1/query_range`
When Devin reads what `step` does
Then the documented behaviour matches the endpoint's actual behaviour for the
`step` parameter (whether DESIGN chose document or implement)

#### Scenario: Two step values are consistent with the corrected claim

Given a metric `checkout_requests_total` for tenant "northwind" over a fixed window
When Devin queries `/api/v1/query_range` with `step=15s` and again with `step=60s`
Then the relationship between the two responses (identical raw points, or
distinct stepped grids) matches what the corrected documentation states

#### Scenario: The verifier black-box agrees with the documentation

Given the verifier's black-box probe (two step values → compare output)
When the probe runs against the endpoint
Then its result (identical vs distinct output) is exactly what the corrected
documentation claims
And there is no gap between the probe's finding and the prose

### Acceptance Criteria

- [ ] The README `/api/v1/query_range` description states the true `step`
  behaviour (accepted-but-not-honoured + raw points, OR honoured + stepped grid,
  per DESIGN). (scenario 1)
- [ ] Two `step` values produce output consistent with the corrected claim.
  (scenario 2)
- [ ] The verifier's black-box result matches the documentation with no gap.
  (scenario 3)

### Outcome KPIs

- **Who**: evaluators wiring Prometheus/Grafana tooling to the read endpoint.
- **Does what**: form a correct expectation of `step` re-sampling before querying.
- **By how much**: 1 of 1 stepped-grid implication reconciled with behaviour; the
  black-box probe and the prose agree (0 gap).
- **Measured by**: the black-box test (two `step` values) + a grep/doc guard on
  the README `step` description.
- **Baseline**: README implies stepped grid; behaviour returns raw points; the
  two disagree.

### Technical Notes

- **DESIGN flag #1 — document-vs-implement.** DISCUSS recommends DOCUMENT
  (proportionate to an honesty pass; "implement the grid" is a real feature
  deserving its own slice). Carries a code touch + mutation obligation ONLY if
  DESIGN picks implement. The in-code field doc is already honest; the residual is
  the README framing.

---

## US-06: The harness GrpcProtobuf framing claim matches its behaviour (DESIGN flag #2)

### Problem

Devin reads that the harness accepts a `Framing::GrpcProtobuf` argument on every
`validate_*` call. Devin reasonably assumes the harness then handles gRPC framing
(strips the length prefix). It does not: `framing` is never branched on in
`validate.rs`/`decode.rs`; it is only echoed into `OtlpViolation`. The enum doc
(`framing.rs:16-18`) admits the caller must strip the prefix, but `lib.rs` and the
README present `GrpcProtobuf` as a first-class supported framing without flagging
that it is inert. Devin passes a length-prefixed gRPC body, the harness fails to
decode it, and the error is confusing because the framing argument looked load-
bearing.

### Who

- Devin Okafor | evaluator feeding the harness real gRPC-framed OTLP bytes |
  passes `GrpcProtobuf` expecting prefix handling and gets a confusing decode
  failure.

### Elevator Pitch

- **Before**: The harness `lib.rs`/README present `GrpcProtobuf` as a supported
  framing, so Devin passes a length-prefixed gRPC body and gets a confusing
  decode failure.
- **After (DESIGN: document)**: The harness docs state `GrpcProtobuf` is a label
  echoed into violations, NOT a behavioural branch — the caller strips the gRPC
  length prefix first. Devin strips the prefix and validation works.
- **After (DESIGN: honour)**: The harness strips the gRPC length prefix when
  `GrpcProtobuf` is asserted, so a length-prefixed body validates directly.
- **Decision enabled**: Devin knows whether to strip the gRPC length prefix
  before calling the harness, and stops hitting confusing decode failures.

### Solution

DESIGN decides **document** (state at `lib.rs`/README level that `GrpcProtobuf` is
a non-behavioural label; the caller strips the prefix) vs **honour** (strip the
length prefix when `GrpcProtobuf` is asserted). DISCUSS recommends DOCUMENT.

### Domain Examples

#### 1: Happy Path (document) — Devin reads the corrected framing note

Devin reads `harness/src/lib.rs` and sees "`Framing` is echoed into violations
for diagnostics; it does NOT change validation. For `GrpcProtobuf`, strip the gRPC
length prefix before calling the harness." Devin strips the prefix and
`validate_logs` succeeds.

#### 2: Edge Case — Devin passes a prefix-stripped body under both framings

Devin validates the same prefix-stripped `ExportLogsServiceRequest` bytes under
`HttpProtobuf` and under `GrpcProtobuf`. Both accept identically (framing is
inert). The corrected doc predicted this.

#### 3: Error/Boundary (document) — Devin passes a length-prefixed body under GrpcProtobuf

Devin passes a still-length-prefixed body under `GrpcProtobuf`. It fails to decode.
Under the corrected DOCUMENT doc this is expected (the doc told Devin to strip
first); under HONOUR this would instead succeed.

### UAT Scenarios (BDD)

#### Scenario: The GrpcProtobuf framing claim matches the harness behaviour

Given Devin is reading the harness `lib.rs` / README description of `Framing`
When Devin reads what `GrpcProtobuf` does
Then the documented behaviour matches the harness's actual handling of the framing
argument (non-behavioural label, or prefix-stripping, per DESIGN)

#### Scenario: Both framings behave consistently with the corrected claim on prefix-stripped bytes

Given prefix-stripped `ExportLogsServiceRequest` bytes
When Devin validates them under `HttpProtobuf` and under `GrpcProtobuf`
Then the two results are consistent with the corrected documentation

#### Scenario: A length-prefixed gRPC body behaves as the corrected doc predicts

Given a length-prefixed gRPC-framed OTLP body
When Devin validates it under `GrpcProtobuf`
Then the outcome (decode failure requiring the caller to strip the prefix, OR
acceptance via harness-side prefix stripping) matches the corrected documentation

### Acceptance Criteria

- [ ] The harness `lib.rs`/README state the true `GrpcProtobuf` behaviour
  (non-behavioural label requiring caller-side prefix stripping, OR harness-side
  stripping, per DESIGN). (scenario 1)
- [ ] Prefix-stripped bytes behave identically under both framings, matching the
  claim. (scenario 2)
- [ ] A length-prefixed body behaves as the corrected doc predicts. (scenario 3)

### Outcome KPIs

- **Who**: evaluators feeding gRPC-framed OTLP bytes to the harness.
- **Does what**: correctly strip (or not) the gRPC length prefix before calling
  the harness.
- **By how much**: 1 of 1 framing claim reconciled with behaviour; 0 confusing
  decode failures caused by an inert-but-load-bearing-looking framing argument.
- **Measured by**: grep/doc guard on the framing description + an acceptance test
  asserting both-framings behaviour on prefix-stripped bytes.
- **Baseline**: `GrpcProtobuf` presented as supported; behaviour is inert; the
  two disagree.

### Technical Notes

- **DESIGN flag #2 — document-vs-implement.** DISCUSS recommends DOCUMENT
  ("honour the framing" is a real capability deserving its own feature). Carries a
  code touch + mutation obligation ONLY if DESIGN picks honour. The
  validation-depth correction for the harness is US-04 (pure prose); this story is
  only the framing decision. May share a PR with US-04.
