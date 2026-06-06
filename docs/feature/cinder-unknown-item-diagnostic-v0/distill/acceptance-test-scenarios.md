# Acceptance Test Scenarios — cinder-unknown-item-diagnostic-v0

- **Wave**: DISTILL (nWave) | **Agent**: Quinn / Scholar | **Date**: 2026-06-06
- **Story**: US-01 — "The unknown-item diagnostic names the id I typed, quoted"
- **Driving port**: the `kaleidoscope-cli` binary (real subprocess).
- **Test file**: `crates/kaleidoscope-cli/tests/unknown_item_diagnostic.rs`
- **Cargo target**: `[[test]] name = "unknown_item_diagnostic"`.

## Persona

Priya Raman, platform SRE, operates Kaleidoscope's tiering layer from the
`kaleidoscope-cli` binary. When she names an item id that was never
placed, she needs the error to name THE ID SHE TYPED — bare and quoted
(`"ghost"`) — so she classifies it as a not-found (re-run with the right
id) rather than an internal fault (escalate / file a bug).

## Scenarios

### Scenario 1 — Unknown item on migrate names the bare quoted id

> `@walking_skeleton @driving_port @error-path @us-01`
> Test fn: `unknown_item_migrate_names_the_bare_quoted_id`
> State today: **RED** (`#[ignore = "RED until DELIVER: …"]`)

```gherkin
Scenario: Unknown item on migrate names the bare quoted id
  Given no item "ghost" has been placed under tenant "acme"
  When Priya runs migrate for item "ghost" under tenant "acme"
  Then the command fails
  And the error names the item she typed, quoted: "ghost"
  And the error does not mention any internal item-wrapper name
```

Observable assertions (through the binary): exit non-zero; stderr
contains `unknown item "ghost" for tenant acme`; stderr does NOT contain
`ItemId(`.

### Scenario 2 — Unknown item on get-tier names the bare quoted id

> `@walking_skeleton @driving_port @error-path @us-01`
> Test fn: `unknown_item_get_tier_names_the_bare_quoted_id`
> State today: **RED** (`#[ignore = "RED until DELIVER: …"]`)

```gherkin
Scenario: Unknown item on get-tier names the bare quoted id
  Given no item "acme/batch-00042" has been placed under tenant "globex"
  When Priya runs get-tier for item "acme/batch-00042" under tenant "globex"
  Then the command fails
  And the error names the item verbatim, quoted: "acme/batch-00042"
  And the error does not mention any internal item-wrapper name
```

Observable assertions: exit non-zero; stderr contains
`unknown item "acme/batch-00042" for tenant globex` (slash preserved,
quoted); stderr does NOT contain `ItemId(`. Proves get-tier is fixed by
the SAME shared arm (DESIGN D3) at the observable boundary.

### Scenario 3 — Known item migrates and exit code is unchanged (control)

> `@driving_port @happy-path @control @us-01`
> Test fn: `known_item_migrates_unchanged`
> State today: **PASS** (un-ignored) — invariant across the fix

```gherkin
Scenario: Known item migrates and the success path is unchanged
  Given item "blk-7781" is placed in Hot under tenant "acme"
  When Priya runs migrate for item "blk-7781" to "warm" under tenant "acme"
  Then the command succeeds
  And she sees: migrated tenant=acme item=blk-7781 from=hot to=warm
```

Observable assertions: exit zero; stdout is exactly
`migrated tenant=acme item=blk-7781 from=hot to=warm\n`. Guards the
success path against a wording-fix regression.

### Scenario 4 — Unknown item still fails closed (control)

> `@driving_port @error-path @control @us-01`
> Test fn: `unknown_item_still_fails_closed`
> State today: **PASS** (un-ignored) — invariant across the fix

```gherkin
Scenario: Unknown item still fails closed (exit code unchanged)
  Given no item "ghost" has been placed under tenant "acme"
  When Priya runs migrate for item "ghost" under tenant "acme"
  Then the command fails closed
  And nothing is reported as a success
```

Observable assertions: exit non-zero; stdout is empty. Deliberately
asserts ONLY exit code + empty stdout (NOT the wording), so it is
invariant across the fix and pins the fail-closed contract independently
of Scenarios 1/2.

## Test-fn → AC map (Coverage, Dim 4 + Dim 8 Check A)

| AC (user-stories.md) | Scenario | Test fn | Tag |
|---|---|---|---|
| AC-1 `unknown-item-migrate-names-the-bare-quoted-id` | 1 | `unknown_item_migrate_names_the_bare_quoted_id` | RED |
| AC-2 `unknown-item-get-tier-names-the-bare-quoted-id` | 2 | `unknown_item_get_tier_names_the_bare_quoted_id` | RED |
| AC-3 `the-message-matches-the-CLI-help-contract` | 1 & 2 (folded — the quoted-phrase-present + `ItemId(`-absent conjunction IS the byte-shape contract; see DECISION 4) | (covered by 1 & 2) | RED |
| AC-4 `known-item-and-exit-1-behaviour-unchanged` | 3 (success) + 4 (fail-closed) | `known_item_migrates_unchanged`, `unknown_item_still_fails_closed` | control |

Every US-01 AC has at least one scenario. US-01 is the only story → full
story coverage.

## Error-path ratio (Dim 1)

3 of 4 scenarios exercise the error/failure path (Scenarios 1, 2, 4);
1 is the success control (Scenario 3). **Error-path ratio = 75% ≥ 40%.**
(The feature's whole point is an error-message fidelity fix, so the
error path is the principal subject — proportionate.)

## Environment coverage (Dim 8 Check B)

Per `devops/environments.yaml`: `clean`, `with-pre-commit`, `ci`. These
are build/test environments for a library message fix, NOT deploy
targets. The acceptance environment is a CLI subprocess + a real temp
data dir (`temp_root` under `env::temp_dir()`), identical across all
three — each test `fs::create_dir_all`s a fresh `clean` data dir, so the
`clean` precondition is encoded in every scenario's Given. The
`with-pre-commit` and `ci` environments run the identical deterministic
assertions (C-DEVOPS-3: no wall-clock → no p95-flake class). No
environment-specific Given is needed because the diagnostic wording is
environment-invariant.

## Mandate-7 / falsifiability self-review checklist

- [x] **RED-not-BROKEN, classified by RUNNING**: tests compile and run;
      the 2 RED ones fail on a business-logic assertion (live arm emits
      `ItemId(…)`), not a compile/scaffold error. No NEW production
      symbol needed (CLI + both subcommands already exist). Proven:
      default `cargo test` → `2 passed; 2 ignored`; `--ignored` → the 2
      FAIL with the `ItemId("ghost")` panic output.
- [x] **Falsifiable, load-bearing assertion**: `!contains("ItemId(")`
      plus the full documented quoted phrase — both false today, both
      true only after the fix. Quoted-presence alone is NOT
      discriminating (it lives inside `ItemId("ghost")`); the
      `ItemId(`-absence is the discriminator. (C-DEVOPS-4.)
- [x] **No Fixture Theater**: Given steps set up PRECONDITIONS only
      (no placement, or a placement of a DIFFERENT/known item). The
      EXPECTED diagnostic wording is never seeded — it is produced by
      production code. A test passing without the DELIVER fix is
      impossible (proven RED).
- [x] **#[ignore] discipline**: pre-commit `cargo test --workspace`
      (NEVER `--no-verify`) stays green; the 2 behavioural-RED tests are
      `#[ignore = "RED until DELIVER: …"]`; the 2 controls are
      un-ignored and pass today. DELIVER removes the 2 `#[ignore]`s.
- [x] **Mandate 1 (hexagonal boundary)**: every test drives through the
      real binary (`CARGO_BIN_EXE_kaleidoscope-cli`); zero internal
      component (`MigrateError`, `ItemId`, store) is constructed or
      asserted on directly. The test imports `cinder` types only for the
      `place_item` SEEDING helper (precondition setup), not for the
      assertion boundary.
- [x] **Mandate 2/3 (business language + observable outcomes)**: Gherkin
      uses Priya / item / tenant / "fails" / "succeeds" / "the error
      names the item she typed" — no HTTP/JSON/DB/status-code/class
      terms. Every Then is an observable outcome (exit code, captured
      stderr/stdout text Priya reads), never internal state.
- [x] **Determinism (C-DEVOPS-3)**: boolean substring + exit-code +
      exact-stdout on a short-lived `Command::output()`; no wall-clock
      threshold; children reaped by `output()` (no hangs).
- [x] **fmt + clippy clean**: `cargo fmt --all` applied;
      `cargo clippy -p kaleidoscope-cli --tests` clean.
- [x] **Guardrails (C-DEVOPS-5)**: known-item stdout unchanged
      (Scenario 3 asserts the exact line); fail-closed exit unchanged
      (Scenario 4); no other diagnostic touched (no src edit).
- [x] **No `@in-memory` / Dim 9e clean**: every scenario uses the real
      filesystem-backed store via the real binary; deleting the real
      adapter would fail all four (Dim 9d).
