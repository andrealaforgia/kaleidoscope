# CI/CD Pipeline - `cli-read-time-range-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

## Posture: zero workflow edits (5th consecutive wave)

`.github/workflows/ci.yml` is **byte-untouched** by this feature.
The five-gate workspace contract established by ADR-0005 absorbs
this feature without modification. This is the FIFTH consecutive
`kaleidoscope-cli` wave at zero workflow churn under the same
job set; the 2baa05c investment continues to amortise.

## Five-gate inheritance

| Gate | Job | This feature's contribution | Mechanism |
|---|---|---|---|
| Gate 1 | `gate-1-test-workspace` (ci.yml:182) | New acceptance test `tests/read_time_range.rs` discharging OK1/OK2/OK3/OK4 + new parser unit tests in `src/lib.rs`'s `#[cfg(test)] mod tests` block | Auto-discovered via new `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`; auto-discovered for inline unit tests via `cargo test --workspace --all-targets`. |
| Gate 2 | `gate-2-fmt-and-clippy` | `cargo fmt --check` and `cargo clippy -D warnings` on the new parser + helper + signature delta + argv helper + usage text delta | Auto-applied; no new `-p` flag (binary crate; no graduation per A4). |
| Gate 3 | `gate-3-build-release` | `cargo build --release --locked` of the new code | Auto-applied; no new `-p` flag. |
| Gate 4 | `gate-4-deny-audit` | `cargo deny check` against `deny.toml` | No-op for this feature (zero new external dependencies per A3; no `deny.toml` change). |
| Gate 5 | `gate-5-mutants-kaleidoscope-cli` (ci.yml:949-1028) | 100% kill rate on the new parser, `days_from_civil` helper, `read()` signature delta, `parse_time_range` helper, `run_read_with` dispatcher delta, `write_usage` text delta - all of which live under `crates/kaleidoscope-cli/src/lib.rs` and `crates/kaleidoscope-cli/src/main.rs` | Auto-covered by the existing `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full); both touched files fall inside the path filter `crates/kaleidoscope-cli/**` at ci.yml:1006. |

## No new job

No new workflow file, no new job, no edit to `ci.yml`. The
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml` is the
ONLY workflow-adjacent diff:

```toml
[[test]]
name = "read_time_range"
path = "tests/read_time_range.rs"
```

## Atomic commit rule

Per ADR-0005 and the FIVE prior `kaleidoscope-cli` waves, the
DELIVER commit lands ALL of the following atomically:

1. New parser `parse_iso8601_utc_nanos` in
   `crates/kaleidoscope-cli/src/lib.rs` (next to its inverse
   formatter at lines 410-420).
2. New `days_from_civil` helper in
   `crates/kaleidoscope-cli/src/lib.rs` (next to existing
   `civil_from_days` at lines 426-438; Hinnant citation + URL +
   public-domain dedication in the comment per DESIGN DD3).
3. New `IsoParseError` typed enum in
   `crates/kaleidoscope-cli/src/lib.rs` (private).
4. Signature delta `read(.., range: TimeRange)` in
   `crates/kaleidoscope-cli/src/lib.rs` (one new parameter; one
   one-token call-site swap).
5. New `parse_time_range` argv-scan helper in
   `crates/kaleidoscope-cli/src/main.rs` (next to
   `parse_observe_otlp` at lines 130-144).
6. `run_read_with` dispatcher delta in
   `crates/kaleidoscope-cli/src/main.rs` (line 162: thread the
   parsed `TimeRange` into the `read(..)` call).
7. `write_usage` text delta in
   `crates/kaleidoscope-cli/src/main.rs` (lines 79-109: add
   `[--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>]` to the
   `read` subcommand line).
8. New acceptance test `crates/kaleidoscope-cli/tests/read_time_range.rs`.
9. New parser unit tests in
   `crates/kaleidoscope-cli/src/lib.rs`'s `#[cfg(test)] mod tests`
   block (at lines 457-651, alongside existing formatter tests).
10. `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`.

All five existing acceptance tests must remain green unchanged:
`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`,
`observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`,
`observe_otlp_read_flag.rs`, `ingest_and_read_roundtrip.rs`.

The two locked OK2-protection files
(`observe_otlp_read_flag.rs`, `observe_otlp_flag.rs`) MUST NOT
appear in the DELIVER commit's diff. Any edit auto-rejects review.

## Trunk-based posture (D8 reminder)

`main` has no required-status-checks and no `enforce_admins`.
CI is feedback, not a gate. The DELIVER commit lands directly on
trunk; CI runs after-the-fact and signals via job status. Fix-
forward via a follow-up commit per Andrea's correction-notes
posture if any gate goes red post-merge.
