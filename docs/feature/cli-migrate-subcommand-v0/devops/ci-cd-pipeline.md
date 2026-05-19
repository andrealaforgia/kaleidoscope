# CI/CD Pipeline - `cli-migrate-subcommand-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

## Posture: zero workflow edits (7th consecutive wave)

`.github/workflows/ci.yml` is **byte-untouched** by this feature.
The five-gate workspace contract established by ADR-0005 absorbs
this feature without modification. This is the SEVENTH consecutive
`kaleidoscope-cli` wave at zero workflow churn under the same job
set; the 2baa05c investment continues to amortise. Prior six:
`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
`cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
`cli-read-time-range-v0`, `cli-stats-time-range-v0`.

## Five-gate inheritance

| Gate | Job | This feature's contribution | Mechanism |
|---|---|---|---|
| Gate 1 | `gate-1-test-workspace` | New acceptance test `tests/migrate_subcommand.rs` discharging OK1/OK2/OK3/OK4 plus tenant-isolation and no-Lumen-touch sub-scenarios. | Auto-discovered via new `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`; `cargo test --workspace --all-targets --locked` picks it up structurally (A2). |
| Gate 2 | `gate-2-fmt-and-clippy` | `cargo fmt --check` and `cargo clippy -D warnings` on the new `migrate()` free function, the new `parse_tier` helper, the two new `Error` variants + Display arms in `src/lib.rs`, and the new `run_migrate` binary helper + dispatch arm + usage paragraph in `src/main.rs`. | Auto-applied; no new `-p` flag (binary crate; no graduation per A4). |
| Gate 3 | `gate-3-build-release` | `cargo build --release --locked` of the new code. | Auto-applied; no new `-p` flag. |
| Gate 4 | `gate-4-deny-audit` | `cargo deny check` against `deny.toml`. | No-op for this feature (zero new external dependencies per A3; no `deny.toml` change). |
| Gate 5 | `gate-5-mutants-kaleidoscope-cli` | 100% kill rate (modulo one acknowledged wire-invisible survivor: `SystemTime::now()` -> `UNIX_EPOCH`) on the new `migrate()` body, the new `parse_tier` four-arm match, the two new `Error` Display arms, and the new `run_migrate` binary helper - all of which live under `crates/kaleidoscope-cli/src/lib.rs` and `crates/kaleidoscope-cli/src/main.rs`. | Auto-covered by the existing `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full); both touched files fall inside the path filter `crates/kaleidoscope-cli/**` (A1). |

## No new job

No new workflow file, no new job, no edit to `ci.yml`. The
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml` is the
ONLY workflow-adjacent diff:

```toml
[[test]]
name = "migrate_subcommand"
path = "tests/migrate_subcommand.rs"
```

## Atomic commit rule

Per ADR-0005 and the SIX prior `kaleidoscope-cli` waves, the
DELIVER commit lands ALL of the following atomically:

1. New public free function `migrate(tenant, data_dir, item_id,
   to_tier_arg, writer) -> Result<(), Error>` in
   `crates/kaleidoscope-cli/src/lib.rs` (DESIGN DD1).
2. New private helper `parse_tier(s: &str) -> Result<Tier, ()>` in
   `crates/kaleidoscope-cli/src/lib.rs` (DESIGN DD3).
3. Two new variants on `kaleidoscope_cli::Error` plus matching
   `Display` arms in `crates/kaleidoscope-cli/src/lib.rs` (DESIGN
   DD4): `InvalidTier { value: String }` and
   `CinderMigrate(MigrateError)`.
4. New `run_migrate` binary helper, new `match` arm on the
   subcommand dispatcher, and new `migrate ...` paragraph in
   `write_usage` in `crates/kaleidoscope-cli/src/main.rs` (DESIGN
   DD6).
5. New acceptance test
   `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`.
6. `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`.

All existing acceptance tests must remain green and UNMODIFIED:
`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`,
`stats_time_range.rs`, `read_time_range.rs`, `observe_otlp_flag.rs`,
`observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`,
`ingest_and_read_roundtrip.rs`.

ANY edit to any of those eight files in the DELIVER commit's diff
auto-rejects review (DESIGN DD7 item 9: the `migrate` function is
wholly new; existing functions and their tests are out-of-scope).

## Trunk-based posture (D8 reminder)

`main` has no required-status-checks and no `enforce_admins`. CI is
feedback, not a gate. The DELIVER commit lands directly on trunk;
CI runs after-the-fact and signals via job status. Fix-forward via
a follow-up commit per Andrea's correction-notes posture if any
gate goes red post-merge.
