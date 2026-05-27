<!-- markdownlint-disable MD024 -->
# User Stories — query-http-common-v0

## Feature framing

`query-api`, `log-query-api`, and `trace-query-api` are the three Kaleidoscope
read-side HTTP crates. Each was built standalone, faithful to the local
contract. After ADR-0048 (rule-of-three trigger) and ADR-0053 (`rule of three
and a bit`, with the lookup-by-id arm landing the fourth identical use of the
fail-closed seam and the error envelope), the duplicated scaffolding has earned
its extraction. The seam is documented as deferred in ADR-0048 Decision 5 /
Placement B, ADR-0053 Decision 5, and listed as `M-5` in
`docs/residuality-followups-roadmap.md`.

This feature extracts the scaffolding into a new workspace-internal crate
`query-http-common`. It is library-only, with no new endpoints and no
user-visible behaviour change. The job-to-be-done is the maintainer's job:
when the cap value, the error reason text, the time-range parser, or the
fail-closed tenant pattern needs to change, the maintainer changes it in one
place and the change propagates to all three read APIs.

All five stories carry the `@infrastructure` tag because none of them adds a
new endpoint, new flag, or new wire-observable behaviour. Each story has a
clear `Decision enabled` line: the maintainer can now do something they could
not do before the slice lands, because the seam is single-sourced. The thinness
discipline (≤ 1 day each, byte-identical behaviour pre/post on the acceptance
suite) is the carpaccio control on a refactor that otherwise risks growing
into a re-architecture.

## System constraints

- AGPL-3.0-or-later applies to the new crate (the three consumer crates are
  AGPL platform components; the new crate is consumed only by them).
- `#![forbid(unsafe_code)]` mirrored from the three consumers.
- The new crate is workspace-internal: it is not published, it has no public
  versioning commitment beyond the workspace, and it depends on substrate
  crates (`axum`, `serde`, `serde_json`, `aegis`) but NOT on the pillar stores
  (`pulse`, `lumen`, `ray`). The bounds parser stays generic over the output
  type or returns `(u64, u64)` (whole seconds), so the three consumers each
  build their own pillar-specific `TimeRange` on top.
- The cap reason texts MUST be preserved byte-for-byte:
  `"window exceeds 86400 seconds"` and `"result exceeds 100000 rows"`.
- The error envelope MUST be preserved byte-for-byte:
  `{"status":"error","error":"<reason>"}`.
- The fail-closed 401 reason text differs per crate (each names its own
  service: `"the query service"`, `"the log query service"`, `"the trace
  query service"`). This pillar-specific suffix is preserved by accepting a
  service-label parameter; the prefix and envelope are shared.
- Mutation testing gate per ADR-0005 Gate 5: kill rate must be 100% on the
  new crate after extraction.
- No `1.0.0` bump on any crate; the new crate starts at `0.1.0`.

---

## US-01: Single-source the read-side caps

`@infrastructure`

### Problem

Today, `crates/query-api/src/lib.rs`, `crates/log-query-api/src/lib.rs`, and
`crates/trace-query-api/src/lib.rs` each declare their own
`pub const MAX_WINDOW_SECONDS: u64 = 86_400;` and
`pub const MAX_RESULT_ROWS: usize = 100_000;`. A maintainer who wants to
lower the result cap to 50,000 (or raise the window cap for traces only) has
to edit three files in lockstep, hold the consistency in their head, and rely
on the acceptance suite to catch a drift. ADR-0050 Decision 1 and Decision 2
pin a single uniform numeric value across the three crates, so the duplication
holds no information today and represents three places that can drift.

### Who

The Kaleidoscope read-side maintainer (Andrea or any crafter agent) who has
to change a workspace-wide policy constant and wants the change to land
atomically.

### Solution

The new crate `query-http-common` owns `pub const MAX_WINDOW_SECONDS: u64 =
86_400` and `pub const MAX_RESULT_ROWS: usize = 100_000`. The three consumer
crates `pub use query_http_common::{MAX_WINDOW_SECONDS, MAX_RESULT_ROWS};` for
backward-compatibility with the existing public surface and stop declaring
the constants themselves.

### Domain examples

1. Maintainer wants to tighten the workspace-wide window cap from 24h to 6h
   to limit a hot-path memory spike on the logs pillar. Before: edit three
   constants. After: edit one constant in `query-http-common`; the three
   acceptance suites pick the new boundary up on the next `cargo test`.
2. Maintainer wants to verify "no consumer crate still declares the constants
   locally" as a property of the workspace. Before: visual grep across three
   files. After: a single grep against `crates/*-api/src/` returns zero hits
   for the literal `MAX_WINDOW_SECONDS =` and `MAX_RESULT_ROWS =`.
3. A crafter implementing a new fourth read-side pillar (a metric-by-id arm,
   for example) writes `use query_http_common::MAX_WINDOW_SECONDS;` and
   inherits the same boundary class, without needing to know the numeric
   value or to re-justify it.

### UAT scenarios (BDD)

#### Scenario: The duplicated cap constants are removed from the three consumer crates

```text
Given `query-http-common` exports `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS`
When I run `grep -rE "MAX_(WINDOW_SECONDS|RESULT_ROWS) ?=" crates/query-api/src crates/log-query-api/src crates/trace-query-api/src`
Then the command exits 1 (no matches) for the assignment form
And the three crates contain only `pub use query_http_common::{...}` re-exports
```

#### Scenario: The workspace test suite stays green after the extraction

```text
Given the cap constants now live in `query-http-common`
When I run `cargo test --workspace`
Then every test that passed before the extraction still passes
And the pre-extraction test count equals the post-extraction test count
```

#### Scenario: A cap change propagates to all three crates from one edit

```text
Given a maintainer changes `MAX_RESULT_ROWS` in `query-http-common` from `100_000` to `50_000`
When the maintainer runs `cargo test --workspace`
Then the cap-rejection scenarios in `query-api`, `log-query-api`, and `trace-query-api` all observe the new boundary
And no other source file in the three consumer crates needed editing
```

### Acceptance criteria

- [ ] `query-http-common` crate exists and exports `MAX_WINDOW_SECONDS: u64`
  and `MAX_RESULT_ROWS: usize` with the values `86_400` and `100_000`.
- [ ] `crates/query-api/src/lib.rs`, `crates/log-query-api/src/lib.rs`, and
  `crates/trace-query-api/src/lib.rs` contain zero `MAX_WINDOW_SECONDS =`
  or `MAX_RESULT_ROWS =` assignments.
- [ ] Each of the three consumer crates re-exports the two constants via
  `pub use query_http_common::{MAX_WINDOW_SECONDS, MAX_RESULT_ROWS};` (so
  downstream callers reading `query_api::MAX_RESULT_ROWS` still compile).
- [ ] `cargo test --workspace` passes with the same test count as pre-slice.

### Outcome KPIs

- **Who**: read-side maintainers and crafter agents changing a cap policy.
- **Does what**: edit a workspace-wide read-side cap in a single file.
- **By how much**: from 3 file edits to 1 (a 67% reduction in scattered edits
  per cap policy change).
- **Measured by**: file-count diff of any future ADR that tunes the caps.
- **Baseline**: today, 3 source-file edits per change of either cap.

### Elevator Pitch

Before: `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS` are declared three times,
one per read-side crate, with no single source of truth.
After: run `grep -rE "MAX_(WINDOW_SECONDS|RESULT_ROWS) ?=" crates/query-api/src
crates/log-query-api/src crates/trace-query-api/src` → sees zero matches; the
constants live only in `query-http-common`.
Decision enabled: the maintainer can tune the workspace-wide read-side cap
policy with a single source-file edit and trust the change to propagate.

### Technical notes

- The three consumer crates must continue to expose `MAX_WINDOW_SECONDS` and
  `MAX_RESULT_ROWS` as part of their own public surface for backward
  compatibility (`pub use ...` is the chosen seam).
- The mutation suite that asserts the numeric values (in each consumer's
  inline test module) is preserved; the test references move to the
  re-exported path.

---

## US-02: Single-source the epoch-seconds time-range parser

`@infrastructure`

### Problem

`query-api` and `log-query-api` each carry an identical
`parse_time_range_seconds(start: &str, end: &str) -> Result<(u64, u64),
String>` (with its private helpers `parse_epoch_seconds` and
`seconds_to_nanos`). `trace-query-api` carries the same parser with the
`Option<&str>` signature (because `service` is the required parameter and
`start`/`end` are extracted with their option-form). The three diverge on
their parameter signature but produce the same `(u64, u64)` output for the
same input. A maintainer who wants to accept a new time format (e.g. RFC3339)
has to edit the same function in three places.

### Who

The Kaleidoscope read-side maintainer changing the wire format that the
three read APIs accept for the time window.

### Solution

`query-http-common` owns one canonical parser that accepts
`Option<&str>` for both bounds (the more permissive shape used by
`trace-query-api`). The two crates whose handlers extract `String` (not
`Option<String>`) wrap their values with `Some(...)` at the call site, which
is a one-line change. The parser returns `Result<(u64, u64), String>` with
identical error reason texts: `"invalid time bounds: <field> is not a number"`,
`"invalid time bounds: <field> is out of range"`,
`"invalid time bounds: <field> is required"`, and
`"invalid time bounds: end is earlier than start"`.

### Domain examples

1. Maintainer accepts a new ISO-8601 string format alongside epoch seconds.
   Before: edit the parser in three places, keeping the error text in sync.
   After: edit `query_http_common::parse_time_range_seconds` once.
2. A regression in the float-tolerance behaviour (Prism's `.toString()` on
   `Date.getTime()/1000`) is caught once, by one mutation suite, not three.
3. A new fourth read-side pillar uses `query_http_common::parse_time_range_seconds`
   directly with no need to re-derive the redaction-symmetric error texts.

### UAT scenarios (BDD)

#### Scenario: The duplicated parser is removed from the consumer crates

```text
Given `query-http-common` exports `parse_time_range_seconds`
When I run `grep -r "fn parse_time_range_seconds" crates/query-api/src crates/log-query-api/src crates/trace-query-api/src`
Then the command exits 1 (no matches)
And each consumer crate calls `query_http_common::parse_time_range_seconds` from its handler
```

#### Scenario: The error reason texts are preserved byte-for-byte

```text
Given an inverted window on any of the three read paths
When I send `?start=200&end=100` (or the equivalent shape)
Then the response body is exactly `{"status":"error","error":"invalid time bounds: end is earlier than start"}`
And this byte sequence matches the pre-extraction acceptance test recording
```

#### Scenario: The full workspace test suite stays green

```text
Given the parser now lives in `query-http-common`
When I run `cargo test --workspace`
Then every inline test, every acceptance test, and every cap test in the three crates passes
And no test count regression is observed
```

### Acceptance criteria

- [ ] `query-http-common::parse_time_range_seconds(start: Option<&str>, end:
  Option<&str>) -> Result<(u64, u64), String>` exists and reproduces every
  error path covered by the three pre-existing inline suites.
- [ ] The two consumer crates `query-api` and `log-query-api` no longer define
  `parse_time_range_seconds`, `parse_epoch_seconds`, or `seconds_to_nanos`
  (the seconds-to-nanos conversion lives in the consumer because each
  consumer builds a pillar-specific `TimeRange`).
- [ ] `trace-query-api` no longer defines the parser pair either; the
  `Option<&str>` shape is now the canonical surface.
- [ ] All bounds-parse-related inline tests in the three consumer crates are
  either removed (because the parser is gone from the crate) or moved into
  `query-http-common`'s own test module, with zero coverage loss.

### Outcome KPIs

- **Who**: read-side maintainers changing the wire time format.
- **Does what**: edit the time-range parser in a single file.
- **By how much**: from 3 parser copies to 1 (67% reduction).
- **Measured by**: file-count diff of any future ADR that touches the
  parser.
- **Baseline**: today, 3 parser copies, ~20 lines each plus tests.

### Elevator Pitch

Before: `parse_time_range_seconds` is defined three times across the read-side
crates and must be edited in lockstep when the wire format changes.
After: run `grep -r "fn parse_time_range_seconds" crates/query-api/src
crates/log-query-api/src crates/trace-query-api/src` → sees zero matches; the
parser lives only in `query-http-common`.
Decision enabled: the maintainer can extend the accepted wire time format
(adding RFC3339, for example) with a single source-file edit and inherit the
redaction-symmetric error texts everywhere.

### Technical notes

- The `Option<&str>` signature is chosen because it is the strictly more
  permissive shape: `query-api` and `log-query-api` wrap their `String` with
  `Some(&...)` at the call site; `trace-query-api` already uses the
  `Option<&str>` shape.
- `seconds_to_nanos` stays in each consumer crate because each consumer
  constructs its own pillar-specific `TimeRange` (`pulse::TimeRange`,
  `lumen::TimeRange`, `ray::TimeRange`). Sharing the function would force
  one of those types into `query-http-common`, which would cross the
  dependency boundary the way ADR-0048 Decision 5 explicitly cautioned
  against.

---

## US-03: Single-source the error envelope helper

`@infrastructure`

### Problem

Each of the three crates defines a private `fn error_response(status:
StatusCode, reason: &str) -> Response` that builds
`(status, Json(json!({"status":"error","error":reason}))).into_response()`.
The shape was first pinned by ADR-0042 (metrics), reproduced by ADR-0047
(logs), and reproduced again by ADR-0048 (traces). With ADR-0053 the same
helper is now used in four arms (two of them inside `trace-query-api`).
A maintainer who wants to add a `"request_id"` field to every error body
would have to edit three files and trust that no arm forgot to include the
new field.

### Who

The Kaleidoscope read-side maintainer changing the read-side error envelope
shape (adding a field, renaming a field, adjusting redaction strictness).

### Solution

`query-http-common` exports `pub fn error_response(status: StatusCode, reason:
&str) -> Response` returning the exact JSON body
`{"status":"error","error":"<reason>"}`. The three consumer crates remove
their private copies and call the shared helper.

### Domain examples

1. Maintainer adds a `"request_id"` field to every error body for log
   correlation. Before: edit three helpers. After: edit one helper in
   `query-http-common`.
2. The redaction posture is tightened to forbid the substring "token". Before:
   the assertion has to be reproduced in three test modules. After: the
   assertion lives once on the shared helper.
3. A future fourth read-side pillar inherits the envelope shape for free.

### UAT scenarios (BDD)

#### Scenario: The duplicated helper is removed from the three consumer crates

```text
Given `query-http-common` exports `error_response`
When I run `grep -rE "fn error_response\(" crates/query-api/src crates/log-query-api/src crates/trace-query-api/src`
Then the command exits 1 (no matches)
And every error arm in the three crates calls `query_http_common::error_response`
```

#### Scenario: The error body bytes are unchanged on every existing error arm

```text
Given the acceptance suite recordings of every 400 and every 401 across the three crates pre-extraction
When the acceptance suite runs after the extraction
Then every response body is byte-identical to its pre-extraction recording
And the status codes are unchanged
```

#### Scenario: The 401 fail-closed reason text is preserved per pillar

```text
Given each consumer crate calls `error_response(StatusCode::UNAUTHORIZED, "no tenant resolvable: the <pillar> service refuses unscoped requests")`
When a request arrives at any of the three crates with `tenant = None`
Then the response body still names the specific pillar ("query", "log query", "trace query")
And the envelope shape is identical across the three responses
```

### Acceptance criteria

- [ ] `query-http-common::error_response(status: StatusCode, reason: &str)
  -> Response` exists and produces the JSON body
  `{"status":"error","error":"<reason>"}`.
- [ ] The three consumer crates no longer declare a private `error_response`
  function.
- [ ] Every 400 and 401 acceptance test in the three consumer crates passes
  with byte-identical response bodies pre/post extraction.
- [ ] The pillar-specific 401 reason texts are unchanged (they remain string
  literals at each call site).

### Outcome KPIs

- **Who**: read-side maintainers changing the error envelope shape.
- **Does what**: edit the error body shape in a single file.
- **By how much**: from 3 helper copies to 1 (67% reduction).
- **Measured by**: byte-identical response body assertion across the
  acceptance suite (pre vs post).
- **Baseline**: today, 3 helper copies, with three independent redaction
  test stanzas.

### Elevator Pitch

Before: `error_response` is a private helper redefined in each of the three
read-side crates, with the JSON shape repeated verbatim and the redaction
assertions reproduced three times.
After: run `cargo test --workspace -p query-api -p log-query-api -p
trace-query-api` → sees every 400 and 401 acceptance test passing with
byte-identical response bodies, with `error_response` now defined exactly
once in `query-http-common`.
Decision enabled: the maintainer can change the error envelope shape (add a
`request_id`, tighten redaction, rename a field) in a single source-file
edit and observe the change propagate to all three pillars.

### Technical notes

- The helper signature is intentionally minimal (`StatusCode + &str`). If a
  future story needs a structured `ErrorBody` newtype, it can be added in a
  follow-on slice without breaking this surface.
- `serde_json::json!` is the implementation detail; the wire shape is the
  contract.

---

## US-04: Single-source the fail-closed tenant resolution pattern

`@infrastructure`

### Problem

Each of the three crates carries the same inline block at the top of every
handler:

```rust
let tenant = match &state.tenant {
    Some(t) => t.clone(),
    None => return error_response(StatusCode::UNAUTHORIZED,
        "no tenant resolvable: the <pillar> service refuses unscoped requests"),
};
```

This pattern appears once in `query-api` (in `handle_query_range`), once in
`log-query-api` (in `handle_logs`), and twice in `trace-query-api` (in
`handle_traces` and `handle_traces_by_id`). Four call sites, one shared
intent: "if the router's `Option<TenantId>` is `None`, return a 401 with the
pillar-named reason text". A maintainer who wants to add a single tracing
event on every 401 (e.g. `tracing::warn!(event = "tenant.fail_closed")`)
must remember to add it in four places.

> Note: the original brief described this as a "tenant extractor (Axum
> `FromRequestParts`)". There is no such extractor in the current code;
> tenant is resolved at the composition root and passed into `ApiState` as
> `Option<TenantId>`. The duplication is the inline `match` block above, not
> an extractor implementation. This story extracts what is actually there.

### Who

The Kaleidoscope read-side maintainer evolving the fail-closed tenancy
behaviour (adding a tracing event, switching to header-based tenancy, or
threading a `request_id`).

### Solution

`query-http-common` exports a helper:

```rust
pub fn resolve_tenant_or_refuse(
    tenant: &Option<TenantId>,
    service_label: &str, // e.g. "query", "log query", "trace query"
) -> Result<TenantId, Response>
```

that returns `Ok(tenant.clone())` on `Some(t)` and
`Err(error_response(UNAUTHORIZED, "no tenant resolvable: the <label> service
refuses unscoped requests"))` on `None`. The four handler arms become
`let tenant = match resolve_tenant_or_refuse(&state.tenant,
"<label>") { Ok(t) => t, Err(resp) => return resp };` which is a single
mechanical line per arm and is byte-for-byte equivalent on the wire.

### Domain examples

1. Maintainer adds `tracing::warn!(event = "tenant.fail_closed", service =
   service_label)` on every 401. Before: edit four call sites in three files.
   After: edit one function in `query-http-common`.
2. Maintainer verifies the three crates produce identical 401 envelopes (up
   to the pillar label). Before: visual diff across three crates. After: one
   inline test in `query-http-common` asserts the shape.
3. A future fourth read-side pillar gets fail-closed tenancy with a one-line
   call; there is no scaffold copy to maintain.

### UAT scenarios (BDD)

#### Scenario: The inline tenant-resolution block disappears from the consumer crates

```text
Given `query-http-common::resolve_tenant_or_refuse` exists
When I run `grep -rE "match &state\.tenant" crates/query-api/src crates/log-query-api/src crates/trace-query-api/src`
Then the command exits 1 (no matches)
And every handler arm uses the shared helper
```

#### Scenario: The 401 response body is byte-identical pre and post

```text
Given the acceptance suite recordings of every 401 across the three crates pre-extraction
When the acceptance suite runs after the extraction
Then every 401 response body is byte-identical to its pre-extraction recording
And the pillar-specific suffix ("the query service", "the log query service", "the trace query service") is preserved
```

#### Scenario: The pillar label flows through cleanly

```text
Given a handler calls `resolve_tenant_or_refuse(&state.tenant, "trace query")` with `tenant = None`
When the helper is invoked
Then the returned `Response` body is exactly `{"status":"error","error":"no tenant resolvable: the trace query service refuses unscoped requests"}`
```

### Acceptance criteria

- [ ] `query-http-common::resolve_tenant_or_refuse(tenant: &Option<TenantId>,
  service_label: &str) -> Result<TenantId, Response>` exists and reproduces
  every 401 reason text by interpolating the `service_label`.
- [ ] The four handler arms in the three consumer crates call the shared
  helper; the inline `match &state.tenant` block is gone.
- [ ] Every 401 acceptance test in the three consumer crates passes with
  byte-identical response bodies pre/post extraction.

### Outcome KPIs

- **Who**: read-side maintainers evolving fail-closed tenancy.
- **Does what**: change the tenant-resolution behaviour in a single file.
- **By how much**: from 4 call-site copies to 1 (75% reduction).
- **Measured by**: file-count diff of any future change to the fail-closed
  401 reason text or tracing event.
- **Baseline**: today, 4 inline `match` blocks across 3 crates.

### Elevator Pitch

Before: the fail-closed tenant `match &state.tenant` block is reproduced at
four handler arms across three crates, with the 401 reason text repeated as
a string literal in each one.
After: run `cargo test --workspace -p query-api -p log-query-api -p
trace-query-api` → sees every 401 acceptance test passing with byte-identical
response bodies; the resolution pattern is defined exactly once in
`query-http-common::resolve_tenant_or_refuse`.
Decision enabled: the maintainer can change the fail-closed tenancy
behaviour (add a tracing event, evolve the reason text, thread a request
context) in a single source-file edit.

### Technical notes

- The helper deliberately accepts `&Option<TenantId>` (a borrow of the option
  living in `ApiState`) so the existing handler shape needs no change beyond
  the call site.
- `aegis::TenantId` is a workspace dep; `query-http-common` takes a direct
  dependency on `aegis` so the helper can return `Result<TenantId, _>`.
- This is the minimum useful extraction. A future story could promote the
  helper into a proper Axum `FromRequestParts` extractor; that is a
  larger change and out of scope here.

---

## US-05: Rewire the three consumer crates onto the shared scaffolding

`@infrastructure`

### Problem

The first four stories deliver `query-http-common` and the three rewirings
piecewise. This story is the integration gate: it verifies that the
duplicated scaffolding is actually GONE (not just shadowed) and that the
total source-line count for the read-side HTTP scaffold across the three
crates falls from approximately 90 to approximately 30 (the residual is the
call-site wiring plus pillar-specific glue, primarily the
`seconds_to_nanos` conversion that necessarily stays per-consumer).

### Who

The Kaleidoscope read-side maintainer wanting one observable property:
"the scaffolding lives in exactly one place".

### Solution

After US-01..US-04 land, this story verifies the integration:

- `cargo test --workspace` is green with the same test count as pre-feature.
- `cargo mutants -p query-http-common` reports a 100% kill rate (per
  ADR-0005 Gate 5).
- The total LOC of duplicated HTTP scaffolding across the three crates,
  measured as the sum of lines containing the patterns (`MAX_WINDOW_SECONDS
  =`, `MAX_RESULT_ROWS =`, `fn parse_time_range_seconds`, `fn
  parse_epoch_seconds`, `fn error_response`, `match &state.tenant`), is
  ≤ 30 (was approximately 90 pre-feature).

### Domain examples

1. Maintainer audits the read-side scaffolding footprint as part of an
   ADR-0050 follow-up. Before: 90 lines of scaffold across three files.
   After: 30 lines of glue plus one canonical 60-line crate.
2. A new contributor reading `crates/log-query-api/src/lib.rs` sees the
   handler logic without 40 lines of parse-and-redact preamble.
3. The mutation gate runs on the canonical scaffolding once, in
   `query-http-common`, rather than three times in three places.

### UAT scenarios (BDD)

#### Scenario: The full workspace test suite is green after rewiring

```text
Given US-01..US-04 have all landed
When I run `cargo test --workspace`
Then every test in every crate passes
And the test count is equal to or greater than pre-feature (no test was
silently deleted)
```

#### Scenario: The mutation gate on `query-http-common` reaches 100%

```text
Given `query-http-common` is the canonical home of the scaffolding
When I run `cargo mutants -p query-http-common --no-shuffle`
Then the report shows zero MISSED, zero TIMEOUT, zero UNVIABLE survivors
And the kill rate is 100%
```

#### Scenario: The duplicated scaffolding LOC falls below the gate

```text
Given the rewiring is complete
When I run the scaffolding-LOC counter (a small shell pipeline that greps the six pattern lines in the three consumer crates' `src/lib.rs` and sums them)
Then the count is ≤ 30 (down from ~90)
And the bulk of the scaffolding lives in `crates/query-http-common/src/lib.rs`
```

### Acceptance criteria

- [ ] `cargo test --workspace` passes with no test-count regression.
- [ ] `cargo mutants -p query-http-common` reports 100% kill rate.
- [ ] The scaffolding-LOC counter (recorded in the slice brief) returns a
  value ≤ 30 across the three consumer crates.
- [ ] `crates/query-http-common/` is listed in the workspace `members`
  array in the root `Cargo.toml`.

### Outcome KPIs

- **Who**: the Kaleidoscope codebase as a whole.
- **Does what**: hosts the read-side HTTP scaffolding in a single source
  location.
- **By how much**: scaffolding-LOC across the three consumer crates falls
  from approximately 90 to ≤ 30 (≥ 67% reduction).
- **Measured by**: a small grep-and-sum shell pipeline run pre and post the
  feature.
- **Baseline**: today, approximately 90 lines of scaffolding across three
  consumer crates.

### Elevator Pitch

Before: the read-side HTTP scaffolding (cap constants, time-range parser,
error envelope helper, fail-closed tenant pattern) is duplicated across the
three consumer crates with approximately 90 lines of scaffold.
After: run `cargo test --workspace && cargo mutants -p query-http-common`
→ sees every test green, a 100% mutation kill rate on the new crate, and
the duplicated scaffolding LOC across the three consumer crates falls to
≤ 30.
Decision enabled: the maintainer can change any element of the read-side
HTTP scaffolding (caps, parser, envelope, tenancy) in a single source-file
edit and trust the change to propagate to the three pillars.

### Technical notes

- This story is the integration gate, not a new piece of work. It runs after
  US-01..US-04 and verifies their composition.
- The LOC counter is a small shell pipeline; the exact form lives in the
  slice brief produced by DESIGN. The KPI target is the number, not the
  command.
- Per ADR-0005 Gate 5, the new crate must hit 100% mutation kill before
  this story is `Done`.

---

## JTBD trace

JTBD analysis was skipped (Decision 4 = No) because this is an internal
refactor with no user-facing behaviour change. The maintainer's job
("change a read-side HTTP scaffolding element in one place and trust the
change to propagate") is named in each story's `Decision enabled` line
and is the unifying outcome the five stories deliver.
