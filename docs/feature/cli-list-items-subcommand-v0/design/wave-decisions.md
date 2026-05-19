# Wave Decisions — `cli-list-items-subcommand-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE. Paradigm: Rust idiomatic.

This wave locks the four open architectural questions handed
over by DISCUSS (D-FunctionShape, D-StderrSummary,
`parse_tier` visibility, D-StderrWording) plus one additional
DD covering the binary-side dispatcher.

Inputs read:

- `docs/feature/cli-list-items-subcommand-v0/discuss/wave-decisions.md`
- `docs/feature/cli-list-items-subcommand-v0/discuss/user-stories.md`
- `docs/feature/cli-list-items-subcommand-v0/discuss/outcome-kpis.md`
- `docs/feature/cli-list-items-subcommand-v0/discuss/slices/slice-01-list-items-subcommand-enumerates-tier.md`
- `crates/kaleidoscope-cli/src/lib.rs` (the `migrate(...)` shape at lines 424-467; private `parse_tier` at lines 475-482; `tier_lowercase` at lines 489-495; `Error` enum at lines 72-88; `cinder_base` at lines 130-132)
- `crates/cinder/src/store.rs` (the `TieringStore::list_by_tier` port at line 102; the `InMemoryTieringStore::list_by_tier` adapter at lines 190-198 confirming `HashMap` iteration randomness)

## DD1 — Library function signature

**Decision.** Add one new public free function to
`crates/kaleidoscope-cli/src/lib.rs`:

```text
pub fn list_items(
    tenant: &TenantId,
    data_dir: &Path,
    tier_arg: &str,
    mut writer: impl Write,
) -> Result<(), Error>
```

Returns `Result<(), Error>` (NOT `Result<usize, Error>`). The
function body performs: parse → open → list_by_tier → sort →
write loop. On any failure branch it returns one of the
existing `Error` variants (`InvalidTier`, `CinderOpen`, `Io`).
No item count is returned to the caller; the binary-side
`run_list_items` helper does not need it (DD3 locks no stderr
summary).

**Rationale.** Mirrors the `migrate(...)` shape at
`crates/kaleidoscope-cli/src/lib.rs:424-467` minus two things:
(a) no `item_id` positional argument; (b) no `otlp_log_path`
parameter (D-OutOfScope-Observe). Returns unit (not `usize`)
because no caller consumes the count: the binary writes
exactly the stdout lines and nothing else (no stderr summary
per DD3). Returning `usize` would create an unused result
warning at the call site or force a `let _ = ...` discard with
zero observable benefit.

**Alternatives considered and rejected.**

- `Result<usize, Error>` returning the line count. Rejected:
  enables the stderr-summary option (DD3 path (a)) but DD3
  locks path (b). Adding a return value that no caller reads
  is overspecification.
- Typed `ListItemsReport { items: Vec<ItemId> }` returned to
  the caller. Rejected: premature abstraction (stdout is the
  only consumer); doubles the allocation (one for the Vec, one
  for the writeln strings); would force the binary-side
  caller to do the `writeln!` loop, contradicting the existing
  `migrate(...)` precedent where the library function owns
  the writer.
- In-`main.rs`-only shape (no library function, body inlined
  in `run_list_items`). Rejected: breaks the in-process
  acceptance-test pattern used by all seven predecessor
  `tests/*.rs` files in the cluster. The test cannot call a
  private helper inside `main.rs`.

## DD2 — Sorting strategy

**Decision.** After `cinder.list_by_tier(&tenant, tier)`
returns its `Vec<ItemId>`, sort in place with
`Vec::sort_unstable()`. `ItemId` implements `Ord` over its
inner `String`, giving lexicographic byte-order sort. Then
loop over the sorted Vec writing one `writeln!(writer, "{}",
id.0)` per element.

**Rationale.** `cinder::InMemoryTieringStore::list_by_tier` at
`crates/cinder/src/store.rs:190-198` iterates a
`HashMap<(TenantId, ItemId), TierEntry>` with randomised
iteration order per process. Without a boundary sort the
operator's `kaleidoscope-cli list-items ... | diff -
expected.txt` would spuriously fail. D-Sort in DISCUSS
required `Vec::sort()` or equivalent; this DD locks
`sort_unstable()` specifically because (a) `ItemId` is owned
and there are no equal-by-key duplicate keys to preserve
(each `ItemId` in the returned Vec is unique by Cinder
invariant), so stable vs unstable is observationally
indistinguishable; (b) `sort_unstable` is the conventional
choice for Vecs without duplicate-key ordering concerns and
is slightly faster.

**Alternatives considered and rejected.**

- `BTreeSet<ItemId>` collected at the call site. Rejected:
  `cinder::TieringStore::list_by_tier` returns `Vec<ItemId>`,
  so a `BTreeSet` would allocate twice (the returned Vec
  plus the BTreeSet) for the same result. The Vec is
  already in hand; sort it.
- `Vec::sort()` (stable). Rejected: `ItemId` values in the
  returned Vec are unique by Cinder invariant (the underlying
  key is `(TenantId, ItemId)`, and we filter by `tenant`,
  yielding distinct `ItemId`s). Stable sort gives no
  observable benefit over unstable here. Picked
  `sort_unstable` for the marginal speed win and to signal
  intent ("ordering by id is total; no equal-key ties").

## DD3 — Stderr summary line: NONE

**Decision.** `run_list_items` in `main.rs` emits NO stderr
summary line on success. Stderr is empty for the happy path
(it remains the failure-path channel exclusively, matching
`migrate`'s posture). On any error variant the existing
`main.rs:71`-shape `eprintln!("kaleidoscope-cli: {e}")` line
fires and the process exits non-zero.

**Rationale.** The natural consumer is a shell pipeline
(`xargs -I {} kaleidoscope-cli migrate ... {} ...`). A
stderr summary line (`list-items ok: items=N`) is harmless on
stdout but adds noise to operator terminals when the pipeline
runs interactively, and it would tempt a future change to
emit a `--quiet` flag. Keeping stderr empty on success
preserves the "stdout = data, stderr = diagnostic" Unix
convention with zero ambiguity. DISCUSS handed both options
to DESIGN as valid; this DD locks (b).

**Alternatives considered and rejected.**

- Emit `list-items ok: items=N` mirroring `stats ok: records=N`
  at `crates/kaleidoscope-cli/src/main.rs:261`. Rejected:
  `stats` emits a one-record summary because its STDOUT IS
  the summary; the stderr line is a redundant audit echo.
  `list-items` STDOUT is N records; the count is trivially
  `wc -l < stdout`. Adding a stderr summary duplicates
  observable information.
- Emit a stderr summary only when stdout is a TTY (smart
  shell detection). Rejected: introduces conditional stderr
  shape — operator scripts cannot rely on stderr being
  empty across environments. Worse contract than either
  always-on or always-off.

## DD4 — `parse_tier` visibility: promote to `pub(crate)`

**Decision.** Promote the existing private `parse_tier(s:
&str) -> Result<Tier, ()>` at
`crates/kaleidoscope-cli/src/lib.rs:475-482` to
`pub(crate)`. The new `list_items` function calls it directly
as `parse_tier(tier_arg).map_err(|_| Error::InvalidTier {
value: tier_arg.to_string() })?` — identical lift pattern to
`migrate()`'s line 432-434.

**Rationale.** The four-line `match s { "hot" => ..., "warm"
=> ..., "cold" => ..., _ => Err(()) }` body is the
authoritative literal-match contract for the entire crate.
Duplicating it inline in `list_items` would create two sites
that must stay in sync if a tier is ever added (e.g. a
hypothetical `archive` tier). One site, two callers, zero
behavioural change at the existing call site. The promotion
is the smallest possible visibility change (`pub(crate)`, not
`pub`) — no public API growth.

**Alternatives considered and rejected.**

- Duplicate the four-line `match` inline in `list_items`.
  Rejected: rule-of-two duplication of a literal-match
  contract; the next CLI subcommand that takes a tier
  argument would make it rule-of-three. Promote now.
- Promote to `pub`. Rejected: no out-of-crate caller exists
  or is foreseen. `pub(crate)` is the minimal exposure.

## DD5 — Stderr wording on invalid tier: reuse existing `Display` verbatim

**Decision.** The OK3 (invalid-tier) failure path reuses the
existing `Error::InvalidTier { value }` `Display`
implementation at
`crates/kaleidoscope-cli/src/lib.rs:98-100` unchanged. The
exact byte-level stderr line is:

```text
kaleidoscope-cli: invalid tier "<value>": expected one of hot, warm, cold
```

The `kaleidoscope-cli: ` prefix comes from the existing
`main.rs:71` `eprintln!` wrapper; the rest comes from the
`Display` impl unmodified. The acceptance test asserts the
SUBSTRING invariant (the verbatim invalid value appears in
the line), which this wording satisfies.

**Rationale.** The `migrate` subcommand already emits this
exact wording for the same `Error::InvalidTier` variant. Two
subcommands rejecting the same invalid tier value with two
different stderr lines would be operationally hostile. The
substring contract (OK3) is satisfied; the byte-level
identity with `migrate`'s OK3 line is a free bonus for
operator muscle memory.

**Alternatives considered and rejected.**

- New `Display` impl variant or wrapper specific to
  `list-items` (e.g. `list-items: bad tier "<v>"`). Rejected:
  multiplies wording surface area, breaks symmetry with
  `migrate`, requires a new `Display` arm or wrapper for no
  user benefit.
- Strip the `expected one of ...` suffix to keep the line
  shorter. Rejected: the suffix is the actionable hint
  ("here's what's valid"); removing it makes the error less
  helpful.

## Open architectural questions: NONE remain

DISCUSS handed over four open questions (D-FunctionShape,
D-StderrSummary, `parse_tier` visibility, D-StderrWording).
All four are locked above (DD1, DD3, DD4, DD5). DD2 covers
the sort-strategy refinement (D-Sort in DISCUSS already
required a boundary sort; DD2 picks `sort_unstable`).

## C4 view

See `docs/feature/cli-list-items-subcommand-v0/design/application-architecture.md`
for L1 + L2 diagrams. L3 not produced — four-step linear
flow (parse → open → list_by_tier → sort → writeln loop)
with no branch fan-out beyond the three error variants
reused from the existing `Error` enum. Reification condition
for L3: addition of a second tier-enumeration shape
(e.g. cross-tenant aggregation per D-OutOfScope-CrossTenant
reversal) that creates a real component boundary inside
the function.

## Reuse verdict

**REUSE** (eleven existing constructs):

- `cinder_base(data_dir)` at `lib.rs:130-132` — the Cinder
  WAL+snapshot path resolver.
- `FileBackedTieringStore::open(...)` — same construction
  pattern as `migrate()` line 446 and `stats_with_tiers`.
- `cinder::NoopRecorder` aliased as `CinderRecorder` — the
  quiescent recorder. No OTLP file (D-OutOfScope-Observe).
- `TieringStore::list_by_tier(&tenant, tier)` at
  `cinder/src/store.rs:102` — the read-only port already
  used by `stats_with_tiers` at `lib.rs:383`.
- `ItemId` and its `Ord` impl for the lexicographic sort.
- `Tier` and `parse_tier` (DD4 promotes the latter to
  `pub(crate)`).
- `Error::InvalidTier { value }` (lib.rs:79-81; Display at
  98-100 — DD5 reuses verbatim).
- `Error::CinderOpen(MigrateError)` (lib.rs:77) — propagated
  on store-open failure.
- `Error::Io(std::io::Error)` (lib.rs:82) — propagated on
  `writeln!` failure via the existing `From<io::Error>` impl
  at lib.rs:112-116.
- `TenantId` from `aegis`.
- The in-process test harness shape from the seven
  predecessor `tests/*.rs` files.

**CREATE NEW** (four things):

- One public free function `list_items(...)` in `lib.rs`
  (~15 lines).
- One binary-side `run_list_items` helper in `main.rs`
  (~10 lines) parallel to `run_migrate`.
- One new dispatch arm `Some("list-items") => ...` in
  `main.rs`'s match block (1 line).
- One new usage paragraph in `print_usage`/`write_usage` in
  `main.rs` (~3 lines).

**No new public type. No new trait. No new module. No new
external crate. No new `Error` variant.** This is strictly
thinner than the `cli-migrate-subcommand-v0` predecessor
which introduced two new `Error` variants (`InvalidTier`
itself, and `CinderMigrate`).

Change surface: two files in `src/` (`lib.rs`, `main.rs`),
one new test file
(`tests/list_items_subcommand.rs`), one new `[[test]]`
block in `Cargo.toml`.

## DEVOPS handoff (`@nw-platform-architect`)

- **External integrations: NONE.** No HTTP, no webhook, no
  third-party API, no vendor SDK. Pure local Cinder WAL read.
  No contract tests recommended.
- **ADR-0005 gates apply unchanged.** No new/amended gate
  needed. `gate-5-mutants-kaleidoscope-cli` auto-covers via
  `--in-diff` against the modified `lib.rs` and `main.rs`.
- **Cargo delta**: one new `[[test]]` block (`name =
  "list_items_subcommand"`) under
  `crates/kaleidoscope-cli/Cargo.toml`. **No new
  `[dependencies]`**.
- **Mutation scope**:
  `crates/kaleidoscope-cli/src/{lib,main}.rs` at 100% kill
  rate per ADR-0005 Gate 5.
- **DELIVER paradigm**: Rust idiomatic. One new public free
  function, one binary-side helper, one promoted visibility
  on a private helper (`parse_tier` → `pub(crate)`), no new
  trait, no new `dyn` boundary.
- **Quality attributes**: Functional Suitability (OK1
  stdout shape, OK2 N=0 empty stdout); Reliability (OK3
  invalid-tier fail-fast, D-NoLumenTouch byte-equivalence,
  D-ReadOnly byte-equivalence on Cinder); Maintainability
  (~30 new production source lines, two files);
  Compatibility (eight locked acceptance test files continue
  to pass green UNMODIFIED).

## Acceptance designer handoff (`@nw-acceptance-designer`)

Translate US-01's AC and OK1..OK3 into `#[test]` functions
under `crates/kaleidoscope-cli/tests/list_items_subcommand.rs`
per `discuss/slices/slice-01-list-items-subcommand-enumerates-tier.md`.
Harness mirrors `tests/migrate_subcommand.rs` (inline
`tenant` / `record` / `temp_root` / `cleanup` / `ndjson`
helpers; rule-of-three extraction deferred per
DISCUSS D-NewTestFile). Eighth `tests/*.rs` in the cluster
using the same harness shape.

The OK3 substring invariant: the stderr line must contain
the verbatim invalid tier value (DD5 reuses the existing
`Error::InvalidTier` `Display` wording — the substring test
already passes against the `migrate` site's stderr; the
same assertion shape is portable to `list-items`).
