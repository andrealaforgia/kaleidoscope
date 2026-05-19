<!-- markdownlint-disable MD013 -->

# Wave Decisions — `cli-stats-time-range-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), 2026-05-19.

DESIGN pinned 4-arg `stats_with_tiers()` w/ trailing
`range: TimeRange` (DD1), Cinder branch ignoring `range` (DD2),
`if let (Some, Some)` auto-handles D-EmptyWindow (DD3), 4th-arg
mechanical update scoped to `stats_cinder_tier_distribution.rs` only
(DD4), zero new constructs (DD5). DEVOPS pinned ZERO workflow edits
(A1+A2) and named OK3 byte-identity as the D-CinderScope guardrail.
DISTILL translates UAT into 6 Rust `#[test]`. No edits to `lib.rs`,
`main.rs`, `Cargo.toml`, `ci.yml`, locked tests. File WILL NOT
compile against today's 3-arg signature — outer-loop RED.

---

## DWD-01 — Rust idiom mix: library-direct for OK1/OK2/OK3/OK4, subprocess for D-EmptyWindow

Tests 1–5 call `kaleidoscope_cli::stats_with_tiers` directly w/ an
explicit `TimeRange` (`stats_cinder_tier_distribution.rs` pattern).
Test 6 spawns `CARGO_BIN_EXE_kaleidoscope-cli` (`cli_binary_smoke.rs`
+ `read_time_range.rs` 5–6 pattern). Rationale: OK1/OK2/OK3/OK4 are
library-layer query contracts — library-direct is the sharpest
witness and mirrors the locked OK4 oracle. D-EmptyWindow at the
binary boundary exercises argv parsing of `--since`/`--until` +
empty-result stdout shape + exit code 0 in one probe — only
observable from outside the library. Mirrors `read_time_range.rs`'s
library-vs-subprocess split exactly.

---

## DWD-02 — Real-File substrate via `env::temp_dir()`

Every test creates `kal-cli-stats-time-range-<scenario>-<pid>-<nanos>`,
mirroring the seven cluster tests. `FileBackedLogStore` WAL+snapshot
and `FileBackedTieringStore` state-snapshot are real-file artefacts;
an in-memory fake would be Testing Theater.

---

## DWD-03 — Scenario coverage (Test ↔ AC ↔ OK#)

| # | Test | AC | OK# | Shape |
|---|---|---|---|---|
| 1 | `bounded_window_returns_only_records_in_half_open_interval` | AC1, AC2, AC3, AC6 | OK1 + OK2 | library |
| 2 | `cinder_lines_are_byte_identical_across_different_time_ranges` | AC6 (D-CinderScope) | OK3 | library |
| 3 | `no_flag_default_is_byte_equivalent_to_time_range_all` | AC4 | OK4 | library |
| 4 | `since_only_uses_u64_max_upper_bound` | AC7 (no `--until`) | OK1 half-bounded | library |
| 5 | `until_only_uses_zero_lower_bound` | AC7 (no `--since`) | OK1 half-bounded | library |
| 6 | `empty_window_via_subprocess_emits_records_zero_then_cinder_lines` | AC5, AC6, AC7 | OK1 + D-EmptyWindow | subprocess |

AC8 discharged structurally. AC9/AC10 (stderr on bad flag) inherited
from locked `read_time_range.rs` 5–6 (same parser/helpers — DD5 RCA).
AC11 (`print_usage`) belongs to `main.rs` inline tests. AC12/AC13
(locked tests stay green) discharged by NOT editing assertions; DD4
scopes 4th-arg update to `stats_cinder_tier_distribution.rs` only.

---

## DWD-04 — Witness timestamps: literal nanos (+ 2026 for subprocess)

Library-direct tests use literal integer nanos so a reviewer verifies
boundaries by mental arithmetic. #1 (OK1+OK2):
`{100,200,300,400,500}` + `(200,400)` → `{200,300}`; `200==since_ns`
INCLUDED (closed lower), `400==until_ns` EXCLUDED (open upper). #2
(OK3): same 5 + `(100,200)` vs `(300,500)` → Lumen counts 1 vs 2,
Cinder byte-identical. #3 (OK4): same 5 + `TimeRange::all()` → all 5,
earliest=…100Z, latest=…500Z. #4/#5 (half-bounded): `{100,200,300}` +
`(200,u64::MAX)`→`{200,300}`; same + `(0,200)`→`{100}`. #6
(subprocess empty window): 3 records at `SEED_EARLIEST_NS_2026 =
1_779_062_400_000_000_000` (2026-05-18T00:00:00Z) + next two nanos;
window `[2030-01-01T00:00:00Z, 2031-01-01T00:00:00Z)` —
unambiguously empty. ISO render `1970-01-01T00:00:00.000000NNNZ` via
`format_iso8601_utc_nanos` (`lib.rs:409-419`).

---

## DWD-05 — Out-of-scope

1. Invalid ISO 8601 fail-fast on `stats --since`/`--until` —
   inherited from locked `read_time_range.rs` 5–6 (DD5 RCA:
   byte-identical helper reuse); symmetric `stats` tests duplicate
   the parser oracle without new signal.
2. `--observe-otlp` composition — `stats` does not support it.
3. Other `query` parameters (severity, body, attrs) — DISCUSS #4.
4. Other time-format variants — inherited from `cli-read-time-range-v0`.
5. Cinder time-bound queries — D-CinderScope defers.
6. Legacy 3-arg `stats()` — untouched; `stats_subcommand.rs` no edits.
7. `print_usage` text — mutation-killed in `main.rs` inline tests.

---

## Definition of Done

- [x] 6 `#[test]` covering OK1/OK2/OK3/OK4 + half-bounded + D-EmptyWindow.
- [x] DWD-01..05 pinned; no edits to `lib.rs`, `main.rs`,
      `Cargo.toml`, `ci.yml`, or any locked test file.
- [x] Outer-loop RED: file fails to compile against 3-arg signature.
- [x] Error-path ratio 1/6 = 17%, UNDER 40%. **Exception**: KPI shape
      is a query-shape change, not an error surface; parser error
      paths inherited from locked `read_time_range.rs` 5–6 via shared
      `parse_time_range` (DD5 RCA). Principal new failure mode is
      D-EmptyWindow (#6); principal new invariant break is
      D-CinderScope (#2 byte-identity). **Accepted** per predecessor
      DISTILL DWD-05 clause.

## Handoff to DELIVER

Recipient: `@nw-software-crafter`. Tasks: (1) extend `stats_with_tiers`
3→4 args appending `range: TimeRange`; swap `TimeRange::all()`→`range`
at `lib.rs:360` (DD1/DD2); (2) `let range = parse_time_range(args)?;`
in `run_stats_with`, thread into call; (3) `print_usage` per AC11 w/
`[--since][--until]` + D-CinderScope + D-EmptyWindow notes; (4)
`[[test]]` entry (A2); (5) mechanical 4th-arg `TimeRange::all()` at
5 call sites in `stats_cinder_tier_distribution.rs` (DD4); leave
`stats_subcommand.rs` untouched; (6) `cargo test` GREEN; (7) mutation
scope `lib.rs`+`main.rs`, 100% kill (Gate 5 auto via `--in-diff`).
