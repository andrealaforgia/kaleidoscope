<!-- markdownlint-disable MD024 -->

# User Stories: cinder-unknown-item-diagnostic-v0

> DISCUSS wave (Luna). One thin slice, contract-fidelity fix. The
> unknown-item diagnostic for both `migrate` and `get-tier` must name the
> id exactly as the operator typed it, quoted (`"ghost"`), matching the
> documented CLI-help contract — instead of leaking the `ItemId` newtype
> Debug form (`ItemId("ghost")`).

## System Constraints

- **Fail-closed behaviour is UNCHANGED.** Unknown item still exits 1.
  Only the wording of the diagnostic changes. No behaviour change.
- **Narrowest correct change.** The only `Display`/format render of the
  `ItemId` newtype across the cinder crate is the
  `MigrateError::UnknownItem` arm at `crates/cinder/src/store.rs:57`
  (`{item:?}`). Every other `ItemId` use is a struct field, trait
  signature, `HashMap` key, test constructor, or the CLI success-line
  rendering which already uses the raw `item_id: &str` from argv (not the
  newtype). The fix must not regress any other diagnostic.
- **No new public-API surface.** A message-string change is not a
  public-API break. Cinder and kaleidoscope-cli are not in the
  semver-pinned set (Gate 2/3 `cargo public-api` / `cargo semver-checks`
  do not flag a private `Display`-arm string). Never 1.0.0.
- **Inherits ADR-0005's five gates.** Per-feature mutation testing at
  100% kill rate scoped to the modified line(s). Rust idiomatic.
- **AGPL-3.0-or-later**, matching the workspace.

---

## US-01: The unknown-item diagnostic names the id I typed, quoted

### Elevator Pitch

- **Before**: `kaleidoscope-cli migrate acme /data ghost warm`
  (and `kaleidoscope-cli get-tier acme /data ghost`) exits 1 with
  `kaleidoscope-cli: cinder migrate: cannot migrate unknown item ItemId("ghost") for tenant acme`
  — the operator reads an internal Rust type name and thinks they hit an
  internal error, not a plain not-found.
- **After**: the same commands exit 1 with
  `kaleidoscope-cli: cinder migrate: cannot migrate unknown item "ghost" for tenant acme`
  — the bare, quoted id they typed, matching the wording the CLI `--help`
  documents.
- **Decision enabled**: the operator instantly recognises this as a
  not-found for the id `ghost` (typo? wrong tenant? never placed?) and
  re-runs with the right id — instead of mistaking it for an internal
  fault and escalating or filing a bug.

### Problem

Priya Raman is an SRE who operates Kaleidoscope's tiering layer from the
`kaleidoscope-cli` binary. When she migrates or queries a Cinder item by
its id and the id does not exist, the error says
`cannot migrate unknown item ItemId("ghost") for tenant acme`. The
`ItemId(...)` newtype wrapper is internal vocabulary that never appears
in any command she typed. She reads it as "the tool broke" rather than
"that id isn't there", so she loses time second-guessing a fault that is
really a simple not-found — and the message contradicts what the CLI
`--help` told her to expect (`"<item_id>"`, bare and quoted).

### Who

- Platform SRE / operator | runs `kaleidoscope-cli migrate` and
  `kaleidoscope-cli get-tier` against Cinder | wants an error that names
  the id she typed so she can tell not-found from internal fault at a
  glance.
- Reads the CLI `--help` as the contract for what messages look like
  (`crates/kaleidoscope-cli/src/main.rs:208`, `:245`).

### Solution

Render the unknown-item diagnostic with the bare, quoted id (`"ghost"`)
instead of the `ItemId` newtype Debug form (`ItemId("ghost")`), for BOTH
the `migrate` and `get-tier` unknown-item paths (they share the single
`MigrateError::UnknownItem` Display arm), so the emitted message matches
the documented CLI-help contract. Exit code and fail-closed behaviour are
unchanged.

### Domain Examples

#### 1: Happy Path (migrate, unknown id) — Priya, `ghost` under `acme`

Priya runs `kaleidoscope-cli migrate acme /var/lib/kal ghost warm`.
No item `ghost` was ever placed under `acme`. The command exits 1 and
stderr reads
`kaleidoscope-cli: cinder migrate: cannot migrate unknown item "ghost" for tenant acme`.
She sees her own id `ghost`, quoted, recognises it as not-found, checks
she meant `ghost-7781`, and re-runs.

#### 2: Edge Case (get-tier, unknown id, composite id) — Priya, `acme/batch-00042`

Priya runs `kaleidoscope-cli get-tier globex /var/lib/kal acme/batch-00042`
— the item was placed under `acme`, not `globex`. The command exits 1 and
stderr reads
`kaleidoscope-cli: cinder migrate: cannot migrate unknown item "acme/batch-00042" for tenant globex`.
The id is reproduced verbatim and quoted (slash and all); she spots the
wrong-tenant mistake immediately.

#### 3: Negative Control (known id, unchanged) — Priya, `blk-7781` under `acme`

`blk-7781` is placed in Hot under `acme`. Priya runs
`kaleidoscope-cli migrate acme /var/lib/kal blk-7781 warm`. The command
exits 0 and stdout reads
`migrated tenant=acme item=blk-7781 from=hot to=warm` — completely
unchanged by this fix. The diagnostic-wording change touches only the
unknown-item error path.

### UAT Scenarios (BDD)

#### Scenario: Unknown item on migrate names the bare quoted id

```gherkin
Scenario: Unknown item on migrate names the bare quoted id
  Given no item "ghost" has been placed under tenant "acme"
  When Priya runs migrate for item "ghost" under tenant "acme"
  Then the command exits non-zero
  And stderr contains: cannot migrate unknown item "ghost" for tenant acme
  And stderr does NOT contain the internal newtype text: ItemId(
```

#### Scenario: Unknown item on get-tier names the bare quoted id

```gherkin
Scenario: Unknown item on get-tier names the bare quoted id
  Given no item "acme/batch-00042" has been placed under tenant "globex"
  When Priya runs get-tier for item "acme/batch-00042" under tenant "globex"
  Then the command exits non-zero
  And stderr contains: cannot migrate unknown item "acme/batch-00042" for tenant globex
  And stderr does NOT contain the internal newtype text: ItemId(
```

#### Scenario: The emitted message matches the CLI-help contract

```gherkin
Scenario: The emitted message matches the CLI-help contract
  Given the CLI help documents the unknown-item wording as
    cannot migrate unknown item "<item_id>" for tenant <tenant>
  When the unknown-item diagnostic is emitted for any id and tenant
  Then the emitted message is that documented shape with <item_id>
    substituted as the bare quoted id and <tenant> as the bare tenant
  And no internal Rust type name appears in the message
```

#### Scenario: Known item migrates and exit codes are unchanged

```gherkin
Scenario: Known item migrates and exit codes are unchanged
  Given item "blk-7781" is placed in Hot under tenant "acme"
  When Priya runs migrate for item "blk-7781" to "warm" under tenant "acme"
  Then the command exits zero
  And stdout reads: migrated tenant=acme item=blk-7781 from=hot to=warm
  And the unknown-item fail-closed exit-1 behaviour is unchanged for
    items that were never placed
```

### Acceptance Criteria

- [ ] `unknown-item-migrate-names-the-bare-quoted-id`: migrate on an
  unplaced id exits non-zero and stderr contains
  `cannot migrate unknown item "ghost" for tenant acme` and does NOT
  contain `ItemId(`. (Scenario 1)
- [ ] `unknown-item-get-tier-names-the-bare-quoted-id`: get-tier on an
  unplaced id exits non-zero and stderr contains
  `cannot migrate unknown item "acme/batch-00042" for tenant globex` and
  does NOT contain `ItemId(`. (Scenario 2)
- [ ] `the-message-matches-the-CLI-help-contract`: the emitted message is
  byte-equal to the documented help shape with the bare quoted id and
  bare tenant substituted; no internal type name leaks. (Scenario 3)
- [ ] `known-item-and-exit-1-behaviour-unchanged`: a known item still
  migrates with the unchanged stdout line and exit 0; unknown items still
  fail closed with exit 1. (Scenario 4)

### Outcome KPIs

- **Who**: Platform SREs operating Cinder via `kaleidoscope-cli`.
- **Does what**: correctly classify an unknown-item failure as a
  not-found (re-run with corrected id) rather than as an internal fault
  (escalate / file bug), on first read of the diagnostic.
- **By how much**: zero internal-type-name leaks in the unknown-item
  diagnostic (1 leak today -> 0); doc-vs-code contract mismatches on this
  message: 1 -> 0.
- **Measured by**: acceptance assertions on stderr wording (must contain
  the quoted bare id, must NOT contain `ItemId(`); the verifier's K18
  expectation (UC-TIER-008/009) flips GREEN.
- **Baseline**: today the message emits `ItemId("ghost")`; K18 is RED;
  CLI help promises `"ghost"` — a confirmed doc-vs-code gap.

### Technical Notes (constraints / dependencies for DESIGN)

- **Locus of the leak (verified)**: `crates/cinder/src/store.rs:57` —
  `"cannot migrate unknown item {item:?} for tenant {tenant}"`. `{item:?}`
  is `Debug` of `ItemId(pub String)` (`crates/cinder/src/tier.rs:51-52`),
  which renders `ItemId("ghost")`.
- **Both paths share one arm (verified)**: `kaleidoscope-cli` `migrate`
  (`crates/kaleidoscope-cli/src/lib.rs:471`) and `get_tier`
  (`crates/kaleidoscope-cli/src/lib.rs:509`) both construct
  `MigrateError::UnknownItem` and surface it via
  `Error::CinderMigrate(e)` Display (`lib.rs:103`), which prepends
  `cinder migrate: `. A single fix at the `MigrateError` Display arm
  covers BOTH subcommands. No per-subcommand fix needed.
- **CLI-help contract (verified)**: `crates/kaleidoscope-cli/src/main.rs:208`
  and `:245` both promise `cannot migrate unknown item "<item_id>" for
  tenant <tenant>`. The fix makes code match help.
- **DESIGN decision — where the fix belongs**: prefer changing the single
  `Display` arm (`store.rs:57`) to emit the quoted bare id (e.g.
  `item.as_str()` quoted, via `{:?}` on the `&str` or an explicit
  `"\"{}\""` form) rather than adding a `Display` impl on `ItemId`. A
  `Display`-on-`ItemId` impl has wider blast radius and would not by
  itself add the quotes the contract requires. The narrowest correct
  change is the arm. DESIGN owns the exact mechanism and must confirm the
  rendered form is the id QUOTED (`"ghost"`), not unquoted (`ghost`).
- **`ItemId::as_str()` exists** (`crates/cinder/src/tier.rs:59`) — the
  clean accessor for the bare id.
- **Render-site audit (done)**: the only newtype `{:?}`/Display render is
  store.rs:57. CLI success lines (`migrate`/`place`) already use the raw
  `item_id` string from argv, so they are unaffected. No other diagnostic
  regresses.
- **Existing tests**: the current
  `migrate_subcommand_unknown_item_exits_nonzero_with_stderr_naming_item`
  only asserts `stderr.contains("ghost-item")` (substring), which is
  satisfied by both old and new wording — it will NOT break, but it also
  did not catch the gap. DISTILL/DELIVER should add the quoted-form and
  no-`ItemId(` assertions (the new AC) to pin the contract.
- **Dependencies**: none external. Self-contained one-message fix.
