# Wave Decisions — `cli-cinder-otlp-wiring-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), DISTILL wave, 2026-05-19.

Mode: PROPOSE — DISCUSS locked the behaviour contract; DESIGN locked
DD1 (`File::try_clone`); DEVOPS locked the KPI-to-test mapping.
DISTILL translates all of that into Rust `#[test]` functions in
`crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs`, RED at
landing (won't link until DELIVER flips the Cinder match arm + adds
the `[[test]]` block).

---

## DWD-01: Rust `#[test]` idiom with `// Given / // When / // Then` blocks

**Decision**: Acceptance tests are `#[test]` functions structured with
`// Given / // When / // Then` comment blocks. No Gherkin. No pytest-bdd.

**Rationale**: Locked by CLAUDE.md + DISCUSS user-stories.md §System
Constraints. Precedents: `tests/observe_otlp_flag.rs`,
`tests/ingest_and_read_roundtrip.rs`. Mandate 2 (business language) is
satisfied by keeping the comment-block text in operator-domain language
(ingest, batch flush, place, tier, sink) while the test body legitimately
holds technical machinery (`Cursor`, `OpenOptions`, `try_clone`).

---

## DWD-02: Real-`File` substrate via `std::env::temp_dir()` (not in-memory)

**Decision**: Tests #1, #2, #3 use a real on-disk file under a unique-
per-test directory in `std::env::temp_dir()`, opened with
`OpenOptions::new().create(true).append(true).open(path)`. The
`temp_root` / `cleanup` helpers are copied verbatim from
`tests/observe_otlp_flag.rs:54-68` (rule-of-three deferral — DISCUSS D4).

**Rationale**: OK6 (the principal KPI) is a property of **OS-level
`O_APPEND` atomicity** under DESIGN DD1's `File::try_clone` mechanism.
An in-memory `SharedBuf` substrate has no `O_APPEND` semantics and
**cannot falsify** OK6 — using one here would be Testing Theater
(passes the test, breaks the KPI). ADR-0039 §7 specifically assigned
the cross-writer-against-real-`File` probe to **this** feature because
the library tests use `SharedBuf` and could not discharge it. Mandate 4
(pure-function extraction) is degenerate here: the wiring is a one-line
match-arm change with zero business logic to extract; the substrate IS
the contract.

---

## DWD-03: Scenario coverage table — Test ↔ Slice AC ↔ OK#

| `#[test]` fn | Slice 01 AC | OK# | What is asserted |
|---|---|---|---|
| `ingest_with_observe_otlp_emits_cinder_place_and_lumen_ingest_lines_per_batch` | AC1 + AC2 | OK7 (+ OK8-shape at new surface) | 6 records / batch_size 3 → exactly 2 `cinder.place.count` lines (tenant=acme, scope=kaleidoscope.cinder, asInt="1", tier="hot") AND exactly 2 `lumen.ingest.count` lines (asInt="3"). Set-containment, not order. |
| `cross_writer_ndjson_validity_under_concurrent_emissions` | AC3 | **OK6 (principal)** | 2 threads × 100 emissions over `File::try_clone`'d handles, with `(i*7)%6` ms and `(i*7+3)%6` ms jitter (offset for de-sync). After join: trailing `\n`, no empty lines, 200 non-empty lines, each parses as `serde_json::Value`, 100 of each metric. |
| `cross_writer_ndjson_validity_under_sequential_alternation` | AC3 sibling | OK6 (cheaper probe) | 5 alternating emissions per writer on one thread → trailing `\n`, all parse, 5 of each metric. If this ever breaks, the concurrent test cannot pass — bisects "writers compose" from "writers compose under jitter". |
| `no_observe_otlp_means_no_file_is_created_even_after_cinder_wiring` | AC4 | OK7 negative + OK8 (no regression on `None` arm) | `ingest(..., None)` produces no file at any path. Re-asserted from the new test surface to prove the wiring's `None` arm preserves existing behaviour. |
| `cinder_writer_over_real_file_is_send_and_sync` | — (subtype-check layer, ADR-0039 §1 + Principle 12c) | OK6 prerequisite | Compile-time `assert_send_sync::<CinderToOtlpJsonWriter<std::fs::File>>()` + Lumen sibling. Catches trait-bound loss at compile time, not runtime. |

**Coverage**: 5 tests; 4 of 5 slice ACs (AC1, AC2, AC3, AC4) covered
directly. AC5 ("existing `observe_otlp_flag.rs` tests pass byte-
equivalently") is covered by **not editing that file** — it is the OK8
byte-equivalence probe by construction.

**Error-path ratio**: 1 of 5 (20%) — **below the 40% mandate target**.
Shortfall accepted: this is a wiring change with a degenerate error
surface. The only other realistic failure mode is `try_clone()` EMFILE/
ENFILE, which DESIGN DD3 documented as untestable without kernel
mocking or a fault-injection seam (both exceed slice scope and violate
`#![forbid(unsafe_code)]` / DISCUSS D5+D6). The OK6 concurrent test
itself wires THREE independent failure modes into one scenario:
per-line JSON parse failure, missing trailing newline, off-by-one
line counts — each diagnostic of a distinct interleaving bug.

---

## DWD-04: Concurrency model — `std::thread::spawn` + deterministic jitter, direct writer invocation

**Decision**: Test #2 spawns two `std::thread`s, drives the writers
**directly** (not through `ingest`), and applies deterministic jitter:
`(i*7)%6` ms on the Lumen thread, `(i*7+3)%6` ms on the Cinder thread
(offset for de-sync).

**Why deterministic jitter, not `fastrand`**: `fastrand` is NOT a
workspace dependency (verified `/Cargo.toml` lines 48-57:
`workspace.dependencies` declares only opentelemetry-proto, prost,
sha2, serde, serde_json). Per task brief + `feedback_decide_dont_ask`:
deterministic posture wins — no new dep, reproducible CI failures (a
`fastrand` flake is opaque; `(i*7)%6` produces the same interleaving
on every run), still surfaces interleaving via the offset between
threads. ADR-0039 §7 item 3 says "concurrent random pause" — the spec
wants scheduling variation, not non-determinism.

**Why drive writers directly (not through `ingest`)**: DISCUSS D4
explicitly anticipates this. Three reasons: (1) spawning two `ingest`
calls would need two data dirs + two reader streams, conflating "what
the CLI does in production" with "what the writers do under
concurrency" — the KPI is the latter; (2) the cross-writer guarantee
is a property of writers sharing one file, not of two `ingest` calls
— isolating the test scope to the writers gives the sharpest failure-
mode signal; (3) test #1 already covers the wiring path (ingest →
file), so this test does not need to re-cover it.

**Why 100 emissions per thread**: matches US-01 Scenario 2 exactly.
Large enough for ~16 full periods of the 6-iteration jitter cycle,
small enough to keep runtime < 1s on cold CI.

**Why a sequential sibling test #3**: cheaper probe — if it breaks,
test #2 cannot pass, so the failure mode is bisected before the more
expensive scenario runs. Different signal: test #3 fails when writer
composition is fundamentally broken (wrong file mode, missing newline
in emit triple); test #2 additionally fails under scheduling jitter.

---

## DWD-05: Out-of-scope confirmations

DISTILL confirms (does not re-litigate):

1. **No multi-process scenarios** (DISCUSS D7, ADR-0039 §7). All tests
   run inside one process. POSIX `O_APPEND` would extend DD1's design
   to multi-process transparently if a future feature lifted the
   deferral.
2. **No `read` subcommand wiring** (DISCUSS D5).
3. **No `cinder.migrate` / `cinder.evaluate` events** (DISCUSS D1) —
   the CLI ingest loop only invokes `cinder.place(...)` at
   `lib.rs:228`. Library-level coverage of the unexercised methods
   lives in `crates/self-observe/tests/cinder_to_otlp_json.rs`.
4. **No production source edits in DISTILL**. DELIVER adds the
   `[[test]]` block to `crates/kaleidoscope-cli/Cargo.toml` AND flips
   the Cinder construction at `lib.rs:163`. The new test file MUST
   NOT compile today — link-time failure of the new `[[test]]` is the
   RED gate.
5. **No edit to `tests/observe_otlp_flag.rs`** (OK8 locked).
6. **No new dev-dependency**. `serde_json`, `aegis`, `cinder`,
   `lumen`, `self-observe` are already crate deps. `fastrand`
   deliberately NOT added (DWD-04).
7. **No fault-injection seam for `try_clone()` failure** — exceeds
   slice scope; DEVOPS A3 mutation-testing on `lib.rs` is the second-
   best probe for the error-propagation correctness.

---

## Handoff

Next wave: DELIVER (`nw-software-crafter`).

Crafter's steps:

1. Add `[[test]] name = "observe_otlp_cinder_wiring", path = "tests/observe_otlp_cinder_wiring.rs"` to `crates/kaleidoscope-cli/Cargo.toml` (per slice brief + DEVOPS A1).
2. Flip the Cinder construction at `crates/kaleidoscope-cli/src/lib.rs:163` to a parallel `match otlp_log_path { Some(_) => CinderToOtlpJsonWriter::new(file_clone), None => CinderRecorder }`. Per DD1: bind `let file = OpenOptions::...?;` then `let file_clone = file.try_clone()?;`, pass `file` to Lumen and `file_clone` to Cinder.
3. `cargo test --package kaleidoscope-cli --test observe_otlp_cinder_wiring` → five tests pass.
4. `cargo test --package kaleidoscope-cli --test observe_otlp_flag` → three tests still pass (OK8).
5. `cargo clippy --workspace --all-targets` → clean.
6. Mutation-testing per DEVOPS A3 (Gate 5).
