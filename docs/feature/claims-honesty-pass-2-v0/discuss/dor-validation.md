# Definition of Ready Validation — claims-honesty-pass-2-v0

Hard gate: each of the 9 DoR items must PASS with evidence for every story before
handoff. This is a documentation-honesty feature; acceptance is guard-style
(false claim absent + true claim present + cross-read matches the cited code).

## Story: US-01 — pulse docs stop lying about volatility and columnar

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | Devin reads `pulse/src/lib.rs:46` "restart loses points" but pulse ships durable `FileBackedMetricStore`; and `lib.rs:20-21,41`/`Cargo.toml:7` promise a columnar Arrow/Parquet/DataFusion/TSDB adapter that is actually JSON+WAL. Stated in reader/honesty domain language. |
| 2. User/persona with specific characteristics | PASS | Devin Okafor, senior platform engineer at Northwind Logistics, evaluating a Datadog replacement; reads crate doc + Cargo.toml then opens `file_backed.rs` and the v1 slice tests. |
| 3. 3+ domain examples with real data | PASS | 3 examples citing real files/symbols: corrected durability posture (vs `file_backed.rs:75-82` + `v1_slice_01`/`v1_slice_06`); scoped volatility (`InMemoryMetricStore` vs `store.rs`/`file_backed.rs`); grep for columnar (vs the dep list). |
| 4. UAT in Given/When/Then (3-7) | PASS | 4 scenarios (durable-survives-restart; no-columnar-overclaim; Cargo.toml names durable adapter; behaviour-unweakened). |
| 5. AC derived from UAT | PASS | 4 AC, each tagged to its scenario; guard-style (false absent / true present / matches `file_backed.rs`). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | One crate, 2 files, 4 scenarios, ~0.3 day. |
| 7. Technical notes: constraints/dependencies | PASS | Doc + Cargo metadata only; Gate 2/3 untouched; mutation N/A; do-not-build columnar; do-not-weaken durable store. |
| 8. Dependencies resolved or tracked | PASS | None; pulse delivered. No cross-slice dependency. |
| 9. Outcome KPIs with measurable targets | PASS | KPI-1: residual pulse inverted/over-claims = 0 (baseline 2); grep/doc-lint guard cross-read against code. |

### DoR Status: PASSED

## Story: US-02 — gateway comments and test prose match the code

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | `main.rs:62-63` "RED-ready NO-OP" but `init_tracing:153-173` installs a real subscriber; `main.rs:118-120`/`:24-25` "force `sink.kind = stub`" but `:121` is `Config::builder().build()` (relies on default); test prose says "RED against the no-op subscriber" but the suite is GREEN. Contributor-honesty domain language. |
| 2. User/persona with specific characteristics | PASS | Devin in contributor mode, reading the gateway source to understand/extend it; trusts the comment over the code beneath it. |
| 3. 3+ domain examples with real data | PASS | 3 examples citing real lines: corrected init_tracing comment (vs `:153-173`); corrected stub comment (vs `:121`); running `cargo test -p kaleidoscope-gateway slice_01` after the corrected test note. |
| 4. UAT in Given/When/Then (3-7) | PASS | 4 scenarios (init_tracing comment; config-default comment; test-module prose; touched-only-stale-prose). |
| 5. AC derived from UAT | PASS | 4 AC, each tagged; guard-style + "matches the code beneath it" + "`#[ignore]` attrs unchanged". |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | One binary, 2 files, 4 scenarios, ~0.3 day. |
| 7. Technical notes: constraints/dependencies | PASS | Doc/comment only; no `#[ignore]` change; no behaviour change; mutation N/A; Gate 2/3 untouched; guardrail: confirm always-run scenarios GREEN before editing prose. |
| 8. Dependencies resolved or tracked | PASS | None; gateway delivered. |
| 9. Outcome KPIs with measurable targets | PASS | KPI-2: residual stale/inaccurate gateway comment loci = 0 (baseline 3 + test prose); grep guard cross-read against `init_tracing`/`Config::builder().build()`; suite green. |

### DoR Status: PASSED

## Story: US-03 — platform Prism claims + prism config match the single-metric reality

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | README `:184` "Unified query and visualisation frontend"/"Grafana" but `apps/prism/README.md:3-6` says "a single PromQL query panel"; cost line `:222` claims "compliance dashboards in Prism" that do not exist; `playwright.config.ts:19` advertises a "Gate 7 … browser matrix" e2e that `testMatch:[...__no-spec-matches-yet__...]` (`:50`) makes vacuous. Evaluator/honesty domain language. |
| 2. User/persona with specific characteristics | PASS | Devin reading the README table first, then spot-checking the module README + CI config; over-trusts Prism as Grafana-class and believes a green e2e gate exists. |
| 3. 3+ domain examples with real data | PASS | 3 examples citing real loci: corrected Prism row (vs `apps/prism/README.md:3-6`); cost line + e2e config; grep `apps/prism/e2e/*.spec.ts` for `UNIMPLEMENTED` (scaffold not touched). |
| 4. UAT in Given/When/Then (3-7) | PASS | 4 scenarios (README row; cost line; e2e advertisement; in-flight scaffolds untouched). |
| 5. AC derived from UAT | PASS | 4 AC, each tagged; guard-style + cross-read against `apps/prism/README.md` and the `testMatch` reality. |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | README + 1 config + module README, 4 scenarios, ~0.4 day. Carries the remove-vs-mark DESIGN flag but the flag does not enlarge DISCUSS scope. |
| 7. Technical notes: constraints/dependencies | PASS | README + config + prism README only; no e2e built; do-not-touch per-spec scaffolds; mutation N/A; DESIGN flag (MARK vs REMOVE) recorded. |
| 8. Dependencies resolved or tracked | PASS | None; aligns to the already-honest `apps/prism/README.md`. |
| 9. Outcome KPIs with measurable targets | PASS | KPI-3: residual prism overstatement loci = 0 (baseline 3); grep/doc-lint guard cross-read against `apps/prism/README.md` + `testMatch`. |

### DoR Status: PASSED

## Feature-level gate

| Check | Status | Evidence |
|-------|--------|----------|
| All stories trace to the JTBD (N:1) | PASS | All three trace to the Earned-Trust honesty job (`wave-decisions.md`, `user-stories.md` header). |
| Every overstatement verified against live code | PASS | The `wave-decisions.md` inventory cites the exact code line that makes each corrected claim true (verified 2026-06-07). |
| Stale/already-fixed findings excluded | PASS | The cli durability MED (now accurate, fsync-durable), query-api `step` (already honest), and the pass-v0-corrected rows are explicitly excluded with evidence. |
| In-flight markers protected | PASS | Guardrails in each story + the risk register protect the crash-durability tests, the gateway `#[ignore]`d AC-01 scenarios, and the prism per-spec `UNIMPLEMENTED` scaffolds. |
| No behaviour change / no false caveat | PASS | Guardrail KPIs; correct-the-claim-only constraint enforced. |
| Scope right-sized | PASS | 3 stories / 3 loci / ~1 day (`story-map.md` Scope Assessment). |

### Feature DoR Status: PASSED — ready for peer review

## Peer review

| Field | Value |
|-------|-------|
| Reviewer | nw-product-owner-reviewer (review mode) |
| Result | **APPROVED** (iteration 1) |
| Iterations | 1 of max 2 |
| Critical / High / Medium issues | 0 / 0 / 0 |
| Low (advisory, no revision required) | 4 |

### Review summary

- **Dimension 0 (Elevator Pitch, BLOCKING)**: PASS for all three stories —
  Before/After/Decision present; the reader-facing surface (rendered rustdoc /
  README / source comments / CI config) is the legitimate entry point for a
  documentation-honesty feature (consistent with the approved `claims-honesty-pass-v0`
  precedent); each "After" describes observable corrected prose; each "Decision
  enabled" names a real adoption/composition decision. No slice is all-infrastructure.
- **Confirmation bias**: none — corrections are solution-neutral; the
  out-of-scope "implement" paths (pulse columnar, prism e2e) are excluded, not
  prescribed. Sad-path coverage present (error/boundary example + behaviour-
  unweakened + in-flight-untouched scenario per story).
- **Completeness**: complete for DISCUSS; no DESIGN handoff per brief; the
  prism-e2e remove-vs-mark DESIGN flag recorded.
- **Clarity/measurability**: every corrected claim cites the exact file:line and
  the canonical honest source it aligns to; KPIs are zero-residual counts with a
  guard method.
- **Testability**: guard-style AC (false absent / true present / cross-read
  matches code) is the correct shape for prose honesty; the two behaviour
  guardrails (suite green, in-flight markers intact) are runtime-verifiable.
- **Priority**: Q1 YES, Q2 ADEQUATE, Q3 CORRECT, Q4 JUSTIFIED → PASS.

Verdict: **approved**. No revision required.

> Handoff to DESIGN is NOT performed by this wave (brief: "Do NOT proceed into
> DESIGN"). The prism-e2e remove-vs-mark decision is carried as a DESIGN flag.
