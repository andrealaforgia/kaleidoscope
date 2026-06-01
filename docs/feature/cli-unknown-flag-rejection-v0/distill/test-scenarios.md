# Test Scenarios — cli-unknown-flag-rejection-v0

Acceptance file: `crates/kaleidoscope-cli/tests/slice_17_unknown_flag_rejection.rs`
Driving port for all rows: the `kaleidoscope-cli` binary argv entry,
spawned as a subprocess via `env!("CARGO_BIN_EXE_kaleidoscope-cli")`.

## AC table

| AC | US | Command (argv after binary) | Setup | Expected exit | Expected stderr (substring) | Expected stdout | Status today |
|----|----|------------------------------|-------|---------------|-----------------------------|-----------------|--------------|
| AC-01 | US-01 | `--bogus` | none | 2 | names `--bogus` + usage block (`kaleidoscope-cli ingest`, `kaleidoscope-cli read`) | (not asserted) | GREEN |
| AC-02 | US-02 | `read acme <data_dir> --bogus` | seed 1 record for `acme` into `<data_dir>` via `ingest` | 2 | `unknown flag "--bogus"` + usage block | EMPTY | RED (`#[ignore]`d) |
| AC-03 | US-03 | `bogus-subcommand` | none | 2 | names `bogus-subcommand` + usage block | (not asserted) | GREEN |
| AC-04 | US-04 | `read acme <data_dir> --observe-otlp <metric_path>` | seed 1 record for `acme` into `<data_dir>` | 0 | `read ok: records=1` | contains `"body":"hi"`; metric file contains `lumen.query.count` | GREEN |

## Observed-today verification (pre-fix)

Probed against the shipped `target/debug/kaleidoscope-cli`:

- AC-01: `--bogus` -> exit 2, stderr `kaleidoscope-cli: unknown subcommand "--bogus"`. GREEN.
- AC-02: `read acme <seeded> --bogus` -> exit 0, stderr `read ok: records=1`, record printed to stdout. This is the silent-accept GAP. RED against the expected exit 2.
- AC-03: `bogus-subcommand` -> exit 2, stderr `kaleidoscope-cli: unknown subcommand "bogus-subcommand"`. GREEN.
- AC-04: `read acme <seeded> --observe-otlp <path>` -> exit 0, record printed, `lumen.query.count` line appended. GREEN.

(Note: with a NON-existent data dir, AC-02 would exit 1 on a lumen I/O
error, masking the gap. Seeding a real store surfaces the true exit-0
silent-accept; see DWD-04.)

## RED-not-BROKEN proof (AC-02)

`cargo test -p kaleidoscope-cli --test slice_17_unknown_flag_rejection -- --ignored`:

```
test ac02_subcommand_unknown_flag_is_rejected_before_any_records_are_read ... FAILED
assertion `left == right` failed: subcommand unknown flag must exit 2; got status ExitStatus(unix_wait_status(0))
  left: Some(0)
 right: Some(2)
```

Clean assertion failure (process exited normally with code 0) — RED, not
a build break / import error / panic. Mandate 7 satisfied.

## Run commands

- Active suite (AC-01/03/04 GREEN, AC-02 skipped):
  `cargo test -p kaleidoscope-cli --test slice_17_unknown_flag_rejection`
- AC-02 RED gate on demand:
  `cargo test -p kaleidoscope-cli --test slice_17_unknown_flag_rejection -- --ignored`
- Full crate regression: `cargo test -p kaleidoscope-cli`
  (verified: 0 failed across all test binaries; exactly 1 ignored.)

## Self-review checklist

- [x] Every AC enters through the binary argv driving port (subprocess).
- [x] US-01..US-04 each covered by at least one AC (traceability).
- [x] Error-path ratio 3/4 = 75% (>= 40% floor).
- [x] Business language in scenario names and prose; technical detail in
      step bodies only.
- [x] Assertions are observable (exit code, stderr substring, stdout
      bytes, metric file content) — no internal-state assertions.
- [x] AC-02 RED-not-BROKEN proven (assertion failure, exit 0 vs 2).
- [x] AC-01/03/04 GREEN against the shipped binary.
- [x] No production scaffold needed (fix is a free function Crafty adds).
- [x] `#[ignore]` rationale pinned (DWD-07): pre-commit safety, atomic
      de-ignore by Crafty in DELIVER.
- [x] Harness helpers duplicated inline per DISCUSS D7 (no shared
      `tests/common`).
- [x] Cargo.toml `[[test]]` entry registered for the new file.
