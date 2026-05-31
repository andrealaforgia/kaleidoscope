# DISTILL Decisions: perf-kpi-ci-gating-v0

Author: `nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-31.
British English; no em dashes in body text.

This feature is atypical for DISTILL. It is NOT a new endpoint with a fresh
RED scaffold. It is a MECHANICAL modification to 28 existing wall-clock KPI
tests (adding one byte-identical guard line as the first body statement) plus
one job-level `env` block in `.github/workflows/ci.yml`. There is no new
production module, no driving port, no driven adapter, no I/O surface. The
DESIGN brief (HEAD `bbded96`) pins the guard text verbatim and the exact
28-site inventory; DISTILL adds the executable acceptance verification that
Crafty runs in DELIVER.

Reconciliation result: 0 contradictions. DESIGN (DD1 to DD6) confirms every
DISCUSS recommendation (inline guard, presence-based contract, early-return,
job-level literal env, ADR-0058). No upstream contradiction blocks scenario
writing.

## DISTILL Decisions

### D1 Verification strategy: behavioural acceptance commands, not a mechanism unit test (RECOMMENDED B)

Two options were weighed.

- **Option A (mechanism unit test).** Write a small Rust test that, with the
  variable absent, asserts skip behaviour, and with the variable present,
  asserts run behaviour. REJECTED. `std::env` is process-global. Cargo runs
  tests in parallel threads within one process, so setting or unsetting
  `KALEIDOSCOPE_PERF_TESTS` inside one test races every other test in the same
  binary, including the 28 real perf tests. To make Option A sound you would
  have to serialise the whole binary, fork a child process per case, or pull in
  a serial-test dependency. That machinery costs more than the four-line guard
  it would test, and it tests `std::env::var` (a `std` primitive) rather than
  the feature's actual observable outcome.

- **Option B (behavioural acceptance commands).** RECOMMENDED and CHOSEN. The
  observable outcome is verified by running cargo two ways and inspecting the
  result. No new test code is written, so the env-global fragility never
  arises. The two commands are the acceptance checks (D3). Crafty runs them by
  hand in DELIVER to confirm GREEN.

Rationale for B over A: the feature's value is "the hook stops flaking and CI
still enforces". That is a property of WHERE the variable is set (absent in the
hook, present in CI), observable end to end by running cargo. A parallel unit
test cannot model a process-global variable without serialising the suite, and
would verify the `std` primitive rather than the feature. Behavioural
verification is both sounder and cheaper here.

### D2 Mandate 7 (RED-ready scaffolding): adapted, declared N/A in its literal form

Mandate 7 requires that new acceptance tests importing not-yet-built production
modules ship with RED scaffold stubs so the test is RED (assertion failure),
not BROKEN (import error). That mechanism is N/A here because there is no new
production module to scaffold: the guard is a four-line preamble inside
existing test bodies, and `std::env::var` already exists.

The RED/GREEN intent is preserved behaviourally, not structurally:

- **RED (pre-DELIVER, the broken state the feature fixes).** Today the 28 perf
  tests run unconditionally. In the local pre-commit hook under parallel-build
  load they FLAKE: `lumen ingest_p95` is observed at 4 to 6 ms against a 3 ms
  threshold, forcing `git commit --no-verify`. That non-deterministic local
  failure IS the red state. It is not a missing-implementation red; it is a
  wrong-environment red.

- **GREEN (post-DELIVER).** With the guard added, a plain local
  `cargo test --workspace` (variable absent) skips all 28 tests with a stderr
  note and exits 0 deterministically. CI sets `KALEIDOSCOPE_PERF_TESTS="1"`, so
  the same 28 tests run their full measurement and enforce unchanged thresholds.

So the conventional RED-to-GREEN of "stub raises, then implement" is replaced by
"hook flakes under load, then hook is deterministic; CI still enforces". The
self-review checklist items 6 to 9 (scaffold markers, AssertionError bodies,
RED-not-BROKEN classification) are therefore N/A and recorded as such in
test-scenarios.md.

### D3 The two acceptance commands

- **Skip without variable.** `cargo test --workspace` with no
  `KALEIDOSCOPE_PERF_TESTS` set. Each of the 28 perf tests prints
  `perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run` to stderr, reports
  `ok`, takes no measurement; the run exits 0.

- **Run with variable.** `KALEIDOSCOPE_PERF_TESTS=1 cargo test -p lumen --test
  v1_slice_01_wal_durability` runs `ingest_p95_latency_under_three_milliseconds`
  through its full 1000-sample measurement and asserts `p95 <= 3_000` exactly
  as before, with no skip note.

These two commands are AC-01 and AC-02 in test-scenarios.md. They are the
executable specification; no `.feature` file or step definitions are produced
because there is no behaviour to drive through a port.

### D4 Walking Skeleton strategy: N/A (no adapter, no I/O port)

The WS strategy decision tree (Strategy A to D) presupposes a feature with at
least a domain or a driven port. This feature has neither a new production
component nor any I/O adapter to exercise; the guard is a test-body preamble.
There is no walking skeleton scenario, no `@walking_skeleton` tag, no
`@real-io` adapter-integration scenario, and no adapter coverage table. US-01
(the single confirmed flaker, lumen ingest_p95, run both ways) plays the role
of the thin end-to-end demonstration; US-04 is the fan-out of the identical
edit across all 28 sites. Mandate 6 (real-I/O per driven adapter) is N/A: zero
new driven adapters.

## US to AC mapping

| Story | Intent | Acceptance check |
|-------|--------|------------------|
| US-01 Local hook skips perf tests | absent variable means skip and pass | AC-01 (skip without variable) |
| US-01 / US-02 Opt-in runs the measurement | present variable means run and enforce | AC-02 (run with variable) |
| US-02 CI enforces thresholds | gate-1-test sets the variable job-level | AC-05 (ci.yml env set) plus AC-02 (run-and-enforce semantics) |
| US-03 Thresholds unchanged | only the guard preamble is added | AC-03 (diff shows guard only, no threshold literal touched) |
| US-04 Complete uniform coverage | all 28 tests gated, no straggler | AC-04 (grep: 11 files, 28 guard occurrences) |
| US-05 Pattern documented | ADR-0058 records policy and mechanism | satisfied by ADR-0058 (already written in DESIGN); no DISTILL test, documentation-only story |

US-05 is documentation-only and optional (it does not block US-01 to US-04);
ADR-0058 already exists at `docs/product/architecture/adr-0058-perf-kpi-ci-gating.md`,
so no acceptance check is added for it.

## The 28 guarded tests

Authoritative checklist for Crafty, cross-checked against
`design/application-architecture.md` Test inventory. Each is a plain
`#[test] fn`; the guard is the FIRST statement of the body. Thresholds are
UNCHANGED.

| # | Crate | Test file | Test fn | Threshold |
|---|-------|-----------|---------|-----------|
| 1 | lumen | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_three_milliseconds | 3 ms (CONFIRMED FLAKER) |
| 2 | lumen | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms |
| 3 | lumen | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms |
| 4 | lumen | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 5 | pulse | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms (CONFIRMED FLAKER) |
| 6 | pulse | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms |
| 7 | pulse | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_fifty_milliseconds | 50 ms |
| 8 | pulse | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 9 | ray | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms |
| 10 | ray | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_five_milliseconds | 5 ms |
| 11 | ray | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms |
| 12 | ray | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 13 | strata | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_five_milliseconds | 5 ms |
| 14 | strata | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_eight_milliseconds | 8 ms |
| 15 | strata | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms |
| 16 | strata | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 17 | cinder | tests/slice_01_walking_skeleton.rs | get_tier_p95_latency_under_fifty_microseconds | 50 us |
| 18 | cinder | tests/v1_slice_01_wal_durability.rs | place_p95_latency_under_two_hundred_microseconds | 200 us |
| 19 | cinder | tests/slice_02_lifecycle.rs | evaluate_p95_latency_under_five_milliseconds | 5 ms |
| 20 | cinder | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 21 | sluice | tests/slice_01_walking_skeleton.rs | enqueue_and_dequeue_p95_under_fifty_microseconds | 50 us |
| 22 | sluice | tests/v1_slice_01_wal_durability.rs | enqueue_p95_latency_under_three_hundred_microseconds | 300 us |
| 23 | sluice | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_hundred_milliseconds | 500 ms |
| 24 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | persist_p95_latency_under_two_milliseconds | 2 ms |
| 25 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | recovery_p95_latency_under_one_and_a_half_seconds | 1.5 s |
| 26 | augur | tests/slice_01_zscore.rs | observe_p95_latency_under_ten_microseconds | 10 us |
| 27 | augur | tests/slice_02_rare_event.rs | observe_p95_latency_under_twenty_microseconds | 20 us |
| 28 | aegis | tests/slice_01_validate.rs | validate_p95_latency_under_two_milliseconds | 2 ms |

11 crates: lumen, pulse, ray, strata, cinder, sluice, beacon, augur, aegis.
Rows 24 and 25 share one file (beacon
`tests/v1_slice_02_filebacked_durable_recovery.rs`), so 28 tests live in 27
distinct files. A grep for `KALEIDOSCOPE_PERF_TESTS` across the nine crate
`tests` directories must return 28 occurrences in 27 files. Out of scope (NOT
gated): `aperture slice_06_forwarding_sink` (asserts a field is present, not a
threshold) and any `Instant`/`elapsed()` used only for timeout or
wait-until-ready scaffolding.

The guard text (verbatim, byte-identical at all 28 sites, FIRST statement of
each body):

```rust
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
```

## RED/GREEN framing

- **RED (current state, pre-DELIVER).** All 28 perf tests run unconditionally.
  Local `cargo test --workspace` under parallel-build load flakes on the
  wall-clock thresholds (lumen ingest_p95 observed at 4 to 6 ms versus 3 ms),
  forcing `--no-verify`. This is the broken behaviour the feature exists to fix.

- **GREEN (post-DELIVER).** Local plain `cargo test --workspace` skips all 28
  with a stderr note and exits 0 deterministically (AC-01). CI sets
  `KALEIDOSCOPE_PERF_TESTS="1"` (AC-05), so the same 28 run and enforce
  unchanged thresholds (AC-02, AC-03), and a real regression turns gate-1-test
  red. Coverage is complete and uniform (AC-04).

Crafty applies the 28 guard lines plus the ci.yml env block in one atomic
DELIVER commit, then verifies AC-01 through AC-05 manually. No new test code is
committed by DISTILL.
