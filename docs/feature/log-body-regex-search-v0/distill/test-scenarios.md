# Test Scenarios — log-body-regex-search-v0

Author: `nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-29.

Acceptance test surface for the slice. The 10 scenarios live in
`crates/log-query-api/tests/slice_05_body_regex.rs` and drive
the single public driving port `log_query_api::router(store,
tenant)` via `tower::ServiceExt::oneshot`. Six scenarios seed a
REAL durable `FileBackedLogStore` adapter (`@real-io`); four
use a counting-failing-store double exclusively to assert the
store is NEVER touched on the four 400 paths.

## Scenario Table

| AC | Story | Given | When | Then | HTTP | Body |
|---|---|---|---|---|---|---|
| AC-01 | US-01 (walking skeleton) | Tenant "acme-prod" has 5 records matching `kafka.*(timeout\|timed out)` (three "kafka connect timeout N" + two "kafka connection timed out N") plus 2 non-matching "ok" records, seeded in `[1716200000, 1716200060)` | SRE GETs `/api/v1/logs?start=&end=&body_regex=kafka.%2A(timeout%7Ctimed%20out)` | Response carries exactly 5 records in ascending observed_time order; no "ok" record appears | 200 | Bare JSON array of 5 `LogRecord`s |
| AC-02 | US-02 | Tenant "acme-prod" has the same 7 records, none matching `cassandra.*timeout` | SRE GETs with `body_regex=cassandra.%2Atimeout` | Calm empty response; never 404, never 500 | 200 | `[]` |
| AC-03 | US-03 | Tenant "acme-prod" has the same 7 records | Automation client GETs WITHOUT the `body_regex` parameter | Every in-window record is returned (backward-compat) | 200 | Bare JSON array of 7 `LogRecord`s |
| AC-04a | US-04a | Counting failing store under tenant "acme-prod" | SRE GETs with `body_regex=[` (unclosed character class; `regex` crate rejects on compile) | Redacted 400; store never touched | 400 | `{"status":"error","error":"invalid body_regex"}` |
| AC-04b | US-04b | Counting failing store under tenant "acme-prod" | SRE GETs with `body_regex=` (empty value; `Some("")` from serde) | Redacted 400; store never touched | 400 | `{"status":"error","error":"invalid body_regex"}` |
| AC-04c | US-04c | Counting failing store under tenant "acme-prod" | SRE GETs with `body_regex=OVERSIZE-AAAA...AAA` (exactly 1025 bytes; prefix `OVERSIZE-` recognisable) | Redacted 400; raw value NEVER echoed (anti-echo asserts `OVERSIZE-` and `1025` absent from body); store never touched | 400 | `{"status":"error","error":"invalid body_regex"}` |
| AC-05 | US-05 | Tenant "acme-prod" has records whose body is "kafka error N" (lowercase 'k') | SRE GETs with `body_regex=KAFKA` (uppercase) | Calm empty response; case-sensitive default per PIN 2 | 200 | `[]` |
| AC-06 | US-06 | Counting failing store under tenant "acme-prod" | SRE GETs with BOTH `body_contains=foo` AND `body_regex=bar` | Redacted 400 with NEW literal reason (distinct from "invalid body_regex"); neither raw value echoed; store never touched | 400 | `{"status":"error","error":"specify body_regex or body_contains, not both"}` |
| AC-07 | US-07 | Tenant "acme-prod" has the 7 records; tenant "globex-staging" has ZERO records | Operator holding the globex-staging credential GETs with `body_regex=kafka.%2Atimeout` under tenant "globex-staging" | Calm empty response; no acme-prod body text leaks across tenants (asserts "connect" absent from body) | 200 | `[]` |
| AC-COMBO | US-01 (conjunctive) | Tenant "acme-prod" has 5 mixed-severity records: 1 INFO+match (A), 2 WARN+match (B, E), 1 INFO+no-match (C), 1 WARN+no-match (D) | SRE GETs with `min_severity=WARN` AND `body_regex=kafka.%2Atimeout` | Exactly 2 records (B and E); INFO records excluded by severity; WARN+no-match excluded by regex; ascending order | 200 | Bare JSON array of 2 `LogRecord`s |

## Coverage Summary

- **Total scenarios**: 10
- **Success-path scenarios**: 5 (AC-01, AC-02, AC-03, AC-05, AC-07, AC-COMBO; AC-02/AC-05/AC-07 are calm-empty arms of the success contract, counted on the 200 side)
- **Error-path scenarios**: 5 (AC-04a, AC-04b, AC-04c, AC-06, plus the calm-empty arms above as alternative-paths)
- **Error-path ratio**: 4 of 10 explicit 400 arms = 40% (meets Dim 1 mandate)
- **Walking skeleton scenarios**: 1 (AC-01)
- **Focused scenarios**: 9 (AC-02..AC-COMBO)
- **`@driving_port`-tagged scenarios**: 10 (every scenario; all enter through `log_query_api::router`)
- **`@real-io`-tagged scenarios**: 6 (AC-01, AC-02, AC-03, AC-05, AC-07, AC-COMBO; the seeded scenarios use the real `FileBackedLogStore` adapter on disk)
- **No-store-call assertions**: 4 (AC-04a, AC-04b, AC-04c, AC-06; counting failing store double asserts `total_store_calls() == 0`)
- **Anti-echo assertions**: 2 (AC-04c asserts `OVERSIZE-` and `1025` absent; AC-06 asserts `foo` and `bar` absent)

## Self-Review Checklist (Mandate 7 RED-not-BROKEN; critique Dim 1-9)

- [x] **Mandate 7**: every scenario is `#[ignore]`'d at DISTILL close; the workspace pre-commit gate stays green. 9 of 10 scenarios reach a `__SCAFFOLD__` panic when run with `--ignored`; AC-06 is already green at DISTILL close because the mutual-exclusion check is purely conditional with no parse-helper dependency (acceptable RED-not-BROKEN under the Mandate 7 reading "the slice's behavioural surface is RED, even if one purely-conditional arm is green by construction"). The RED state is recorded in `wave-decisions.md` § D2.
- [x] **Story coverage (Dim 4 / Dim 8 Check A)**: every US-01..US-07 has at least one AC (AC-01..AC-07 plus the conjunctive AC-COMBO).
- [x] **Mutual exclusion test (Dim 1)**: AC-06 pins the new literal reason `"specify body_regex or body_contains, not both"` and proves it is its own redaction class (distinct from `"invalid body_regex"`).
- [x] **Anti-echo on the empty 400 arm**: AC-04b reaches the empty-value rejection arm; the reason is byte-equal to `"invalid body_regex"` and contains no raw value (the raw value is the empty string, so the echo question is vacuously satisfied; the assertion shape mirrors AC-04c for consistency).
- [x] **Anti-echo on the over-cap 400 arm**: AC-04c uses a recognisable `OVERSIZE-` prefix and asserts neither `OVERSIZE-` nor `1025` appears in the body; the store is never touched.
- [x] **Anti-echo on the invalid-syntax 400 arm**: AC-04a uses a single `[` as the raw value; the reason is byte-equal to `"invalid body_regex"`; the `regex::Error::Display` impl is NEVER called (the production code maps via `.map_err(|_| "invalid body_regex")`); the store is never touched.
- [x] **Case-sensitive test (Dim 1, Dim 4)**: AC-05 seeds lowercase-'k' bodies and queries with `body_regex=KAFKA`; the calm-empty response pins the case-sensitive default per ADR-0056 PIN 2. The inline `(?i)` escape hatch is pinned at the per-helper inline test in `parse-helper-spec.md` (item 9), not duplicated at the acceptance layer (one acceptance test, one behaviour; the per-helper test pins the escape hatch).
- [x] **Cross-tenant test (Dim 1, Dim 4)**: AC-07 pins per-tenant isolation by querying for a pattern that matches only tenant A's records under tenant B's credential. The body never contains "connect" (a marker borrowed from the acme-prod fixtures).
- [x] **Combo test (Dim 1, Dim 4)**: AC-COMBO pins conjunctive AND between `min_severity` and `body_regex`. Five records cover all four (severity x match) combinations: I+match excluded by severity, W+match kept, I+no-match excluded by both, W+no-match excluded by regex, W+match kept. The two matching records (B and E) are returned in ascending order.
- [x] **GWT format compliance (Dim 2)**: every scenario follows Given / When / Then in the docstring; the test body's three sections (setup, request, assertions) follow the same shape.
- [x] **Business language purity (Dim 3)**: scenario docstrings use the SRE / operator / on-call vocabulary established by slice 04. Technical terms in the assertions (status code, JSON body shape) are confined to the test code where they reflect the user-observable HTTP contract pinned by ADR-0047 / ADR-0056.
- [x] **Walking skeleton user-centricity (Dim 5)**: AC-01's docstring frames the outcome as "Maria sees every shape of the kafka-timeout failure family in one request" (a user goal), not as "the request passes through axum + serde + handler + lumen + FileBackedLogStore" (a technical wiring statement). The Then asserts she sees the matching records (a user observation), not that the store wrote a row or that a function was called.
- [x] **Priority validation (Dim 6)**: the scenario count matches the carpaccio gate exactly (7 user stories + 1 conjunctive composition + 2 anti-echo splits on US-04 = 10 scenarios). No secondary concerns are addressed; the 1024-byte boundary one-byte-at-a-time is pinned at the inline unit-test layer per `parse-helper-spec.md` (not duplicated at the acceptance layer).
- [x] **Observable behaviour assertions (Dim 7)**: every Then asserts a return value from a driving-port call (HTTP status, response body) or a counting double (`total_store_calls() == 0`). No assertion checks internal state, private fields, or method call counts on the production code.
- [x] **Environment coverage (Dim 8 Check B)**: the single environment declared in `../devops/environments.yaml` (`clean`) is exercised by the walking skeleton AC-01 (the given precondition "FileBackedLogStore opened in a per-test tempdir" matches the `clean` environment's "log-query-api binary on host with lumen filebacked store").
- [x] **Walking skeleton boundary (Dim 9)**: WS Strategy A declared in `wave-decisions.md` § D3; AC-01 uses the real `FileBackedLogStore` adapter (`open_durable_store`) and seeds via real `ingest` calls; if the real adapter were deleted, AC-01 would fail to import `FileBackedLogStore`. Adapter-integration coverage is met across the six `@real-io` scenarios.

## Handoff to DELIVER

Crafty's outer-loop sequence (one scenario at a time; remove
`#[ignore]` per step):

1. **AC-01 (walking skeleton)** — fill `parse_body_regex` (empty
   -> over-cap -> compile) AND the `Predicate::matches` body_regex
   arm AND the dispatch wiring. Confirm AC-01 turns green.
2. **AC-02 (calm empty)** — once AC-01 is green, AC-02 should
   turn green with no further production code (the same paths
   are exercised; a pattern that compiles and matches no
   record returns `[]`).
3. **AC-03 (default unchanged)** — confirm the `(None, None,
   None)` dispatch arm is intact; AC-03 should turn green
   without further code.
4. **AC-04a (invalid syntax)** — fill the `Regex::new` compile
   arm of `parse_body_regex`; AC-04a turns green.
5. **AC-04b (empty)** — fill the empty-string arm of
   `parse_body_regex` (the order check `empty -> over-cap ->
   compile` is load-bearing); AC-04b turns green.
6. **AC-04c (over-cap)** — fill the length-cap arm; AC-04c
   turns green.
7. **AC-05 (case-sensitive)** — confirm `Regex::new` uses the
   default case-sensitive grammar; AC-05 turns green.
8. **AC-06 (mutual exclusion)** — already green at DISTILL
   close (the check is purely conditional and does not depend
   on `parse_body_regex`); de-ignore and confirm.
9. **AC-07 (cross-tenant)** — confirm `query_with(&tenant,
   range, predicate)` is called with the resolved tenant as
   the first argument; AC-07 turns green.
10. **AC-COMBO (conjunctive)** — confirm the
    `(Some(floor), None, Some(re))` dispatch arm calls
    `Predicate::new().min_severity(floor).body_regex(re)`;
    AC-COMBO turns green.

Inline unit tests in `parse-helper-spec.md` § Test surface
(items 1-9) fill in alongside step 1 to pin the per-byte
1024 / 1025 cap boundary, the order of checks, the
redaction-never-echoes-raw-value posture, and the inline
`(?i)` escape hatch. Mutation testing
(`gate-5-mutants-log-query-api`, `gate-5-mutants-lumen`)
picks up the new code via `--in-diff` automatically.
