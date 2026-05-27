<!-- markdownlint-disable MD024 -->

# DISTILL Decisions — log-body-text-search-v0

Author: `nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-27.
Inputs: DISCUSS (`user-stories.md`, `story-map.md`), DESIGN
(`wave-decisions.md`, `application-architecture.md`,
`parse-helper-spec.md`), ADR-0055.

## DISTILL Decisions

### D1: Scaffold strategy — additive, Mandate 7 compliant

The DISTILL wave lands two scaffold edits and one new acceptance
suite, in service of RED-not-BROKEN:

1. `crates/lumen/src/predicate.rs` grows the additive
   `body_contains: Option<String>` field, the
   `Predicate::body_contains(s: impl Into<String>) -> Self` builder,
   and the `&& self.body_contains.is_none()` clause on
   `is_empty()`. The `Predicate::matches` arm body is REPLACED by
   an early guard
   `if self.body_contains.is_some() { unimplemented!("__SCAFFOLD__
   log-body-text-search-v0 RED") }` so any predicate carrying the
   new field panics RED with a recognisable marker.
2. `crates/log-query-api/src/lib.rs` grows the additive
   `body_contains: Option<String>` field on `LogsParams`, the
   `MAX_BODY_CONTAINS_LEN: usize = 1024` constant, the
   `parse_body_contains(_: &str) -> Result<String, &'static str>`
   helper (body
   `unimplemented!("__SCAFFOLD__ log-body-text-search-v0 RED")`),
   the new parse step in `handle_logs` (after the severity parse,
   before the store touch), and the four-arm dispatch that composes
   `min_severity` x `body_contains` per ADR-0055 Decision 7.
3. `crates/log-query-api/tests/slice_04_body_contains.rs` lands as
   a new file with 8 `#[tokio::test]` scenarios. Every scenario is
   `#[ignore]`'d at DISTILL close (D2 below) and tagged with the
   relevant `@US-*` and Mandate annotations.

The scaffold compiles workspace-wide (`cargo build --workspace
--all-targets` green); the existing acceptance suites
(`slice_01_logs_read.rs`, `slice_02_caps.rs`,
`slice_03_severity_filter.rs`) stay green unchanged (verified
locally: 6+8 scenarios across the three prior slices pass; 8
scenarios across slice 04 are ignored at DISTILL close).

### D2: RED state — every slice-04 test is `#[ignore]` at DISTILL close

Kaleidoscope's pre-commit hook
(`scripts/hooks/pre-commit:93`) runs `cargo test --workspace
--all-targets --locked` as Gate 1. An ENABLED test that panics
via `unimplemented!` would block every commit, including the
DISTILL handoff commit itself. The safer posture (named in the
DISTILL prompt) is:

- Every slice-04 scenario carries `#[ignore = "DISTILL RED: ..."]`.
- The walking-skeleton AC-01 carries the explicit Crafty
  instruction in its ignore reason
  (`"... Crafty de-ignores FIRST in DELIVER (walking skeleton)"`)
  so the outer-loop sequence is named in source.
- The RED state is verifiable on demand:
  `cargo test -p log-query-api --test slice_04_body_contains
  ac_01 -- --ignored` panics with `__SCAFFOLD__
  log-body-text-search-v0 RED` at
  `crates/log-query-api/src/lib.rs:299:5` (the `parse_body_contains`
  body). Verified at DISTILL close.

Mandate 7 RED-not-BROKEN is honoured by the `__SCAFFOLD__` panic
sites: the failing test names a missing behaviour, not a setup
error. The `#[ignore]` posture is the project convention
(mirrors `query-http-common`'s function-body tests at DISTILL
close per ADR-0054 / Mikado step F).

### D3: Walking Skeleton Strategy A — in-memory real-adapter wiring

Slice 04 is BROWNFIELD on `/api/v1/logs`. The walking skeleton
was built across four earlier slices (read endpoint, caps,
severity filter, `query-http-common` extraction). The slice-04
walking skeleton (AC-01) drives the same proven shape:

- The driving port is `log_query_api::router(store, tenant)`,
  the SINGLE public surface of the crate (Mandate 1 hexagonal
  boundary).
- The driven adapter under test is the REAL durable
  `FileBackedLogStore` opened by `common::open_durable_store` (a
  fresh tempdir per scenario; the same adapter the gateway writes
  through). Tagged `@real-io @adapter-integration` so Sentinel's
  Dim 9 boundary-proof passes.
- No `@in-memory` walking skeleton; no mock for the lumen seam.
  Strategy A as named in the wave-decisions vocabulary (real local
  resource adapter for the WS; in-memory only for the counting
  failing store used in the no-store-call assertions on 400 paths).

The cross-tenant scenario (AC-05) also uses the REAL durable
store (a second tenant exists implicitly in the same store; the
acme-prod records are seeded; the globex-staging tenant is queried
empty), so the platform's per-tenant isolation invariant is
exercised end-to-end through the real adapter.

### D4: AC-CAP decision — documented as `#[ignore]` with a Crafty handoff note

The `ac_cap_filter_runs_before_result_cap` scenario is included
in the suite (it pins ADR-0055 Decision 6, filter BEFORE cap),
but in a small-fixture form: 3 matching + 2 non-matching
records, well under the 100_000-row cap. The fully realistic
form would seed 200_000+ records into `FileBackedLogStore` at
runtime, which is expensive (slow CI, large tempdir) and
duplicates the bulk-double pattern already proven in
`slice_03_severity_filter.rs::BulkSeverityLogStore`.

DISTILL pins the scenario `#[ignore]` with an explicit Crafty
handoff note in its rustdoc: Crafty MAY add a
`BulkBodyContainsLogStore` test double in DELIVER if mutation
testing or `cargo public-api` exposes a `>` -> `>=`
cap-boundary mutant that the small-fixture scenarios miss. The
small-fixture AC-01..AC-05 scenarios already exercise the
filter-and-dispatch arm end-to-end; the bulk variant adds
cap-volume realism but no new behavioural surface.

### D5: Predicate seam — EXTENDING `lumen::Predicate` per DD3

The DISTILL wave honours DD3 (ADR-0055 Decision 10) at the
seam: `lumen::Predicate` grows one field, one builder, one
`matches` arm (scaffolded), and one `is_empty` clause. The
slice does NOT introduce handler-side post-`query_with`
filtering; the predicate IS the filter at the store boundary
(symmetric with `service` and `min_severity`).

The four-arm dispatch in `handle_logs` (none / severity-only /
body-only / both) is the natural shape; the
`Predicate::new().min_severity(floor).body_contains(target)`
builder chain mirrors the slice-03 pattern byte-for-byte.

## US to AC mapping

| Story | AC scenarios (slice_04_body_contains.rs) | DELIVER order |
|---|---|---|
| US-01 walking skeleton (`body_contains=kafka%20timeout` narrows the response) | `ac_01_known_substring_narrows_the_response_to_matching_records`; `ac_cap_filter_runs_before_result_cap` (filter-before-cap pin) | 1st de-ignored |
| US-02 unmatched substring returns calm `[]` (NEVER 404, NEVER 500) | `ac_02_unknown_substring_returns_calm_empty_array` | 4th |
| US-03 default unchanged (no parameter -> today's behaviour) | `ac_03_missing_body_contains_returns_every_in_window_record` | 2nd |
| US-04 empty `body_contains` is a redacted 400; 1024-byte cap (DD6) | `ac_04a_empty_body_contains_returns_400_with_literal_envelope`; `ac_04b_over_cap_body_contains_returns_400_with_redacted_envelope` | 3rd |
| US-05 case-sensitive matching pinned | `ac_04c_case_sensitive_match_pinned` | 5th |
| US-06 cross-tenant isolation holds for `body_contains` | `ac_05_cross_tenant_isolation_holds_for_body_contains` | 6th |

Every US has at least one AC; every AC carries the
`@US-<id>` tag in its rustdoc preface; `@walking_skeleton
@driving_port @real-io @adapter-integration` on AC-01;
`@driving_port` on every other scenario; `@real-io` on the
durable-store scenarios (AC-02, AC-03, AC-04c, AC-05, AC-cap)
and not on the counting-store scenarios (AC-04a, AC-04b — the
purpose is to PROVE the store is NEVER touched, so a real
durable adapter would be a category error).

The DELIVER ordering mirrors story-map.md's priority rationale:
US-01 first (walking skeleton derisks the seam), US-03 second
(backward-compat), US-04 third (the only new error arm),
US-02 fourth (calm-empty contract), US-05 fifth
(case-sensitivity), US-06 last (cross-tenant invariant).

## Mandate 7 RED-ready confirmed

Three `__SCAFFOLD__` panic sites are pinned in production code,
each compiles green and each panics on the first invocation that
exercises the unimplemented behaviour:

1. `crates/lumen/src/predicate.rs:67` (line of the
   `unimplemented!("__SCAFFOLD__ log-body-text-search-v0 RED")`
   guard at the top of `Predicate::matches`). Triggered by any
   predicate carrying `body_contains: Some(_)`.
2. `crates/log-query-api/src/lib.rs:299` (line of the
   `unimplemented!("__SCAFFOLD__ log-body-text-search-v0 RED")`
   in the `parse_body_contains` body). Triggered by any non-`None`
   `params.body_contains` reaching the handler.
3. The acceptance suite at
   `crates/log-query-api/tests/slice_04_body_contains.rs`
   exercises both panic sites end-to-end (AC-01 panics inside
   `parse_body_contains`; AC-03 — once de-ignored — exercises the
   `None`-arm path; AC-05 — once de-ignored — exercises the
   tenant-bucket lookup that PRECEDES `matches`, so it tests the
   isolation invariant orthogonally).

The `__SCAFFOLD__` marker is unique to this slice (a workspace
grep for `__SCAFFOLD__ log-body-text-search-v0` returns exactly
the three sites above + the test file's commentary), so Crafty's
diff in DELIVER is unambiguous: every site must be removed when
the real bodies land.

## RED state evidence

Captured at DISTILL close (2026-05-27, this wave):

1. `cargo build --workspace --all-targets`: green. Workspace
   compiles end-to-end after the scaffold edits; no warnings on
   the changed crates.
2. `cargo clippy -p lumen -p log-query-api --all-targets
   --locked -- -D warnings`: green. Scaffold survives the
   strict-warning posture of the pre-commit Gate 2; the
   `#[allow(dead_code)]` annotations on `MAX_BODY_CONTAINS_LEN`
   and `parse_body_contains` are deliberate (the constant and the
   helper are referenced by the handler but the
   `dead_code` lint cannot see across the `unimplemented!` panic
   path).
3. `cargo test -p log-query-api --tests` (slice 01-04 combined):
   green. Slice 01 (6 tests pass), slice 02 (the cap suite,
   covered in earlier output), slice 03 (8 tests pass), slice 04
   (8 tests ignored).
4. `cargo test -p log-query-api --test slice_04_body_contains
   ac_01 -- --ignored`: RED via
   `__SCAFFOLD__ log-body-text-search-v0 RED` panic at
   `crates/log-query-api/src/lib.rs:299:5`. The walking skeleton
   fails for a behavioural reason (the parser body is
   unimplemented), not a setup error.

The slice is ready for DELIVER. Crafty's first action is to
de-ignore AC-01, implement `parse_body_contains`, and remove the
`Predicate::matches` `__SCAFFOLD__` guard — the smallest GREEN
move that satisfies the walking skeleton. The remaining
scenarios are de-ignored one at a time per the established
outer-loop convention.

## Handoff to DELIVER

The DELIVER wave (`@nw-software-crafter`, Crafty) inherits:

1. The two scaffold edits in `crates/lumen/src/predicate.rs` and
   `crates/log-query-api/src/lib.rs` — both compile green; both
   panic `__SCAFFOLD__` when exercised; the production-code
   diff that lands in DELIVER is the smallest change that
   replaces the two `unimplemented!` calls with real bodies and
   removes the `body_contains.is_some()` early guard in
   `Predicate::matches`.
2. The acceptance suite at
   `crates/log-query-api/tests/slice_04_body_contains.rs` — 8
   scenarios, all `#[ignore]`'d, ordered for the outer-loop
   sequence named in § "US to AC mapping" above.
3. The parse-helper-spec test surface
   (`docs/feature/log-body-text-search-v0/design/parse-helper-spec.md`)
   — Crafty adds the 7 inline unit tests in the `#[cfg(test)]
   mod tests` block of `crates/log-query-api/src/lib.rs` next to
   the existing `parse_min_severity_*` tests as part of the
   GREEN move on AC-04a (empty rejection) and AC-04b (over-cap
   rejection).
4. The slice-04 test scenario annotations (every `#[ignore]`
   reason names the missing behaviour explicitly) — Crafty's
   de-ignore order follows the reasons verbatim.
