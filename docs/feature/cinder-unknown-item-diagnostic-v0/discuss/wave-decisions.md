# Wave Decisions: cinder-unknown-item-diagnostic-v0 (DISCUSS)

> Luna, DISCUSS wave. Autonomous overnight run. Small, well-bounded
> contract-fidelity fix: one diagnostic message, two callers, one slice.

## Origin

Bea Verifier's UC-TIER coverage batch, issue 011 (expectation K18,
UC-TIER-008/009): the unknown-item diagnostic leaks the internal `ItemId`
newtype Debug form where the operator expects the bare id they typed,
quoted.

## Verified loci (read in code, not assumed)

- **The leak**: `crates/cinder/src/store.rs:55-58` —
  `MigrateError::UnknownItem` Display arm renders
  `"cannot migrate unknown item {item:?} for tenant {tenant}"`. `{item:?}`
  is `Debug` of `ItemId(pub String)` (`crates/cinder/src/tier.rs:51-52`)
  -> prints `ItemId("ghost")`.
- **The contract (CLI help)**: `crates/kaleidoscope-cli/src/main.rs:208`
  (migrate) and `:245` (get-tier) BOTH promise
  `cannot migrate unknown item "<item_id>" for tenant <tenant>` — bare,
  quoted id. Doc-vs-code mismatch confirmed.
- **Both subcommands share ONE arm**: `kaleidoscope-cli` `migrate`
  (`crates/kaleidoscope-cli/src/lib.rs:471`) and `get_tier`
  (`crates/kaleidoscope-cli/src/lib.rs:509`) both construct
  `MigrateError::UnknownItem` and render it via `Error::CinderMigrate(e)`
  Display (`lib.rs:103`, prepends `cinder migrate: `). One fix at the
  cinder Display arm fixes both paths. get-tier does NOT need its own fix.
- **Render-site audit (blast radius)**: the ONLY `{:?}`/Display render of
  the `ItemId` newtype anywhere in the cinder crate is store.rs:57. Every
  other `ItemId` occurrence is a struct field, trait signature, `HashMap`
  key, test constructor, or the CLI success-line rendering — and the CLI
  success lines (`migrate`/`place`) already use the raw `item_id: &str`
  from argv, not the newtype. No other diagnostic regresses from changing
  this arm.
- **Clean accessor exists**: `ItemId::as_str()`
  (`crates/cinder/src/tier.rs:59`).
- **Existing test gap**: `migrate_subcommand.rs:309-312` asserts only
  `stderr.contains("ghost-item")` — satisfied by both old and new wording,
  so it neither breaks nor catches the leak. The NEW AC (quoted form +
  no-`ItemId(`) pins the contract.

## Autonomous decisions

| Decision | Choice | Rationale |
|---|---|---|
| Feature Type | Backend (CLI / error-diagnostic fidelity) | one operator-facing message; no UI/UX surface beyond stderr text |
| Walking Skeleton | No | brownfield, tiny; the slice IS the skeleton |
| UX research | Lightweight | TUI error-message heuristics (what/why/what-to-do, no internal jargon) suffice |
| JTBD | error-names-the-id-you-typed | from the brief; the operator's diagnostic anchor |
| Slicing | ONE slice (migrate + get-tier together) | shared single Display arm; splitting by subcommand would halve an indivisible one-line change |
| Persona | Priya Raman, Platform SRE | reused from existing cinder CLI tests (`migrate_subcommand.rs`, "Priya") |

## DECISIONS FLAGGED FOR DESIGN (Morgan)

1. **Where the fix belongs.** Prefer changing the single
   `MigrateError::UnknownItem` Display arm (`store.rs:57`) to emit the
   quoted bare id over adding a `Display` impl on `ItemId`. Reasons:
   (a) the arm change is the narrowest correct fix — it touches exactly
   the one operator-facing message; (b) a `Display`-on-`ItemId` impl has
   wider blast radius (any future `{item}` site) and, on its own, does
   NOT add the quotes the contract requires (it would emit `ghost`, not
   `"ghost"`). DESIGN owns the exact mechanism and MUST confirm the
   rendered form is QUOTED (`"ghost"`), e.g. `{:?}` on `item.as_str()`
   (a `&str` Debug yields the quoted form) or an explicit quoting format.
   If DESIGN finds a `Display`-impl approach demonstrably cleaner AND
   safe (no quote loss, no blast radius), it may choose it with rationale.
   Precedent: the CLI Error Display already emits a quoted form via
   `{value:?}` on a string (`crates/kaleidoscope-cli/src/lib.rs:107`,
   `invalid tier "warm"`), so `{:?}` on `item.as_str()` (a `&str`) is the
   in-codebase-consistent way to produce `"ghost"`.
   Edge to weigh (peer-review low finding): an item id containing a
   double-quote (e.g. `gh"ost`). `{:?}` on a `&str` escapes it
   (`"gh\"ost"`); a naive explicit quote-wrap would not. DESIGN should
   pick a mechanism whose escaping is consistent and self-documenting —
   `{:?}` on `as_str()` is the safer default. Non-blocking at DISCUSS; the
   contract examples use plain ids.

2. **get-tier sharing.** Confirmed at DISCUSS: get-tier shares the same
   `MigrateError::UnknownItem` arm — no separate get-tier fix. DESIGN
   should re-confirm and ensure the acceptance coverage exercises both
   subcommands through the one arm.

3. **No-regression confirmation.** DESIGN should re-grep `ItemId` render
   sites to confirm store.rs:57 is the sole Display/`{:?}` operator-facing
   render and that no other diagnostic changes. Per-feature mutation
   testing at 100% on the modified line(s) (ADR-0005 Gate 5).

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| A `Display`-on-`ItemId` impl is chosen and silently drops the required quotes | Low | Medium | DISCUSS flags QUOTED-form requirement explicitly; AC asserts the literal `"ghost"` quoted form |
| A hidden second render site exists | Low | Medium | Render-site audit done at DISCUSS (only store.rs:57); DESIGN re-greps |
| Existing substring test masks a wording regression | Low | Low | new AC adds must-contain-quoted-id + must-not-contain-`ItemId(` assertions |

## DIVERGE artifacts

No DIVERGE wave for this fix (small brownfield contract correction).
Recorded as a minor risk: persona and job are grounded in the brief and
existing cinder CLI tests rather than a `job-analysis.md`. Acceptable for
a one-message fidelity fix.

## Constraints carried forward

- Fail-closed exit 1 on unknown item UNCHANGED — only wording changes.
- No OTHER diagnostic or the known-item success path changes.
- Inherits ADR-0005's five gates; per-feature mutation 100% on modified
  lines. Rust idiomatic. AGPL-3.0-or-later. NEVER 1.0.0. No semver
  concern (private Display-arm string; cinder/kaleidoscope-cli not in the
  public-api/semver-pinned set).
