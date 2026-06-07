# Definition of Ready Validation — speed-up-local-precommit-v0

9-item hard gate. Each item must PASS with evidence.

## US-01: The local commit gate finishes in five minutes or less

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1 Problem statement clear, domain language | PASS | "Every `git commit` runs `cargo test --workspace --all-targets --locked`, 10-20 min, breaks flow, pressures `--no-verify`." Verified against hook lines 92-93. |
| 2 User/persona with specific characteristics | PASS | Devon, committing maintainer (human or crafter agent), short edit-commit loops, wants main green without losing flow. |
| 3 3+ domain examples with real data | PASS | codex pure-fn fix; lumen `wal.rs` durability edit; the >5-min boundary case — all with real crate paths. |
| 4 UAT in Given/When/Then (3-7) | PASS | 3 scenarios (fast code gates quickly; durability crate still quick; slow suites not run). |
| 5 AC derived from UAT | PASS | `the-local-hook-finishes-under-5-minutes` + 2 supporting AC, each traceable to a scenario. |
| 6 Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; hook edit + measurement; ~1 day. |
| 7 Technical notes: constraints/dependencies | PASS | DESIGN D1 (subset) and D2 (clippy scope) flagged with measurement requirement. |
| 8 Dependencies resolved or tracked | PASS | Depends on US-03 (deep gate already in CI — verified, exists today). |
| 9 Outcome KPIs with measurable targets | PASS | KPI 1: p95 <= 5 min from 10-20 min baseline, measured by hook timing. |

### DoR Status: PASSED

## US-02: The fast subset still catches the cheap, common mistakes

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1 Problem statement clear, domain language | PASS | "A fast hook is worthless if it stops catching everyday unit/fmt/clippy breaks." |
| 2 User/persona with specific characteristics | PASS | Devon, relies on the hook to stop obvious breakage before main. |
| 3 3+ domain examples with real data | PASS | sieve unit off-by-one; pulse fmt drift; ray redundant-clone clippy — real crate paths. |
| 4 UAT in Given/When/Then (3-7) | PASS | 3 scenarios (unit break; fmt drift; clippy lint). |
| 5 AC derived from UAT | PASS | `the-fast-subset-still-catches-unit-fmt-clippy-failures` + fmt/clippy/deny unchanged. |
| 6 Right-sized | PASS | 3 scenarios; ships with US-01 (same slice). |
| 7 Technical notes | PASS | Subset MUST include unit tests (rules out a zero-test subset). |
| 8 Dependencies tracked | PASS | Same slice as US-01. |
| 9 Outcome KPIs | PASS | KPI 2 (guardrail): 100% of unit/fmt/clippy classes still caught. |

### DoR Status: PASSED

## US-03: The deep suite still runs in CI as the authoritative gate

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1 Problem statement clear, domain language | PASS | "Moving slow tests off local is only safe if they still gate somewhere — CI." |
| 2 User/persona with specific characteristics | PASS | Devon, trading local wait for CI coverage, wants proof coverage did not vanish. |
| 3 3+ domain examples with real data | PASS | clean push green in CI; lumen durability exercised only in CI; pulse torn-tail deep-only red — real test paths. |
| 4 UAT in Given/When/Then (3-7) | PASS | 3 scenarios (deep suite runs in CI; deep CI gate unchanged; deep-only regression caught by CI not the fast hook — negative control). |
| 5 AC derived from UAT | PASS | `the-deep-suite-still-runs-in-CI` + no-deletion AC. |
| 6 Right-sized | PASS | Verification story; no-change assertion + diff check; < 1 day. |
| 7 Technical notes | PASS | Constrains feature NOT to touch ci.yml gate-1; reviewer/DESIGN diff check enforces. |
| 8 Dependencies tracked | PASS | None; CI gate-1 exists today (ci.yml:182, verified). |
| 9 Outcome KPIs | PASS | KPI 3 (guardrail): 100% deep suite in CI, 0 deletions. |

### DoR Status: PASSED

## US-04: A CI-results-watching cadence is established

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1 Problem statement clear, domain language | PASS | "Once deep tests leave the local block, nobody is forced to wait — a deep-only regression could sit on main unnoticed." |
| 2 User/persona with specific characteristics | PASS | Devon (human or agent), pushes to main, must keep it green, wants deep-only regressions in minutes/hours not days. |
| 3 3+ domain examples with real data | PASS | post-push green via `ci-watch.sh`; agent polls, sees in_progress; deep-only torn-tail failure surfaced — concrete. |
| 4 UAT in Given/When/Then (3-7) | PASS | 3 scenarios (status reported; deep-only failure surfaced; cadence + honesty documented). |
| 5 AC derived from UAT | PASS | `a-CI-results-watching-cadence-is-established` + honesty-trade-documented. |
| 6 Right-sized | PASS | 3 scenarios; small `gh` script + docs; ~1 day. |
| 7 Technical notes | PASS | Mechanism+cadence DESIGN D3; honesty trade D5 (ADR home). |
| 8 Dependencies tracked | PASS | Logically follows US-01 (the mitigation for moving deep tests off local); shippable as Slice 2. |
| 9 Outcome KPIs | PASS | KPI 4: deep-only regression detected within 1 cadence interval. |

### DoR Status: PASSED

## Overall DoR: PASSED

All 4 stories pass the 9-item gate (each has 3 Given/When/Then scenarios).
Anti-pattern scan: no Implement-X
(stories start from Devon's pain), no generic data (real crate/test
paths throughout), no technical AC (AC are observable: "hook completes in
<= 5 min", "commit is not created", "prints latest main run status"), no
oversized story (max 3 scenarios each), examples present and concrete.
Solution-neutral: the exact subset, clippy scope, and watch mechanism are
deferred to DESIGN (D1-D6) rather than prescribed.
