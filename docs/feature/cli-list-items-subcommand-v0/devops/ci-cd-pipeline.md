# CI/CD Pipeline - `cli-list-items-subcommand-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

## Posture: zero workflow edits (10th consecutive wave)

`.github/workflows/ci.yml` is **byte-untouched** by this feature.
The five-gate workspace contract established by ADR-0005 absorbs
this feature without modification. This is the TENTH consecutive
`kaleidoscope-cli` wave at zero workflow churn under the same job
set; the 2baa05c investment continues to amortise. Prior nine:
`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
`cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
`cli-read-time-range-v0`, `cli-stats-time-range-v0`,
`cli-migrate-subcommand-v0`, `cli-migrate-observe-otlp-v0`, and
(implicit baseline) `cli-cinder-otlp-wiring-v0` precursor.

## Five-gate inheritance

| Gate | Job | This feature's contribution | Mechanism |
|---|---|---|---|
| Gate 1 | `gate-1-test-workspace` | New acceptance test `tests/list_items_subcommand.rs` discharging OK1 (happy path + determinism + N=0 empty-stdout), OK2 (tenant isolation), OK3 (invalid tier - two sub-scenarios `COLD` and `lukewarm`). All ten prior `tests/*.rs` files continue to pass byte-untouched. | Auto-discovered via new `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`; `cargo test --workspace --all-targets --locked` picks it up structurally (A2). |
| Gate 2 | `gate-2-fmt-and-clippy` | `cargo fmt --check` and `cargo clippy -D warnings` on the new `list_items(...)` free function in `src/lib.rs`, the promoted-visibility `parse_tier` (`pub(crate)` change in `src/lib.rs`), and the new `run_list_items` + dispatch arm + usage-text update in `src/main.rs`. | Auto-applied; no new `-p` flag (binary crate; no graduation per A4). |
| Gate 3 | `gate-3-build-release` | `cargo build --release --locked` of the updated code. | Auto-applied; no new `-p` flag. |
| Gate 4 | `gate-4-deny-audit` | `cargo deny check` against `deny.toml`. | No-op for this feature (zero new external dependencies per A3; no `deny.toml` change). |
| Gate 5 | `gate-5-mutants-kaleidoscope-cli` | 100% kill rate on the new surface: the `parse_tier(...)?` short-circuit, the `FileBackedTieringStore::open(...)?` call, the `cinder.list_by_tier(&tenant, tier)` call, the `Vec::sort_unstable()` boundary sort (pinned by OK1 determinism), the `writeln!(writer, "{}", id.0)` loop, the `Some("list-items")` dispatch arm in `main.rs`, and the `run_list_items` helper. All of this lives under `crates/kaleidoscope-cli/src/lib.rs` and `crates/kaleidoscope-cli/src/main.rs`. No inherited survivor (no clock; no recorder; no time-bearing OTLP line). | Auto-covered by the existing `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full); both touched files fall inside the path filter `crates/kaleidoscope-cli/**` (A1). |

## No new job

No new workflow file, no new job, no edit to `ci.yml`. The
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml` is the
ONLY workflow-adjacent diff:

```toml
[[test]]
name = "list_items_subcommand"
path = "tests/list_items_subcommand.rs"
```

## Atomic commit rule

Per ADR-0005 and the NINE prior `kaleidoscope-cli` waves, the
DELIVER commit lands ALL of the following atomically:

1. New `pub fn list_items(tenant, data_dir, tier_arg, writer) ->
   Result<(), Error>` free function in
   `crates/kaleidoscope-cli/src/lib.rs` (DESIGN DD1).
2. `parse_tier` visibility promotion from private to `pub(crate)`
   in `crates/kaleidoscope-cli/src/lib.rs` (DESIGN DD4).
3. New `run_list_items` binary-side helper in
   `crates/kaleidoscope-cli/src/main.rs`, parallel to `run_migrate`.
4. New `Some("list-items") => ...` dispatch arm in `main.rs`'s match
   block (one line).
5. New usage paragraph for `list-items` in `print_usage`/`write_usage`
   in `main.rs` (~3 lines).
6. New acceptance test
   `crates/kaleidoscope-cli/tests/list_items_subcommand.rs`.
7. `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`.

All existing acceptance tests must remain green; their assertions
remain UNMODIFIED. Specifically: `stats_subcommand.rs`,
`stats_cinder_tier_distribution.rs`, `stats_time_range.rs`,
`read_time_range.rs`, `observe_otlp_flag.rs`,
`observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`,
`migrate_subcommand.rs`, `migrate_observe_otlp_flag.rs`,
`ingest_and_read_roundtrip.rs`, `cli_binary_smoke.rs` are
byte-untouched. No mechanical signature-match suffixes are needed
this wave (no signature growth on any existing function;
`list_items` is a brand-new free function).

ANY non-mechanical edit to any of those eleven locked files in the
DELIVER commit's diff auto-rejects review.

## Trunk-based posture (D8 reminder)

`main` has no required-status-checks and no `enforce_admins`. CI is
feedback, not a gate. The DELIVER commit lands directly on trunk;
CI runs after-the-fact and signals via job status. Fix-forward via
a follow-up commit per Andrea's correction-notes posture if any
gate goes red post-merge.
