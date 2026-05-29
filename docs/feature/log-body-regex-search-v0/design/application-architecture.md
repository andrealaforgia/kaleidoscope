# Application Architecture — log-body-regex-search-v0

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-29.
Mode: propose. Scope: application.

## System Context

The slice is a brownfield carpaccio extension of the log read
endpoint `GET /api/v1/logs`, served by `crates/log-query-api`
out of the durable `lumen::FileBackedLogStore`. The driving
adapter is the existing axum router (ADR-0047); the driven port
is the existing `lumen::LogStore::query_with(&tenant, range,
&Predicate)` seam (`crates/lumen/src/store.rs:89`). The shared
HTTP scaffolding lives in `query-http-common` (ADR-0054). The
existing four-arm dispatch (cross product of `min_severity` x
`body_contains`, verified at
`crates/log-query-api/src/lib.rs:195-212`) grows by one
parameter and one cross-check; no new component is introduced.
Per-tenant isolation, the half-open `[start, end)` window
(ADR-0047 § 3), the window cap and result cap (ADR-0050), the
fail-closed tenancy seam, the redacted error envelope, and the
bare JSON array success shape are PRESERVED unchanged.

## Sequence Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Operator as Operator / Client
    participant Handler as log_query_api::handle_logs
    participant Common as query_http_common::*
    participant ParseR as parse_body_regex
    participant Store as lumen::LogStore::query_with
    participant Pred as Predicate::matches

    Operator->>Handler: GET /api/v1/logs?start=&end=[&min_severity=][&body_contains=][&body_regex=]
    Handler->>Common: resolve_tenant_or_refuse(&state.tenant)
    alt no tenant resolvable
        Common-->>Operator: 401 status:error
    end
    Handler->>Common: parse_time_range(&start, &end)
    alt malformed or inverted window
        Common-->>Operator: 400 status:error
    end
    Handler->>Handler: window-cap check (end - start > MAX_WINDOW_SECONDS)
    alt window over cap
        Handler-->>Operator: 400 reason WINDOW_TOO_LARGE
    end
    Handler->>Handler: parse_min_severity if Some (ADR-0052)
    alt unknown severity
        Handler-->>Operator: 400 reason "unknown severity"
    end
    Handler->>Handler: parse_body_contains if Some (ADR-0055)
    alt empty or over-cap body_contains
        Handler-->>Operator: 400 reason "invalid body_contains"
    end
    Handler->>Handler: mutual-exclusion (body_contains.is_some && body_regex.is_some)
    alt both body filters present
        Handler-->>Operator: 400 reason "specify body_regex or body_contains, not both"
    end
    Handler->>ParseR: parse_body_regex(raw) if body_regex Some
    ParseR->>ParseR: empty? len > 1024? Regex::new fails?
    alt any of empty / over-cap / compile-failure
        ParseR-->>Handler: Err "invalid body_regex"
        Handler-->>Operator: 400 reason "invalid body_regex"
    end
    ParseR-->>Handler: Ok(Regex)
    Handler->>Handler: build Predicate (one of 6 reachable shapes)
    Handler->>Store: query (no filter) OR query_with(&tenant, range, &Predicate)
    Store->>Pred: matches(record) for each in-window record
    Pred-->>Store: bool
    Store-->>Handler: Vec<LogRecord> (post-filter; per-tenant)
    Handler->>Handler: result-cap check (records.len > MAX_RESULT_ROWS)
    alt over result cap
        Handler-->>Operator: 400 reason TOO_MANY_ROWS
    end
    Handler-->>Operator: 200 bare JSON array
```

## Changes Per File

| File | Change | Net new lines (approx) |
|---|---|---|
| `crates/log-query-api/src/lib.rs` | Add `body_regex: Option<String>` field on `LogsParams`. Add `const MAX_BODY_REGEX_LEN: usize = 1024;` next to `MAX_BODY_CONTAINS_LEN`. Add free function `parse_body_regex(raw: &str) -> Result<Regex, &'static str>`. Add the mutual-exclusion check between the `body_contains` parse and the `body_regex` parse. Add the new dispatch arms (cross product of `min_severity` x exactly-one-of `{none, body_contains, body_regex}` = 6 reachable arms, pruning the 2 forbidden arms by mutual exclusion). Add `use regex::Regex;`. Inline unit tests pin the 1024 boundary, the empty-string rejection, the compile-failure rejection, and the redaction posture. | ~35 |
| `crates/lumen/src/predicate.rs` | Add `use regex::Regex;`. Drop `PartialEq, Eq` from the existing `#[derive(...)]` on `Predicate` (`Regex` does not implement either). Add field `body_regex: Option<Regex>`. Add builder `pub fn body_regex(mut self, re: Regex) -> Self`. Add the new arm at the end of `matches` (after the body_contains arm): `if let Some(re) = self.body_regex.as_ref() { if !re.is_match(&record.body) { return false; } }`. Add `&& self.body_regex.is_none()` to the `is_empty` conjunction. | ~12 |
| `crates/lumen/Cargo.toml` | Add `regex = "1"` to `[dependencies]`. Same version spelling as `crates/query-api/Cargo.toml:62`. Resolves to `Cargo.lock` pin `1.12.3` with zero lockfile diff. | 1 |
| `crates/log-query-api/tests/slice_01_body_regex.rs` (DELIVER, not DESIGN) | NEW acceptance file covering the eight scenarios pinned in `user-stories.md` US-01 plus the conjunctive-with-`min_severity` scenario. | DISTILL output |

## Error Contract

| Case | HTTP | Body (literal) |
|---|---|---|
| Tenant cannot be resolved | 401 | `{"status":"error","error":"the log query is unavailable: no tenant resolvable"}` (existing, ADR-0047 / ADR-0054) |
| Window malformed or inverted | 400 | existing `parse_time_range` reason via `query_http_common::REASON_INVALID_TIME_RANGE` |
| Window over `MAX_WINDOW_SECONDS` | 400 | `query_http_common::REASON_WINDOW_TOO_LARGE` (existing) |
| Unknown `min_severity` | 400 | `{"status":"error","error":"unknown severity"}` (existing, ADR-0052) |
| Empty `body_contains` | 400 | `{"status":"error","error":"invalid body_contains"}` (existing, ADR-0055) |
| `body_contains` over 1024 bytes | 400 | `{"status":"error","error":"invalid body_contains"}` (existing, ADR-0055) |
| BOTH `body_contains` AND `body_regex` present | 400 | `{"status":"error","error":"specify body_regex or body_contains, not both"}` (NEW) |
| Empty `body_regex` | 400 | `{"status":"error","error":"invalid body_regex"}` (NEW) |
| `body_regex` over 1024 bytes | 400 | `{"status":"error","error":"invalid body_regex"}` (NEW) |
| `body_regex` fails to compile | 400 | `{"status":"error","error":"invalid body_regex"}` (NEW) |
| Result count over `MAX_RESULT_ROWS` (post-filter) | 400 | `query_http_common::REASON_TOO_MANY_ROWS` (existing) |
| Store error | 500 | existing tracing event + `the backing log store could not be read` |
| Success (any cardinality including zero) | 200 | bare JSON array of `LogRecord` |

## Mutual Exclusion Logic

The mutual-exclusion check sits AFTER `body_contains` is parsed
(so its own empty / over-cap 400 surfaces first) and BEFORE
`body_regex` is parsed (so an honest mutual-exclusion 400 is not
masked by an unrelated regex-syntax 400). Both `body_contains`
and `body_regex` being `Some` is the only state pruned; all
other combinations of presence flow into the 6-arm dispatch.

```text
match params.body_contains.as_deref() { None => None, Some(raw) =>
    Some(parse_body_contains(raw)?) };   // 400 on empty / over-cap

// NEW: mutual exclusion check; runs after body_contains parse,
// before body_regex parse. Store is NEVER touched on this path.
if body_contains.is_some() && params.body_regex.is_some() {
    return query_http_common::error_response(
        StatusCode::BAD_REQUEST,
        "specify body_regex or body_contains, not both",
    );
}

let body_regex = match params.body_regex.as_deref() { None => None,
    Some(raw) => Some(parse_body_regex(raw)?) };   // 400 on empty
                                                   // / over-cap /
                                                   // compile fail

// Dispatch: 6 reachable arms by the cross product
// min_severity x exactly-one-of { none, body_contains, body_regex }.
let predicate = match (min_severity, body_contains, body_regex) {
    (None, None, None)                  => /* fall through to store.query */,
    (Some(sev), None, None)             => Predicate::new().min_severity(sev),
    (None, Some(target), None)          => Predicate::new().body_contains(target),
    (None, None, Some(re))              => Predicate::new().body_regex(re),
    (Some(sev), Some(target), None)     => Predicate::new().min_severity(sev).body_contains(target),
    (Some(sev), None, Some(re))         => Predicate::new().min_severity(sev).body_regex(re),
    // The two (Some, Some, Some) and (None, Some, Some) and
    // (Some, Some, Some) arms are UNREACHABLE — pruned by the
    // mutual-exclusion check above. The match is exhaustive on
    // the tuple shape; Rust's compiler will require either
    // unreachable!() or a catch-all `_` for the pruned arms.
};
```

The dispatch is a closed enumeration over a 3-tuple of
`Option`s; Rust's match exhaustiveness ensures every reachable
combination is named explicitly. The two combinations forbidden
by mutual exclusion are caught BEFORE the match and never reach
the dispatch.

## Combinations Table

For reference, the 8 logical states of the 3-tuple
`(min_severity, body_contains, body_regex)` and their
disposition:

| min_severity | body_contains | body_regex | Disposition |
|---|---|---|---|
| None | None | None | `store.query(&tenant, range)` (unfiltered fall-through; backward-compat) |
| Some | None | None | `query_with(&tenant, range, &Predicate::new().min_severity(sev))` |
| None | Some | None | `query_with(&tenant, range, &Predicate::new().body_contains(target))` |
| None | None | Some | `query_with(&tenant, range, &Predicate::new().body_regex(re))` |
| Some | Some | None | `query_with(&tenant, range, &Predicate::new().min_severity(sev).body_contains(target))` |
| Some | None | Some | `query_with(&tenant, range, &Predicate::new().min_severity(sev).body_regex(re))` |
| None | Some | Some | 400 mutual exclusion (NEVER reaches dispatch) |
| Some | Some | Some | 400 mutual exclusion (NEVER reaches dispatch) |

Six reachable arms; two pruned. The pruning is enforced by an
explicit conditional, not by leaving the arms unreachable in
the match.

## Handler Order (PIN ENFORCEMENT)

The handler MUST execute these steps in this exact order:

1. `query_http_common::resolve_tenant_or_refuse` (existing, 401 on
   failure; UNCHANGED).
2. `query_http_common::parse_time_range` (existing, 400 on failure;
   UNCHANGED).
3. Window cap (existing, 400 on failure; UNCHANGED).
4. `parse_min_severity` if `params.min_severity.is_some()`
   (existing, 400 on failure; UNCHANGED).
5. `parse_body_contains` if `params.body_contains.is_some()`
   (existing from ADR-0055, 400 on failure; UNCHANGED).
6. **NEW** Mutual-exclusion check: if `body_contains.is_some() &&
   params.body_regex.is_some()`, return 400 with the literal
   `"specify body_regex or body_contains, not both"`. Store is
   NEVER touched on this path. Runs BEFORE the body_regex parse
   so an honest cross-check is not masked by an unrelated
   compile-failure 400.
7. **NEW** `parse_body_regex` if `params.body_regex.is_some()`.
   400 on empty / over-cap / compile-failure. Store is NEVER
   touched on this path.
8. Build the `Predicate` for the 6-arm dispatch and call
   `store.query` or `store.query_with`. Tenant resolution
   precedes any predicate evaluation (the tenant is the FIRST
   argument).
9. Result cap (existing, 400 on failure; measures the
   post-filter vector; UNCHANGED).
10. `success_response(records)` (existing 200 arm; UNCHANGED).

The new parse step at (7) is its OWN gate; it is NOT folded into
`parse_min_severity` or `parse_body_contains`. The new
mutual-exclusion check at (6) is its OWN gate; it is NOT folded
into either parse. The order between (6) and (7) is load-bearing
for the redaction posture: an honest mutual-exclusion 400 must
not be masked by a regex compile-failure 400 when BOTH parameters
are syntactically valid but mutually-exclusively present.

## Composition with `min_severity`

When both `min_severity` AND `body_regex` are present, the
composed `Predicate::matches` enforces conjunctive AND: a
record passes iff `record.severity_number >= floor` AND
`re.is_match(&record.body)`. The two arms are independent (one
on `severity_number`, one on `body`); arm order in `matches` is
not load-bearing because AND is commutative. The acceptance
suite SHALL include a scenario where both parameters are present.

## Earned-Trust posture (PIN ENFORCEMENT)

Three orthogonal layers reproduced from ADR-0055 Verification:

- **Compile-time subtype**: `Predicate.body_regex: Option<Regex>`
  references `regex::Regex` which is the workspace-pinned crate
  at lock `1.12.3`. The `matches` arm calls `re.is_match(&record.body)`
  where `record.body: String` (verified at
  `crates/lumen/src/record.rs:54`); removing the field or
  changing the type fails the compile.
- **AST structural**: the acceptance suite references
  `body_regex` and `invalid body_regex` and `specify body_regex
  or body_contains, not both` by literal in both URLs and
  assertion texts. A mutant that drops the parse step, the
  dispatch arm, or the mutual-exclusion check is killed by at
  least one scenario.
- **Behavioural gold-test**: the slice-01 acceptance suite
  exercises every Domain Example from `user-stories.md` US-01
  (happy path, calm-empty, default-unchanged, three 400 arms
  with no-store-call assertions, case-sensitive pin,
  mutual-exclusion, cross-tenant isolation).

The existing ADR-0047 startup probe continues to run unchanged.
The slice does not introduce a new substrate dependency that
needs its own probe (the `regex` crate is in-process pure
computation; there is no environment lie to verify against).
