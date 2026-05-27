<!-- markdownlint-disable MD013 -->

# Test Scenarios — log-body-text-search-v0

Author: `nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-27.
File under test: `crates/log-query-api/tests/slice_04_body_contains.rs`
(8 scenarios; all `#[ignore]`'d at DISTILL close — see
`wave-decisions.md` § D2). Contract pinned by ADR-0055.

## Scenario table

| AC-id | Story | Given | When | Then | HTTP status | Body |
|---|---|---|---|---|---|---|
| AC-01 | US-01 walking skeleton | Tenant `acme-prod` has 5 records seeded into a REAL durable `FileBackedLogStore` inside `[1716200000s, 1716200060s)`: 3 records whose `body` carries `"kafka timeout"` + 2 heartbeat records | The SRE GETs `/api/v1/logs?start=1716200000&end=1716200060&body_contains=kafka%20timeout` for `acme-prod` | Exactly 3 records returned (each `body` contains `"kafka timeout"`); ascending `observed_time_unix_nano` order; no heartbeat record appears | 200 | Bare JSON array of 3 `LogRecord`s |
| AC-02 | US-02 calm empty | Tenant `acme-prod` has the same 5 records, none whose `body` contains `"cassandra"` | The support engineer GETs over the window with `body_contains=cassandra` | Response is the calm empty bare array `[]`; NEVER 404; NEVER 500 | 200 | `[]` |
| AC-03 | US-03 default unchanged | Tenant `acme-prod` has the same 5 records | The automation client GETs over the window with NO `body_contains` parameter | All 5 in-window records returned (backward-compat) | 200 | Bare JSON array of 5 `LogRecord`s |
| AC-04a | US-04 empty 400 | The SRE fat-fingers `body_contains=` (empty value); a counting-failing store is wired so the no-store-call assertion is testable | The endpoint validates the parameter | The error envelope is the literal `{"status":"error","error":"invalid body_contains"}`; the store is NEVER touched | 400 | `{"status":"error","error":"invalid body_contains"}` |
| AC-04b | US-04 over-cap 400 + anti-echo | The client sends a 1025-byte value with the recognisable prefix `"OVERSIZE-"` (the 1024-byte cap is strictly exceeded); a counting-failing store is wired | The endpoint validates the parameter | The error envelope is the SAME literal as the empty arm; the body does NOT contain the prefix `"OVERSIZE-"`; the body does NOT contain the length `"1025"`; the store is NEVER touched | 400 | `{"status":"error","error":"invalid body_contains"}` |
| AC-04c | US-05 case-sensitive | Tenant `acme-prod` has records whose body starts with lowercase `"kafka timeout"` | The SRE GETs with `body_contains=KAFKA` (uppercase) | The response is `[]` (the byte-wise match treats `K`=0x4B and `k`=0x6B as distinct) | 200 | `[]` |
| AC-05 | US-06 cross-tenant isolation | Tenant `acme-prod` has the 5-record fixture; tenant `globex-staging` has ZERO records in the window; the router is built with `Some(globex-staging)` | The operator (with the globex-staging credential) GETs with `body_contains=kafka%20timeout` | The response is `[]`; the body NEVER contains the substring `"broker"` (a marker from the acme-prod records); no acme-prod record leaks across | 200 | `[]` |
| AC-cap | US-01 filter-before-cap | Small-fixture stand-in: 3 matching + 2 non-matching records inside the window, well under MAX_RESULT_ROWS | The SRE GETs with `body_contains=kafka%20timeout` | 200 with 3 records (filter ran first; cap measured the post-filter vector) | 200 | Bare JSON array of 3 `LogRecord`s |

Notes:

- AC-04a, AC-04b, AC-04c, and AC-05 cover ADR-0055 Decisions 4
  (empty rejection), 5 (literal envelope + length cap + anti-echo),
  3 (case-sensitive), and the per-tenant isolation invariant from
  ADR-0047 respectively.
- AC-cap is the small-fixture proxy for the cap-after-filter
  invariant; the bulk-double variant is a candidate Crafty
  addition in DELIVER (see `wave-decisions.md` § D4).
- Every Given uses concrete values (the canonical window
  `[1716200000s, 1716200060s)`; the canonical service name
  `"checkout"`; the canonical substring `"kafka timeout"`); zero
  abstractions ("Given sufficient records" — never used).
- Every Then expresses an observable outcome (status code + body
  shape + content assertions); zero internal-state assertions
  (no DB checks; no `mock.called` counts; the no-store-call
  proof in AC-04a / AC-04b is via a real driven-port test adapter
  that counts its OWN public-method calls, not a mock library).

## Self-review checklist

### Mandate 7 RED-not-BROKEN

- [x] Every `#[ignore]`'d test asserts on a real behaviour, not a
      placeholder. The `#[ignore]` reasons cite the specific
      scaffold site (`"parse_body_contains scaffolded"`,
      `"Predicate::matches arm scaffolded"`, `"dispatch arm
      scaffolded"`) so Crafty's de-ignore order is unambiguous.
- [x] AC-01 (walking skeleton) panics with the recognisable
      `__SCAFFOLD__ log-body-text-search-v0 RED` marker when run
      via `cargo test ... -- --ignored`. Verified at DISTILL
      close. The panic is at `crates/log-query-api/src/lib.rs:299`
      (the `parse_body_contains` body), not a setup error.
- [x] The `#[ignore]` posture is a deliberate compromise with the
      project's pre-commit Gate 1 (`cargo test --workspace
      --all-targets`); the alternative (an enabled panicking
      walking skeleton) would block every commit until Crafty's
      first DELIVER move lands.

### Coverage and traceability

- [x] Every US-01..US-06 has at least one AC scenario
      (US-01 has 2: AC-01 + AC-cap; US-04 has 2: AC-04a + AC-04b;
      US-02, US-03, US-05, US-06 each have 1). The full mapping
      is named in `wave-decisions.md` § "US to AC mapping".
- [x] Each scenario carries the `@US-<id>` tag in its rustdoc
      preface so traceability is greppable (mirrors slice-03
      pattern).

### Cap-after-filter pinned

- [x] AC-cap exercises the small-fixture form of the
      filter-before-cap invariant (ADR-0055 Decision 6). The
      bulk-double form (mirroring
      `slice_03_severity_filter.rs::BulkSeverityLogStore`) is a
      named Crafty handoff for DELIVER — see
      `wave-decisions.md` § D4.

### Case-sensitive test

- [x] AC-04c pins ADR-0055 Decision 3 (case-sensitive match).
      The fixture body is lowercase `"kafka timeout"`; the
      query is uppercase `"KAFKA"`; the expected response is
      `[]`. The test kills a
      `String::contains` -> `to_lowercase().contains` mutant.

### Anti-echo tests (BOTH error arms)

- [x] AC-04a (empty arm): the assertion verifies the literal
      envelope `{"status":"error","error":"invalid body_contains"}`
      via `body["error"] == "invalid body_contains"` and the
      `is_error_envelope` shape helper. There is no raw value to
      echo (the value is empty), but the assertion pins the
      literal class label so a future mutant that interpolates a
      different reason is killed.
- [x] AC-04b (over-cap arm): the fixture is a 1025-byte value with
      the recognisable prefix `"OVERSIZE-"`; the assertion verifies
      the body does NOT contain `"OVERSIZE-"` (the raw oversize
      value is NEVER echoed) and does NOT contain `"1025"` (the
      length is NEVER echoed). Two redaction surfaces pinned in
      one scenario.

### Cross-tenant test

- [x] AC-05 pins the per-tenant isolation invariant against the
      new filter arm. The fixture seeds `acme-prod` with the
      kafka-timeout records and leaves `globex-staging` empty;
      the router is built with `Some(globex-staging)`; the
      response is `[]`; the body NEVER contains `"broker"` (a
      marker borrowed from the acme-prod records to prove
      cross-tenant leak detection works).

### Length cap test (1024 bytes pinned)

- [x] AC-04b uses a 1025-byte fixture (`"OVERSIZE-" + "A".repeat(1016)`);
      the assertion `oversize_raw.len() == 1025` is inline. The
      1024-byte INCLUSIVE boundary (DD6: 1024 served; 1025
      refused) is pinned at the test level. The inline unit
      test `parse_body_contains_accepts_input_at_exactly_the_cap`
      (per parse-helper-spec) is a Crafty addition during the
      GREEN move on AC-04b.

### `query-http-common` reuse confirmed

- [x] `parse_body_contains` lives in `log-query-api`, NOT in
      `query-http-common` (per DESIGN DD3 / ADR-0055 Decision 9:
      single-pillar concern; `body` exists only on
      `lumen::LogRecord`). The handler uses
      `query_http_common::error_response`,
      `query_http_common::resolve_tenant_or_refuse`,
      `query_http_common::MAX_RESULT_ROWS`,
      `query_http_common::MAX_WINDOW_SECONDS`,
      `query_http_common::REASON_*`, and
      `query_http_common::parse_time_range` verbatim — zero new
      copies of any of them. Verified by reading the scaffold
      edit in `crates/log-query-api/src/lib.rs`: the new
      `parse_body_contains` Err arm flows into
      `query_http_common::error_response(StatusCode::BAD_REQUEST,
      reason)` exactly as the severity unknown arm already does.

### Hexagonal boundary (Mandate 1)

- [x] Every scenario drives the system through
      `log_query_api::router(store, tenant)` — the single public
      driving port of the crate. Zero scenarios touch
      `Predicate`, `LogStore`, or `parse_body_contains` directly.
- [x] Driven-port test doubles (`CountingFailingLogStore`) are
      test adapters for the `lumen::LogStore` driven port, not
      internal `log-query-api` components — driving the router
      over them still honours the boundary.

### Business language (Mandate 2)

- [x] The rustdoc prefaces use business language (Sara the SRE;
      Marcus the platform engineer; Priya the support engineer;
      "kafka timeout"; "on-call"; "incident"; "runbook"). HTTP
      mechanics are confined to the assertion-level Rust code
      (`StatusCode::OK`, `body["error"]`) which is the natural
      vocabulary of a Rust acceptance test against axum.

### User journey completeness (Mandate 3)

- [x] AC-01 (walking skeleton) traces the full journey: SRE has
      a substring in hand -> issues the GET -> receives the
      narrowed response -> can act on it. Every assertion is
      about the wire response the SRE observes, not internal
      side effects.
- [x] AC-04a / AC-04b assert the FULL journey for the rejected
      arm: bad input -> 400 with the literal envelope -> store
      NEVER touched. The store-not-touched assertion is the
      proof the journey terminated cleanly (no half-finished
      query, no data leak).
