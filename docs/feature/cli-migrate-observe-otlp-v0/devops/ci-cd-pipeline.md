# CI/CD Pipeline - `cli-migrate-observe-otlp-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

## Posture: zero workflow edits (9th consecutive wave)

`.github/workflows/ci.yml` is **byte-untouched** by this feature.
The five-gate workspace contract established by ADR-0005 absorbs
this feature without modification. This is the NINTH consecutive
`kaleidoscope-cli` wave at zero workflow churn under the same job
set; the 2baa05c investment continues to amortise. Prior eight:
`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
`cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
`cli-read-time-range-v0`, `cli-stats-time-range-v0`,
`cli-migrate-subcommand-v0`, and (this) `cli-migrate-observe-otlp-v0`.

## Five-gate inheritance

| Gate | Job | This feature's contribution | Mechanism |
|---|---|---|---|
| Gate 1 | `gate-1-test-workspace` | New acceptance test `tests/migrate_observe_otlp_flag.rs` discharging OK1 (happy path), OK2 (no-flag no-file sub-scenario), OK3 (UnknownItem subprocess), OK4 (InvalidTier subprocess). The locked `tests/migrate_subcommand.rs` continues to pass with four mechanical `, None` suffixes (DD5 #3-#6) and no assertion edits, serving as the OK2 byte-equivalence probe on stdout. | Auto-discovered via new `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`; `cargo test --workspace --all-targets --locked` picks it up structurally (A2). |
| Gate 2 | `gate-2-fmt-and-clippy` | `cargo fmt --check` and `cargo clippy -D warnings` on the updated `migrate()` 6-arg signature, the new `match otlp_log_path` arm in `src/lib.rs`, and the updated `run_migrate` / `run_migrate_with` / `write_usage` in `src/main.rs`. | Auto-applied; no new `-p` flag (binary crate; no graduation per A4). |
| Gate 3 | `gate-3-build-release` | `cargo build --release --locked` of the updated code. | Auto-applied; no new `-p` flag. |
| Gate 4 | `gate-4-deny-audit` | `cargo deny check` against `deny.toml`. | No-op for this feature (zero new external dependencies per A3; no `deny.toml` change). |
| Gate 5 | `gate-5-mutants-kaleidoscope-cli` | 100% kill rate on the new surface: the `match otlp_log_path { Some => ..., None => ... }` arms, the `OpenOptions::create(true).append(true).open(path)?` call site, the `CinderToOtlpJsonWriter::new(file)` construction, and the `main.rs` `as_deref()` propagation. All of this lives under `crates/kaleidoscope-cli/src/lib.rs` and `crates/kaleidoscope-cli/src/main.rs`. Inherits the one acknowledged wire-invisible survivor (`SystemTime::now()` -> `UNIX_EPOCH`) from the predecessor wave. | Auto-covered by the existing `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full); both touched files fall inside the path filter `crates/kaleidoscope-cli/**` (A1). |

## No new job

No new workflow file, no new job, no edit to `ci.yml`. The
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml` is the
ONLY workflow-adjacent diff:

```toml
[[test]]
name = "migrate_observe_otlp_flag"
path = "tests/migrate_observe_otlp_flag.rs"
```

## Atomic commit rule

Per ADR-0005 and the EIGHT prior `kaleidoscope-cli` waves, the
DELIVER commit lands ALL of the following atomically:

1. `migrate()` signature grows a sixth `Option<&Path>` parameter
   in `crates/kaleidoscope-cli/src/lib.rs` (DESIGN DD1).
2. Internal `match otlp_log_path { Some => ..., None => ... }`
   arm replaces the unconditional `Box::new(CinderRecorder)` at the
   `FileBackedTieringStore::open` call site (DESIGN DD2).
3. `run_migrate` / `run_migrate_with` thread `parse_observe_otlp`
   through to the new sixth argument; `write_usage` gains a
   `[--observe-otlp <path>]` suffix and one explanatory sentence
   (DESIGN DD3).
4. SIX mechanical `, None` suffixes at the existing call sites
   (DD5): `main.rs:279` (`run_migrate_with`); `lib.rs:843` (inline
   white-box test); and `migrate_subcommand.rs:187, 233, 440, 513`
   (four locked test call sites; zero assertion edits).
5. New acceptance test
   `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs`.
6. `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`.

All existing acceptance tests must remain green; their assertions
remain UNMODIFIED. Specifically: `stats_subcommand.rs`,
`stats_cinder_tier_distribution.rs`, `stats_time_range.rs`,
`read_time_range.rs`, `observe_otlp_flag.rs`,
`observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`,
`ingest_and_read_roundtrip.rs`, `cli_binary_smoke.rs` are
byte-untouched; `migrate_subcommand.rs` gains four mechanical
`, None` suffixes and nothing else.

ANY non-mechanical edit to any of those nine files in the DELIVER
commit's diff auto-rejects review.

## Trunk-based posture (D8 reminder)

`main` has no required-status-checks and no `enforce_admins`. CI is
feedback, not a gate. The DELIVER commit lands directly on trunk;
CI runs after-the-fact and signals via job status. Fix-forward via
a follow-up commit per Andrea's correction-notes posture if any
gate goes red post-merge.
