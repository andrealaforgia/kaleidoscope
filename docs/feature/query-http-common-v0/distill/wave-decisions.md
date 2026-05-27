# DISTILL Decisions — query-http-common-v0

Author: `@nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-27.

This document records the DISTILL-wave pins for `query-http-common-v0`,
maps the five DISCUSS stories to the unit tests Crafty will turn green
in DELIVER, and surfaces the Mandate 7 RED-not-BROKEN scaffold the
DELIVER wave inherits.

The feature is a pure refactor extraction with NO wire-observable
behaviour change. There are NO new Gherkin acceptance scenarios; the
existing acceptance suites of the three consumer crates (`query-api`,
`log-query-api`, `trace-query-api`) ARE the K2 byte-identity gate.
DISTILL's contribution is the new crate's INLINE UNIT TESTS, one per
artefact of the public API, scaffolded RED via `#[ignore]` for the
function bodies and GREEN immediately for the data-only surface.

## DISTILL Decisions

### D1: scaffold strategy — inline unit tests in the new crate

PIN: the acceptance tests for `query-http-common` are `#[cfg(test)] mod
tests` inline in `crates/query-http-common/src/lib.rs`. NO separate
`tests/` integration harness, NO BDD `*.feature` files, NO step
definitions.

Rationale: the public API is six items (two consts, four reason
consts, one struct, three free functions) over `&str`, `Option<&str>`,
and `TenantId`. There is no driving port in the hexagonal sense — the
crate is consumed as a LIBRARY by three sibling adapter crates. The
"acceptance test" surface IS the public API surface; inline unit tests
ARE the right grain. Gherkin would introduce ceremony with no
expressiveness gain.

### D2: data-only consts are GREEN at DISTILL close

PIN: the cap constants (`MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS`), the
four reason text consts (`REASON_INVALID_TIME_RANGE`,
`REASON_WINDOW_TOO_LARGE`, `REASON_TOO_MANY_ROWS`,
`REASON_MISSING_TENANT`), and the `ErrorBody` serialisation are
implemented at DISTILL close and their tests pass. These are DATA, not
LOGIC; Scholar can write them directly per Mandate 7's data-fixture
permission and the redaction-symmetric constraints from ADR-0054.

Rationale: the constants and the `Serialize` derive carry no business
logic. Their values are byte-for-byte equal to the strings today
emitted by the three consumer crates' inline code (verified by grep
during DISTILL phase 1). The K2 acceptance gate (Crafty's DELIVER step
E-G) re-runs the three consumers' existing acceptance suites against
post-rewire code; this inline test is the typed-seam regression net.

### D3: function bodies are RED via `unimplemented!` + `#[ignore]`

PIN: the three free functions (`parse_time_range`,
`resolve_tenant_or_refuse`, `error_response`) are
`unimplemented!("__SCAFFOLD__ query-http-common-v0 RED")` at DISTILL
close. Their unit tests in `#[cfg(test)] mod tests` are annotated
`#[ignore = "DELIVER step C/D — __SCAFFOLD__"]` so the workspace
pre-commit hook (`cargo test --workspace` by default skips
`#[ignore]`'d tests) passes.

Rationale: Mandate 7 RED-not-BROKEN. The tests MUST exist and MUST
identify failing observable behaviours, but they MUST NOT break the
workspace gate. `#[ignore]` with the explicit reason string is the
idiomatic Rust convention; DELIVER's Crafty removes the
`#[ignore]` ONE AT A TIME (outer-loop Outside-In TDD convention) as
he implements each behaviour, watching one test fail RED, then green
it, then commit, then move to the next.

### D4: `TimeRange` location pin — in `query-http-common`, seconds-level

PIN: `pub struct TimeRange { pub start_secs: u64, pub end_secs: u64 }`
lives in `query-http-common`. It is the SECONDS-level pair the parser
returns. The three consumer crates each keep their own
pillar-specific NANOSECOND `TimeRange` (`pulse::TimeRange`,
`lumen::TimeRange`, `ray::TimeRange`) and a private `seconds_to_nanos`
helper.

Rationale: ADR-0054's `parse_time_range -> Result<TimeRange,
&'static str>` signature requires a `TimeRange` type to exist in this
crate. ADR-0048 Decision 5 and the DESIGN DD2 pin both caution
explicitly against forcing one of the three pillar-specific
nanosecond `TimeRange`s into this crate. The resolution is to define a
new SECONDS-level `TimeRange` here — semantically distinct from the
pillar types, but byte-cheap because it's two `u64`s. The consumer's
cap-arm reads `tr.end_secs.saturating_sub(tr.start_secs)` directly
against `MAX_WINDOW_SECONDS`; the nanosecond conversion happens AFTER
the cap arm in the per-consumer `seconds_to_nanos` call.

### D5: walking-skeleton strategy — Strategy A (in-memory)

PIN: the unit tests in this crate are pure data + in-memory operations.
No filesystem, no network, no subprocess. The crate has NO driven
adapter (no I/O of any kind), so Mandate 1 Hexagonal Boundary
Enforcement and Mandate 4 Pure Function Extraction collapse into a
single posture: every public API item is testable in-process with no
fixture machinery.

Rationale: there is no I/O surface in `query-http-common` to integrate
against. The Earned Trust discipline lands one level deeper at the
three CONSUMER binaries (whose composition roots already run their
own startup probes); those probes are UNCHANGED by this refactor. The
K2 byte-identity gate in the consumers' existing acceptance suites is
the runtime evidence the new crate has not silently changed the
contract.

### D6: K2 byte-identity enforcement — Crafty re-runs the consumers' suites

PIN: KPI 2 (byte-identity of every 400 and 401 response body pre/post
extraction) is enforced by Crafty in DELIVER step E-G via the existing
acceptance suites of the three consumer crates:

- `crates/query-api/tests/*` (the query_range acceptance suite)
- `crates/log-query-api/tests/*` (the logs read acceptance suite)
- `crates/trace-query-api/tests/slice_01_traces_read.rs` (the traces
  read + lookup-by-id acceptance suite)

Crafty's protocol is: snapshot the pre-extraction acceptance suite
output (e.g. via `cargo test -p query-api -- --nocapture 2>&1 | tee
pre.txt`), perform the rewire (Mikado step E for query-api, F for
log-query-api, G for trace-query-api), re-run the same `cargo test`
invocation, diff. Any byte-level divergence in the response body of an
existing 400 or 401 scenario is a K2 failure. Scholar in DISTILL does
NOT need to add new scenarios for this — the existing suites are the
gate.

## US-to-AC mapping

The five DISCUSS stories collapse to the new crate's public API
surface and the K2 gate. Mapping:

| Story | What it asks for | Inline tests in `query-http-common` | K2 gate (Crafty step E-G) |
|-------|------------------|-------------------------------------|---------------------------|
| US-01 | Single-source caps | `test_max_window_seconds_value`, `test_max_result_rows_value` | three consumer suites' cap-rejection scenarios |
| US-02 | Single-source time-range parser | nine `test_parse_time_range_*` tests (#[ignore]) | three consumer suites' bounds-parse scenarios |
| US-03 | Single-source error envelope | `test_error_body_serialises_to_expected_json`, `test_error_body_field_order_is_status_then_error`, five `test_error_response_*` tests (#[ignore]) | every 400 and 401 scenario across the three suites |
| US-04 | Single-source fail-closed tenant pattern | three `test_resolve_tenant_or_refuse_*` tests (#[ignore]) | three consumer suites' 401 scenarios (four arms total) |
| US-05 | Integration gate | implicit — the workspace test count parity check | `cargo test --workspace` post-Mikado-step-H |

Total: 24 inline unit tests in `query-http-common`. 7 GREEN at DISTILL
close (data-only); 17 RED via `#[ignore]` (function bodies; DELIVER
de-ignores one at a time).

## Mandate 7 RED-ready scaffold confirmed

Scaffold artefacts:

- `crates/query-http-common/Cargo.toml` (new): package metadata, deps
  (axum 0.7, serde, serde_json, aegis), lints, AGPL-3.0-or-later,
  `publish = false`, version `0.1.0`.
- `crates/query-http-common/src/lib.rs` (new): AGPL header, crate-level
  docs, `#![forbid(unsafe_code)]`, the six data items GREEN, three
  `unimplemented!("__SCAFFOLD__ ...")` function bodies, the inline test
  module with 24 tests (7 GREEN, 17 `#[ignore]`).
- `Cargo.toml` workspace root: `"crates/query-http-common"` added to
  the `members` array, positioned before the three consumer crates.

Verification commands and outcomes (run during DISTILL phase 4):

- `cargo build -p query-http-common` — GREEN
- `cargo build --workspace` — GREEN
- `cargo test -p query-http-common` — 7 passed, 0 failed, 17 ignored

Mandate 7 properties satisfied:

- RED-not-BROKEN: the workspace pre-commit gate passes (no
  `unimplemented!` is reached by a non-ignored test); the 17 ignored
  tests exist, compile, and identify the failing behaviours DELIVER
  will implement.
- One-at-a-time outer loop: Crafty removes `#[ignore]` for one test,
  watches it fail with the `__SCAFFOLD__` panic message (or the
  partial-implementation failure), implements until it passes, commits
  atomically, moves to the next.
- Data fixtures are PRECONDITIONS not OUTPUTS: every test asserts a
  return value or an observable response property of the FUNCTION
  UNDER TEST; no fixture computes the expected output.

## RED state evidence

Run at DISTILL phase 4 close (2026-05-27):

```
$ cargo build --workspace
   Compiling trace-query-api v0.1.0 (...)
   Compiling log-query-api v0.1.0 (...)
   Compiling query-http-common v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.68s

$ cargo test -p query-http-common
running 24 tests
test tests::test_error_body_field_order_is_status_then_error ... ok
test tests::test_error_body_serialises_to_expected_json ... ok
test tests::test_error_response_body_is_byte_identical_json_envelope ... ignored
test tests::test_error_response_carries_internal_server_error_status ... ignored
test tests::test_error_response_carries_unauthorized_status ... ignored
test tests::test_error_response_content_type_is_application_json ... ignored
test tests::test_error_response_returns_given_status_code ... ignored
test tests::test_max_result_rows_value ... ok
test tests::test_max_window_seconds_value ... ok
test tests::test_parse_time_range_accepts_equal_bounds_as_empty_range ... ignored
test tests::test_parse_time_range_accepts_valid_integer_bounds ... ignored
test tests::test_parse_time_range_accepts_zero_as_lower_bound ... ignored
test tests::test_parse_time_range_error_never_echoes_raw_value ... ignored
test tests::test_parse_time_range_rejects_inverted_bounds_with_named_reason ... ignored
test tests::test_parse_time_range_rejects_negative_bounds ... ignored
test tests::test_parse_time_range_rejects_non_numeric_end ... ignored
test tests::test_parse_time_range_rejects_non_numeric_start ... ignored
test tests::test_parse_time_range_truncates_fractional_seconds ... ignored
test tests::test_reason_constants_match_callsite_texts ... ok
test tests::test_reason_constants_never_contain_a_credential_marker ... ok
test tests::test_resolve_tenant_or_refuse_refuses_none_with_401 ... ignored
test tests::test_resolve_tenant_or_refuse_returns_some_tenant_unchanged ... ignored
test tests::test_resolve_tenant_or_refuse_uses_service_label_in_reason ... ignored
test tests::test_time_range_is_constructible_with_start_le_end ... ok

test result: ok. 7 passed; 0 failed; 17 ignored; 0 measured; 0 filtered out
```

Acceptance gate: 7 GREEN data-only tests, 17 RED-via-ignore function
body tests, 0 FAILED, 0 BROKEN. Workspace build green. Mandate 7
RED-ready scaffold is in place; DELIVER handoff to Crafty is unblocked.

## Handoff readiness

- Inline unit tests live in `crates/query-http-common/src/lib.rs`
  under `#[cfg(test)] mod tests`.
- The DELIVER wave reads `docs/feature/query-http-common-v0/design/mikado-plan.md`
  (Steps A-H), this `wave-decisions.md`, and `test-scenarios.md`.
- The K2 byte-identity gate runs against the EXISTING acceptance
  suites of the three consumer crates; no new scenario files are
  produced by DISTILL.
- The K4 mutation gate (100% kill rate) runs post-Mikado via the new
  `gate-5-mutants-query-http-common` CI job DEVOPS pinned in
  `.github/workflows/ci.yml`.
