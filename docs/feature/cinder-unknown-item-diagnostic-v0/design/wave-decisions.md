# Wave Decisions: cinder-unknown-item-diagnostic-v0 (DESIGN)

> Morgan, DESIGN wave. Autonomous overnight run. Mode: PROPOSE.
> Date: 2026-06-06. A small, well-bounded contract-fidelity fix: one
> `Display` arm, two callers, one slice. The DESIGN is deliberately
> proportionate to the scope — no new ADR, no new topology, no semver
> event.

## Scope recap (from DISCUSS, re-confirmed in code)

The `MigrateError::UnknownItem` `Display` arm leaks the internal
`ItemId` newtype `Debug` form (`ItemId("ghost")`) where the documented
CLI-help contract promises the bare, quoted id (`"ghost"`). Both
`migrate` and `get-tier` route through that single arm. The fix changes
the one arm so code matches the documented contract; fail-closed exit-1
behaviour is unchanged.

## Code re-confirmation (read, not assumed)

| Claim | Locus | Verdict |
|---|---|---|
| The leak: arm renders `{item:?}` (Debug of `ItemId`) | `crates/cinder/src/store.rs:55-58` | **CONFIRMED** — `"cannot migrate unknown item {item:?} for tenant {tenant}"`. |
| `ItemId` is `ItemId(pub String)` with `as_str()` | `crates/cinder/src/tier.rs:51-62` | **CONFIRMED** — newtype derives `Debug`; `as_str()` at :59 returns `&str`. |
| `store.rs:57` is the ONLY `{:?}`/Display render of the `ItemId` newtype in cinder | re-grep `ItemId`/`item:?`/`item}` across `crates/cinder/src` | **CONFIRMED** — every other occurrence is a struct field, trait signature, `HashMap` key, re-export, or a test/bin constructor (`cinder_crash_target.rs:83`). No other operator-facing diagnostic renders the newtype. |
| migrate + get-tier both build `MigrateError::UnknownItem` | `crates/kaleidoscope-cli/src/lib.rs:471` (migrate), `:509` (get_tier) | **CONFIRMED** — identical construction; both surface via `Error::CinderMigrate(e)` Display (`lib.rs:103`), which prepends `cinder migrate: `. |
| In-codebase quoting precedent | `crates/kaleidoscope-cli/src/lib.rs:107` | **CONFIRMED** — `invalid tier {value:?}` renders `invalid tier "warm"` via `{:?}` on a `String`. The same `{:?}`-on-a-string idiom is the established way this codebase produces a quoted token in a diagnostic. |
| CLI-help contract promises the quoted bare id | `crates/kaleidoscope-cli/src/main.rs:208` (migrate), `:245` (get-tier) | **CONFIRMED** — both promise `cannot migrate unknown item "<item_id>" for tenant <tenant>`. The fix makes code match help. |
| Existing substring test stays green and never caught the leak | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs:309-324` | **CONFIRMED** — asserts only `stderr.contains("ghost-item")` and `stderr.contains("unknown item")`, both satisfied by old AND new wording. Notably the test's own comment block (:314-319) already documents the *intended* quoted form `"ghost-item"`, underscoring the doc-vs-code gap. |

## DECISION 1 — The chosen rendering mechanism

**Chosen**: change the single `Display` arm at `crates/cinder/src/store.rs:55-58`
to render `{:?}` applied to `item.as_str()` (i.e. `Debug` of the `&str`),
not `Debug` of the `ItemId` newtype. The format-string id placeholder
becomes the quoted bare id.

Conceptual shape (DESIGN states the contract; the crafter writes the
final source):

- before: `"cannot migrate unknown item {item:?} for tenant {tenant}"`
  where `{item:?}` = `Debug` of `ItemId` → `ItemId("ghost")`.
- after: the id placeholder renders `Debug` of `item.as_str()` (a
  `&str`) → `"ghost"`. The tenant placeholder is unchanged.

**Why this mechanism (three reasons):**

1. **It yields the QUOTED form the contract requires.** `Debug` of a
   `&str` emits the value wrapped in double quotes (`"ghost"`), which is
   byte-equal to the documented help shape. A bare `{}` on `as_str()`
   would emit unquoted `ghost` and miss the contract.
2. **It mirrors the established in-codebase precedent.** `lib.rs:107`
   already produces a quoted token via `{value:?}` on a string. Using
   the same idiom keeps the diagnostic family self-consistent and
   self-documenting — a reader who knows the `invalid tier "warm"` line
   recognises the same shape here.
3. **Its escaping is consistent and self-documenting at the
   double-quote edge.** An id containing a double quote (e.g. `gh"ost`)
   renders under `{:?}`-on-`&str` as `"gh\"ost"` — the quote is escaped,
   so the quoting stays unambiguous. A naive explicit wrap (`"\"{}\""`
   or `format!`-with-literal-quotes) would emit `"gh"ost"` and break the
   visual delimiter. The DISCUSS low-finding edge is therefore handled
   correctly by the chosen mechanism for free. The contract examples use
   plain ids, but choosing the escaping-correct mechanism costs nothing
   and removes the edge entirely.

## DECISION 2 — Alternative evaluated and REJECTED: a `Display` impl on `ItemId`

**Alternative**: add `impl fmt::Display for ItemId` and render the arm
with `{item}` (Display) instead of `{item:?}` (Debug).

**Rejected.** Two disqualifying reasons:

1. **It does not, by itself, satisfy the contract.** A natural `Display`
   for a newtype emits the inner value bare (`ghost`), with no quotes.
   To meet the contract the arm would *still* need to add the quotes
   around `{item}` — so the `Display` impl does not actually carry the
   fix; the arm change does. The impl would be dead weight relative to
   the requirement.
2. **Wider blast radius.** A `Display` impl on `ItemId` becomes callable
   at every present and future `{item}` site across the workspace
   (cinder, the bridges, any downstream crate). That changes the public
   behaviour of a re-exported public type (`cinder::ItemId`,
   `lib.rs:64`) far beyond the one operator-facing message this feature
   touches — the opposite of the narrowest-correct change DISCUSS
   flagged. It would also invite future callers to print ids unquoted,
   re-opening the very class of inconsistency we are closing.

**Verdict**: the single-arm change (Decision 1) is strictly narrower,
fully correct, and contract-faithful. The `Display`-impl path is more
code, broader surface, and still incomplete. Rejected.

## DECISION 3 — get-tier shares the same arm (no separate fix)

**Confirmed.** `get_tier` (`lib.rs:509`) and `migrate` (`lib.rs:471`)
construct the identical `MigrateError::UnknownItem` value and both
render through `Error::CinderMigrate(e)` Display (`lib.rs:103`). The one
arm change at `store.rs:57` fixes both subcommands. No per-subcommand
code change exists or is needed.

**The `cinder migrate:` prefix on a get-tier error is PRE-EXISTING and
OUT of scope.** `Error::CinderMigrate` Display prepends `cinder migrate:`
regardless of which subcommand produced the error; on a `get-tier`
failure this reads slightly oddly but it is the established,
deliberately-consistent wording (`get_tier`'s own doc-comment at
`lib.rs:491-498` states the get-tier UnknownItem stderr is
"byte-identical to the migrate subcommand's UnknownItem path so the
operator stderr experience is consistent"). The verifier's K18
expectation asserts the `cannot migrate unknown item "ghost"` substring,
which holds whether or not the `cinder migrate:` prefix is present.
Changing the prefix is a separate concern (a different message-shape
decision) and is explicitly NOT part of this slice.

## DECISION 4 — No new ADR

**A one-line `Display`-string fidelity fix is NOT architecturally
significant.** It introduces no new component, port, dependency,
quality-attribute trade-off, or public-API surface; it does not change a
boundary or a contract direction — it makes existing code match an
existing documented contract. The relevant prior architectural decisions
(the cinder tiering port, the CLI error-Display composition) already
exist and are unchanged. Per the ADR-when-decided / single-significant-
decision convention, this belongs in wave-decisions.md, not a new ADR.
**No ADR created.** (If a later reviewer disagrees, the bar to clear is
"what new architectural choice would the ADR record?" — there is none;
the choice here is a rendering-fidelity detail wholly contained in one
private `Display` arm.)

## DECISION 5 — No semver bump

**Confirmed: this is not a public-API break.**

- The changed symbol is the *body* of a private `fmt::Display`
  implementation. `Display` output is documentation/behaviour, not API
  signature; no type, trait, or function signature changes.
- `cinder` and `kaleidoscope-cli` are **not** in the Gate 2/3
  semver-pinned set; `cargo public-api` / `cargo semver-checks` do not
  flag a `Display`-arm string change (the public surface of
  `MigrateError` is unchanged — same variants, same fields, same
  `Display`/`Error` impls).
- **No semver bump. NEVER 1.0.0** (per CLAUDE.md and Andrea's standing
  rule; not a decision the agent makes regardless).

## MANDATORY Reuse Analysis

| Candidate | Locus | Disposition | Justification |
|---|---|---|---|
| `MigrateError::UnknownItem` `Display` arm | `crates/cinder/src/store.rs:55-58` | **EXTEND (edit in place)** | The one operator-facing render of the unknown-item diagnostic. The fix is a one-token change inside this existing arm (id placeholder: `Debug`-of-newtype → `Debug`-of-`&str`). No new arm, no new error variant. |
| `ItemId::as_str()` | `crates/cinder/src/tier.rs:59` | **REUSE** | The existing clean accessor that yields the bare `&str`. The chosen mechanism (`{:?}` on `item.as_str()`) consumes it directly; no new accessor needed. |
| `Error::CinderMigrate(e)` Display composition | `crates/kaleidoscope-cli/src/lib.rs:103` | **REUSE UNCHANGED** | Prepends `cinder migrate:` and delegates to `MigrateError`'s Display. Because the fix lives in the delegate, both subcommands inherit it with zero CLI-side change. |
| `{value:?}`-on-a-string quoting precedent | `crates/kaleidoscope-cli/src/lib.rs:107` | **REUSE as pattern** | Establishes `{:?}`-on-a-string as the in-codebase idiom for a quoted diagnostic token; the fix follows it, keeping the diagnostic family consistent. |
| `Display` impl on `ItemId` | (would be new) | **CREATE — REJECTED** | See Decision 2: wider blast radius on a public type, and does not by itself satisfy the quoting contract. |
| New error variant / new function / new module | — | **CREATE — NOT NEEDED** | The fix is fully contained in one existing arm reusing one existing accessor. **Nothing new is created.** |

**Reuse verdict**: this feature EXTENDS one existing `Display` arm,
REUSES `ItemId::as_str()` and the existing CLI Display composition, and
creates NOTHING new. Strictly the narrowest correct change.

## Test seam (for DISTILL / acceptance-designer)

A CLI **subprocess** test (the established `migrate_subcommand.rs`
black-box style — spawn the built binary, assert on stderr/stdout/exit),
exercising **both** subcommands through the shared arm:

1. **migrate, unknown id** — spawn `migrate <tenant> <data_dir> ghost warm`
   on a tenant with no item `ghost`. Assert:
   - exit code is non-zero (exit 1 — fail-closed UNCHANGED);
   - stderr **contains** `cannot migrate unknown item "ghost" for tenant <tenant>`
     (the quoted bare id — pins the contract);
   - stderr does **NOT** contain `ItemId(` (the leak is gone);
   - stdout is empty.
2. **get-tier, unknown id (composite id)** — spawn
   `get-tier <tenant> <data_dir> acme/batch-00042` for a tenant that
   never placed it. Assert:
   - exit non-zero;
   - stderr **contains** `cannot migrate unknown item "acme/batch-00042" for tenant <tenant>`
     (verbatim, slash preserved, quoted);
   - stderr does **NOT** contain `ItemId(`.
   This proves get-tier is covered by the same arm with no separate fix.
3. **Known-item control (unchanged)** — place `blk-7781` in Hot under a
   tenant, then `migrate <tenant> <data_dir> blk-7781 warm`. Assert exit
   0 and stdout `migrated tenant=<t> item=blk-7781 from=hot to=warm`.
   The success path is untouched by this fix.
4. **Exit-1-unchanged** — covered by the non-zero assertion in (1)/(2):
   unknown item still fails closed; only the wording changed.

**Existing test note**: `migrate_subcommand.rs:309-324` (the substring
`ghost-item` / `unknown item` assertions) stays GREEN under the new
wording (the substrings still appear). The NEW assertion that *pins* the
fix is the **must-contain quoted form** + **must-NOT-contain `ItemId(`**
pair — that is the assertion the existing test lacked and that catches
any future regression to the newtype-Debug form. DISTILL should add the
two new assertions (the AC) rather than weaken the existing one.

**Mutation note (DEVOPS/DELIVER)**: per-feature mutation at 100% kill
rate (ADR-0005 Gate 5) scoped to the single modified line in
`store.rs:57`. The quoted-form + no-`ItemId(` assertion pair is what
kills the mutant that reverts the placeholder to `{item:?}`.

## Constraints (carried forward, unchanged)

- Fail-closed exit-1 on unknown item is UNCHANGED — only wording changes.
- No OTHER diagnostic and no success path changes (render-site audit:
  `store.rs:57` is the sole newtype render).
- Rust idiomatic (data + free functions + traits; the change is inside an
  existing trait-impl method body).
- Inherits ADR-0005's five gates; per-feature mutation 100% on the
  modified line.
- AGPL-3.0-or-later, matching the workspace.
- NEVER 1.0.0; no semver bump (Decision 5).

## Upstream Changes

**None.** No upstream crate, port, trait, or config changes. The fix is
contained in one existing `Display` arm in `crates/cinder/src` and is
inherited by `kaleidoscope-cli` with zero CLI-side edits. No new
dependency, no new task, no new crate.

## C4 / topology note

**No C4 diagram produced — intentionally, proportionate to scope.** A
one-arm `Display`-string fidelity fix introduces no new container, no new
component, no new external system, no new data store, and no new
relationship. The cinder tiering port and the CLI error-Display
composition already exist and are documented in the brief's cinder
sections; this feature changes the *text inside one existing edge*, not
the topology. Adding a diagram would misrepresent a message fix as a
structural change. The sequence is fully captured by the prose:
`CLI subcommand → MigrateError::UnknownItem → Error::CinderMigrate
Display → stderr`, with only the rendered id token changing.
