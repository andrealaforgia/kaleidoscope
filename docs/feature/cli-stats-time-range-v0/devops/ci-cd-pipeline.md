# CI/CD Pipeline - `cli-stats-time-range-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

## Posture: zero workflow edits (6th consecutive wave)

`.github/workflows/ci.yml` is **byte-untouched** by this feature.
The five-gate workspace contract established by ADR-0005 absorbs
this feature without modification. This is the SIXTH consecutive
`kaleidoscope-cli` wave at zero workflow churn under the same
job set; the 2baa05c investment continues to amortise. Prior
five: `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
`cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
`cli-read-time-range-v0`.

## Five-gate inheritance

| Gate | Job | This feature's contribution | Mechanism |
|---|---|---|---|
| Gate 1 | `gate-1-test-workspace` | New acceptance test `tests/stats_time_range.rs` discharging OK1/OK2/OK3/OK4. | Auto-discovered via new `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`; `cargo test --workspace --all-targets --locked` picks it up structurally (A2). |
| Gate 2 | `gate-2-fmt-and-clippy` | `cargo fmt --check` and `cargo clippy -D warnings` on the `stats_with_tiers` signature delta + Lumen call-site token swap + `run_stats_with` dispatcher delta + `write_usage` text delta. | Auto-applied; no new `-p` flag (binary crate; no graduation per A4). |
| Gate 3 | `gate-3-build-release` | `cargo build --release --locked` of the new code. | Auto-applied; no new `-p` flag. |
| Gate 4 | `gate-4-deny-audit` | `cargo deny check` against `deny.toml`. | No-op for this feature (zero new external dependencies per A3; no `deny.toml` change). |
| Gate 5 | `gate-5-mutants-kaleidoscope-cli` | 100% kill rate on the `stats_with_tiers` parameter declaration + Lumen call-site token swap + `run_stats_with` dispatcher delta + `write_usage` text delta - all of which live under `crates/kaleidoscope-cli/src/lib.rs` and `crates/kaleidoscope-cli/src/main.rs`. | Auto-covered by the existing `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full); both touched files fall inside the path filter `crates/kaleidoscope-cli/**` (A1). |

## No new job

No new workflow file, no new job, no edit to `ci.yml`. The
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml` is the
ONLY workflow-adjacent diff:

```toml
[[test]]
name = "stats_time_range"
path = "tests/stats_time_range.rs"
```

## Atomic commit rule

Per ADR-0005 and the SIX prior `kaleidoscope-cli` waves, the
DELIVER commit lands ALL of the following atomically:

1. `stats_with_tiers` signature delta in
   `crates/kaleidoscope-cli/src/lib.rs` (one new 4th parameter
   `range: TimeRange` appended; DESIGN DD1).
2. Lumen call-site token swap in
   `crates/kaleidoscope-cli/src/lib.rs:359-361`:
   `lumen.query(tenant, TimeRange::all())` becomes
   `lumen.query(tenant, range)` (DESIGN DD2).
3. `run_stats_with` dispatcher delta in
   `crates/kaleidoscope-cli/src/main.rs:226-235`: one new line
   `let range = parse_time_range(args)?;`; thread `range` into
   the `stats_with_tiers(...)` call as the new 4th argument.
4. `write_usage` text delta for the `stats` subcommand in
   `crates/kaleidoscope-cli/src/main.rs:81-119`: add
   `[--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>]` plus the
   half-open `[since, until)` note plus the D-CinderScope note
   (range applies to Lumen lines only) plus the D-EmptyWindow
   note.
5. New acceptance test
   `crates/kaleidoscope-cli/tests/stats_time_range.rs`.
6. Mechanical 4th-arg update at five call sites in
   `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
   (DESIGN DD4): each `stats_with_tiers(&acme, &data, &mut
   stdout)` becomes `stats_with_tiers(&acme, &data, &mut stdout,
   TimeRange::all())`; import line gains `TimeRange` from
   `lumen::`. No assertion text edited.
7. `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`.

All existing acceptance tests must remain green (`stats_subcommand.rs`
unchanged; `stats_cinder_tier_distribution.rs` post-mechanical-update;
`observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`,
`observe_otlp_read_flag.rs`, `ingest_and_read_roundtrip.rs`,
`read_time_range.rs` unchanged).

`stats_subcommand.rs` MUST NOT appear in the DELIVER commit's
diff. Any edit auto-rejects review (DESIGN DD4 / DD6 item 1:
exercises only the legacy 3-arg `stats()` which is out-of-scope).

## Trunk-based posture (D8 reminder)

`main` has no required-status-checks and no `enforce_admins`.
CI is feedback, not a gate. The DELIVER commit lands directly on
trunk; CI runs after-the-fact and signals via job status. Fix-
forward via a follow-up commit per Andrea's correction-notes
posture if any gate goes red post-merge.
