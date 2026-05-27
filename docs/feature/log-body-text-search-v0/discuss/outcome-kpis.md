# Outcome KPIs: log-body-text-search-v0

## Feature: log-body-text-search-v0

### Objective

By the close of slice 01, an on-call SRE (or an automated incident
classifier, or a support engineer triaging a customer ticket) can ask `GET
/api/v1/logs` for "records whose body contains this exact substring" with
one optional query-string parameter `body_contains=<string>` and have the
platform return only the matching records, in the same JSON shape as
today. The default (no parameter) behaves exactly as before. The error
envelope on a malformed (empty) value is the existing redaction-preserving
400, sourced from `query-http-common`. No new envelope shape. No new
status code. No cap change. Slice <= 1 day.

The slice also serves as the FIRST real-world consumer of
`query-http-common` (ADR-0054, M-5) born AFTER the extraction. KPI-3 below
encodes that double-value posture: if `query-http-common` is sound, the
slice gets caps, tenant resolution, the error envelope, and the bounds
parser for free (by declaration of dependency); the line count in
`log-query-api` grows under 30 lines.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|---|---|---|---|---|---|
| KPI-1 | SRE operators, automated incident classifiers, and support engineers triaging customer reports on `/api/v1/logs` | Issue narrowed reads using `body_contains=<substring>` and receive ONLY records whose `body` field actually contains the substring | 100% of returned records on the acceptance fixture have `body` containing the supplied substring AND 100% of fixture records whose `body` contains the substring appear in the response. The match is HONEST: the substring is in `body`, not in `severity_text`, not in any attribute, not in any resource attribute, not by accident on JSON field overlap | 100% of in-window records returned today (no body filter exists in the HTTP boundary) | Seeded-fixture acceptance assertion in `crates/log-query-api/tests/slice_01_body_contains.rs`: for every returned record `r`, `assert!(r.body.contains(substring))`; for every fixture record `r` with `r.body.contains(substring)` and `range.contains(r.observed_time_unix_nano)`, `assert!(response.iter().any(\|x\| x == &r))` | Leading (outcome) |
| KPI-2 | Existing log-query-api clients (Marcus's hourly automation, any current `curl` user, the alerting pipe from log-query-severity-filter-v0, any prism log panel when it lands) | Continue issuing requests WITHOUT `body_contains` and receive the same response as the slice-prior tag (whether they pass `min_severity` or not) | 0 broken clients: 100% of slice-prior acceptance scenarios in `tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`, and `tests/slice_01_severity_filter.rs` continue to pass on the new build, with NO test deletion and NO test rewrite | Today: 100% pass on the slice-prior tag (M-5 close) | Existing acceptance suites continue green after the slice ships; the new file `tests/slice_01_body_contains.rs` is ADDITIVE | Leading (guardrail) |
| KPI-3 | The `query-http-common` (ADR-0054) extraction itself, validated by being consumed by a slice born AFTER it | Provide the cap constants, the reason constants, the error envelope helper, the tenant seam, and the bounds parser to a brand-new parse-and-wire arm WITHOUT any duplication in the consumer crate | 0 new duplications in `log-query-api`: no new `MAX_RESULT_ROWS` const, no new `MAX_WINDOW_SECONDS` const, no new `REASON_*` const, no re-implementation of `error_response`, no re-implementation of `resolve_tenant_or_refuse`, no re-implementation of `parse_time_range`. New lines added to `crates/log-query-api/src/lib.rs` under 30 (envelope: a `body_contains` parse helper, a parameter field on `LogsParams`, a dispatch arm for the composed predicate) | Today: `query-http-common` has three consumers, ALL born BEFORE the extraction. This slice is the FIRST consumer born after | Static-grep assertions in CI (no inline copies of the cap constants, the reason texts, or the envelope shape inside `crates/log-query-api/`); line-count assertion on the diff of `crates/log-query-api/src/lib.rs` between the slice-prior tag and the slice-close tag | Leading (validation of M-5) |
| KPI-4 | SRE operators reading the acceptance suite to learn the platform's matching posture | Discover from a test (not from a comment, not from folklore, not from source-code spelunking) that `body_contains` matching is CASE-SENSITIVE: `body_contains=KAFKA` does NOT match a record whose body is `kafka timeout` | 1 acceptance scenario explicitly asserts `KAFKA` returns `[]` against a `kafka timeout` fixture; the scenario name carries the word `case-sensitive` so a search of the test file surfaces it | Today: no test exists because no parameter exists; the case-sensitivity rule is undefined | Acceptance test `case_sensitive_matching_is_pinned_by_acceptance_test` (or the equivalent named-scenario test fn) in `tests/slice_01_body_contains.rs`: status code, response array length, and a negative substring assertion that the body of the (absent) `kafka timeout` record never appears in the response bytes | Leading (documentation-via-test) |

### Metric Hierarchy

- **North Star (KPI-1)**: the substring filter is HONEST. Every record in
  the response carries the substring in its `body`; every fixture record
  carrying the substring in its `body` is in the response. The slice's
  whole point is "operator asks for records carrying this string, server
  delivers exactly those records, no more no less". If KPI-1 does not
  hold, the slice has no value regardless of what else ships.
- **Leading indicators**:
  - Narrowed-read adoption: count of `/api/v1/logs` requests carrying
    `body_contains`, vs total. (Not instrumented at slice 01; the slice
    ships no new metric per ADR-0050 Decision 8 posture — the platform
    has no live observability of its own at v0/v1. Recorded as a
    follow-up.)
  - Average post-filter record count vs pre-filter record count on the
    same window-and-tenant-and-substring tuple. Not instrumented at slice
    01; recorded as a follow-up.
- **Guardrail metrics (KPI-2, KPI-3, KPI-4)**:
  - Backward-compat (KPI-2): pre-existing acceptance suites stay green;
    pre-existing response bytes are byte-equal on the no-parameter path
    and on the `min_severity`-only path.
  - M-5 validation (KPI-3): `query-http-common` is the SOLE provider of
    the cap constants, the reason constants, the envelope helper, the
    tenant seam, and the bounds parser. The slice ships ZERO new copies
    of any of them. The line-count budget on `log-query-api` (under 30
    new LOC) is the honest measure that the shared crate paid for itself
    on its first post-extraction consumer.
  - Case-sensitivity discoverable (KPI-4): the platform's case-sensitive
    posture is documented IN the acceptance suite, not in a comment, not
    in a wiki, not in a runbook. Future operators learn the rule from a
    test.
  - Redaction: the new 400 arm honours ADR-0047 Decision 1 + ADR-0050
    Decision 7 + ADR-0052 Decision 1 (no raw parameter value, no
    forwarded credential).
  - Cap preservation: `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS`
    consumed from `query-http-common`; the existing cap acceptance
    scenarios stay green; the result cap measures the post-filter
    records vector.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|---|---|---|---|---|
| KPI-1 | Seeded fixture in `tests/slice_01_body_contains.rs` | Per-record assertion (every returned record's body contains the substring) plus completeness assertion (every fixture record whose body contains the substring is in the response) | On every CI run (existing per-feature mutation + acceptance gate) | crafter (DELIVER wave) |
| KPI-2 | Existing acceptance suites (`tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`, `tests/slice_01_severity_filter.rs`) | CI green gate on the pre-existing suites; NO test deletion, NO test rewrite | On every CI run | crafter (DELIVER wave) |
| KPI-3 | `crates/log-query-api/src/lib.rs` source diff between the slice-prior tag and the slice-close tag | Static-grep CI assertions: `! grep -n 'MAX_RESULT_ROWS\s*:\s*usize' crates/log-query-api/src/`, `! grep -rE 'window exceeds 86400 seconds\|result exceeds 100000 rows\|no tenant resolvable' crates/log-query-api/src/`, `! grep -n 'fn error_response\|fn parse_time_range\|fn resolve_tenant' crates/log-query-api/src/`. Line-count diff: `git diff <prior-tag>..HEAD -- crates/log-query-api/src/lib.rs \| awk '/^\+[^+]/ {a++} END {print a}'` < 30 | On every CI run | crafter (DELIVER wave) |
| KPI-4 | New acceptance test in `tests/slice_01_body_contains.rs` | Negative-match assertion: `body_contains=KAFKA` against a `kafka timeout` fixture returns `[]`; the test function name carries the literal `case_sensitive` | On every CI run | crafter (DELIVER wave) |

No new dashboard. No new metric counter. No new tracing event beyond the
existing `tracing::error!` calls in the 500 arm. This is consistent with
ADR-0050 Decision 8: at v0/v1 the platform has no live observability stack
of its own; a contract-shaped outcome IS the signal.

### Hypothesis

We believe that exposing a substring filter on `LogRecord.body` through
one optional query-string parameter on `GET /api/v1/logs`, for SRE
operators, automated incident classifiers, and support engineers triaging
customer reports who hold a known error string in hand, will produce
honest server-side narrowing (every returned record contains the
substring; every record carrying the substring is returned) while keeping
every existing client byte-equal on the default path AND validating
`query-http-common` (ADR-0054, M-5) as a real source of caps, envelopes,
tenant seam, and bounds parser. We will know this is true when:

- KPI-1: every returned record's `body` contains the substring AND every
  fixture record whose `body` contains the substring is returned.
- KPI-2: every pre-existing acceptance scenario in
  `tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`, and
  `tests/slice_01_severity_filter.rs` stays green.
- KPI-3: zero new duplications of cap constants, reason texts, envelope
  helper, tenant seam, or bounds parser appear inside `log-query-api`;
  the new-line budget on `crates/log-query-api/src/lib.rs` is under 30.
- KPI-4: a named acceptance scenario pins case-sensitive matching with
  `KAFKA` returning `[]` against a `kafka timeout` fixture.

### Handoff to DEVOPS

No instrumentation requested at slice 01. The KPIs above are
CI-test-fixture measured (KPI-1, KPI-4), CI-gate enforced (KPI-2), and
CI-static-grep enforced (KPI-3). A successor slice may add
narrowed-read-adoption counters and post-filter record-count histograms
once the platform has a live observability stack of its own; that is OUT
of this slice's scope and recorded as a forward-looking item.
