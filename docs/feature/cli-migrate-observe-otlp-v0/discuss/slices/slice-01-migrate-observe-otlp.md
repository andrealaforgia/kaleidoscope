# Slice 01 — Manual migrations also land in the `--observe-otlp` file

**Story**: US-01
**Outcome KPIs**: OK1 (principal), OK2, OK3, OK4
**Tag**: operator-visible (not `@infrastructure`)
**Estimated effort**: well under 1 day

## Goal

Add `--observe-otlp <path>` to the `kaleidoscope-cli migrate`
subcommand. When set, every successful `migrate()` call emits one
NDJSON line to `<path>` via `CinderToOtlpJsonWriter::record_migrate`,
naming tenant, from-tier, to-tier. When absent, behaviour is
byte-equivalent to today; the locked `migrate_subcommand.rs` tests
pass unchanged.

## What ships

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | `migrate(...)` gains `otlp_log_path: Option<&Path>`. Recorder construction at line 434 becomes conditional, mirroring `ingest` at lines 155-184. |
| `crates/kaleidoscope-cli/src/main.rs` | `run_migrate_with` calls `parse_observe_otlp(args)?` and threads through. Usage text gains `[--observe-otlp <path>]`. |
| `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` | NEW. Mirrors `migrate_subcommand.rs` harness. Hosts OK1, OK2 (no-flag), OK3 (unknown-item subprocess), OK4 (invalid-tier subprocess). |
| `crates/kaleidoscope-cli/Cargo.toml` | New `[[test]] name = "migrate_observe_otlp_flag", path = "tests/migrate_observe_otlp_flag.rs"`. No new dep. |

## IN scope

- The `migrate` subcommand only.
- The `migrate` event only (one `cinder.migrate(...)` per successful call).
- Single-process single-call shape (no in-process concurrency).

## OUT of scope

- Bulk migrate.
- `--dry-run`.
- JSON output of the stdout line.
- `--observe-otlp` on the pre-flight `get_entry` read (only the actual
  migrate emits).
- Wiring on `cinder.place` or `cinder.evaluate` from the migrate path.
- Changes to `CinderToOtlpJsonWriter` public API (ADR-0039 §1 locked).

## Learning hypothesis

Disproves any assumption that the migrate path needs a different
recorder-construction shape from `ingest`. ADR-0039 §1 locks the
writer's public surface, so store-open-time construction is the only
compile-tractable seam. The happy-path acceptance test is the
empirical probe that the pattern transports cleanly.

## Acceptance criteria (DISTILL translates to `#[test]` fns)

- `migrate_with_observe_otlp_emits_one_cinder_migrate_count_line_with_full_attributes`:
  seed `acme/batch-00042` in Hot, call `migrate(..., "cold",
  otlp_log_path = Some(<sink>))`, assert `Ok(())`, exact stdout
  `migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`,
  one non-empty line in `<sink>` with `metric.name ==
  "cinder.migrate.count"`, `tenant_id == "acme"`, `scope.name ==
  "kaleidoscope.cinder"`, `asInt == "1"`, point attrs contain
  `from="hot"` and `to="cold"`, file ends with `\n`.
- `migrate_with_observe_otlp_absent_is_byte_equivalent_and_creates_no_file`:
  seed `acme/batch-00007` in Hot, call `migrate(..., "hot",
  otlp_log_path = None)`, assert exact stdout
  `migrated tenant=acme item=acme/batch-00007 from=hot to=hot\n`, no
  file created at any candidate sink path.
- `migrate_subcommand_unknown_item_with_observe_otlp_set_emits_no_line`
  (subprocess): spawn binary with `migrate acme <data> ghost-item warm
  --observe-otlp <sink>` against empty Cinder; assert non-zero exit,
  stderr substring `ghost-item` and `unknown item`, empty stdout, no
  `cinder.migrate.count` line in `<sink>` (if it exists).
- `migrate_subcommand_invalid_tier_with_observe_otlp_set_creates_no_file`
  (subprocess): spawn binary with `migrate acme <data> item_id LUKEWARM
  --observe-otlp <sink>` against a `<sink>` that does NOT pre-exist;
  assert non-zero exit, stderr substring `LUKEWARM`, empty stdout,
  `<sink>` file does NOT exist after the call.
- `existing_migrate_subcommand_tests_continue_to_pass_byte_equivalently`
  (meta, verified by CI): `cargo test --package kaleidoscope-cli
  --test migrate_subcommand` exits 0 with zero assertion edits.

## Dependencies

- `cli-migrate-subcommand-v0` shipped.
- `cinder-to-otlp-json-bridge-v0` shipped.
- `cli-cinder-otlp-wiring-v0` shipped (precedent for store-open-time
  recorder construction; reuses `parse_observe_otlp` helper).

## Effort estimate

30 minutes wiring change inside `migrate`; 30 minutes `main.rs`
thread-through + usage text; 1-2 hours new acceptance test (four
scenarios); 30 minutes `Cargo.toml` + local green run.

## Definition of Done

- All AC green under `cargo test --package kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean.
- Dogfood: seed a Cinder Hot placement; run `cargo run --bin
  kaleidoscope-cli -- migrate acme /tmp/kdata acme/batch-00042 cold
  --observe-otlp /tmp/audit.ndjson`; `cat /tmp/audit.ndjson | jq .`
  shows one line with `from=hot, to=cold`.
