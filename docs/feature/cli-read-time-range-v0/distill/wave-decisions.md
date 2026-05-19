<!-- markdownlint-disable MD013 -->

# Wave Decisions — `cli-read-time-range-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), 2026-05-19.

DESIGN pinned 5-arg `read()` w/ trailing `range: TimeRange` (DD1),
parser split lib/main (DD2), parser scope `[1970, 9999]` + Hinnant
attribution (DD3). DEVOPS pinned CI exit code as the gate and
filesystem-absence as the OK4 probe. DISTILL translates UAT
scenarios into Rust `#[test]` and locks the test-shape mix. No
edits to `lib.rs`, `main.rs`, `Cargo.toml`, `ci.yml`, locked tests.
File will NOT compile against today's 4-arg `read()` — outer-loop RED.

---

## DWD-01 — Rust idiom mix: library-direct for OK1/OK2/OK3, subprocess for OK4

Tests 1–4 call `kaleidoscope_cli::read` directly with an explicit
`TimeRange` (`observe_otlp_read_flag.rs` pattern). Tests 5–6 spawn
`CARGO_BIN_EXE_kaleidoscope-cli` (`cli_binary_smoke.rs` pattern).

**Rationale**: OK1/OK2/OK3 probe the library-layer query contract —
library-direct is the sharpest witness. OK4 probes the
binary-boundary fail-fast invariant: argv parser must reject malformed
input BEFORE `FileBackedLogStore::open`. Only observable from outside
the library; the binary's argv layer IS the unit under test.

---

## DWD-02 — Real-File substrate via `env::temp_dir()`

Every test creates `kal-cli-read-time-range-<scenario>-<pid>-<nanos>`
under `env::temp_dir()`, mirroring the four existing cluster tests.
`FileBackedLogStore`'s WAL + snapshot are real-file artefacts; an
in-memory fake would be Testing Theater.

---

## DWD-03 — Scenario coverage (Test ↔ AC ↔ OK#)

| # | Test | AC | OK# | Shape |
|---|---|---|---|---|
| 1 | `bounded_window_returns_only_records_in_half_open_interval` | AC1, AC2 | OK1 | library |
| 2 | `no_flag_default_is_byte_equivalent_to_time_range_all` | AC3 | OK2 | library |
| 3 | `since_only_uses_u64_max_upper_bound` | AC5 (no `--until`) | OK3 | library |
| 4 | `until_only_uses_zero_lower_bound` | AC5 (no `--since`) | OK3 | library |
| 5 | `invalid_since_value_fails_fast_naming_flag_in_stderr` | AC7 | OK4 | subprocess |
| 6 | `invalid_until_value_fails_fast_naming_flag_in_stderr` | AC8 | OK4 | subprocess |

AC4 (order-independent parsing) discharged structurally by tests 5–6
argv lists. AC6 (round-trip) and AC9 (`print_usage` text) belong to
inline `#[cfg(test)]` in `lib.rs`/`main.rs`. AC10 (locked tests stay
green) discharged by NOT editing them; per DESIGN DD1/DD4 they hit
the no-flag default `TimeRange::all()`.

---

## DWD-04 — Witness timestamps: literal nanos `{100, 200, 300, 400, 500}`

OK1 ingests `{100, 200, 300, 400, 500}` and queries `(200, 400)` →
expected `{200, 300}`. Boundaries: `200 == since_ns` INCLUDED;
`400 == until_ns` EXCLUDED; `100`/`500` outside. OK3a uses
`{100, 200, 300}` + `(200, u64::MAX)` → `{200, 300}`. OK3b uses
`{100, 200, 300}` + `(0, 200)` → `{100}`. OK2 uses `{10, 11, 12,
13}` + `TimeRange::all()` → all four.

**Rationale**: easy literal integers let a reviewer verify boundary
inclusion/exclusion by mental arithmetic. ISO 8601 parsing is
exercised by the subprocess argv path in tests 5–6 where bad values
ARE the assertion target. `observed_time_unix_nano` accepts any
`u64`.

---

## DWD-05 — Out-of-scope

1. **`--observe-otlp` composition** with `--since` / `--until`
   (DESIGN DD5#4 / DISCUSS D5). `otlp_log_path = None` in every
   library-direct test.
2. **Other `LogStore::query` parameters** (severity, body, attrs) —
   DESIGN DD5#5 / DISCUSS D6.
3. **Other time-format variants** — only two valid shapes plus
   `not-an-iso` (shape failure) and `2026-13-32T25:99:99Z` (calendar
   failure). Missing-`Z`, non-digit, year>9999 belong to the inline
   `lib.rs` unit tests.
4. **Round-trip property `parse(format(ns)) == ns`** — AC6 belongs
   to inline `lib.rs` per DESIGN DD2 rationale 2.
5. **`print_usage` text contract** — AC9 is mutation-killed in
   `main.rs` inline tests.

---

## Definition of Done

- [x] 6 `#[test]` functions covering OK1/OK2/OK3/OK4.
- [x] DWD-01 through DWD-05 pinned.
- [x] No edits to `lib.rs`, `main.rs`, `Cargo.toml`, `ci.yml`, locked
      tests.
- [x] Outer-loop RED: file fails to compile against today's 4-arg
      `read()`.
- [x] Error-path ratio: 2/6 = 33%, UNDER 40%. **Exception**: KPI
      shape is intrinsically 1-error-of-4 (`outcome-kpis.md`); OK4
      split into `--since`/`--until` symmetric sub-tests gives 2/6.
      Inflating duplicates OK4 without new signal. **Accepted.**

## Handoff to DELIVER

Recipient: `@nw-software-crafter`. Inputs: this file + DESIGN + DEVOPS
wave-decisions + 6 RED tests. Crafter inner-loop tasks:
(1) add 5th param `range: TimeRange` to `read`, single-token swap at
`lib.rs:284`; (2) add private `parse_iso8601_utc_nanos` +
`days_from_civil` to `lib.rs` next to their inverses, inline
mutation-killing tests including AC6 round-trip; (3) add private
`parse_time_range` to `main.rs` next to `parse_observe_otlp`, wire
into `run_read_with`, update `print_usage` per AC9; (4) add `[[test]]`
entry to `Cargo.toml`; (5) update every in-tree `read()` caller to
pass `TimeRange::all()` as 5th arg, locked subprocess tests need no
edit; (6) `cargo test --package kaleidoscope-cli` GREEN; (7) mutation
scope `lib.rs` + `main.rs`, 100% kill (ADR-0005 Gate 5).
