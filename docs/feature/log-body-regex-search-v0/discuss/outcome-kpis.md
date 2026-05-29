# Outcome KPIs: log-body-regex-search-v0

The slice's KPIs follow the established post-extraction shape
(ADR-0054 / ADR-0055): the user-observable behaviour is the KPI,
and the measurement is an automated test in the acceptance suite
or a CI assertion against the source. There is NO live
observability stack to wire to at this stage of the platform; the
contract IS the signal.

## K1 — Pattern matches are HONEST

| | |
|---|---|
| **Who** | SRE operators and automation clients of the log read API who hold a known regex pattern. |
| **Does what** | Issue narrowed read requests (`body_regex=<pattern>`) instead of running multiple `body_contains` queries or grepping client-side. |
| **By how much** | Every record in the response is matched by the regex (per `Regex::is_match` against `LogRecord.body`); no record in the fixture whose body matches the regex is omitted. False-positive rate = 0; false-negative rate = 0. |
| **Measured by** | Acceptance test in `crates/log-query-api/tests/slice_01_body_regex.rs` (DISTILL output) asserting (a) `for every returned r, regex.is_match(r.body) == true`, (b) `for every fixture r, regex.is_match(r.body) implies r is in response`. The mutation-killed arm in `Predicate::matches` is the orthogonal subtype-level check. |
| **Baseline** | 100% of in-window records returned today on a no-filter request; Maria runs 3-4 separate `body_contains` queries to approximate the regex behaviour. |
| **Target** | The acceptance test passes on every CI build (binary outcome). |

## K2 — Zero behaviour regression on `/api/v1/logs` without `body_regex`

| | |
|---|---|
| **Who** | Existing automation (Marcus's classifier) and existing operators using the endpoint pre-slice. |
| **Does what** | Calls `GET /api/v1/logs?start=&end=` (with or without `min_severity` and `body_contains`) and gets exactly today's response shape and content. |
| **By how much** | Response is byte-equal to the slice-prior response for the same inputs on every combination of `{absent, present}` x `{absent, present}` of `min_severity` x `body_contains`. |
| **Measured by** | The existing acceptance suites at `crates/log-query-api/tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`, `tests/slice_01_severity_filter.rs`, and `tests/slice_01_body_contains.rs` MUST stay green unchanged. The new `slice_01_body_regex.rs` adds a "parameter absent returns every record" scenario as an extra belt-and-braces check. |
| **Baseline** | Today's behaviour at HEAD (commit 1bfa609 and the gate-5 follow-up at d96a807). |
| **Target** | Zero diff on the four pre-existing test suites; binary outcome. |

## K3 — Invalid regex returns 400 BEFORE the store is touched

| | |
|---|---|
| **Who** | Every client of `/api/v1/logs` (operators, automation, accidental probes). |
| **Does what** | Receives HTTP 400 with the literal envelope `{"status":"error","error":"invalid body_regex"}` on (a) empty value, (b) value over 1024 bytes, (c) value that the `regex` crate refuses to compile, and the SAME envelope text in all three cases. |
| **By how much** | The store is NEVER queried on any 400 path. Compile-failure latency is bounded by the `regex` crate's compile cost on a 1024-byte pattern (sub-millisecond on typical inputs; linear-time worst case). |
| **Measured by** | Acceptance suite scenarios in `slice_01_body_regex.rs`: (a) empty rejection with no-store-call assertion; (b) over-cap rejection with no-store-call assertion; (c) invalid-syntax rejection with no-store-call assertion. Each scenario uses a `FailingLogStore` double whose `query_with` panics; the panic firing is the cross-check that the store was not touched. |
| **Baseline** | Today the parameter does not exist; sending it has no effect (serde ignores unknown fields with `#[serde(deny_unknown_fields)]` off, which is the current default; a future strict-deserialiser change would alter the baseline). |
| **Target** | All three 400 scenarios pass with the no-store-call assertion; binary outcome. |

## K4 — `query-http-common` reuse confirmed (no new MAX_*, no new error_response duplicate, no new envelope)

| | |
|---|---|
| **Who** | Maintainers of `query-http-common` and `log-query-api`. |
| **Does what** | The slice consumes the shared scaffold (`MAX_RESULT_ROWS`, `MAX_WINDOW_SECONDS`, `REASON_*`, `error_response`, `resolve_tenant_or_refuse`, `parse_time_range`) and adds ZERO new copies. The new local constants are `MAX_BODY_REGEX_LEN` (1024) and the local reason literals `"invalid body_regex"` and `"specify body_regex or body_contains, not both"`; none belongs to the shared scaffold (they are body-pillar-specific). |
| **By how much** | (a) `crates/log-query-api/src/lib.rs` introduces 0 new copies of any `query_http_common::` export; (b) the new lines in `crates/log-query-api/src/lib.rs` total under 40 (parse helper + parameter field + dispatch arm + mutual-exclusion check); (c) the envelope on every 400 arm is the shared `error_response`. |
| **Measured by** | A CI static-grep step (parallel to the one introduced for `log-body-text-search-v0`) asserts the SOLE source of cap consts and reason literals in `log-query-api` remains `query_http_common::`. The new-LOC budget is enforced by code review against the slice brief. |
| **Baseline** | Pre-slice, `log-query-api` consumes the shared scaffold (verified by direct read at lines 64, 130, 137-139, 147-152, 165-170, 180-185, 220-225 of `crates/log-query-api/src/lib.rs`). |
| **Target** | Zero new duplications; under-40-LOC budget; binary outcome on the static-grep CI step. |

## K5 — `gate-5-mutants-lumen` exercises the new Predicate arm at 100% kill rate

| | |
|---|---|
| **Who** | Maintainers of `lumen` and contributors auditing the mutation surface of the predicate. |
| **Does what** | The `Predicate::matches` arm for `body_regex` (`if let Some(re) = self.body_regex.as_ref() { if !re.is_match(&record.body) { return false; } }`) is mutation-tested by the workspace `gate-5-mutants-lumen` workflow shipped in gate-5-mutants-lumen-v0 (commit d96a807). |
| **By how much** | Every mutant `cargo mutants` produces on the new arm + builder + `is_empty` clause is killed by the acceptance suite. Mutant categories: (a) `is_match` -> `is_match_at(.., 0)` (kill via a fixture with a match in the middle); (b) negation flip in `if !re.is_match(...)` (kill via the calm-empty scenario); (c) `as_ref` -> `as_deref`-shape mutants if applicable; (d) the `is_empty` clause dropping the new field (kill via a unit test asserting `Predicate::new().body_regex(re).is_empty() == false`). The slice extends the acceptance suite so every mutant has at least one observable killer. |
| **Measured by** | `cargo mutants --in-diff origin/main` scoped to the lumen crate in the existing CI workflow; the run completes with a 100% kill rate per ADR-0005 Gate 5. The crafter's DELIVER artefact MUST include the mutants run output. |
| **Baseline** | The `gate-5-mutants-lumen` workflow exists (d96a807); pre-slice the lumen crate's mutation kill rate is 100% (the gate would not have shipped otherwise). |
| **Target** | 100% kill rate on the lumen crate's mutants run for this slice; binary outcome on the CI gate. |

## Summary table

| KPI | Target | Method | Baseline | Slice goal |
|---|---|---|---|---|
| K1 Honest matches | False-pos = 0, false-neg = 0 | Acceptance suite assertion | 100% of records returned today | Match-only response under `body_regex` |
| K2 Zero regression | Byte-equal slice-prior response when `body_regex` absent | Existing acceptance suites green + new "absent" scenario | HEAD at d96a807 | No-op default arm |
| K3 Fast-fail invalid | 400 with literal envelope BEFORE store call (3 arms: empty / over-cap / invalid syntax) | No-store-call assertion in 3 scenarios | Today the parameter does not exist | Bounded abuse surface |
| K4 Reuse confirmed | 0 new duplications; <40 new LOC | Static-grep CI step + code review | Existing shared-scaffold consumption | Real-world validation of ADR-0054 + ADR-0046 dep reuse |
| K5 Mutants killed | 100% kill rate on lumen | `cargo mutants` in `gate-5-mutants-lumen` workflow | 100% pre-slice | New `Predicate::body_regex` arm mutation-safe |

## What is NOT a KPI in this slice

- **Wire / payload latency**: no live observability stack exists
  at this stage of the platform (per ADR-0050 Decision 8 and
  ADR-0055 § Decision 13). A successor slice MAY add adoption
  counters and post-filter record-count histograms once a live
  observability consumer exists.
- **Adoption rate**: no client tracking exists. A successor slice
  in DEVOPS scope MAY add it.
- **Regex compile-cost histogram**: bounded by the 1024-byte cap
  and the `regex` crate's linear-time guarantee; measurement
  surface is deferred to the observability slice.
- **Cross-pillar reuse of the regex parser**: only logs carry a
  `body` field today; metrics and traces have no analogous field.
  A successor slice for trace `name` regex matching (if any) would
  duplicate the parser pattern, NOT call this slice's helper.
