# DISTILL — cli-evaluate-policy-subcommand-v0

Author: orchestrator-direct (agent quota exhausted)
Date: 2026-05-19

## DWD-01 — Idiom

Rust integration test file at
`crates/kaleidoscope-cli/tests/evaluate_policy_subcommand.rs`.
5 `#[test]` functions. Library-direct for OK1/OK3/OK4;
subprocess for OK2 (two scenarios — one per bad arg position).

## DWD-02 — Real-File substrate

`std::env::temp_dir()` with PID and nanos namespacing.
`FileBackedTieringStore::open` for seeding and verification.

## DWD-03 — Coverage

| Test | OK# | Idiom |
|------|-----|-------|
| evaluate_policy_migrates_aged_hot_items_and_reports_count | OK1 | library-direct |
| evaluate_policy_is_idempotent_under_repeated_invocation | OK3 | library-direct |
| evaluate_policy_subcommand_invalid_hot_to_warm_secs_exits_nonzero | OK2 | subprocess |
| evaluate_policy_subcommand_invalid_warm_to_cold_secs_exits_nonzero | OK2 | subprocess |
| evaluate_policy_with_observe_otlp_emits_one_line_per_migration | OK4 | library-direct |

## DWD-04 — Aged-placement seeding

The Cinder `place` API accepts an explicit `placed_at`
SystemTime. To seed an item that the policy considers "aged",
test #1, #2, and #5 call `place_aged(..., SystemTime::now() -
Duration::from_secs(7200))` — two-hour-old Hot items. With
policy `hot_to_warm = 1 hour`, the items are well past the
threshold and migrate to Warm on the first evaluate_at call.
The second call sees their fresh `migrated_at` (= now of first
call) and skips them — OK3 idempotency.

## DWD-05 — Out-of-scope

- No dry-run test (Cinder API gives no preview hook).
- No per-tenant filter test (DISCUSS D5).
- No floating-point or unit-suffixed duration test.

## RED gate

File imports `kaleidoscope_cli::evaluate_policy` which does not
exist on `lib.rs` yet. Compile failure is the outside-in
starting signal; DELIVER adds the function.
