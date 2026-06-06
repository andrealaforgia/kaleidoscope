# ADR-0067 — Beacon SLO operator path: the `[[slo]]` rule-file table, validate-then-synthesise-then-merge, all-or-nothing reload

**Status**: Accepted
**Date**: 2026-06-06
**Author**: Morgan (autonomous DESIGN dispatch, `nw-solution-architect`)
**Governs**: `beacon-slo-operator-path-v0`
**Relates to**: ADR-0036 (beacon SLO MWMBR synthesis — the engine; this ADR **corrects** three inconsistencies in it, see "Reconciliation of ADR-0036" below), ADR-0063 (beacon SIGHUP reload atomic swap and state carryover — the reload contract honoured here, unmodified), ADR-0034 (reload semantics — the governing reload contract, unmodified), ADR-0033/0037 (loader / evaluator-orchestrator seams), ADR-0035 (sink trait), ADR-0005 (the five delivery gates).

British English throughout, no em dashes.

## Context

Beacon's SLO multi-window multi-burn-rate (MWMBR) engine is correct and tested but **library-only**: it ships dead. The four-quadrants assessment (Q3) records the gap verbatim — "an operator cannot declare an SLO in a rule file today; the MWMBR engine is correct but unreachable from the product surface." This is the UNWIRED set (Tested But Unwired). This feature wires the correct-but-dead headline feature to the existing operator surface (the TOML rule file plus the running `beacon-server` reloaded via `kill -HUP`, ADR-0063). It is genuinely valuable wiring, not a low-severity nicety.

### The correct-but-dead engine (verified in source)

- `synthesise_slo` (`crates/beacon/src/slo.rs:106-156`) maps a `&Slo` to a `Vec<Rule>` deterministically (no clock, no RNG). It always produces exactly **FOUR** rules, one per `MWMBR_TABLE` row (`slo.rs:64-93`): page `14.4 / 1h / 5m` (critical), page `6.0 / 6h / 30m` (critical), ticket `3.0 / 1d / 2h` (warning), ticket `1.0 / 3d / 6h` (warning). The firing predicate is `error_rate > budget * threshold`, `budget = 1.0 - target_availability` (`slo.rs:114`).
- **The actual synthesised rule name format is `{service}_slo_{page|ticket}_{long}_{short}`** (`slo.rs:124-127`), e.g. `checkout_slo_page_1h_5m`. The DISCUSS user stories illustrate names without the `_slo_` infix (`checkout_page_1h_5m`); **the shipped code is the authority** and the `_slo_` infix stands. DISTILL writes acceptance assertions against the real names.
- The `Slo` struct (`slo.rs:37-57`): `service: String`, `sli_good_events: String`, `sli_total_events: String`, `target_availability: f64`, `error_budget_period: Duration`, `sinks: Vec<SinkConfig>`, `source_path: Option<String>`.
- The only callers of `synthesise_slo` in the whole repo are in `crates/beacon/tests/slice_05_slo_burn_rate.rs` (20/20 green). `beacon-server` has zero SLO references.

### The "poisons-its-file" before state (verified)

`FileShape` (`loader.rs:260-265`) is `#[serde(deny_unknown_fields)]` and contains ONLY `rules: Vec<RawRule>`. A file containing an `[[slo]]` table makes `toml::from_str` fail with "unknown field `slo`"; `parse_file` (`loader.rs:157-168`) returns `Err(diag)`, so the **entire file is skipped** — every `[[rules]]` table in that same file is lost too, surfaced as one `LoaderDiagnostic`. An `[[slo]]` block today is NOT silently ignored and NOT parsed: it poisons its file.

### Two doc-lies in the same area (this feature makes them true / corrects them)

1. **`slo.rs:49-51`** claims non-30d `error_budget_period` values "are rejected by the loader". The loader has no SLO handling, so nothing rejects anything. Wiring the loader's `error_budget_period == 30d` validation makes this claim TRUE (F3 / US-03).
2. **`slo.rs:24-26`** (and `slice_05:27-30`) claim a 24-hour-trace cross-validation test that does NOT exist (deferred to a "slice 05b"). F5 decides the honest fix.
3. **`target_availability = 1.0`** yields `budget = 0`, `limit = 0`, predicate `error_rate > 0` — a degenerate always-fire rule, unguarded (`slo.rs:114`). The moment the operator path exists this is a loaded gun. F3 closes it.

## Decision

**WIRE the engine.** Extend the rule-file loader to accept an `[[slo]]` table, deserialise it into a `RawSlo`, validate it, convert to the existing `Slo`, expand via `synthesise_slo` verbatim, and merge the four synthesised rules into the same `LoadOutcome.rules` catalogue the hand-authored `[[rules]]` populate. The pipeline is **parse → validate → convert → synthesise → merge**, mirroring the existing `RawRule → Rule` path. No engine change; the only new code is the wire shape, its validation/conversion, and the merge.

### F1 — The `[[slo]]` TOML schema and the `FileShape` extension

**Table name: `[[slo]]`** (singular, array-of-tables). Chosen over `[[slos]]` to match the existing `[[rules]]` convention where the singular table name repeats per instance, reads naturally in a file (`[[slo]]` once per declared SLO), and matches the `Slo` type name. A file may contain `[[rules]]` and `[[slo]]` together.

**The wire shape (a new private `RawSlo` in `loader.rs`, beside `RawRule`):**

| TOML key | Type | Maps to `Slo` field | Notes |
|---|---|---|---|
| `service` | string (required) | `service` | Used in the synthesised rule names + `slo_service` label. |
| `good_events_query` | string (required) | `sli_good_events` | PromQL "good events" numerator. Clear operator-facing name; the `sli_` prefix is an internal field name, not exposed on the wire. |
| `total_events_query` | string (required) | `sli_total_events` | PromQL "total events" denominator. |
| `target_availability` | float (required) | `target_availability` | Validated strictly in `(0.0, 1.0)`, see F3. |
| `error_budget_period` | string (default `"30d"`) | `error_budget_period` | A humantime duration string (`"30d"`), parsed via the existing `humantime::parse_duration` path the loader already uses for `for_duration`/`interval`. Validated `== 30d`, see F3. Default `"30d"` so the common case needs no key. |
| `sinks` | array of tables (default `[]`) | `sinks` | **Reuses the existing `RawSink` wire shape verbatim** (`loader.rs:311-323`: `kind`, `url`, `channel`, `topic`, `auth_token_env`) and the same `SUPPORTED` sink-kind + url/topic validation in `RawRule::into_rule`. The SLO's sinks flow to every synthesised rule (the engine clones `slo.sinks` into each rule, `slo.rs:153`). |

`source_path` is **not** a wire field. The loader knows the file path it is parsing (`parse_file`'s `path`) and populates `Slo.source_path` from it during conversion, so each synthesised rule carries `slo_source` for correlation (`slo.rs:135-137`).

**`FileShape` gains a second `#[serde(default)]` vector beside `rules`:**

```rust
#[serde(deny_unknown_fields)]
struct FileShape {
    #[serde(default)]
    rules: Vec<RawRule>,
    #[serde(default)]
    slo: Vec<RawSlo>,   // NEW
}
```

`deny_unknown_fields` is **kept** on `FileShape`, on `RawSlo`, and on `RawSink`. `RawSlo` carries its own `#[serde(deny_unknown_fields)]` so an unknown SLO sub-field (`targt_availability`) still earns a parse error. The `BLESSED_FIELDS` list (`loader.rs:199-213`) that drives the "did you mean" Levenshtein suggestion is **extended** with the new SLO keys (`service`, `good_events_query`, `total_events_query`, `target_availability`, `error_budget_period`) so a near-miss on an SLO key earns the same suggestion a near-miss on a rule key does. (The existing `sinks`, `kind`, `url`, `channel`, `topic`, `auth_token_env` are already blessed and shared.)

### F2 — Merge semantics (synthesised SLO rules coexisting with hand-authored `[[rules]]`)

**Synthesised rule naming** is the engine's existing `{service}_slo_{page|ticket}_{long}_{short}` (`slo.rs:124-127`). The `_slo_` infix already namespaces synthesised rules away from typical hand-authored names; it is the stable identifier used for collision detection and for ADR-0063 state carryover.

**Name-collision policy: REFUSE the load with a clear diagnostic.** A name collision — between two synthesised rules (two SLOs whose `service` collide, or a hand-authored rule named identically to a synthesised one), or any duplicate `name` in the merged catalogue — refuses the file/load with a diagnostic, rather than last-wins precedence or silent shadowing. Refuse is chosen over precedence because:

- It is consistent with the all-or-nothing honesty the whole feature exists to deliver: an operator must never get a silently-shadowed rule (US-04 AC: "never a silent shadow").
- Precedence (last-wins, or synthesised-wins) is a silent drop by another name — the operator loses alerting coverage they believe they have, the exact harm US-04 forbids.
- Namespacing synthesised names harder (e.g. a reserved prefix the operator cannot type) was considered and rejected: it would make the names un-referenceable by hand-authored `inhibits`, and the `_slo_` infix already gives practical separation. Refuse-on-collision is the honest, simplest guard.

**Merge mechanism and ordering.** Within `parse_file`, after the existing `[[rules]]` loop builds the file's rules, a second loop converts each `RawSlo → Slo`, calls `synthesise_slo`, and extends the file's rule vector with the four synthesised rules. `load_rules` then `extend`s `outcome.rules` across files in the existing sorted-path order (`loader.rs:124`). Ordering within the merged `Vec<Rule>` is: per file, hand-authored rules first then synthesised rules, files in sorted path order — deterministic, matching the loader's existing determinism guarantee, and irrelevant to evaluation (each rule is evaluated independently by its own task; ordering affects only diagnostic output). **Collision detection runs over the whole merged catalogue** (a duplicate-name scan), because a collision can span two files (a hand-authored rule in `disk.toml` colliding with a synthesised name from `checkout.toml`). The duplicate scan is the load-level guard; it raises a `LoaderDiagnostic` naming both the duplicated name and the offending file(s).

**A file holding both `[[rules]]` and `[[slo]]`** loads both: its hand-authored rules plus its SLOs' synthesised rules, all into the one catalogue, subject to the duplicate-name scan. The rules-only path is byte-for-byte unchanged (the new `slo` vector defaults to empty), so `slice_05_slo_burn_rate.rs` and the beacon-server rule tests stay green.

### F3 — Validation rules, exact rejection messages, and the reload interaction

Two validations run in the `RawSlo → Slo` conversion (a new `RawSlo::into_slo(&self, source_path) -> Result<Slo, String>`, mirroring `RawRule::into_rule`'s `Result<Rule, String>`), **before** `synthesise_slo` is ever called, so a degenerate rule is never synthesised, never merged, never evaluated:

1. **`target_availability` strictly in `(0.0, 1.0)`.** Reject `target <= 0.0` or `target >= 1.0` (this kills the degenerate always-fire at `1.0`, and the nonsensical budgets at `0.0` / negative at `> 1.0`). Message:

   > `invalid target_availability 1.0 (must be strictly greater than 0 and strictly less than 1) in SLO "checkout"`

   (the literal value and the SLO's `service` are interpolated; the file name is added by the loader's diagnostic wrapper, which already prefixes `{file}:`).

2. **`error_budget_period == 30d`.** Reject any other duration. Message:

   > `unsupported error_budget_period "7d" (only "30d" is supported at v0) in SLO "checkout"`

   This makes the `slo.rs:49-51` doc claim TRUE. **Upstream doc fix (handed to DELIVER):** update the `slo.rs:49-51` comment so its wording matches the shipped rejection path ("rejected by the loader's SLO validation" rather than the bare "rejected by the loader").

Both messages follow the project CLI error pattern (what happened / why / what to do): they name the offending value, state the allowed range/value, and identify the SLO. Each returns `Err(String)` from `into_slo`, which `parse_file` wraps into a per-file `LoaderDiagnostic` (`loader.rs:174-180`) exactly as a malformed rule does — **report-and-fail-the-file**, not a process crash.

**Reload interaction (the all-or-nothing honouring).** A malformed SLO is a `LoaderDiagnostic` for its file:

- **At startup**: the file is skipped (report-and-skip); if no valid rule remains anywhere, the existing `has_any_rules()` startup refusal fires (`main.rs:77-84`), exactly as for a malformed rule file.
- **Under SIGHUP reload (ADR-0063)**: a malformed SLO edit is REFUSED and the previous catalogue is KEPT, via the existing reload guard (`main.rs:343`): `broken_edit_added_nothing = has_diagnostics() && added_count == 0`. A file whose only change is a now-malformed SLO yields a diagnostic and adds no new valid name, so the reload refuses, retains the previous catalogue, and emits `beacon.reload.refused` — **no degenerate always-fire rule ever reaches evaluation, no partial apply** (ADR-0063 all-or-nothing). This requires **no new reload code**: the existing guard already covers it because a refused SLO surfaces as a `LoaderDiagnostic` just like a refused rule.

### F4 — SIGHUP reload carryover and expansion-aware counts

An SLO edit reloads exactly like a rule edit, because the ADR-0063 reload re-reads the dir via the same `load_rules` (`main.rs:301-302`); once the loader synthesises SLOs, a reload picks up SLO edits for free. The refuse-vs-apply rules carry over from ADR-0063 unchanged (F3 above).

**The `added` / `removed` / `rules_loaded` counts are EXPANSION-AWARE by construction — no new code, no SLO-aware event field.** The existing reload computes its counts over the synthesised rule names (`main.rs:338-340, 408-410`): `new_names` is built from `outcome.rules` (which already contains the four synthesised rules per SLO), and `added = new_names.difference(live_names).count()`. Therefore **one newly-added SLO shows `added = 4`** (its four synthesised names are new), `rules_loaded` counts the four, and removing an SLO shows `removed = 4`. This is the correct, honest count — it reflects what actually changed in the live catalogue (four evaluators started). A dedicated "1 SLO added" field is **rejected** as redundant: the operator reads `slo_service` labels on the four rules to see they belong to one SLO, and the four-rule count is the operationally true number of evaluators. The `beacon.reload.succeeded` event is unchanged.

**Scaling.** Each SLO adds four rules (four evaluator tasks); this stays well within ADR-0063's established 35-rule scaling target where task respawn on reload is cheap, so the four-rules-per-SLO expansion needs no new scaling consideration. The performance envelope is inherited from ADR-0063, not re-litigated.

**State carryover by stable synthesised name.** ADR-0063 sub-decision 2 matches in-flight `RuleState` by `name`. Synthesised rules have stable, deterministic names (`{service}_slo_{...}`), so a firing synthesised rule survives an unrelated SLO edit by name and keeps its `Firing` `since` with no re-page — exactly as a hand-authored rule does. Adding a second, unrelated SLO adds its four names and leaves the surviving rule's `since` intact (US-05 Domain Example 3). This requires no new carryover code: the synthesised names flow through the existing name-keyed durable store and resolver carryover.

### F5 — The missing 24-hour cross-validation test

**DECISION: DELIVER the cross-validation test (the honest fix), and ALSO correct the `slo.rs:24-26` doc wording to match what ships.** The test is bounded and the engine is deterministic (no clock, no RNG), so a synthetic 24-hour trace asserting the synthesised firing pattern against a hand-authored reference is a finite, in-tree test with no new dependency. The DISCUSS wave (FLAG-5) left this a DESIGN/DISTILL call; DESIGN's call is **deliver it**, because:

- The doc (`slo.rs:24-26`) and ADR-0036 ("Cross-validation contract", lines 101-117) both assert this test exists; the honest resolution is to make the assertion true, not to delete the headline correctness claim of the SLO engine.
- It is the cheapest place to pin the engine's `budget * threshold` arithmetic against the workbook before the operator path makes the engine reachable.

**Specifics handed to DISTILL** (DISTILL owns the acceptance/test design; DELIVER's crafter writes it): a deterministic synthetic 24-hour metric trace with two arms — (a) a sustained error rate above the budget that MUST fire the page-level rules (1h/5m and 6h/30m), and (b) a sustained error rate within budget that MUST NOT fire any rule — asserted against a hand-authored reference firing pattern, with byte-equal synthesised PromQL snapshotted for determinism (the `@property` "byte-identical across loads" scenario in US-01 already covers determinism; this test adds the firing-correctness arm). **Note the ADR-0036 references to `.cue` reference fixtures are corrected** (see reconciliation): the reference is a hand-authored PromQL/expected-firing reference, the catalogue language is TOML, not CUE.

### Reconciliation of ADR-0036 (correcting its inconsistencies, not propagating them)

ADR-0036 contains three inconsistencies that the shipped code contradicts; ADR-0067 records the truth and ADR-0036 is to be annotated (a superseding correction note, not a silent edit — ADRs are immutable; DELIVER adds a "Corrected by ADR-0067" note to ADR-0036):

1. **"five-rule" vs "four rows".** ADR-0036 says "a five-rule alert set" (line 9) and "five synthesised rules" (line 137), but its own `MWMBR_TABLE` (lines 36-42) and the shipped `MWMBR_TABLE` (`slo.rs:64-93`) have **four rows**, and `synthesise_slo` produces **exactly four rules**. **The truth is FOUR rules per SLO.**
2. **The phantom `annotations` field.** ADR-0036 (lines 84-90) shows the synthesised `Rule` carrying an `annotations` map (`summary`, `source_slo`). The shipped `Rule` type has **no `annotations` field**; the source correlation is carried as a `slo_source` **label** (`slo.rs:135-137`), and there is no `summary`. **The truth is: no `annotations` field; correlation via the `slo_source` label.**
3. **The non-existent CUE-schema validation.** ADR-0036 (lines 119-133, 150-152) says "Beacon's catalogue language is CUE" and "the CUE schema validates `error_budget_period == "30d"`". The shipped catalogue language is **TOML** (`loader.rs`; ADR-0033/0034 record the TOML fallback), and there is **no CUE schema** anywhere. **The truth is: validation is the Rust loader (this ADR's F3), not a CUE schema; the rule-file language is TOML.**

## Public-API and semver posture

- **The loader and beacon-server changes are additive.** `FileShape` gains a defaulted `slo` vector (rules-only files parse byte-identically); `RawSlo` / `RawSlo::into_slo` are new but private to the loader (`pub`-insulated, like `RawRule`); the public `Rule`, `LoadOutcome`, `load_rules`, and `synthesise_slo` surfaces are **unchanged**. No breaking change.
- **No new public library surface** beyond what is already `pub` (`synthesise_slo` and `Slo` already exist and are reused). The wire shape is private to `loader.rs`.
- **Beacon is not enrolled in the Gate 2/3 public-API surface tracking** (only otlp-conformance-harness, spark, sieve, codex are, per the recent findings). No public-api gate fires for this change.
- **Semver: additive minor, or none.** The change adds a TOML capability without altering any public Rust signature; a minor bump of the `beacon` crate is defensible if the team versions on wire-format capability, but no API consumer breaks. Pre-1.0; **NEVER 1.0.0** (CLAUDE.md; semver 1.0.0 is Andrea's call).

## Test seam (for DISTILL)

Real TOML rule files in a temp dir + the real `load_rules` + the real `beacon-server` reload. The SIGHUP reload path is already exercised by `beacon-sighup-reload-v0`'s harness; **DISTILL reuses that harness** (start `beacon-server` with a `--rules` temp dir and a backend stub, edit the dir, send SIGHUP, observe the `tracing` events and the synthesised rules' firing behaviour). All assertions are black-box against the real synthesised rule names (`checkout_slo_page_1h_5m`, etc.) and the `beacon.reload.succeeded` / `beacon.reload.refused` events. No reach into private `into_slo` or `synthesise_row`.

## Reuse Analysis (RCA hard gate — extend, do not reinvent)

| Existing machinery | Path | Decision |
|---|---|---|
| `synthesise_slo` (SLO → 4 rules, deterministic) | `crates/beacon/src/slo.rs:106-156` | **REUSE verbatim.** No engine change. The whole feature exists to reach it. |
| `MWMBR_TABLE` (the four workbook rows) | `slo.rs:64-93` | **REUSE verbatim.** |
| `Slo` struct (deserialisation target) | `slo.rs:37-57` | **REUSE** as the domain type `RawSlo` converts into. |
| `load_rules` + `LoadOutcome` + sorted-path determinism | `loader.rs:111-132` | **EXTEND** — the per-file parse loop gains a second pass over `[[slo]]`; the `extend` and sorting are unchanged. |
| `FileShape` | `loader.rs:260-265` | **EXTEND** — add a `#[serde(default)] slo: Vec<RawSlo>` beside `rules`; keep `deny_unknown_fields`. |
| `RawSink` wire shape + sink validation (`SUPPORTED`, url/topic) | `loader.rs:311-323, 329-361` | **REUSE verbatim** for the SLO's `sinks`. |
| `RawRule::into_rule` pattern (`Result<_, String>` → per-file `LoaderDiagnostic`) | `loader.rs:325-372` | **MIRROR** — `RawSlo::into_slo` follows the same shape and error path. |
| `BLESSED_FIELDS` + Levenshtein "did you mean" | `loader.rs:199-229` | **EXTEND** — add the five new SLO keys so a near-miss earns a suggestion. |
| `parse_duration` (humantime) | `loader.rs:375-379` | **REUSE** for `error_budget_period`. |
| beacon-server reload orchestrator (build-new → swap → abort-old; refuse guard; expansion-aware counts) | `crates/beacon-server/src/main.rs:280-440` | **REUSE verbatim** — no reload change; SLO support falls out of the shared `load_rules` re-read and the name-set count. |
| ADR-0063 all-or-nothing reload + name-keyed state carryover | ADR-0063 | **HONOUR unchanged** — synthesised names flow through the existing name-keyed carryover. |
| **`RawSlo` type + `RawSlo::into_slo` + the duplicate-name collision scan** | NEW in `loader.rs` | **CREATE.** Justification: no existing wire shape maps `[[slo]]`; no existing code detects cross-file name collisions (the rules-only path could not collide a synthesised name with a hand-authored one because synthesised rules did not exist in the catalogue). This is the minimal new surface — one private wire struct, one conversion fn mirroring `into_rule`, one duplicate-name scan in `load_rules`. |

**Net new surface:** one private `RawSlo` struct + `into_slo` conversion (mirrors `RawRule`/`into_rule`); one defaulted field on `FileShape`; five new entries in `BLESSED_FIELDS`; one duplicate-name scan in `load_rules`. No new engine logic, no new reload logic, no new public Rust API, no new external dependency.

## Alternatives considered

1. **Retract / delete the dead engine.** Rejected: the engine is correct, tested, documented as the headline SLO feature, and wanted by the verifier's B06 contract. Deleting it abandons real value and a passing 20/20 test suite to avoid a small wiring job; the honest move is to make it reachable, not to erase the promise.
2. **Precedence on name collision (last-wins or synthesised-wins) instead of refuse.** Rejected: precedence is a silent drop of one rule — the operator loses alerting coverage they believe they have, the exact harm US-04 forbids ("never a silent shadow"). Refuse-on-collision is the only policy consistent with the feature's all-or-nothing honesty.
3. **A separate dedicated `*.slo.toml` file (or a separate `--slos DIR`) instead of inline `[[slo]]` in the rule file.** Rejected: it forks the operator's config surface (two file kinds, two directories, two reload paths) for no benefit; the loader already walks one `--rules` tree and the merge into one catalogue is the whole point (US-04). Inline `[[slo]]` alongside `[[rules]]` keeps one file kind, one loader, one reload, one catalogue.
4. **A reserved un-typeable namespace prefix for synthesised names** (to make collisions impossible by construction instead of detecting them). Rejected: it makes synthesised names un-referenceable from hand-authored `inhibits`, and the existing `_slo_` infix already gives practical separation; an explicit refuse-on-collision diagnostic is clearer to the operator than a magic prefix.
5. **An SLO-aware `beacon.reload.succeeded` field (`slos_added` separate from `added`).** Rejected (F4): redundant. The expansion-aware `added=4` is the operationally true count of new evaluators; the `slo_service` label on the four rules already groups them. Adding a field complicates the event the verifier's B03 reads for no operator gain.
6. **`[[slos]]` (plural table name).** Rejected (F1): inconsistent with the existing `[[rules]]` singular-repeated convention and with the `Slo` type name; `[[slo]]` reads naturally per declared SLO.

## Consequences

- **Positive:** the headline SLO feature becomes operator-reachable (0% → 100%); the engine, loader, catalogue, and ADR-0063 reload path are reused verbatim (one private wire struct + one conversion + one scan are the only new code); two false doc claims become true / corrected (the 30d rejection, the cross-validation test); the degenerate always-fire gun is disarmed before the operator path arms it; the existing rules-only path is byte-identical (no regression); ADR-0036's three inconsistencies are reconciled to the shipped truth; SLO reload and state carryover fall out of ADR-0063 for free.
- **Negative / accepted:** the synthesised rule names carry a `_slo_` infix that differs from the DISCUSS illustration (DISTILL asserts the real names); the `added=4` reload count for one SLO may briefly surprise an operator who declared "one SLO" (mitigated by the `slo_service` label and documented here); a malformed SLO refuses its whole file (consistent with the existing report-and-fail-the-file posture, not new harm).
- **Enforcement:** the validation refusals, the collision refusal, the all-or-nothing reload behaviour, the expansion-aware counts, and the cross-validation firing pattern are all **behaviour the acceptance suite pins black-box**, plus the per-feature 100% mutation gate on the modified `loader.rs` / `slo.rs` doc-comment lines (CLAUDE.md, ADR-0005 Gate 5). There is no import-graph rule to add; the invariants are properties of the loader's parse-validate-merge function and the reused reload function, best guarded by acceptance tests + mutation.
- **No external integration; no contract-test recommendation.** The operator entry points are a local TOML file and a POSIX SIGHUP; the only dependencies the load reaches are the local rules directory (via the already-tested loader) and the local durable store. No third-party API, webhook, or OAuth provider.
