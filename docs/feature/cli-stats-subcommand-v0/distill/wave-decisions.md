# Wave Decisions — `cli-stats-subcommand-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), DISTILL wave, 2026-05-19.

Mode: TRANSLATE. DISCUSS pinned the slice (US-01, OK1/OK2/OK3) and
the wire-shape contract; DESIGN pinned `stats(tenant, data_dir,
writer) -> Result<usize, Error>` (DD2), `records.first()` /
`records.last()` (DD3), and the hand-rolled nanosecond ISO 8601
formatter (DD1). DISTILL translates the five UAT scenarios into Rust
integration tests under
`crates/kaleidoscope-cli/tests/stats_subcommand.rs`. The file fails
to compile against today's `lib.rs` (`stats` does not exist) — that
is the RED gate Crafty turns green in DELIVER.

---

## DWD-01: Rust integration test idiom (mirror the four precedents)

Flat `#[test]`-function module with `// Given / // When / // Then`
comment blocks. Imports the library API directly
(`kaleidoscope_cli::{ingest, read, stats, DEFAULT_BATCH_SIZE}`) and
drives every assertion through that surface. DISCUSS § System
Constraints fixes the idiom: "Rust `#[test]` functions ... not
Gherkin `.feature` files." Same shape as the four sibling test
files. No HTTP, no subprocess; the binary glue is exercised
separately by `cli_binary_smoke.rs`.

---

## DWD-02: Real-File substrate via `temp_root()` + `cleanup()`

Every test instantiates a fresh real on-disk `temp_root("<name>")`
under `std::env::temp_dir()` and runs the full ingest → stats round-
trip through real `FileBackedLogStore`. No `InMemoryLogStore`; no
fake recorder. The four precedent files do the same. The OK1
consistency-with-`read` invariant (test #5) is byte-for-byte
discharged only through the same adapter `read()` uses. Critique
Dim 9d litmus: deleting the real adapter would break test #5
immediately — the WS strategy is "real local-resource adapter", and
this hexagon's only driven adapter is exercised in every test.

---

## DWD-03: Scenario coverage table (Test ↔ Slice AC ↔ OK#)

| # | Test fn | Slice AC | OK# | Category |
|---|---|---|---|---|
| 1 | `stats_populated_tenant_emits_three_lines_in_order` | same | OK1+OK2 | Happy path |
| 2 | `stats_empty_tenant_emits_records_zero_and_no_timestamps` | same | OK3 | Boundary |
| 3 | `stats_single_record_tenant_emits_identical_earliest_and_latest` | same | OK2 | Edge |
| 4 | `stats_for_acme_does_not_count_globex_records_in_same_data_dir` | same | OK1 | Cross-tenant invariant |
| 5 | `stats_count_matches_read_count_for_same_tenant_and_data_dir` | same | OK1 | Cross-function consistency |

Every slice AC has exactly one test. Every OK# has at least one
failing test if violated (OK1: 1, 4, 5; OK2: 1, 3; OK3: 2). Non-
happy-path share = 4 of 5 (80%), well above the Mandate 1 / Critique
Dim 1 floor of 40%.

---

## DWD-04: Timestamp seeding — round Unix nanos, byte-exact asserted

Seeds are literal `u64` constants picked so DD1's formatter renders
byte-exact ISO 8601 UTC strings asserted as string literals:

| Constant | Nanos | Rendered (always 9 ns digits per DD1) |
|---|---|---|
| `SEED_EARLIEST_NS` | `1_779_062_400_000_000_000` | `2026-05-18T00:00:00.000000000Z` |
| `SEED_LATEST_NS`   | `1_779_148_800_000_000_000` | `2026-05-19T00:00:00.000000000Z` |
| `SEED_SINGLE_NS`   | `1_778_457_600_000_000_000` | `2026-05-11T00:00:00.000000000Z` |

Conversion for 2026-05-18 (commented in the test file for spot-
checkability): days 1970-01-01 → 2026-01-01 = 56*365 + 14 leap =
20_454; days 2026-01-01 → 2026-05-18 = 31+28+31+30+17 = 137; total
20_591; seconds = 20_591 * 86_400 = 1_779_062_400; nanos = secs *
10^9. The seven populated records (tests #1 and #4) sit at
`SEED_EARLIEST_NS + i * 14_400_000_000_000` for i=0..6 (4 h step),
so t_0 = 2026-05-18T00:00:00.000000000Z and t_6 =
2026-05-19T00:00:00.000000000Z and min/max equal the two seed
constants exactly.

Round-second seeds (not sub-second noise) keep the assertions
readable. The formatter's nanosecond branch is still exercised
because the byte-exact `.000000000Z` suffix is part of every
asserted literal — a mutation that elides the `.NNNNNNNNN` block
dies on byte-mismatch. Test #4's `globex` records use
`SEED_SINGLE_NS + i` (2026-05-11 nanos, outside acme's window) so a
union-bug surfaces on both count (10 not 7) AND earliest (collapses
to 2026-05-11).

---

## DWD-05: Out-of-scope confirmation

Confirmed (not re-litigated):

1. **No Cinder** (DISCUSS D2, DESIGN DD5). No Cinder import; no
   `cinder_base`; no `Tier::*`. Setup `ingest()` calls do touch
   Cinder transitively but no assertion looks at Cinder state.
2. **No `--observe-otlp`** (DISCUSS D3, DESIGN DD5). No OTLP path
   constructed; no `parse_observe_otlp` call. Every `ingest()` and
   `read()` setup call passes `None` for `otlp_log_path`. The
   `stats()` signature per DD2 takes no such parameter.
3. **No JSON / CSV / `--format=...`** (DISCUSS D4, DESIGN DD5).
   Every assertion is on plain-text key=value lines split on `\n`.
   `serde_json` appears only in the `ndjson()` helper that prepares
   setup input, never in an assertion shape over `stats()` output.
4. **No filtering / sorting / multi-tenant aggregate** (DISCUSS D7,
   DESIGN DD5). `stats()` calls take (tenant, data_dir, writer)
   only; no `--since`, no `--severity-min=`, no all-tenants form;
   no sort-order assertion beyond what falls out of the
   `LogStore` ascending-`observed_time_unix_nano` invariant.
5. **No production source mutation by DISTILL**. This wave produces
   exactly two files: the new test file and this document. No edit
   to `lib.rs`, `main.rs`, `Cargo.toml`, or any other `tests/*.rs`.
   Crafty adds the `[[test]]` block and the `stats()` function
   atomically in DELIVER.
