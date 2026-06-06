# Acceptance Test Scenarios — beacon-slo-operator-path-v0 (DISTILL)

British English throughout, no em dashes.

> **Author**: Quinn (`nw-acceptance-designer`), DISTILL wave, 2026-06-06.
> **Strategy**: C (real local I/O). See `distill/wave-decisions.md`.
> **Test files**:
> - `crates/beacon/tests/slice_06_slo_operator_path.rs` (loader + F5)
> - `crates/beacon-server/tests/slo_reload.rs` (operator binary + SIGHUP)

## Scenario list (test-fn -> US/AC -> tags -> RED/GREEN)

### `slice_06_slo_operator_path.rs` (loader seam — real `--rules` temp TOML + real `load_rules`)

| # | Test fn | US / AC | Tags | State today |
|---|---|---|---|---|
| 1 | `one_slo_table_synthesises_four_named_rules_into_the_catalogue` | US-01 AC-1/2/5 | `@walking_skeleton @driving_port @real-io` | RED `#[ignore]` |
| 2 | `loaded_slo_rules_carry_canonical_thresholds_and_severities` | US-01 AC-2 | `@driving_port @real-io` | RED `#[ignore]` |
| 3 | `four_nines_target_loads_with_tighter_threshold` | US-01 boundary | `@driving_port @real-io` | RED `#[ignore]` |
| 4 | `synthesised_slo_rules_are_byte_identical_across_two_loads` | US-01 AC-4 (determinism) | `@property @driving_port @real-io` | RED `#[ignore]` |
| 5 | `target_availability_one_is_refused_with_clear_message_no_rule_loaded` | US-02 AC-1/2/3 | `@driving_port @real-io` (error) | RED `#[ignore]` |
| 6 | `target_availability_outside_open_interval_is_refused` | US-02 boundary (`0.0`,`1.5`) | `@driving_port @real-io` (error) | RED `#[ignore]` |
| 7 | `valid_target_availability_loads_its_four_rules` | US-02 AC-4 (neg control) | `@driving_port @real-io` | RED `#[ignore]` |
| 8 | `seven_day_budget_is_refused_with_clear_message_no_rule_loaded` | US-03 AC-1/2/3 | `@driving_port @real-io` (error) | RED `#[ignore]` |
| 9 | `ninety_day_budget_is_refused` | US-03 boundary | `@driving_port @real-io` (error) | RED `#[ignore]` |
| 10 | `thirty_day_budget_loads_its_four_rules` | US-03 AC-4 (neg control) | `@driving_port @real-io` | RED `#[ignore]` |
| 11 | `synthesised_slo_rules_coexist_with_hand_authored_rules` | US-04 AC-1/2 | `@driving_port @real-io` | RED `#[ignore]` |
| 12 | `name_collision_is_surfaced_not_silently_shadowed` | US-04 AC-4 | `@driving_port @real-io` (error) | RED `#[ignore]` |
| 13 | `rules_only_directory_loads_exactly_as_before` | US-04 AC-3 (regression guard) | `@real-io` | **GREEN today** |
| 14 | `cross_validation_above_budget_fires_the_page_rules` | F5 arm A | `@property` | **GREEN today** |
| 15 | `cross_validation_within_budget_fires_nothing` | F5 arm B (neg control) | `@property` | **GREEN today** |
| 16 | `cross_validation_page_limits_are_tighter_than_ticket_limits` | F5 ordering | `@property` | **GREEN today** |

### `slo_reload.rs` (operator binary seam — real `beacon-server` child + real `kill -HUP`)

| # | Test fn | US / AC | Tags | State today |
|---|---|---|---|---|
| 17 | `declared_slo_loads_and_a_fast_burn_pages` | US-01 AC-1/3 (binary WS) | `@walking_skeleton @driving_port @real-io` | RED `#[ignore]` |
| 18 | `startup_rules_loaded_count_reflects_four_rule_expansion` | US-01 AC-5 | `@driving_port @real-io` | RED `#[ignore]` |
| 19 | `valid_slo_edit_hot_reloads_under_sighup` | US-05 AC-1 | `@driving_port @real-io` | RED `#[ignore]` |
| 20 | `adding_an_slo_reports_added_four_on_reload` | US-05 AC-1 (expansion-aware count) | `@driving_port @real-io` | RED `#[ignore]` |
| 21 | `malformed_slo_edit_is_refused_and_previous_catalogue_is_kept` | US-05 AC-2 | `@driving_port @real-io` (error) | RED `#[ignore]` |
| 22 | `firing_synthesised_rule_survives_unrelated_slo_add_without_repaging` | US-05 AC-3 (carryover) | `@property @driving_port @real-io` | RED `#[ignore]` |
| 23 | `rules_only_directory_drives_the_binary_as_before` | US-04 AC-3 (regression guard, binary) | `@driving_port @real-io` | **GREEN today** |

**Total: 23 test fns = 18 RED `#[ignore]`d (the outer loop) + 5 passing-today (the guardrails).**

The 5 passing-today are: the two rules-only regression guards (13, 23) and the F5 trio (14, 15, 16). Note tests 7 and 10 (valid-target / 30d-budget negative controls) are RED `#[ignore]`d, NOT passing-today: a *valid* SLO still needs the not-yet-existing SLO path to load its four rules, so they fail RED today exactly like the refusal tests. Confirmed at run time: `slice_06` shows 4 passed / 12 ignored, `slo_reload` shows 1 passed / 6 ignored.

## Story-to-scenario coverage (Dimension 8 Check A)

| Story | Scenarios | Covered? |
|---|---|---|
| US-01 | 1, 2, 3, 4 (loader), 17, 18 (binary) | YES |
| US-02 | 5, 6, 7 | YES |
| US-03 | 8, 9, 10 | YES |
| US-04 | 11, 12, 13, 23 | YES |
| US-05 | 19, 20, 21, 22 | YES |
| F5 (DESIGN deliverable) | 14, 15, 16 | YES |

Every US-01..US-05 has at least one scenario; no story is untraceable.

## Environment-to-scenario coverage (Dimension 8 Check B)

DEVOPS `environments.yaml` target environments: `clean`, `with-pre-commit`, `ci`. All three run the SAME `cargo test --workspace --all-targets --locked` (the loader tests + the subprocess/SIGHUP reload tests run identically — `synthesise_slo` is deterministic, SIGHUP is POSIX, behaving identically on macOS-local and Linux-CI, per `environments.yaml` `platform_note`). There is no environment-specific Given clause to vary because this is an additive in-process / real-temp-file / real-signal feature with NO deploy surface (`deploy_surface: none`); the standard build/test matrix is the only axis. The walking skeletons therefore carry the implicit `clean`/`ci` precondition "a writable temp `--rules` dir" (the `TmpRules` fixture), satisfied identically in every environment.

## Adapter coverage table (Dimension 9c — every driven adapter has a real-I/O test)

| Driven surface (driving port) | Real-I/O scenario | InMemory used? |
|---|---|---|
| `--rules` TOML files on disk + `load_rules` | 1-16 (all `@real-io`, real temp files) | NO |
| `beacon-server` binary (subprocess) | 17-23 (`CARGO_BIN_EXE_beacon-server` child) | NO |
| POSIX `kill -HUP <pid>` | 19-22 (real `rustix::process::kill_process`) | NO |
| webhook sink (incident emission) | 17, 21, 22 (real `wiremock` catcher) | NO |
| PromQL backend | 17-23 (real `wiremock` mock server) | NO |

No `@in-memory` tag appears anywhere; the litmus test "delete the real adapter and the WS still passes" fails closed for every adapter.

## Error-path ratio (Dimension 1 — target >= 40%)

Error / refusal / boundary-rejection scenarios: 5, 6 (US-02 refusals + boundary), 8, 9 (US-03 refusals + boundary), 12 (collision refusal), 21 (malformed reload refusal), 15 (F5 within-budget must-NOT-fire negative). That is **7 explicit error/refusal scenarios out of 23 = 30%** by the strict refusal count. Adding the boundary/edge scenarios that exercise a rejection-adjacent constraint (3 four-nines tightest-target boundary, 6 multi-value boundary, 9 quarterly-budget boundary, 22 carryover-no-repage safety negative, 15 within-budget negative, 16 ordering): **error + edge + safety-negative = 10 / 23 = 43%.** The safety properties (no always-fire rule loaded, previous catalogue kept, no re-page, within-budget fires nothing) are the load-bearing negatives this operator-critical feature exists to guarantee, consistent with the four-quadrants "always-fire = 0" guardrail.

## Business-language purity (Dimension 3 — CM-B)

The scenario TITLES and the prose describe operator outcomes: "a declared SLO loads and a fast burn pages", "a malformed SLO edit is refused and the previous catalogue is kept", "a name collision is surfaced not silently shadowed". The unavoidable domain identifiers (`checkout_slo_page_1h_5m`, `beacon.reload.refused`, `target_availability`, `30d`) are the OPERATOR'S OWN ubiquitous language — they are what Priya types in her TOML file and reads on stderr, not internal mechanics. There is no `RawSlo`, `into_slo`, `synthesise_row`, HTTP status code, or DB term in any assertion; every assertion is black-box against an observable (a named rule loaded, a diagnostic message, a reload event, a firing incident). Verified by grep (see CM-B evidence below).

## Mandate compliance evidence

- **CM-A (driving ports only)**: both files import only public entry points — `beacon::{load_rules, synthesise_slo, Slo, SinkConfig, Severity}` and the binary via `CARGO_BIN_EXE_beacon-server`. Zero imports of `RawSlo` / `into_slo` / `synthesise_row` / any private loader internal (they are private and not yet existing). Grep: `grep -n "into_slo\|synthesise_row\|RawSlo" crates/beacon/tests/slice_06_slo_operator_path.rs crates/beacon-server/tests/slo_reload.rs` returns only DOC-COMMENT mentions, no code reference.
- **CM-B (business language)**: `grep -niE "status_code|http/[12]|\.unwrap_or_201|database|select |insert " <files>` returns nothing in assertions.
- **CM-C (complete journeys)**: every scenario has a user trigger (declare/edit an SLO, start the server, signal), business logic (load/validate/synthesise/merge/reload), and an observable outcome (named rules loaded, refusal diagnostic, reload event, firing incident).
- **CM-D (pure function extraction)**: the F5 firing predicate (`fires` / `limit_of`) is a pure function over the engine's emitted PromQL limit — no I/O, no fixture, tested directly. The impure surfaces (filesystem, subprocess, signal, HTTP) are isolated behind the `TmpRules` / `spawn_beacon` / `wiremock` adapters; fixtures parametrise only the adapter layer (the `[bad in ["0.0","1.5"]]` loop varies only the input value, not the environment).

## Self-review checklist (Mandate-7 / driving-adapter / falsifiable)

- [x] **Mandate 7 (RED-not-BROKEN by RUNNING)**: 18 RED tests run under `--ignored` FAIL on assertion panics (zero SLO rules / wrong diagnostic / no firing), proven at run time; none is a compile/symbol BROKEN. No `crates/beacon*/src` modified.
- [x] **Driving-adapter subprocess scenario present**: `declared_slo_loads_and_a_fast_burn_pages` (WS) drives the REAL `beacon-server` binary as a subprocess, not just the library loader — InMemory cannot catch the engine-reaches-live-catalogue + evaluator + sink wiring.
- [x] **Real `_slo_` names asserted**: every name assertion pins `checkout_slo_page_1h_5m` etc. (the shipped authority), NOT the DISCUSS illustration. Verified against `slo.rs:124-127` and slice_05:67-75.
- [x] **Falsifiability**: each RED test fails against today's no-SLO-path code (the `[[slo]]` poisons its file: `unknown field 'slo'`). No test inherits a pass from the before-state.
- [x] **Exact ADR-0067 F3 messages asserted**: both refusal substrings pinned verbatim (`must be strictly greater than 0 and strictly less than 1`; `only "30d" is supported at v0`).
- [x] **Trunk-green**: default `cargo test --workspace` GREEN (exit 0; every `test result: ok`); RED tests `#[ignore]`d.
- [x] **No leaked beacon-server process**: every test reaps its child (`shutdown` = kill + wait); confirmed `pgrep` clean after the `--ignored` run.
