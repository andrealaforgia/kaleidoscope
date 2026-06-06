# DISCUSS Decisions: perf-kpi-ci-non-gating-v0

## Key Decisions

- [D1] **Feature type: Infrastructure / CI-hygiene.** Changes the CI
  workflow and docs only. No `crates/*/src/` production source. The 28
  wall-clock KPI tests are NOT modified — they already carry the ADR-0058
  early-return guard; this feature changes only *whether the env var is
  set*. (orchestrator decision; verified against ci.yml and the test
  bodies)
- [D2] **Walking skeleton: No** (brownfield CI). US-01 + US-03 is the
  thinnest end-to-end slice (de-gate Gate 1 + prove correctness still
  gates), demonstrable in a single CI run. (see story-map.md)
- [D3] **UX research depth: Lightweight.** The persona is the maintainer
  reading a GitHub Actions result. Happy path dominates; the journey is a
  CI-result-interpretation flow. (orchestrator decision)
- [D4] **JTBD: the trustworthy-green-build job.** "When CI runs on a
  shared, variable runner, a RED build must mean a correctness regression,
  not a slow-disk minute. I want the wall-clock perf KPIs to stay visible
  as a tracked signal, but a breach on noisy CI hardware must NOT fail the
  build, because a build that flakes red on hardware variance trains the
  team to ignore red — which is the real danger." No DIVERGE artifacts
  exist; the job is unambiguous and the inventory is empirically grounded.
  (orchestrator decision)
- [D5] **Carpaccio: two slices.** R1 = de-gate Gate 1 (urgent fix, US-01 +
  US-03); R2 = add the non-gating perf job + honesty note (visibility,
  US-02 + US-04). Both pass the carpaccio taste tests (vertical,
  demonstrable, independently valuable, thin). (see story-map.md Scope
  Assessment)
- [D6] **This feature corrects ADR-0058.** ADR-0058 (Accepted,
  2026-05-31) made the wall-clock KPIs CI-GATING (Gate 1 sets
  `KALEIDOSCOPE_PERF_TESTS=1`). This feature reverses the *gating* decision
  (perf becomes non-gating) while PRESERVING ADR-0058's guard mechanism
  and its no-threshold-chasing stance. The new ADR (DESIGN deliverable)
  must SUPERSEDE ADR-0058's CI-gating clause and cite the durable-fsync
  reason ADR-0058 did not consider. (Luna decision, grounded in the
  ADR-0058 text)
- [D7] **No threshold change at any wave.** The fix is location + gating
  semantics, never budget values (memory
  `project_p95_wallclock_flakes_overnight`). (constraint, hard)

## Requirements Summary

The maintainer needs `gate-1-test` to be green iff the correctness suite
passes, so a red build is trustworthy. The mechanism: remove
`KALEIDOSCOPE_PERF_TESTS=1` from the `gate-1-test` job (the 28 perf tests
then self-skip via the existing ADR-0058 guard); add a separate
non-gating `perf-kpis` job that sets the variable, runs the family, and
reports p95 numbers; document the durable-op budgets as dev-indicative,
not CI-contractual.

## Constraints Established (mirror of user-stories.md System Constraints)

- C1 Durability not weakened (fsync stays; durable budgets reflect
  durable cost).
- C2 Correctness gating not loosened (Gate 1 still hard-gates the
  non-perf suite; US-03 negative control).
- C3 No threshold chasing (no literal/sample/percentile change).
- C4 Visibility preserved (de-gating != deleting; numbers still reported).
- C5 Whole family (28 tests, 11 crates), not just `place`.
- C6 Local hook already correct (`scripts/hooks/pre-commit` does NOT set
  the variable — verified line 92-93; perf tests already self-skip
  locally). No local de-gating needed.
- C7 Trunk-based posture (CI is feedback, not a hard merge block; memory
  `project_kaleidoscope_pure_trunk_based`).
- C8 No crate version impact; never 1.0.0.
- C9 British English; em dashes only as structural separators.

## Verified Facts (auditable, re-confirmed this wave)

1. **`gate-1-test` sets the env at job level.**
   `.github/workflows/ci.yml:141` — `KALEIDOSCOPE_PERF_TESTS: "1"` in the
   `gate-1-test` job-level `env` block; the gating invocation is `cargo
   test --workspace --all-targets --locked` at line 184.
2. **`place` is durable and the test asserts 200 us p95.**
   `crates/cinder/tests/v1_slice_01_wal_durability.rs:255` —
   `place_p95_latency_under_two_hundred_microseconds`; self-skips unless
   `KALEIDOSCOPE_PERF_TESTS` set (line 256); measures 1000 `place()`
   calls, asserts `samples[950]` p95 <= 200 us (line 282-286). `place`
   does a per-record fsync (orchestrator cite: `file_backed.rs:285`
   append_wal -> :433 `fsync_backend.fsync_file`, ADR-0049 / ADR-0060).
3. **The family is 28 tests across 11 crates** (NOT ~20 — the
   orchestrator's estimate is superseded by the audited inventory from
   `perf-kpi-ci-gating-v0`, which my own grep reproduces). Full table
   below.
4. **The local pre-commit hook does NOT set the variable.**
   `scripts/hooks/pre-commit:92-93` runs `cargo test --workspace
   --all-targets --locked` with no `KALEIDOSCOPE_PERF_TESTS`. So perf
   tests already self-skip locally; the local flake the orchestrator
   worried about is already handled by ADR-0058. No local change needed.
5. **A directly contradictory ADR exists.**
   `docs/product/architecture/adr-0058-perf-kpi-ci-gating.md` (Accepted,
   2026-05-31) decides the KPIs ARE CI-gating. This feature must supersede
   its gating clause (D6, F2).

## Complete Inventory of Wall-Clock KPI Tests in Scope (28 tests, 11 crates)

Re-confirmed from the audited `perf-kpi-ci-gating-v0` DISCUSS inventory
and reproduced by grep this wave. Each is a `#[test] fn` that measures
wall-clock time with `std::time::Instant` and asserts a p95 threshold,
guarded by the ADR-0058 `KALEIDOSCOPE_PERF_TESTS` preamble.

| # | Crate | Test file | Test fn | Threshold | Durable (fsync)? |
|---|-------|-----------|---------|-----------|------------------|
| 1 | lumen | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_three_milliseconds | 3 ms | YES (WAL) |
| 2 | lumen | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms | partial |
| 3 | lumen | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | no (read) |
| 4 | lumen | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | no |
| 5 | pulse | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | no (read) |
| 6 | pulse | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms | partial |
| 7 | pulse | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_fifty_milliseconds | 50 ms | YES (WAL) |
| 8 | pulse | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | no |
| 9 | ray | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms | partial |
| 10 | ray | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_five_milliseconds | 5 ms | YES (WAL) |
| 11 | ray | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | no (read) |
| 12 | ray | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | no |
| 13 | strata | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_five_milliseconds | 5 ms | partial |
| 14 | strata | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_eight_milliseconds | 8 ms | YES (WAL) |
| 15 | strata | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | no (read) |
| 16 | strata | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | no |
| 17 | cinder | tests/slice_01_walking_skeleton.rs | get_tier_p95_latency_under_fifty_microseconds | 50 us | no (in-mem read) |
| 18 | cinder | tests/v1_slice_01_wal_durability.rs | place_p95_latency_under_two_hundred_microseconds | 200 us | YES (per-record fsync) |
| 19 | cinder | tests/slice_02_lifecycle.rs | evaluate_p95_latency_under_five_milliseconds | 5 ms | partial |
| 20 | cinder | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | no |
| 21 | sluice | tests/slice_01_walking_skeleton.rs | enqueue_and_dequeue_p95_under_fifty_microseconds | 50 us | no (in-mem) |
| 22 | sluice | tests/v1_slice_01_wal_durability.rs | enqueue_p95_latency_under_three_hundred_microseconds | 300 us | YES (WAL fsync) |
| 23 | sluice | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_hundred_milliseconds | 500 ms | no |
| 24 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | persist_p95_latency_under_two_milliseconds | 2 ms | YES (durable put) |
| 25 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | recovery_p95_latency_under_one_and_a_half_seconds | 1.5 s | no |
| 26 | augur | tests/slice_01_zscore.rs | observe_p95_latency_under_ten_microseconds | 10 us | no (in-mem) |
| 27 | augur | tests/slice_02_rare_event.rs | observe_p95_latency_under_twenty_microseconds | 20 us | no (in-mem) |
| 28 | aegis | tests/slice_01_validate.rs | validate_p95_latency_under_two_milliseconds | 2 ms | no (compute) |

Durable column is Luna's working classification for US-04's honesty note
(which ops the dev-indicative caveat most applies to). DESIGN should
confirm the fsync-bearing set; the in-memory ops (`get_tier`, `observe`,
`enqueue_and_dequeue` in-mem path) keep meaningful budgets even on CI.

## Flags to DESIGN (Morgan, solution-architect) and DEVOPS (platform-architect)

1. **F1 — Exact CI restructure.** Remove the `env:
   KALEIDOSCOPE_PERF_TESTS: "1"` block from `gate-1-test`
   (`.github/workflows/ci.yml:140-141`). Add a non-gating perf job that
   sets the variable and runs the family. DEVOPS owns: new job in
   `ci.yml` vs separate workflow file; `continue-on-error: true` vs
   `|| true`-style reporting; run-on-every-push vs scheduled. Backs
   US-01 + US-02.
2. **F2 — Supersede ADR-0058.** Author a new ADR that SUPERSEDES the
   CI-gating decision of ADR-0058 while preserving its guard preamble and
   no-threshold-chasing stance. The new ADR must record the durable-fsync
   reason ADR-0058 omitted (place/enqueue/WAL-ingest became durable via
   ADR-0049 / ADR-0060). Recommended: YES, mandatory — leaving ADR-0058
   "Accepted" while contradicting it in `ci.yml` would be an honesty
   violation. Backs US-04.
3. **F3 — Assert-but-swallow vs report-only.** A real choice for the
   non-gating job: (a) keep the tests asserting and let the job's
   `continue-on-error` swallow a failure, OR (b) a report-only mode that
   LOGS the p95 and never panics (so the number always prints even on a
   breach). Recommended: report-only or at minimum ensure the p95 prints
   before any assert, so C4 (visibility on breach) holds. DESIGN/DEVOPS
   decide. Backs US-02.
4. **F4 — Honesty-note placement.** ADR-only, or ADR + a referenced
   comment at the durable-op test sites? Recommended: ADR is the citable
   home; a one-line reference comment at `place` / `enqueue` / WAL-ingest
   sites is optional polish. Backs US-04.
5. **F5 — Scope is the whole family (28 tests, 11 crates).** The
   non-gating job runs the entire family, not just `place`; fixing only
   `place` would leave the family flaky if perf were ever re-gated.
   Confirmed scope, not a question. Backs US-02 / C5.
6. **F6 — Local hook: no change needed.** `scripts/hooks/pre-commit`
   already does not set the variable (verified). The orchestrator's
   question (5) is answered: no local de-gating applies. DESIGN should
   confirm and NOT add the variable to the hook.

## Upstream Changes

- No DISCOVER / DIVERGE artifacts for this feature. The most relevant
  upstream is the prior `perf-kpi-ci-gating-v0` feature and ADR-0058,
  which this feature corrects (D6, F2).

## Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **No DIVERGE grounding** (no `recommendation.md` / `job-analysis.md`) | High | Low | Acceptable for a narrow, well-understood CI-hygiene change; the job is unambiguous and facts are empirically grounded. Noted per the workflow gate. |
| **Silent re-gating drift** — a future feature re-adds `KALEIDOSCOPE_PERF_TESTS=1` to a gating job | Medium | High | The new ADR (F2) makes non-gating the citable standard; US-04 records why. |
| **Visibility lost** — de-gating degenerates into the perf numbers vanishing | Medium | Medium | US-02 + C4 mandate the non-gating reporting job; F3 ensures the number prints even on breach. |
| **Correctness gating accidentally loosened** | Low | Critical | US-03 negative control + C2; DISTILL must demonstrate Gate 1 still reds on a real break. |
| **Inventory drift** — a perf test added between DISCUSS and DELIVER is missed by the perf job | Low | Low | The 28-test inventory is reproduced here; the non-gating job runs the family by env-var presence, so any guarded test is automatically included. |
