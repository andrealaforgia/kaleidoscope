# DESIGN Decisions — log-body-text-search-v0

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.
Mode: propose (Decision 1 of `/nw-design`).

## Wave context

Brownfield carpaccio slice on `crates/log-query-api`, parallel in
shape to `log-query-severity-filter-v0`. ONE optional query-string
parameter `body_contains=<string>` on `GET /api/v1/logs` filters
returned `LogRecord`s to those whose `body` field contains the
supplied substring. First real-world consumer of `query-http-common`
(ADR-0054, M-5) born AFTER the extraction; the slice exercises the
shared surface (cap consts, `REASON_*` consts, `error_response`,
`resolve_tenant_or_refuse`, `parse_time_range`) and verifies the
extraction is sufficient for a brand-new parse-and-wire arm.

## Read-first verification (source-grounded)

The DESIGN wave verified every load-bearing premise by direct read
before pinning the flags below:

- [x] `crates/lumen/src/predicate.rs:25-28` — confirms `Predicate`
      today carries `service: Option<String>` and
      `min_severity: Option<SeverityNumber>` ONLY. No
      `body_contains` field exists. FLAG 3 verification: lumen
      surface extension IS required if the filter lives in the
      predicate.
- [x] `crates/lumen/src/predicate.rs:53-66` — confirms `matches` is
      conjunctive AND across set filters; an unset filter is
      skipped. A new arm
      `if let Some(target) = self.body_contains.as_deref() { if !record.body.contains(target) { return false; } }`
      composes cleanly with the existing two arms.
- [x] `crates/lumen/src/predicate.rs:70-72` — confirms `is_empty()`
      checks both fields; the new field MUST be added to this
      check so an empty `Predicate::new()` still reports `true`.
- [x] `crates/lumen/src/store.rs:89` — confirms `LogStore::query_with`
      signature; trait is unchanged by this slice.
- [x] `crates/lumen/src/store.rs:175` — confirms
      `InMemoryLogStore::query_with` filters via
      `predicate.matches(r)`. Extending `Predicate` lights up this
      adapter with zero additional code in `in_memory`.
- [x] `crates/lumen/src/file_backed.rs:245` — confirms
      `FileBackedLogStore::query_with` filters via
      `predicate.matches(r)`. Same: extending `Predicate` lights
      up this adapter with zero additional code in `file_backed`.
- [x] `crates/lumen/src/record.rs:54` — confirms `LogRecord.body`
      is a plain `String`. `String::contains(&str)` is the
      substring primitive; byte-wise; case-sensitive by default.
- [x] `crates/lumen/src/lib.rs:57` — confirms `Predicate` is on
      the lumen public surface (`pub use predicate::Predicate;`).
      Adding a field + builder is a public-surface change governed
      by Gate 2 `cargo public-api`. ADR-0055 is therefore required
      per FLAG 6.
- [x] `crates/log-query-api/src/lib.rs:104-109` — confirms
      `LogsParams { start, end, min_severity }` shape post-M-5
      rewire; the new `body_contains: Option<String>` field lands
      beside `min_severity`.
- [x] `crates/log-query-api/src/lib.rs:120,127,129,137-142,155-159,180-185,190-193`
      — confirms ALL the shared scaffolding (tenant seam, time
      parser, cap reasons, error envelope, result cap) is consumed
      via `query_http_common::`. Zero local re-declarations exist
      to duplicate.
- [x] `crates/log-query-api/src/lib.rs:165-172` — confirms the
      existing dispatch is `match min_severity { Some -> query_with
      with predicate carrying min_severity; None -> query }`. The
      new dispatch grows the predicate-bearing branch to ALSO
      carry `body_contains`, and the `None`-arm trigger condition
      becomes "both filters absent".
- [x] `crates/query-http-common/src/lib.rs` — confirms the public
      surface this slice consumes; no new helper needed in
      `query-http-common` (the body-contains parser is a
      single-pillar concern; `body` exists on `lumen::LogRecord`
      only, not on `pulse::MetricPoint` or `ray::Span`).
- [x] `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md`
      — read; the originating contract for `/api/v1/logs`,
      Decision 1 fixes the bare-JSON-array success shape, the
      `{status:"error", error:"<reason>"}` envelope, and the
      redaction posture this slice reuses.
- [x] `docs/product/architecture/adr-0052-log-query-severity-filter.md`
      — read; the immediate sibling slice. This slice mirrors
      ADR-0052 in shape: one optional parameter, one new parse
      helper, one new dispatch arm, one new 400 reason class,
      filter-BEFORE-cap interaction.
- [x] `docs/product/architecture/adr-0054-query-http-common-extraction.md`
      — read; this slice is the FIRST consumer of the M-5
      extraction born AFTER the extraction shipped. KPI-3 (the
      slice's CI assertion suite) is the honest measure that the
      shared crate paid for itself.
- [x] `ls docs/product/architecture/adr-0055*` — verified the slot
      is free; `adr-0054-query-http-common-extraction.md` is the
      latest ADR in the workspace.

## DESIGN Decisions

### DD1: Substring matching, NOT regex (FLAG 1 pinned)

Slice 01 ships `String::contains(&str)` — substring matching ONLY.
Regex is a separate future slice (`log-body-regex-search-vN`) with
its own ReDoS budget and expression-grammar contract (PCRE vs RE2
vs the `regex` crate's syntax). The substring path is the simplest
predicate over a `String` field, the most predictable in cost, and
the lowest-surprise default.

### DD2: Case-sensitive matching, byte-wise (FLAG 2 pinned)

`body_contains=KAFKA` does NOT match a record whose body is
`kafka timeout`. The match is byte-wise via `String::contains`.
Rationale: grep is operator muscle memory; a case-folding default
risks false-positive matches on platform boilerplate (a customer
substring `INFO connection refused` should NOT match the
platform's own `severity_text: "INFO"` text). Case-insensitive
matching is a future slice (a separate `body_contains_ci=<string>`
parameter or a `case_sensitive=false` flag); the slice's KPI-4
acceptance test pins the case-sensitive rule so operators learn
the posture from where they will look.

### DD3: Lumen Predicate seam — EXTEND (FLAG 3 pinned)

Grep-verified premise: `lumen::Predicate` does NOT today carry a
`body_contains` field (`crates/lumen/src/predicate.rs:25-28`). The
slice EXTENDS `Predicate` with:

- One additive field `body_contains: Option<String>` on the
  struct.
- One new builder method
  `Predicate::body_contains(s: impl Into<String>) -> Self`,
  mirroring the existing `service(name)` and `min_severity(sev)`
  builders.
- One new arm in `Predicate::matches`:
  `if let Some(target) = self.body_contains.as_deref() { if !record.body.contains(target) { return false; } }`.
- One new clause in `is_empty()`: `self.body_contains.is_none()`
  joined with the existing two `is_none()` checks via AND.

The alternative (apply the substring filter handler-side on the
`Vec<LogRecord>` returned by `query_with`, leaving `lumen::Predicate`
byte-identical) is REJECTED for slice 01: it splits the predicate
semantics across two crates, breaks the "the predicate IS the
filter" invariant the lumen surface established, and prevents the
v1 columnar substrate from pushing the substring scan into the
storage adapter where it belongs.

The `lumen::LogStore` trait signatures stay byte-identical to the
prior tag (Gate 2 `cargo public-api`); the `query_with` method on
both `InMemoryLogStore` (`crates/lumen/src/store.rs:175`) and
`FileBackedLogStore` (`crates/lumen/src/file_backed.rs:245`)
already routes through `predicate.matches(r)`. Extending the
predicate lights up BOTH adapters with zero new impl code in
either store file.

### DD4: Empty `body_contains` value is a 400 (FLAG 4 pinned)

`?body_contains=` arrives as `Some("")` from serde. The handler
rejects it as a 400 with the literal reason `invalid body_contains`
via `query_http_common::error_response`. Rationale: an empty
substring is meaningless on `String::contains` (every string
contains the empty substring, so the filter would silently match
every record, observably indistinguishable from no filter). The
slice refuses the ambiguity out loud, symmetric with
`log-query-severity-filter-v0`'s rejection of `Some("")` on the
severity parameter (ADR-0052 Decision 5). The store is NEVER
touched on the empty-string 400 arm.

### DD5: Anti-echo on the empty-string 400 (FLAG 5 pinned)

The 400 body is the LITERAL envelope
`{"status":"error","error":"invalid body_contains"}`. The reason
text is a static literal constant; the raw `body_contains` value
is NEVER interpolated into the response. For the empty-string arm
specifically there is no non-empty raw value to echo; the pin
records the redaction posture for any future arm that adds a
raw-value-bearing reason (e.g. `body_contains exceeds maximum
length`, `body_contains contains invalid UTF-8`). Symmetric with
ADR-0047 Decision 1, ADR-0050 Decision 7, ADR-0052 Decision 1.

### DD6: Length cap on `body_contains` — 1024 bytes (CAP pinned)

The parser rejects any non-empty value whose byte length strictly
exceeds 1024 bytes. The rejection uses the SAME literal envelope
as the empty-string arm: HTTP 400 with
`{"status":"error","error":"invalid body_contains"}`. No second
reason class is introduced; the redaction posture is the same
(the raw oversize value is NEVER echoed). Rationale: an unbounded
substring length lets a malicious client ship megabytes inside a
query-string parameter; the cap is large enough to accommodate any
honest error string a human or runbook would carry (kafka stack
traces, full sentence reasons) and small enough to refuse abuse.
1024 bytes is the same order of magnitude the axum stack already
imposes on request lines for header fields, so the cap is
internally consistent rather than novel.

### DD7: ADR-0055 IS authored (FLAG 6 pinned)

DD3 lands as a lumen public-surface extension (one field, one
builder method, one new `matches` arm, one new `is_empty` clause);
the change is visible in `cargo public-api` diff. ADR-0001
established immutability for ADRs in this repository; ADR-0047,
ADR-0050, ADR-0052, and ADR-0054 are all preserved unchanged. The
contract growth lands as a new ADR-0055 with cross-references back
to ADR-0047 (originating logs read contract), ADR-0052
(immediate sibling — first optional parameter), and ADR-0054 (the
shared-scaffolding consumer relationship). ADR-0055 number
verified free (`ls docs/product/architecture/adr-0055*` returns
no hits; ADR-0054 is the latest).

## Reuse Analysis

| Component | File | Decision | Justification |
|---|---|---|---|
| `LogsParams` struct | `crates/log-query-api/src/lib.rs:104-109` | EXTEND (add `body_contains: Option<String>`) | Natural additive field parallel to the existing `min_severity: Option<String>`; serde deserialises a missing parameter as `None`; private struct, no `cargo public-api` diff. |
| `parse_body_contains` parse helper | `crates/log-query-api/src/lib.rs` (NEW free fn next to `parse_min_severity`) | NEW small helper | Mirrors the `parse_min_severity` pattern; one place to enforce empty-string rejection and the 1024-byte length cap; one place mutation testing targets. The helper is HTTP-boundary-shaped; it does NOT belong on `lumen::Predicate`. |
| `query_http_common::resolve_tenant_or_refuse` | `crates/query-http-common/src/lib.rs:235` | REUSE | First real-world validation of M-5 from a slice born AFTER the extraction. Zero re-implementation. |
| `query_http_common::MAX_WINDOW_SECONDS` / `MAX_RESULT_ROWS` | `crates/query-http-common/src/lib.rs:69,75` | REUSE | Cap consts; the body-contains filter must NOT alter either cap value or either cap location. |
| `query_http_common::error_response` / `ErrorBody` | `crates/query-http-common/src/lib.rs:264` | REUSE | The 400 envelope shape is contract-pinned via this helper; zero new envelope code in `log-query-api`. |
| `query_http_common::parse_time_range` | `crates/query-http-common/src/lib.rs:174` | REUSE | The window-bounds parser stays unchanged; the new parameter does NOT alter window semantics. |
| `query_http_common::REASON_WINDOW_TOO_LARGE`, `REASON_TOO_MANY_ROWS` | `crates/query-http-common/src/lib.rs:89,94` | REUSE | The cap-reason literals; the slice does NOT introduce a new cap-reason class. |
| `lumen::Predicate` | `crates/lumen/src/predicate.rs:25-28` | EXTEND (add `body_contains: Option<String>` field, `body_contains(s)` builder, one `matches` arm, one `is_empty` clause) | Grep-verified absent today; DD3 pins this as the seam. Public-surface change governed by ADR-0055 and Gate 2 `cargo public-api`. |
| `lumen::LogStore::query_with` | `crates/lumen/src/store.rs:89` | REUSE (trait signature unchanged) | The existing trait method carries the new predicate field via `predicate.matches(r)`. Gate 2 confirms zero trait-signature diff. |
| `InMemoryLogStore::query_with` | `crates/lumen/src/store.rs:159-180` | REUSE (zero impl change) | Already routes via `predicate.matches(r)` at line 175; the extended predicate's new `matches` arm fires automatically. |
| `FileBackedLogStore::query_with` | `crates/lumen/src/file_backed.rs:229-250` | REUSE (zero impl change) | Already routes via `predicate.matches(r)` at line 245; same automatic light-up as `InMemoryLogStore`. |

Zero CREATE NEW decisions at the workspace level beyond the new
ADR-0055 and the new design / acceptance artefacts. No new crate,
no new module, no new external dependency.

## Architecture Summary

- **Pattern**: brownfield additive carpaccio on an existing HTTP
  read endpoint; ports-and-adapters preserved (the `lumen::LogStore`
  port is unchanged; the predicate value-object grows one field;
  both adapters route automatically through `matches`).
- **Paradigm**: idiomatic Rust (data + free functions + traits
  where polymorphism is genuine), per the project's CLAUDE.md
  Development Paradigm note. The slice adds data (one struct
  field, one parser free fn, one builder method) and one new arm
  in a pure function (`Predicate::matches`); no new trait, no new
  `dyn` indirection.
- **Key components touched**:
  - `crates/log-query-api/src/lib.rs` — one new field on
    `LogsParams`, one new parse helper, one new parse step in the
    handler, one extended dispatch arm.
  - `crates/lumen/src/predicate.rs` — one new field, one new
    builder, one new `matches` arm, one new `is_empty` clause.
  - No edits to any other file in the workspace.
- **Surface change posture**: `lumen::Predicate` grows additively
  (one new builder method; the existing constructors and builders
  remain byte-identical; an old caller is byte-compatible).
  `cargo public-api` diff is non-empty BUT BACKWARD-COMPATIBLE
  (additive only). The `lumen::LogStore` trait signatures stay
  byte-identical.

## Technology Stack

No new external dependencies. The slice uses:

- `axum` (existing, ADR-0047) — HTTP routing and `Query` extractor.
- `serde` (existing) — `Deserialize` derive on `LogsParams`.
- `String::contains` (Rust std) — the substring primitive.

No third-party API consumed; no network call; no new crate
dependency declared in any `Cargo.toml`.

## Constraints Established

- `query-http-common` is the SOLE provider of caps, reasons,
  envelope helper, tenant seam, and bounds parser. Zero new
  duplications in `log-query-api` (KPI-3 CI-enforced).
- New lines in `crates/log-query-api/src/lib.rs` under 30 (KPI-3
  CI-enforced).
- `lumen::Predicate` grows additively (one field + one builder +
  one `matches` arm + one `is_empty` clause); the `LogStore`
  trait stays byte-identical (Gate 2 `cargo public-api`).
- `lumen::Predicate::matches` retains conjunctive AND composition;
  the new arm fits between the existing two arms without
  reordering them.
- Substring matching, NOT regex.
- Case-sensitive matching, byte-wise.
- `LogRecord.body` field ONLY; `severity_text`, attributes, and
  resource attributes are OUT of scope.
- Empty `body_contains` and over-cap `body_contains` are both 400
  with the literal envelope; the raw value is NEVER reflected.
- Default (parameter absent) is byte-equal to the slice-prior
  response (KPI-2 CI-enforced).
- The result cap measures post-filter records (cap AFTER filter,
  symmetric with `log-query-severity-filter-v0`).
- Cross-tenant isolation invariant holds for the new arm; the
  tenant-bucket lookup at `crates/lumen/src/store.rs:166-172` and
  `crates/lumen/src/file_backed.rs:236-242` precedes any
  predicate evaluation.

## DEVOPS Handoff

- **No new crate.** `Cargo.toml` workspace membership unchanged.
- **No new external dependency.** No new license to record.
- **No new CI job.** The existing `gate-5-mutants-log-query-api`
  workflow scopes mutation testing to `crates/log-query-api/`
  via `--in-diff`; the existing `gate-5-mutants-lumen` workflow
  (or the workspace-default mutants gate at the lumen crate)
  scopes mutation testing to `crates/lumen/` via `--in-diff`.
  Both workflows pick up the per-feature modified files
  automatically (`cargo mutants --in-diff origin/main` shape
  used elsewhere in the methodology).
- **No new external integration.** No third-party API consumed;
  no consumer-driven contract test recommendation. The store
  call uses an in-process trait method against the durable
  `FileBackedLogStore`, which is a first-party library.
- **Gate 2 `cargo public-api`**: expected diff is additive only
  on `lumen` (one new pub builder method on `Predicate`); zero
  diff on `log-query-api`, `query-http-common`, `aegis`, every
  other crate. The crafter MUST snapshot the new public-api
  baseline as part of the DELIVER wave.
- **Earned-Trust startup probe (ADR-0049 / ADR-0050 / ADR-0052)**
  is UNCHANGED by this slice. The probe issues a parameter-less
  empty-range `query`; the new optional parameter does not alter
  the probe shape, and the predicate extension is field-additive
  with a `Default` impl that preserves the existing probe
  behaviour (the probe's predicate, if any, is byte-equal to the
  prior tag).

## Upstream Changes

**None.** Zero DISCUSS assumptions changed. Zero DIVERGE artefacts
to back-propagate against (no DIVERGE wave was run for this thin
slice). The slice composes additively on top of ADR-0047,
ADR-0050, ADR-0052, and ADR-0054 without altering any of them.

## Handoff to DISTILL

The DISTILL wave (`@nw-acceptance-designer`) inherits:

1. The six DESIGN decisions (DD1-DD7) above, each pinned with
   rationale and source-grounded verification.
2. The Reuse Analysis table: every component touched, every
   component reused.
3. `application-architecture.md` (this design wave's sibling
   artefact): the sequence diagram, the per-file change table,
   the error contract table.
4. `parse-helper-spec.md`: the exact signature and the named test
   cases the acceptance suite must include.
5. `docs/product/architecture/adr-0055-log-body-text-search.md`:
   the durable contract growth pin for `lumen::Predicate` and the
   HTTP read endpoint.

The crafter (DELIVER wave) MUST honour the file-level constraints
named in `application-architecture.md` § "Changes Per File" and
the parser test surface named in `parse-helper-spec.md`.
