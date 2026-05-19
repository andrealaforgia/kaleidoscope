# Wave Decisions — `cli-read-observe-otlp-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), DISTILL wave, 2026-05-19.

Mode: TRANSLATE. DISCUSS UAT Scenarios + DESIGN DD3 fix the test shape:
three `#[test]` fns in one new file
`crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` mirroring
`observe_otlp_flag.rs` (commit `3af7e82`) and
`observe_otlp_cinder_wiring.rs` (prior wave). The RED gate is the
compile failure: tests pass 4 args to `kaleidoscope_cli::read`; the
shipped signature takes 3.

---

## DWD-01: Rust integration test idiom — `#[test]` fns, not Gherkin

**Decision**: Three Rust `#[test]` functions with `// Given / // When
/ // Then` comment blocks inline. No `.feature` files, no `cucumber-rs`,
no proc-macro BDD framework.

**Rationale**: (1) Project idiom is locked by DISCUSS US-01 § System
Constraints and by both precedent test files. (2) `cargo test` is the
runner — the new test runs under existing Gate 1 at zero workflow-edit
cost. (3) G/W/T comments preserve BDD traceability for review without
the framework cost. Test names encode the user goal
(`read_with_observe_otlp_emits_one_lumen_query_count_line`).

---

## DWD-02: Real-File substrate via `temp_dir()` — match prior wave

**Decision**: `std::env::temp_dir() + format!("kal-cli-otlp-read-{name}-{pid}-{nanos}")`
per-test root + best-effort `fs::remove_dir_all` cleanup. No `tempfile`
crate; no in-memory `Vec<u8>` substrate for the OTLP file.

**Rationale**: (1) Mirrors `observe_otlp_flag.rs:54-68` and
`observe_otlp_cinder_wiring.rs:89-103` exactly. (2) DESIGN DD1's
`OpenOptions::create(true).append(true).open(path)` contract is
meaningless against an in-memory substrate; the `O_APPEND` kernel
semantics behind OK3 only manifest against a real `File`. (3) Unique-
per-test naming isolates parallel `cargo test` runs; best-effort
cleanup leaves failure-path artefacts for inspection. (4) Walking-
skeleton substrate tier is Strategy C: real Lumen `FileBackedLogStore`
+ real `LumenToOtlpJsonWriter` + real `File`. No InMemory adapter on
the critical path.

---

## DWD-03: Scenario coverage table — Test ↔ OK# ↔ Slice AC

| # | Test fn | OK# | UAT Scenario | Tag |
|---|---------|-----|--------------|-----|
| 1 | `read_with_observe_otlp_emits_one_lumen_query_count_line` | OK1 (principal) | "Read with `--observe-otlp` emits one `lumen.query.count` line" | `@walking_skeleton @driving_port @US-01` |
| 2 | `read_without_observe_otlp_creates_no_file_and_preserves_stdout` | OK2 (guardrail) | "Read without `--observe-otlp` creates no file and preserves existing behaviour" | `@guardrail @driving_port @US-01` |
| 3 | `ingest_then_read_share_one_observe_otlp_file_in_one_session` | OK3 (leading) | "Ingest then read in one session share one `--observe-otlp` file" | `@cross_subcommand @driving_port @US-01` |

Test names are verbatim from the slice's AC bullets (`slice-01-…md`).
Tag column is documentary (Rust `#[test]` has no tag system); the file's
module doc-comment carries the same traceability.

UAT Scenario #5 ("Existing tests continue to pass byte-equivalently")
is enforced by CI running the full suite per commit, not by any new
test in this wave. The two prior test files remain unmodified per
DESIGN DD5 item 5 and the task's hard constraint.

**Mandate 1 (driving-port boundary)**: All three tests enter through
`kaleidoscope_cli::ingest` and `kaleidoscope_cli::read` only. Zero
imports of `FileBackedLogStore`, `LumenToOtlpJsonWriter`, etc., on the
production path under test. `LogRecord` / `SeverityNumber` are imported
from `lumen` for fixture construction (the precedent posture); they
are the wire-format input the operator's `ingest` reader consumes, not
internal surface.

**Mandate 2 (business-language)**: Test names are user-goal-framed.
G/W/T comments speak in operator terms ("Priya invokes…"). OTLP-JSON
field references (`scopeMetrics[0].metrics[0].name`) are operator-
visible wire surface — Priya's sidecar matches on exactly these
field paths — not implementation leak.

---

## DWD-04: No concurrency probe — single writer only

**Decision**: Test #3 (OK3) is sequential. No multi-threaded probe
analogous to
`cross_writer_ndjson_validity_under_concurrent_emissions` from the
prior wave.

**Rationale**: (1) Only one writer participates in `read` —
`LumenToOtlpJsonWriter` and only that (DESIGN DD1, DD5 item 2). No
cross-writer contention to probe. (2) Within-process footprint is one
thread: `read()` calls `lumen.query(...)` exactly once
(`lib.rs:258-260`), producing exactly one OTLP-JSON line. No
within-process concurrency to defend against. (3) Cross-`ingest`/
`read` append safety is a POSIX `O_APPEND` kernel guarantee, probed at
the writer level in the prior wave; re-probing at this surface
duplicates signal without new information. (4) DISCUSS § Domain
Examples #3 and `outcome-kpis.md` § Cross-feature alignment both
explicitly bound OK3 to sequential shell-session semantics.

No OK6-equivalent KPI in this feature — there is no cross-writer
surface to defend.

---

## DWD-05: Out-of-scope confirmations

DISTILL produces exactly TWO artefacts:

1. `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` (three
   `#[test]` fns per DWD-03).
2. This `distill/wave-decisions.md`.

Out of scope for DISTILL:

1. **No production source change.** The `read()` signature extension
   (DESIGN DD3) is the DELIVER crafter's edit. Tests pass 4 args; the
   shipped signature takes 3; the compile failure IS the RED gate.
   `src/lib.rs` and `src/main.rs` are not edited in this wave.
2. **No `Cargo.toml` edit.** The new `[[test]]` block is the DELIVER
   crafter's edit per the DEVOPS handoff annotation.
3. **No test-harness extraction.** Rule-of-three trigger arrives at
   N=3 test files; extraction to `tests/common.rs` is deferred to a
   follow-up refactor per DESIGN DD4 last row + DISCUSS D6.
4. **No edit to prior test files.** `observe_otlp_flag.rs` and
   `observe_otlp_cinder_wiring.rs` remain byte-identical (locked).
5. **No Cinder events from `read`.** `read()` constructs no Cinder
   store (DISCUSS D2 + DESIGN DD5 item 2). Test #3 asserts Cinder
   lines produced by the PRIOR `ingest()` call, not by `read()`.
6. **No multi-process probe.** DISCUSS D4 + DESIGN DD5 item 3. OK3
   runs `ingest()` then `read()` sequentially in one test process.
7. **No new dependency.** Test imports `aegis`, `kaleidoscope_cli`,
   `lumen`, `serde_json` — all already (dev-)deps. No `Cargo.lock`
   churn.
8. **No new ADR.** DESIGN DD5 item 7 already established no ADR-0039
   §9 extension is warranted.
