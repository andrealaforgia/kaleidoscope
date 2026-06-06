# Wave Decisions — beacon-slo-operator-path-v0 (DESIGN)

British English throughout, no em dashes.

> **Author**: Morgan (`nw-solution-architect`), DESIGN wave, 2026-06-06, PROPOSE mode, autonomous overnight dispatch.
> **Governing ADR**: `docs/product/architecture/adr-0067-beacon-slo-operator-path.md`.
> **Inputs**: DISCUSS artefacts (US-01..US-05, FLAG-1..FLAG-5), ADR-0036 (the engine, corrected here), ADR-0063 (the reload contract, honoured unchanged), and the shipped code (`slo.rs`, `loader.rs`, `beacon-server/src/main.rs`).
> **Paradigm**: Rust idiomatic (data + free functions + traits only where needed), set in CLAUDE.md. Not re-decided.

## The five flagged decisions, resolved

### F1 — The `[[slo]]` TOML schema and the `FileShape` extension

- **Table name `[[slo]]`** (singular, array-of-tables), matching the `[[rules]]` singular-repeated convention and the `Slo` type name. `[[slos]]` rejected.
- **New private `RawSlo` in `loader.rs`**, `#[serde(deny_unknown_fields)]`, beside `RawRule`. Wire keys → `Slo` fields:

  | TOML key | maps to | notes |
  |---|---|---|
  | `service` (required) | `service` | drives synthesised names + `slo_service` label |
  | `good_events_query` (required) | `sli_good_events` | operator-facing name; `sli_` is internal |
  | `total_events_query` (required) | `sli_total_events` | |
  | `target_availability` (required) | `target_availability` | validated `(0,1)`, F3 |
  | `error_budget_period` (default `"30d"`) | `error_budget_period` | humantime string, parsed via existing `parse_duration`; validated `== 30d`, F3 |
  | `sinks` (default `[]`) | `sinks` | **reuses `RawSink` verbatim** + the same `SUPPORTED`/url/topic validation |

- `source_path` is **not** a wire key; the loader fills it from the file path during conversion.
- `FileShape` gains `#[serde(default)] slo: Vec<RawSlo>` beside `rules`. `deny_unknown_fields` **kept** on `FileShape`, `RawSlo`, `RawSink`.
- `BLESSED_FIELDS` **extended** with the five new SLO keys so a near-miss still earns "did you mean".

### F2 — Merge semantics

- **Naming**: the engine's existing `{service}_slo_{page|ticket}_{long}_{short}` (`slo.rs:124-127`), e.g. `checkout_slo_page_1h_5m`. The `_slo_` infix namespaces synthesised rules.
- **Collision policy: REFUSE the load** with a `LoaderDiagnostic` naming the duplicated `name` and the file(s). Any duplicate `name` in the merged catalogue (synthesised-vs-synthesised, synthesised-vs-hand-authored, or two hand-authored) refuses. Precedence/last-wins rejected as a silent drop (US-04 "never a silent shadow").
- **Merge**: per file, `[[rules]]` → rules first, then each `[[slo]]` → `synthesise_slo` → four rules appended; `load_rules` `extend`s across files in the existing sorted-path order. Ordering within the merged `Vec` is deterministic and evaluation-irrelevant (each rule has its own task).
- **Collision scan runs over the whole merged catalogue** (a duplicate-name scan in `load_rules`), because a collision can span two files.
- A file with both `[[rules]]` and `[[slo]]` loads both. The rules-only path is byte-identical (the `slo` vector defaults empty), so `slice_05_slo_burn_rate.rs` and the beacon-server rule tests stay green.

### F3 — Validation rules, exact messages, reload interaction

Validation runs in a new `RawSlo::into_slo(...) -> Result<Slo, String>` (mirrors `RawRule::into_rule`), **before** `synthesise_slo`:

1. **`target_availability` strictly in `(0.0, 1.0)`** — reject `<= 0.0` or `>= 1.0`. Message:
   `invalid target_availability 1.0 (must be strictly greater than 0 and strictly less than 1) in SLO "checkout"`
   (the loader's diagnostic wrapper already prefixes `{file}:`). Kills the degenerate always-fire (`budget = 0`).
2. **`error_budget_period == 30d`** — reject any other. Message:
   `unsupported error_budget_period "7d" (only "30d" is supported at v0) in SLO "checkout"`
   Makes the `slo.rs:49-51` doc claim TRUE.

Each returns `Err(String)` → per-file `LoaderDiagnostic` (report-and-fail-the-file, not a crash).

**Reload interaction (all-or-nothing, ADR-0063)**: a malformed SLO is a `LoaderDiagnostic` like a malformed rule. At startup the file is skipped (and if nothing valid remains, the `has_any_rules()` startup refusal fires). Under SIGHUP, the existing `broken_edit_added_nothing = has_diagnostics() && added_count == 0` guard (`main.rs:343`) refuses the reload, retains the previous catalogue, emits `beacon.reload.refused`. **No new reload code** — a refused SLO surfaces exactly as a refused rule. No degenerate rule ever reaches evaluation.

### F4 — SIGHUP reload carryover and expansion-aware counts

- **Counts are expansion-aware BY CONSTRUCTION — no new code, no new event field.** The existing reload computes `new_names` from `outcome.rules` (which already holds the four synthesised rules per SLO) and `added = new_names.difference(live_names).count()` (`main.rs:338-340, 408`). **One new SLO → `added = 4`**, `rules_loaded` counts the four, removing an SLO → `removed = 4`. This is the honest count (four evaluators started). A dedicated `slos_added` field rejected as redundant (`slo_service` label already groups them).
- **State carryover by stable synthesised name**: ADR-0063 sub-decision 2 matches `RuleState` by `name`; synthesised names are stable and deterministic, so a firing synthesised rule survives an unrelated SLO edit by name, keeps its `Firing` `since`, no re-page (US-05 Domain Example 3). No new carryover code.

### F5 — The missing 24-hour cross-validation test

- **DECISION: DELIVER the test (the honest fix) AND correct the `slo.rs:24-26` doc wording.** The engine is deterministic (no clock/RNG); a synthetic 24-hour trace asserting the firing pattern against a hand-authored reference is bounded and in-tree, no new dependency. ADR-0036 also asserts this test exists, so the honest resolution is to make it true.
- **Specifics handed to DISTILL**: two arms — a sustained above-budget error rate that MUST fire the page rules (1h/5m, 6h/30m); a within-budget rate that MUST NOT fire any rule; asserted against a hand-authored reference firing pattern; byte-equal PromQL snapshotted for determinism. The reference is hand-authored PromQL/expected-firing, NOT `.cue` (ADR-0036's `.cue` reference is corrected — the catalogue language is TOML).

## Reuse Analysis (RCA hard gate)

| Existing machinery | Path | Decision |
|---|---|---|
| `synthesise_slo` | `slo.rs:106-156` | REUSE verbatim |
| `MWMBR_TABLE` | `slo.rs:64-93` | REUSE verbatim |
| `Slo` struct | `slo.rs:37-57` | REUSE (conversion target) |
| `load_rules` + `LoadOutcome` + sorted-path determinism | `loader.rs:111-132` | EXTEND (second pass over `[[slo]]`) |
| `FileShape` | `loader.rs:260-265` | EXTEND (`#[serde(default)] slo`) |
| `RawSink` + sink validation | `loader.rs:311-323, 329-361` | REUSE verbatim |
| `RawRule::into_rule` pattern | `loader.rs:325-372` | MIRROR for `RawSlo::into_slo` |
| `BLESSED_FIELDS` + Levenshtein | `loader.rs:199-229` | EXTEND (five SLO keys) |
| `parse_duration` (humantime) | `loader.rs:375-379` | REUSE for `error_budget_period` |
| beacon-server reload orchestrator | `main.rs:280-440` | REUSE verbatim (no reload change) |
| ADR-0063 all-or-nothing + name-keyed carryover | ADR-0063 | HONOUR unchanged |
| **`RawSlo` + `into_slo` + duplicate-name scan** | NEW in `loader.rs` | CREATE (minimal: one wire struct, one conversion, one scan) |

**Net new surface**: one private `RawSlo` + `into_slo`; one defaulted `FileShape` field; five `BLESSED_FIELDS` entries; one duplicate-name scan. No new engine/reload logic, no new public Rust API, no new dependency.

## Constraints honoured

- beacon-server is operator-critical: validation (F3) is load-bearing; no degenerate always-fire rule ever reaches evaluation.
- The merge must not regress the hand-authored rule path: `slice_05_slo_burn_rate.rs` + beacon-server rule tests stay green (rules-only path byte-identical).
- ADR-0063 all-or-nothing reload honoured: malformed SLO refused, previous catalogue kept, no partial apply.
- Inherits ADR-0005's five gates; per-feature mutation 100% on the modified `loader.rs` / `slo.rs` lines.
- Rust idiomatic (CLAUDE.md). NEVER bump any crate to 1.0.0.
- The beacon flush-only-not-fsync durability caveat is OUT OF SCOPE and not pulled in.

## Upstream changes handed to DELIVER

1. **`slo.rs:49-51` doc comment**: reword to match the shipped rejection ("rejected by the loader's SLO validation"), making the claim true (F3).
2. **`slo.rs:24-26` doc comment**: keep the cross-validation claim and DELIVER the test that backs it (F5).
3. **ADR-0036 correction note**: ADR-0036 is immutable; DELIVER appends a "Corrected by ADR-0067" note recording the three reconciliations:
   - FOUR rules per SLO (not "five"); the `MWMBR_TABLE` has four rows.
   - No `annotations` field on the synthesised `Rule`; correlation is the `slo_source` **label**.
   - Validation is the Rust TOML loader (ADR-0067 F3), not a CUE schema; the catalogue language is TOML, not CUE; reference fixtures are PromQL/expected-firing, not `.cue`.

## Public-API and semver posture

- Loader + beacon-server changes are **additive** (`FileShape` gains a defaulted field; `RawSlo`/`into_slo` private; public `Rule`/`LoadOutcome`/`load_rules`/`synthesise_slo` unchanged).
- Beacon is **not** enrolled in the Gate 2/3 public-API surface tracking (only otlp-conformance-harness/spark/sieve/codex are); no public-api gate fires.
- Semver: **additive minor, or none**; no consumer breaks. Pre-1.0; **NEVER 1.0.0** (Andrea's call).

## Test seam (for DISTILL)

Real TOML rule files in a temp dir + the real `load_rules` + the real `beacon-server` reload. **DISTILL reuses the `beacon-sighup-reload-v0` harness**: start `beacon-server` with a `--rules` temp dir + a backend stub, edit the dir, send SIGHUP, observe `tracing` events and synthesised-rule firing. Black-box against the real names (`checkout_slo_page_1h_5m`, etc.) and the `beacon.reload.succeeded`/`refused` events; no reach into private `into_slo`/`synthesise_row`.

## Note on the DISCUSS illustrative names

The DISCUSS user stories illustrate synthesised names without the `_slo_` infix (`checkout_page_1h_5m`). The **shipped code is the authority**: the real format is `{service}_slo_{page|ticket}_{long}_{short}` (`slo.rs:124-127`), e.g. `checkout_slo_page_1h_5m`. DISTILL writes assertions against the real names.

## Peer review (nw-solution-architect-reviewer, iteration 1 — APPROVED)

nWave order honoured: DESIGN runs before DEVOPS/DISTILL/DELIVER, so the absence of production code/tests/CI is expected, not a rejection reason. The review judged design soundness, the F1-F5 resolutions, the merge/validation/reload correctness, the ADR-0036 reconciliation, the Reuse completeness, and ADR quality.

**Verdict: APPROVED. Critical issues: 0. High issues: 0.**

Strengths cited: every mechanism claim grounded in verified `file:line`; the F4 expansion-aware-count is a genuine architectural insight (added=4 falls out by construction, zero new code); the F3 reload reuses the existing `broken_edit_added_nothing` guard verbatim so all-or-nothing is honoured with no new reload code; the ADR-0036 reconciliation is source-verified (FOUR rules; no `annotations` field, confirmed `types.rs:62-88`; TOML not CUE) and respects ADR immutability via an appended correction note; the DISCUSS-vs-shipped `_slo_` name discrepancy is caught and the shipped code declared authority.

Priority validation: Q1 largest-bottleneck = YES (reachability 0%->100% of the headline engine); Q2 simpler-alternatives = ADEQUATE (6 alternatives, the simpler-looking ones rejected with reasons, chosen design is the simplest); Q3 constraint-prioritization = CORRECT (operator-critical no-degenerate-rule drives F3; durability caveat excluded not over-solved); Q4 data-justified = JUSTIFIED (mechanism claims cite shipped `file:line`).

Two low/optional notes; the substantive one (the ADR-0063 35-rule scaling envelope) folded into ADR-0067 F4 ("Scaling" paragraph). No iteration 2 required.

Self-application (Earned Trust): the review verified the F4 count claim against `main.rs:338-340,408` and the no-`annotations` claim against `types.rs:62-88` rather than trusting the ADR's prose — the probe was run, not assumed.

## Changelog

- 2026-06-06: DESIGN wave authored. Resolved F1-F5; produced ADR-0067 with the Reuse Analysis, the ADR-0036 reconciliation, the collision-refuse policy, the validation messages, the expansion-aware-count finding, and the F5 deliver-the-test call. Extended `brief.md` with the feature's Application Architecture section + the C4 sequence diagram + the For-Acceptance-Designer and DEVOPS-handoff notes.
