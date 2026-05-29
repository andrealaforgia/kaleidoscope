# DISTILL Decisions — log-body-regex-search-v0

Author: `nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-29.
Mode: propose. Scope: acceptance suite + RED-ready scaffold.

This wave lands the acceptance suite for the slice that grows
`GET /api/v1/logs` by ONE optional query-string parameter
`body_regex=<pattern>`, plus the minimum scaffold on
`crates/lumen` and `crates/log-query-api` that lets the suite
COMPILE and reach the `__SCAFFOLD__` RED markers. The DESIGN
brief is ADR-0056 and the four DESIGN artefacts in
`../design/`. The DISCUSS brief is the seven user stories in
`../discuss/user-stories.md`.

## DISTILL Decisions

### D1 — Scaffold strategy: panic guards inside the new code paths

The two new code paths (`parse_body_regex` in `log-query-api`
and the `body_regex` arm inside `Predicate::matches` in `lumen`)
are scaffolded with `unimplemented!("__SCAFFOLD__
log-body-regex-search-v0 RED")` panics. The signatures are
final (Crafty does NOT alter them); only the bodies are
replaced in DELIVER. This keeps the acceptance suite COMPILING
against the final surface AND lets every scenario reach a
panic that names the slice, so a stray ignored-test run in CI
surfaces the RED state with an obvious marker.

Rationale: this is the same posture slice 04 (body_contains)
took at DISTILL close (`crates/log-query-api/src/lib.rs:288-296`
in the pre-shipping snapshot is the precedent); the scaffold
posture is the workspace convention.

### D2 — RED state: 9 of 10 scenarios panic at scaffold; 1 already green

The slice's 10 scenarios decompose as:

- 9 scenarios reach either `parse_body_regex` (panics) or the
  `Predicate::matches` body_regex arm (panics) — the canonical
  RED state.
- 1 scenario — **AC-06 (mutual exclusion)** — already passes
  green at DISTILL close. The mutual-exclusion check
  (`if body_contains.is_some() && params.body_regex.is_some()`)
  is purely a handler-side conditional that returns 400 BEFORE
  any parse helper is called; the check has no dependency on
  `parse_body_regex`. The check IS the implementation; there
  is no further code Crafty fills in DELIVER for this arm
  alone.

This is acceptable under Mandate 7 (RED-not-BROKEN): the RED
state is `#[ignore]`'d at the test level so the pre-commit
gate stays green, and de-ignoring AC-06 in DELIVER simply
moves it from "ignored" to "passing" without any production
code change. The slice's behavioural surface is still 90%
RED at handoff (9 of 10 scenarios). Crafty surfaces this as a
DELIVER note when de-ignoring AC-06.

### D3 — Walking skeleton strategy: Strategy A (in-process Router)

The walking skeleton (AC-01) drives the router via
`tower::ServiceExt::oneshot` against an axum `Router` built
over a REAL durable `FileBackedLogStore` adapter in a per-test
tempdir. This is the SAME shape every prior slice on
`/api/v1/logs` has taken (slice 01..slice 04); no network
port is bound; the production composition root (the thin
binary with its env-var tenant resolution + Earned-Trust
probe) is a separate concern outside the slice 01 surface.

The skeleton answers "can a user accomplish their goal?": the
SRE issues ONE request with a regex covering every shape of
the kafka-timeout failure family and gets back the matching
records, with the unrelated records stripped server-side. The
five `@walking_skeleton @driving_port @real-io
@adapter-integration` tags are concentrated on AC-01.

### D4 — Predicate PartialEq decision: drop the derive (ADR-0056 Decision 4 / DD2)

`regex::Regex` does NOT implement `PartialEq` or `Eq`. The
existing `#[derive(Debug, Clone, Default, PartialEq, Eq)]` on
`lumen::Predicate` is therefore relaxed to
`#[derive(Debug, Clone, Default)]`. A workspace grep for
predicate equality use confirms zero production callers rely
on the derived `==`; lumen acceptance suites compare predicate
behaviour via `matches`, not by structural equality.

Alternatives considered and rejected:

- Hand-implement `PartialEq` comparing `re.as_str()` — dishonest
  (`(?i)kafka` and `kafka` compile to behaviourally-different
  regexes; structural equality of the source string is not
  semantic equality of the matcher).
- Keep the field as `Option<String>` and compile per record —
  rejected by DD2 / ADR-0056 Decision 4 on cost grounds.

Outcome: the derive is relaxed; the relaxation is part of
ADR-0056's accepted `cargo public-api` diff. Confirmed in this
DISTILL wave; the scaffold removes `PartialEq, Eq` from the
derive in the same commit that adds the `body_regex` field.

### D5 — Mutual-exclusion check location: handler, BEFORE parse_body_regex

The mutual-exclusion check sits inside `handle_logs`,
immediately AFTER `parse_body_contains` returns and BEFORE
`parse_body_regex` is called. This is the location pinned by
ADR-0056 Decision 7 and `application-architecture.md` § Handler
Order step 6. The placement guarantees:

1. The mutual-exclusion 400 surfaces an HONEST cross-check
   failure, NOT a downstream compile failure.
2. `parse_body_contains` already ran, so its own empty /
   over-cap 400 surfaces first (the cross-check only fires if
   `body_contains` parsed successfully).
3. The store is NEVER touched on this path.
4. `parse_body_regex` itself stays a one-parameter pure
   function over the raw string; it does not need to know
   about other parameters.

### D6 — Cap pin uniform with body_contains (1024 bytes inclusive)

`MAX_BODY_REGEX_LEN = 1024` lives next to `MAX_BODY_CONTAINS_LEN`
in `crates/log-query-api/src/lib.rs`. The cap is INCLUSIVELY 1024
bytes (1024 served, 1025 refused), mirroring `MAX_BODY_CONTAINS_LEN`
exactly (ADR-0055 Decision 5 / ADR-0056 Decision 5 / DD3).
Operator-facing consistency: one rule for every body-related
parameter. The constant is `#[allow(dead_code)]` at the DISTILL
RED-ready snapshot because `parse_body_regex` is a panic
scaffold; the allow is removed in DELIVER when Crafty fills the
helper body (it reads the constant).

## US to AC mapping

| User Story | Acceptance Criterion | Tag set |
|---|---|---|
| US-01 (walking skeleton: a known pattern matches the failure family) | AC-01 `ac_01_known_pattern_matches_failure_family` | `@walking_skeleton @driving_port @real-io @adapter-integration @US-01` |
| US-02 (unknown pattern returns calm empty) | AC-02 `ac_02_unknown_pattern_returns_empty` | `@driving_port @real-io @US-02` |
| US-03 (missing body_regex preserves today's behaviour) | AC-03 `ac_03_missing_body_regex_returns_all` | `@driving_port @real-io @US-03` |
| US-04a (invalid regex syntax is a redacted 400) | AC-04a `ac_04a_invalid_regex_returns_400` | `@driving_port @US-04a` |
| US-04b (empty body_regex is the same redacted 400) | AC-04b `ac_04b_empty_string_returns_400` | `@driving_port @US-04b` |
| US-04c (over-cap body_regex is the same redacted 400) | AC-04c `ac_04c_length_over_cap_returns_400` | `@driving_port @US-04c` |
| US-05 (case-sensitive default) | AC-05 `ac_05_case_sensitive_default` | `@driving_port @real-io @US-05` |
| US-06 (body_contains and body_regex mutually exclusive) | AC-06 `ac_06_mutual_exclusion_returns_400` | `@driving_port @US-06` |
| US-07 (cross-tenant isolation holds for body_regex) | AC-07 `ac_07_cross_tenant_isolation` | `@driving_port @real-io @US-07` |
| US-01 conjunctive arm (composition with `min_severity`) | AC-COMBO `ac_combo_severity_x_regex` | `@driving_port @real-io @US-01` |

Every US has at least one AC. US-04 is split a / b / c to match the
three distinct rejection arms (empty, over-cap, invalid syntax)
declared by DISCUSS; they share an envelope but pin different
boundary mutants.

## Mandate 7 RED-ready confirmed

The new code paths carry explicit `__SCAFFOLD__
log-body-regex-search-v0 RED` markers that surface inside any
scenario that exercises them:

- `crates/lumen/src/predicate.rs` — the `body_regex.is_some()`
  guard at the top of `Predicate::matches` panics with the
  `__SCAFFOLD__` marker the moment a predicate carrying a
  compiled regex reaches a record.
- `crates/log-query-api/src/lib.rs` — the entire
  `parse_body_regex` body is
  `unimplemented!("__SCAFFOLD__ log-body-regex-search-v0 RED")`.

The pre-commit gate
(`cargo test --workspace --all-targets --locked`) passes because
every slice_05 scenario is `#[ignore]`'d. The RED state is
verifiable on demand:

```text
$ cargo test -p log-query-api --test slice_05_body_regex
  ac_01 -- --ignored
...
thread 'ac_01_known_pattern_matches_failure_family' panicked at
  crates/log-query-api/src/lib.rs:402:5:
not implemented: __SCAFFOLD__ log-body-regex-search-v0 RED
```

Crafty de-ignores AC-01 FIRST in DELIVER (walking skeleton; the
outer-loop convention) and fills the parser body, the predicate
arm, and the dispatch wiring as he goes. The remaining
scenarios are de-ignored one at a time as the implementation
fills out.

## RED state evidence

Verified at DISTILL close:

```text
$ cargo build --workspace --all-targets
   Finished `dev` profile [...] target(s) in 1.38s

$ cargo test -p log-query-api --test slice_05_body_regex
running 10 tests
test ac_01_known_pattern_matches_failure_family ... ignored
test ac_02_unknown_pattern_returns_empty ... ignored
test ac_03_missing_body_regex_returns_all ... ignored
test ac_04a_invalid_regex_returns_400 ... ignored
test ac_04b_empty_string_returns_400 ... ignored
test ac_04c_length_over_cap_returns_400 ... ignored
test ac_05_case_sensitive_default ... ignored
test ac_06_mutual_exclusion_returns_400 ... ignored
test ac_07_cross_tenant_isolation ... ignored
test ac_combo_severity_x_regex ... ignored
test result: ok. 0 passed; 0 failed; 10 ignored; 0 measured;
  0 filtered out

$ cargo test -p log-query-api --test slice_05_body_regex
  ac_01 -- --ignored
test ac_01_known_pattern_matches_failure_family ... FAILED
thread '[...]' panicked at crates/log-query-api/src/lib.rs:402:5:
not implemented: __SCAFFOLD__ log-body-regex-search-v0 RED
```

Workspace build clean; 10 tests `#[ignore]`'d in slice_05;
AC-01 surfaces the RED scaffold marker when run with
`--ignored`.

## PartialEq note

Per D4 above, the existing `#[derive(Debug, Clone, Default,
PartialEq, Eq)]` on `lumen::Predicate` is relaxed to
`#[derive(Debug, Clone, Default)]`. The relaxation lands in
the DISTILL scaffold commit (so the new `body_regex:
Option<Regex>` field compiles); the relaxation is part of
ADR-0056's accepted `cargo public-api` diff. Workspace grep
confirmed no production caller relies on the derive (the
search was: `Predicate.*==`, `==.*Predicate`,
`assert_eq.*Predicate`, `PartialEq.*Predicate` — zero hits in
production paths; the only hits are the workspace's other
predicate structs in `pulse` and `strata`, which keep their
own derives unchanged).

Status: **dropped**. Documented here and pinned in
ADR-0056 § Decision 4 (Load-bearing consequence).

## Walking-skeleton boundary (Dim 9 / WS Strategy)

WS Strategy declared: **Strategy A (in-process Router via
oneshot, real durable FileBackedLogStore adapter)**. Mirrors
slice 01..slice 04 exactly. The walking-skeleton scenario
AC-01 tags `@real-io @adapter-integration` because the
`FileBackedLogStore` is touched on real disk inside a per-test
tempdir — the same adapter the production gateway writes
through. The litmus test "if I deleted the real adapter,
would this WS still pass?" answers NO: the suite imports
`lumen::FileBackedLogStore` and opens a real on-disk store.

The driven port `lumen::LogStore` has two adapters:
`InMemoryLogStore` (no I/O) and `FileBackedLogStore` (real
filesystem I/O via the on-disk store under
`crates/lumen/src/file_backed.rs`). AC-01, AC-02, AC-03,
AC-05, AC-07, and AC-COMBO seed the real `FileBackedLogStore`;
AC-04a / AC-04b / AC-04c / AC-06 use a counting failing store
purely to assert the store is NEVER touched on the four 400
paths — these are NOT InMemory stand-ins; they are explicit
adapter doubles whose only job is to count calls. The
adapter-integration coverage requirement is met (the
acceptance suite exercises the real `FileBackedLogStore` on
the matching, non-matching, default, case-sensitive,
cross-tenant, and conjunctive arms).

## Story scope

Generated from the 7 user stories in `../discuss/user-stories.md`
exclusively. Behaviours OUT of scope per DISCUSS are NOT
exercised: no alternative regex backend test, no multi-field
matching test, no multiple-regexes-per-request test, no
combined-body_contains-AND-body_regex AND-compose test (the
mutual-exclusion 400 is the relevant behaviour at slice 01),
no per-pattern compile timeout test, no regex-compile cache
test.

## Files touched (DISTILL wave)

| File | Change | Net new lines |
|---|---|---|
| `crates/lumen/Cargo.toml` | Add `regex = "1"` to `[dependencies]` with the ADR-0056 cross-reference comment block. | ~10 |
| `crates/lumen/src/predicate.rs` | Drop `PartialEq, Eq` from the derive; add `use regex::Regex;`; add `body_regex: Option<Regex>` field; add `body_regex(re)` builder; add `__SCAFFOLD__` panic guard at the top of `matches`; extend `is_empty` conjunction. | ~30 |
| `crates/log-query-api/Cargo.toml` | Add `regex = "1"` to `[dependencies]` (the helper returns a `Regex` and the dispatch hands it to the predicate). | ~9 |
| `crates/log-query-api/src/lib.rs` | Add `use regex::Regex;`; add `body_regex: Option<String>` field on `LogsParams`; add `#[allow(dead_code)] const MAX_BODY_REGEX_LEN: usize = 1024;`; add the mutual-exclusion check; add the `body_regex` parse arm in the handler; grow the dispatch from 4 to 6 reachable arms (with the unreachable pruned arms named); add the `parse_body_regex` scaffold (signature final, body panics). | ~75 |
| `crates/log-query-api/tests/slice_05_body_regex.rs` | NEW. 10 `#[tokio::test]` scenarios, all `#[ignore]`'d, covering US-01..US-07 + AC-COMBO. | ~500 |
| `docs/feature/log-body-regex-search-v0/distill/wave-decisions.md` | NEW (this file). | ~200 |
| `docs/feature/log-body-regex-search-v0/distill/test-scenarios.md` | NEW (sibling). | ~120 |

Net change: 7 files; ~950 lines added. No production crate
under `crates/*/src/` carries any new behaviour (the
scaffolds panic). The acceptance suite is the load-bearing
deliverable.

## Upstream Changes

**None.** Zero DISCUSS / DESIGN / DEVOPS assumptions changed.
The slice composes additively on top of ADR-0056 and the
DESIGN artefacts in `../design/` without altering any of them.
No upstream-changes artefact is produced.
