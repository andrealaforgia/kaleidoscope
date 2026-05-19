# Wave Decisions — `cli-place-subcommand-v0` / DISTILL

- **Wave**: DISTILL
- **Author**: Quinn (`nw-acceptance-designer`)
- **Date**: 2026-05-19
- **Mode**: Translate DISCUSS's UAT scenarios + DESIGN's DD1-DD5 +
  DEVOPS's A1-A4 into `crates/kaleidoscope-cli/tests/place_subcommand.rs`.

## DWD-01: Five `#[test]` functions covering OK1-OK4

Exactly five Rust integration tests, with OK4 split across two
sub-scenarios because the flag-present / flag-absent invariant is
two-sided:

| # | KPI | Boundary | Shape |
|---|---|---|---|
| 1 | OK1 (place-success — North Star) | library-direct | `place(..., "new-item", "hot", &mut buf, None)`; assert Ok, exact stdout, post-call `get_entry().tier == Hot` |
| 2 | OK2 (overwrite-semantics — guardrail) | library-direct | place Hot, then place Cold; assert second Ok, stdout `tier=cold`, post-call `get_entry().tier == Cold` |
| 3 | OK3 (invalid-tier fail-fast) | subprocess | spawn binary with `LUKEWARM`; assert exit non-zero, stderr contains `LUKEWARM`, seeded item unchanged |
| 4 | OK4 emission (flag present) | library-direct | `place(..., Some(&otlp_path))`; sink contains one line with substrings `cinder.place.count`, `acme`, `hot` |
| 5 | OK4 inverse (flag absent) | library-direct | `place(..., None)`; candidate sink path does NOT exist after the call |

Five matches the predecessor `migrate_subcommand.rs` shape (six
tests for two KPIs across two boundaries) prorated for this
feature's one-step body vs `migrate`'s two-step body.

## DWD-02: Library-direct vs subprocess split

**Tests 1, 2, 4, 5: library-direct** (`place(...)` against a
`Vec<u8>` writer). Pins LIBRARY contract — stdout bytes, recorder
match, `parse_tier` short-circuit, `place` invocation, OTLP-JSON
emission — without subprocess fork cost.

**Test 3 only: subprocess** (`Command::new(bin())`). Exercises
BINARY boundary — argv parsing, dispatcher arm, `kaleidoscope-cli: {e}`
stderr prefix, exit-code propagation. Unique to the subprocess
path; the library never sees argv or the binary-name prefix.

No OK1/OK4 subprocess variant: the locked `cli_binary_smoke.rs`
already exercises the `Some("...") => run_X` dispatcher pattern
generically; once Crafty lands the `Some("place")` arm, smoke
coverage extends automatically. Adding a redundant subprocess test
would duplicate without marginal KPI signal.

## DWD-03: OK4 emission asserted via substring, not full JSON parse

Test #4 asserts the sink line contains `cinder.place.count`,
`acme`, `hot` — NOT the full OTLP-JSON envelope. Rationale:

- The envelope shape emitted by `CinderToOtlpJsonWriter::emit` is
  already pinned by the locked `tests/observe_otlp_cinder_wiring.rs`
  against `ingest`'s emission. Both emitters call the SAME
  `record_place(tenant, tier)` on the SAME adapter at
  `crates/self-observe/src/cinder_otlp_json.rs:260`. Re-pinning
  here duplicates coverage.

- `outcome-kpis.md > OK4 > Measured by` names the substring
  contract verbatim. DISTILL honours upstream literally rather
  than tightening.

The migrate-side OTLP test uses a full JSON parse because `migrate`
was a NEW emitter. `place` is NOT a new emitter — `ingest` already
emits `cinder.place.count` from the same writer.

## DWD-04: Test #2 uses production `place()` for both calls, not the seed helper

The first placement in Test #2 calls
`kaleidoscope_cli::place(...)` — the same surface under test —
rather than `place_item(...)` (direct
`FileBackedTieringStore::open + cinder.place(...)`). Rationale:

- The OK2 contract is "second `place` call overwrites the first".
  Both calls must be on the same code path. Using the helper would
  test "direct trait call overwritten by CLI call", which is
  weaker. Using the library function for both pins the CLI's
  idempotent invocation shape AND double-covers OK1.

- The helper is still used by Test #3 — there the seed serves only
  as a no-mutation oracle on the OK3 fail-fast path. Its purpose
  is "seed Cinder with a known pre-state", not "drive the system
  under test".

Mirrors the predecessor `migrate_subcommand.rs > Test #2` posture
inverted: there the seed for idempotent migrate uses the helper
because `migrate` is under test; here `place` is under test, so the
first call goes through `place(...)`.

## DWD-05: No Gherkin tags on `#[test]` functions

Rust integration tests have no mechanical equivalent of
`@walking_skeleton` / `@driving_port` / `@kpi` / `@US-01` tags.
Traceability lives in docstrings and `// Given / // When / // Then`
comment blocks per the cluster idiom (ten predecessor
`tests/*_subcommand.rs` files): the file docstring names
US-01 / OK1-OK4 with per-test mapping; each preamble names the KPI
it pins. Honours DISCUSS `user-stories.md > System Constraints`
("Rust `#[test]` with `// Given / // When / // Then`, not Gherkin")
and DEVOPS A2 (Gate 1 auto-discovers via
`cargo test --workspace --all-targets --locked`).

## Handoff

**Next agent**: `nw-software-crafter` (DELIVER wave).

**RED gate**: `crates/kaleidoscope-cli/tests/place_subcommand.rs`
references `kaleidoscope_cli::place` (free function) which does
NOT exist on `crates/kaleidoscope-cli/src/lib.rs` today.
Compilation fails. That compile failure IS the RED gate for
outside-in TDD.
