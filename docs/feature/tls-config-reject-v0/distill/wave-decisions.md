# Wave Decisions — tls-config-reject-v0 (DISTILL)

- **Feature ID**: tls-config-reject-v0
- **Wave**: DISTILL (nWave)
- **Designer**: Scholar (nw-acceptance-designer)
- **Date**: 2026-06-04
- **Story**: US-TLS-01 (7 ACs)
- **Test file**: `crates/aperture/tests/slice_09_tls_config_reject.rs` (slice 09 — next free
  number; existing files run 01–08, with 08 = graceful_shutdown)

## Inputs read

- `docs/feature/tls-config-reject-v0/discuss/user-stories.md` (US-TLS-01, 7 ACs, 6 BDD scenarios).
- `docs/product/architecture/adr-0061-…security-knob.md` (refusal seam, event, exit code,
  behaviour matrix, supersession scope, comment correction).
- `docs/product/architecture/brief.md` §"For Acceptance Designer — tls-config-reject-v0"
  (driving port + per-AC observables).
- `docs/feature/tls-config-reject-v0/devops/{wave-decisions.md, environments.yaml}`
  (determinism verdict, fixed-port D4 guidance, pre-subscriber `eprintln!` caveat).
- Real code: `crates/aperture/src/{config/mod.rs, main.rs, observability.rs, sinks.rs}`;
  `crates/aperture/tests/{slice_07_tls_schema_knob.rs, cli_smoke.rs, common/mod.rs}`.

## Decisions

### D1 — Slice number 09

Existing `tests/` runs `slice_01`…`slice_08` (08 = graceful_shutdown). slice_07 is the TLS
schema knob (superseded contract). New file: `slice_09_tls_config_reject.rs`.

### D2 — Two driving-port surfaces, refusal proven on both

Per brief.md: in-process seam (`Config::from_toml_str`→`into_config`) AND binary subprocess
(`aperture --config <file>`). Every refusal row (tls-only, spiffe-only, both) gets a seam
test (the strongest AC-4 guarantee — no `Config`, no bind path) and a binary `@real-io`
test (the operator-visible exit-2 + named-knob-stderr + connect-refused surface).

### D3 — Refusal binary tests use default ports (collision-safe); positive control uses ephemeral

Per DEVOPS D3/D4: the refusal path never binds, so binary refusal tests run collision-free
against default 4317/4318. The positive-bind negative controls (AC-5/6) use the ephemeral
`127.0.0.1:0` override (`config/mod.rs:226-236`), exactly as slice_07 does. **No
default-port positive-bind test is added to the parallel suite** (gateway discipline).

### D4 — All refusal tests `#[ignore]`d (RED-not-BROKEN); negative controls left green

The 6 refusal tests are written against the existing public API, compile today, and FAIL
behaviourally (proven: `ac1_…refuses_config_construction --ignored` fails with
*"into_config returns Err(ConfigError): Config { … tls_enabled: true … }"*). Each carries
`#[ignore = "RED until DELIVER: tls-config-reject-v0"]` so `cargo test --workspace` stays
green at the DISTILL commit. The 4 negative-control tests + the AC-7 marker are NOT ignored:
they pass today and DELIVER must keep them green (non-regression guard).

### D5 — AC-7 is DELIVER-verified, not a runtime test

The brief classifies AC-7 (the `sinks.rs:94-95` comment) as a code-review/lint observable.
Asserting source-comment text from an integration test couples the suite to a source line.
A marker test records the decision; the correction itself is a DELIVER code task.

## Verification

```
$ cargo test -p aperture --test slice_09_tls_config_reject --locked
test result: ok. 5 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out

$ cargo test -p aperture --test slice_07_tls_schema_knob --locked   # UNCHANGED
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test --workspace --all-targets --locked
# zero test-result lines with non-zero failures across the workspace — fully green

$ cargo test -p aperture --test slice_09_tls_config_reject -- --ignored \
    ac1_tls_enabled_true_refuses_config_construction
test result: FAILED   # RED-not-BROKEN: fails on business reason (into_config returns Ok),
                      # not a compile/setup error
```

- New refusal tests: **compile + RED** (ignored at the commit). ✔
- `slice_07_tls_schema_knob.rs`: **byte-for-byte unchanged**, still passing (asserts the
  superseded warn-and-continue contract). ✔ (`git status` shows only the new file added.)
- `cargo test --workspace --all-targets --locked`: **green**. ✔

## DELIVER handoff tasks

| # | Task | Owner | Reference |
|---|------|-------|-----------|
| **T1** | Add the reject branch to `RawConfig::into_config`: if `tls.enabled` OR `auth.spiffe.enabled` is true, return `Err(ConfigError)` naming the knob(s) — verbatim substrings `tls.enabled` / `auth.spiffe.enabled` so the string-matching tests identify the offender. Remove `warn_if_v0_security_knob_set` (`compose.rs:56-76`) and its call site (`compose.rs:127`). | software-crafter | ADR-0061 §"Refusal point", §"Refusal event"; config/mod.rs:481-530 |
| **T2** | Route the `main.rs` `ConfigError` line through a structured channel carrying `event=config_validation_failed` + the named knob, satisfying the observable even in the pre-subscriber `eprintln!` window (`main.rs:33-39`). DELIVER picks JSON-via-subscriber vs structured `eprintln!`. | software-crafter | ADR-0061 §"Caveat on emission timing"; DEVOPS D3 |
| **T3** | **Un-ignore the 6 refusal tests** in `slice_09_tls_config_reject.rs` (remove `#[ignore = "RED until DELIVER: tls-config-reject-v0"]`) — they go GREEN once T1+T2 land. | software-crafter | this file D4 |
| **T4** | **Flip `slice_07_tls_schema_knob.rs`** to the refusal contract. It currently asserts the SUPERSEDED warn-and-continue behaviour (`tls_not_supported_in_v0` warn line + bound listener) and passes against today's code; after T1 it will FAIL. Update those scenarios to assert refusal, OR retire the superseded scenarios and let slice_09 own the contract. **Do not delete the schema-parse / defaults-off scenarios** (still valid — ADR-0008 schema preserved). | software-crafter | ADR-0061 §Consequences/Negative; DEVOPS D6; slice_07:67-183 |
| **T5** | **Correct the `sinks.rs:94-95` comment** (AC-7): replace the false *"the config validator rejects it ahead of this sink"* with the now-true statement that `tls.enabled=true` / `auth.spiffe.enabled=true` cause config validation to refuse startup (ADR-0061) before the sink is constructed. No comment may claim a rejection the code does not perform. | software-crafter | ADR-0061 §"Comment correction"; sinks.rs:94-95 |
| **T6** | Per-feature mutation 100% on the new reject branch (`gate-5-mutants-aperture --in-diff`). The two-knob truth table (3 refuse rows) + 2 negative controls supply the kill coverage. | software-crafter | CLAUDE.md / ADR-0005 Gate 5; DEVOPS D5 |

## Peer review

`nw-acceptance-designer-reviewer` (Sentinel) could not be dispatched from this subagent
context (no Task tool available here). A rigorous structured self-review against the 9
critique dimensions was performed (below); a top-level `@nw-acceptance-designer-reviewer`
run is **recommended** before DELIVER.

### Self-review (acceptance-designer critique dimensions 1–9)

```yaml
review_id: "accept_rev_self_tls-config-reject-v0"
reviewer: "nw-acceptance-designer (self-review; top-level reviewer recommended)"
artifact: "crates/aperture/tests/slice_09_tls_config_reject.rs + distill/*.md"
iteration: 1

strengths:
  - "AC-4 (no plaintext bind on refusal) asserted three ways: structural seam (no Config => bind path unreachable, refactor-proof), black-box connect-refused on the binary, and no-silent-proceed on both-true. The strongest guarantee does not depend on ordering discipline."
  - "Every refusal row of the two-knob truth table has BOTH a seam test and a binary @real-io test; both negative-control rows have a seam test and a spawn-and-bind test. Full truth-table coverage feeds the Gate-5 100% kill requirement."
  - "RED-not-BROKEN proven empirically: an ignored refusal test was run with --ignored and failed on the business reason (into_config returned Ok), not a compile/setup error. slice_07 left byte-for-byte unchanged (git status confirms only the new file)."
  - "Port-collision discipline mirrors DEVOPS D4 exactly: refusal tests are collision-safe (no bind) on default ports; positive control on ephemeral 127.0.0.1:0; no default-port positive-bind test added."

issues_identified:
  happy_path_bias:
    - issue: "Error/refusal ratio is 6/11 = 55% (>= 40%). Negative controls are first-class non-regression guards, not happy-path padding."
      severity: "none"
  gwt_format:
    - issue: "Rust integration tests, not Gherkin .feature files (the codebase convention — slice_*.rs). GWT is expressed as the file-header behaviour matrix + per-test arrange/act/assert. Consistent with every existing aperture slice test."
      severity: "none"
  business_language:
    - issue: "Vocabulary (tls.enabled, config_validation_failed, exit 2) is the operator-contract language of US-TLS-01/ADR-0061, not leaked internals. Test names read as operator outcomes."
      severity: "none"
  coverage_gaps:
    - issue: "All 7 ACs mapped (ac-coverage.md). AC-7 is DELIVER-verified per brief.md classification, with a marker test recording the decision."
      severity: "none"
  walking_skeleton_centricity:
    - issue: "WS = the binary refusal (@real-io), framed as the operator goal (refuse-to-start, named knob, no cleartext port), demo-able to the compliance stakeholder. Not a layer-connectivity framing."
      severity: "none"
  observable_behavior:
    - issue: "Every Then asserts an observable: exit code, stderr substring, Result is_err/is_ok, bound-port check, or connect-refused. No private-field or call-count assertion. (Dimension 7 PASS.)"
      severity: "none"
  traceability_coverage:
    - issue: "Check A — US-TLS-01 is the single story; all 11 tests trace to its ACs via the ac* naming + ac-coverage.md table. Check B — both DEVOPS environments (clean, ci) run the SAME deterministic checks; no per-environment Given divergence exists or is needed (SLIM, no env matrix)."
      severity: "none"
  priority_validation:
    - issue: "The refusal path (the security defect) carries the suite weight; negative controls guard the common case. Largest-bottleneck addressed (silent plaintext downgrade)."
      severity: "none"
  walking_skeleton_boundary:
    - issue: "No new driven adapter introduced. The binary @real-io tests exercise real subprocess + real temp file + real exit code + real connect-refused — real I/O at the operator entry point. Negative controls bind real ephemeral listeners. No InMemory double stands in for a local resource on the WS. (Dimension 9 PASS.)"
      severity: "none"

approval_status: "approved (self); top-level nw-acceptance-designer-reviewer recommended"
critical_issues_count: 0
high_issues_count: 0
medium_issues_count: 0
note: >
  One carried DELIVER mechanism flag (the pre-subscriber eprintln! structured-line window,
  T2) is a DELIVER task, not a DISTILL defect — recorded in the handoff table and
  io-strategy.md. The acceptance observable (event=config_validation_failed substring +
  named knob) is satisfiable by either DELIVER mechanism.
```

### Review proof display

- [x] Review YAML feedback (complete) — above.
- [x] Revisions made — none required (0 critical, 0 high, 0 medium DISTILL defects).
- [ ] Re-review iteration 2 — not triggered.
- [x] Quality gate status — **PASSED** (self-approved; top-level reviewer recommended,
      non-blocking).

## Definition of Done (DISTILL → DELIVER gate)

1. [x] All acceptance scenarios written with step definitions (11 tests; 6 refusal RED-ignored, 5 green).
2. [x] Test pyramid: acceptance (slice_09) + planned unit/mutation coverage on the reject branch (Gate 5, T6).
3. [~] Peer review — self-review PASSED; top-level `@nw-acceptance-designer-reviewer` recommended.
4. [x] Tests run in CI/CD — Gate 1 (`cargo test --workspace --all-targets --locked`) auto-covers; Gate 5 mutates the branch (DEVOPS D1).
5. [x] Story demonstrable from acceptance tests — the binary refusal `@real-io` walking skeleton is demo-able to the compliance stakeholder.

**Do NOT proceed into DELIVER from this wave.** Handoff tasks T1–T6 above are DELIVER's.
