# Wave Decisions — cinder-unknown-item-diagnostic-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Agent**: Quinn / Scholar (`nw-acceptance-designer`)
- **Date**: 2026-06-06
- **Mode**: Autonomous overnight run. **SLIM** — one diagnostic-wording
  slice. One thin vertical slice, four acceptance tests (2 RED, 2
  control), all through the real binary entry point.

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `discuss/user-stories.md` (US-01, 4 AC, 4 UAT scenarios) | The 4 AC verbatim → 4 test functions; the SRE persona (Priya); the exact documented phrase `cannot migrate unknown item "<item_id>" for tenant <tenant>`; the `ghost` / `acme/batch-00042` ids; fail-closed-unchanged guardrail; KPI = zero `ItemId(` leaks | − AC-3 (`the-message-matches-the-CLI-help-contract`) is a byte-shape restatement of AC-1/AC-2's "must contain quoted phrase + must not contain `ItemId(`"; folded into the two RED tests rather than a separate scenario (a standalone byte-equal check would re-assert the same two substrings). Recorded in DECISION 4. |
| `design/wave-decisions.md` (D1-D5, reuse table, test seam) | The chosen mechanism (`{:?}` on `item.as_str()` → quoted bare id); get-tier shares the single arm (one test proves both); the in-codebase `{value:?}` quoting precedent; the subprocess test seam (spawn binary, assert stderr/exit); the "ADD assertions, do NOT weaken the existing substring test" instruction | − none. DESIGN owns the exact source mechanism (DELIVER writes it); DISTILL encodes only the observable contract. |
| `devops/wave-decisions.md` (C-DEVOPS-1..6, environments) | C-DEVOPS-3 (deterministic boolean substring + exit-code assertions, NO wall-clock → no p95-flake class); C-DEVOPS-4 (falsifiability mandatory — assert pair must fail on today's leak, pass only on the fix); C-DEVOPS-5 (guardrails green: exit-1 unchanged, known-item stdout unchanged); environments `clean` + `with-pre-commit` + `ci` | − none. The acceptance-test environment (`acceptance_test_environment` in environments.yaml) is exactly the CLI subprocess + real temp data dir used here. |
| `discuss/outcome-kpis.md` (North Star) | North Star = zero `ItemId(` leaks; leading = byte-equal-to-help quoted id + verifier K18 (UC-TIER-008/009) GREEN | − KPIs are in-suite acceptance assertions (K6 idiom), not live telemetry → no `@kpi` observability scenario is applicable (nothing to emit). Soft-gate satisfied: the KPI is the acceptance assertion itself. |
| `docs/product/architecture/brief.md` cinder note | Driving port = the `kaleidoscope-cli` binary; the For-Acceptance-Designer subprocess seam (both subcommands through the shared arm) | − read via the DEVOPS summary which quotes it; no new driving port beyond the CLI binary. |

## Walking-skeleton strategy

**Strategy: real CLI subprocess + real temp data dir (no test doubles).**

This feature has no NEW driven adapter and no new wiring — it is a
text-rendering fidelity fix on an EXISTING, fully-wired path
(`CLI subcommand → MigrateError::UnknownItem → Error::CinderMigrate
Display → stderr`). The only honest way to observe the contract (the
operator-facing stderr token) is to spawn the real built binary
(`CARGO_BIN_EXE_kaleidoscope-cli`) against a real temp Cinder data dir
on the real filesystem and read captured stderr + exit code — exactly
what Priya sees. Every one of the four tests is this shape; there is no
in-memory tier in play. **No `@in-memory` anywhere** (Dim 9e clean).

Litmus (Dim 9d): "If I deleted the real adapter, would the test still
pass?" — No. The tests spawn the real binary which opens the real
`FileBackedTieringStore`; the seeding helper (`place_item`) writes a
real WAL to disk that the subprocess reopens. The wiring is exercised
end-to-end.

Both unknown-item scenarios are the walking skeletons (the simplest
complete user journey that delivers the observable value: "I typed a
bad id and the error names it the way I recognise it"). They are tagged
in-prose as the driving-port scenarios (binary entry).

## DECISION 1 — One new test file, existing harness mirrored

A NEW file `crates/kaleidoscope-cli/tests/unknown_item_diagnostic.rs`
(+ a `[[test]]` block in `Cargo.toml`), NOT an edit to
`migrate_subcommand.rs`. Rationale:

1. **C-DEVOPS-4 / DESIGN instruction**: do NOT weaken the existing
   substring test (`migrate_subcommand.rs:309-324`, which stays green
   under both wordings and never caught the leak). A new file ADDS the
   discriminating assertions without touching the old one.
2. The contract under test (the cross-subcommand diagnostic WORDING) is
   a distinct concern that spans both `migrate` and `get-tier`; a
   dedicated file names that concern.
3. The harness (`tenant`, `temp_root`, `cleanup`, `cinder_base`,
   `place_item`, `bin`) is duplicated inline, mirroring the cluster's
   D-NewTestFile convention (rule-of-three extraction deferred — this is
   the established shape across the sibling test files).

## DECISION 2 — Four tests: 2 behavioural-RED + 2 controls

| Test fn | AC | State today | After DELIVER |
|---|---|---|---|
| `unknown_item_migrate_names_the_bare_quoted_id` | AC-1 (+AC-3 shape) | **RED** (ignored) | GREEN |
| `unknown_item_get_tier_names_the_bare_quoted_id` | AC-2 (+AC-3 shape) | **RED** (ignored) | GREEN |
| `known_item_migrates_unchanged` | AC-4 (success half) | **PASS** (un-ignored) | PASS |
| `unknown_item_still_fails_closed` | AC-4 (fail-closed half) | **PASS** (un-ignored) | PASS |

The get-tier RED test proves DESIGN D3 (get-tier shares the single arm,
no separate fix) at the observable boundary: it fails today on the same
leak and will be fixed by the same one-arm change.

## DECISION 3 — RED-not-BROKEN classification (Mandate 7), proven by RUNNING

No NEW production symbol is required: the CLI, both subcommands, the
`migrate`/`get_tier` library functions, and the diagnostic path all
already exist and compile (verified: `cargo build -p kaleidoscope-cli`
succeeds; the existing `migrate_subcommand.rs` / `get_tier_subcommand.rs`
already import and call these). Therefore the two unknown-item tests are
**behaviourally RED, not broken scaffolds** — they compile and run, and
fail on a business-logic assertion (the live arm emits `ItemId("ghost")`
instead of the quoted bare id). No `todo!()` stub, no scaffold.

**The `#[ignore]` decision**: the pre-commit hook runs
`cargo test --workspace` and the project NEVER uses `--no-verify`, so a
behaviourally-RED test MUST carry `#[ignore = "RED until DELIVER: …"]`
to keep the default suite green. The two controls are un-ignored (they
pass today AND after the fix). DELIVER removes the two `#[ignore]`
attributes when the fix lands.

## DECISION 4 — AC-3 folded into AC-1/AC-2 (no separate byte-equal test)

AC-3 (`the-message-matches-the-CLI-help-contract`) asserts the emitted
message is the documented shape with the bare quoted id + bare tenant
substituted, no internal type name. That is precisely the conjunction
already asserted by AC-1 and AC-2: `contains("unknown item \"<id>\" for
tenant <tenant>")` (the documented shape, substituted) AND
`!contains("ItemId(")` (no internal type name). A standalone byte-equal
test would re-assert the same two substrings against the same two
subprocess runs. Folding AC-3 into AC-1/AC-2 avoids a redundant
scenario while leaving AC-3 fully covered (the in-prose scenario header
of each RED test names the documented phrase verbatim). This is the
golden-path-+-key-alternatives selection rule (do not test every
combination of the same business rule).

## Falsifiability note (load-bearing)

Probed empirically against the freshly-built binary at DISTILL time:

```
$ kaleidoscope-cli migrate acme <data> ghost warm
kaleidoscope-cli: cinder migrate: cannot migrate unknown item ItemId("ghost") for tenant acme   (exit 1)
$ kaleidoscope-cli get-tier globex <data> acme/batch-00042
kaleidoscope-cli: cinder migrate: cannot migrate unknown item ItemId("acme/batch-00042") for tenant globex   (exit 1)
```

The bare quoted substring `"ghost"` appears INSIDE the leaked
`ItemId("ghost")` too, so quoted-presence ALONE does not discriminate
old from new wording. The **discriminating pair** is:

1. the full documented phrase `unknown item "ghost" for tenant acme`
   is PRESENT (false today — today's substring is
   `unknown item ItemId("ghost") for tenant acme`), AND
2. `ItemId(` is ABSENT (false today — it is present).

Both are false on today's output and true only on the fixed output —
this is the C-DEVOPS-4 falsifiability requirement met. The
`!contains("ItemId(")` assertion is the load-bearing one; it is the
assertion that kills the Gate-5 mutant that reverts the arm to
`{item:?}`.

### Proven RED — run evidence

```
$ cargo test -p kaleidoscope-cli --test unknown_item_diagnostic
  test known_item_migrates_unchanged ... ok
  test unknown_item_get_tier_names_the_bare_quoted_id ... ignored, RED until DELIVER: …
  test unknown_item_migrate_names_the_bare_quoted_id ... ignored, RED until DELIVER: …
  test unknown_item_still_fails_closed ... ok
  test result: ok. 2 passed; 0 failed; 2 ignored

$ cargo test -p kaleidoscope-cli --test unknown_item_diagnostic -- --ignored
  test unknown_item_get_tier_names_the_bare_quoted_id ... FAILED
  test unknown_item_migrate_names_the_bare_quoted_id ... FAILED
  panic: stderr contains the documented quoted-id phrase `unknown item "ghost" for tenant acme`
         (got: "… cannot migrate unknown item ItemId(\"ghost\") for tenant acme\n\n")
  test result: FAILED. 0 passed; 2 failed; 2 filtered out
```

Default `cargo test` GREEN (controls pass, RED ones skipped); `--ignored`
shows the two RED ones FAILING on the documented-phrase / `ItemId(`-leak
assertions. RED-not-BROKEN confirmed by running, not by inspection.

## Reconciliation with prior waves

- **DESIGN test seam** (4-point: migrate-unknown, get-tier-unknown,
  known-item control, exit-1-unchanged) → encoded 1:1 as the four tests.
  DESIGN's "exit-1-unchanged covered by the non-zero assertion" is
  promoted to its own un-ignored control (`unknown_item_still_fails_
  closed`) so the fail-closed contract is pinned independently of the
  wording (it survives even while the wording test is RED).
- **DEVOPS C-DEVOPS-1** (no new CI job): the new file lands under
  `crates/kaleidoscope-cli/tests/`, auto-covered by the existing
  `gate-5-mutants-kaleidoscope-cli --in-diff` job; no CI-config change.
- **DEVOPS C-DEVOPS-3** (deterministic, no wall-clock): all four tests
  are boolean substring + exit-code + exact-stdout assertions on a
  short-lived `Command::output()` — no timing threshold, no p95-flake
  class. Children are reaped by `output()` itself (no hangs).

## Quality gates run

- `cargo fmt --all` — applied, clean.
- `cargo clippy -p kaleidoscope-cli --tests` — clean (no warnings).
- `cargo test -p kaleidoscope-cli` (full package, default) — GREEN, 0
  failed, the new target reports `2 passed; 2 ignored`.
- `cargo test -p kaleidoscope-cli --test unknown_item_diagnostic --
  --ignored` — the 2 RED tests FAIL as designed.

## What this DISTILL wave does NOT do

- Does NOT modify any `crates/*/src` (the one-arm fix is DELIVER's).
- Does NOT remove the `#[ignore]` attributes (DELIVER does, when the fix
  lands and the tests go GREEN).
- Does NOT weaken or touch the existing `migrate_subcommand.rs`
  substring test (C-DEVOPS-4).
- Does NOT add a CI job or bump any version (C-DEVOPS-1 / C-DEVOPS-2).
- Does NOT proceed into DELIVER.
