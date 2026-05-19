# Wave Decisions — `cli-stats-cinder-tier-distribution-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), DISTILL wave, 2026-05-19.

DESIGN locks the function shape (`stats_with_tiers`), iteration
strategy, construction site, and Option B selective emission. DISCUSS
locks the keys, order, and no-flag contract. DISTILL turns the four
KPIs and five UAT scenarios into five `#[test]` functions in
`crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`.

---

## DWD-01: Rust idiom — `#[test]` with G/W/T comments, not Gherkin

**Decision**: Mirror the locked predecessor's file shape. Each
scenario is one `#[test]` function with contiguous `// Given …`,
`// When …`, `// Then …` comment blocks. No `cucumber-rs`, no
`.feature` files.

**Rationale**: The repo's acceptance idiom is established by six
prior test files in this crate alone (`stats_subcommand.rs`, four
`observe_otlp_*.rs`, `ingest_and_read_roundtrip.rs`). Introducing
a Gherkin runner for one file breaks DESIGN DD5's "no new external
dependency" posture and CLAUDE.md's Rust-idiomatic constraint.

---

## DWD-02: Real-File substrate + direct Cinder seeding

**Decision**: Each test creates a per-test tmp data_dir (PID + nanos
suffix for parallel-test isolation), seeds Lumen and Cinder per the
matrix below, calls `stats_with_tiers(&tenant, &data, &mut buf)`,
asserts on captured stdout bytes, and cleans up. Cinder is seeded by
direct `TieringStore::place` calls on a `FileBackedTieringStore`
opened against the same `cinder_base(data_dir)` the SUT will reopen.

| Test | Lumen seed | Cinder seed |
|---|---|---|
| #1 multi-tier | `ingest` (7 records → 1 Hot side-effect) | `seed_cinder(acme, 4, 12, 47)` → final 5/12/47 |
| #2 hot-only | `ingest` (3 records → 1 Hot side-effect) | `seed_cinder(acme, 2, 0, 0)` → final 3/0/0 |
| #3 orphan | none | `seed_cinder(acme, 2, 0, 1)` only |
| #4 OK4 compat | `FileBackedLogStore::open + LogStore::ingest` (4 records, bypasses `ingest()`) | none → 0/0/0 |
| #5 isolation | `ingest` (2 records → 1 Hot) | `seed_cinder(acme, 4, 0, 0)` + `seed_cinder(globex, 9, 0, 0)` |

**Why direct seeding, not `ingest()`**: `kaleidoscope_cli::ingest()`
places one Hot Cinder item per *batch* (lib.rs:243-244). For tests
#1/#2/#5 the count would be emergent from `(records, batch_size)`
rather than from intent; for Test #4 the automatic Hot placement is
exactly what prevents the empty-Cinder state the OK4 invariant
requires. Test #4 therefore drops to the Lumen API directly to seed
records without touching Cinder.

**Why Real-File and not InMemory**: `FileBackedTieringStore` is the
adapter the SUT reopens. Seeding via `InMemoryTieringStore` leaves
the on-disk Cinder snapshot empty — the SUT would see zero
placements regardless of what the test "seeded".

---

## DWD-03: Scenario coverage (Test ↔ AC ↔ OK#)

| Test | UAT scenario | AC | OK# | WS? |
|---|---|---|---|---|
| #1 multi_tier_emits_six_lines | populated multi-tier | AC1, AC6, AC8 | OK1 (principal) | yes — operator decides "migrations are happening" |
| #2 hot_only_omits_warm_and_cold | populated Lumen, hot-only Cinder | AC2, AC6 | OK1 + Option B | no — boundary |
| #3 empty_lumen_with_populated_cinder | orphan tier metadata | AC3 | OK3 | yes — operator detects orphan tier metadata |
| #4 populated_lumen_zero_cinder_byte_equivalent | OK4 backwards-compat | AC4, AC9 | OK4 (guardrail) | no — contract |
| #5 acme_does_not_count_globex | tenant isolation | AC7 | OK2 (leading) | no — boundary |

**Error path ratio note**: `stats_with_tiers` either returns
`Ok(count)` or propagates `LumenOpen` / `LumenQuery` / `CinderOpen` /
`Io` (all four already covered by the predecessor test cluster and
`ingest_and_read_roundtrip` — DESIGN DD5 confirms no new error
variant). Adding error-path scenarios here would duplicate adapter-
level coverage; the 40% target is deliberately relaxed for this
feature.

**Walking-skeleton count**: 2 of 5. Tests #1 and #3 trace full
operator journeys end-to-end against real `FileBackedTieringStore`
and are demo-able to a non-technical stakeholder.

---

## DWD-04: Cinder line ordering — `hot` → `warm` → `cold`

**Decision**: Test #1 asserts the exact six-line order (`records`,
`earliest`, `latest`, `hot`, `warm`, `cold`); other tests assert
order indirectly via line presence/absence plus the index of `hot=`.
Test #1 is the sole byte-exact oracle — any future refactor that
reorders DESIGN DD2's array or rewrites the loop via `next_forward()`
fails Test #1 on the line-index mismatch.

---

## DWD-05: Out-of-scope (DISCUSS/DESIGN-locked)

1. No `--observe-otlp` flag interaction (no flag — DISCUSS D6).
2. No `migrate()` / `evaluate_at()` test (read-only invariant
   asserted in Test #1 via post-call `cinder_count` re-reads).
3. No per-item dump (DISCUSS D4).
4. No JSON / CSV / `--format=…` (DISCUSS D5).
5. No `--cinder-only` / `--no-lumen` (DISCUSS D6).
6. No `main.rs::run_stats` repoint test (`cli_binary_smoke.rs`
   covers end-to-end process exec).
7. No new error-variant tests (DESIGN DD5 — none introduced).
8. No modification of `tests/stats_subcommand.rs` (DISCUSS D10
   hard contract; DESIGN DD6 #8 confirms). That file remains the
   byte-level oracle for OK4 zero-Cinder cases and runs unmodified.

---

## Handoff to DELIVER

Recipient: `@nw-software-crafter` (Crafty). Receives this
wave-decisions.md plus the new test file (five RED `#[test]`s — the
file will not compile because `kaleidoscope_cli::stats_with_tiers`
does not yet exist).

**RED → GREEN sequencing**: add `stats_with_tiers` stub matching DD1
signature (compile passes, runtime fails); implement body per
DD2/DD3/DD4; run Test #4 first (OK4 byte-equivalence guardrail —
likeliest regression surface); run Tests #1/#2/#3/#5; confirm
`tests/stats_subcommand.rs` still GREEN (OK4 oracle); repoint
`main.rs::run_stats`; confirm `cli_binary_smoke.rs` still GREEN;
mutation-test under existing `gate-5-mutants-kaleidoscope-cli` (the
function body falls inside `--in-diff`). 100% kill per ADR-0005.

**Cargo.toml note**: the new file is auto-discovered under Cargo's
default `autotests = true` — no `[[test]]` block required. DELIVER
may add one for explicitness if matching the other six tests' style.
