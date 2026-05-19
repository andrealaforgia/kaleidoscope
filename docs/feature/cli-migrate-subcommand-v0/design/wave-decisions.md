# Wave Decisions — `cli-migrate-subcommand-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE. DISCUSS named two decisions for DESIGN to lock —
D-FunctionShape (signature of the new library function) and
D-ErrorVariant (naming of the new error variants). DISCUSS also
deferred D-StderrWording and the exact tier-parser shape under
D-LowerCase to DESIGN. The remaining surface is heavily constrained
by precedent: `stats_with_tiers()` (`crates/kaleidoscope-cli/src/lib.rs:349-382`)
sets the free-function shape; `ingest()`'s no-flag arm
(`crates/kaleidoscope-cli/src/lib.rs:168-180`) sets the quiescent
Cinder-recorder pattern; `tier_lowercase` (`crates/kaleidoscope-cli/src/lib.rs:389-395`)
sets the render direction whose inverse this feature introduces on
the parse side.

Scope inherited from DISCUSS (locked, not re-litigated): one new
subcommand `migrate`; one new free function in `lib.rs`; one
modified `main.rs` dispatcher (new arm + `run_migrate` helper +
usage text); one new acceptance test file
`tests/migrate_subcommand.rs`; quiescent `NoopRecorder` on the
Cinder side; `SystemTime::now()` at the call site (D-Timestamp);
Cinder-only open, no Lumen touch (D-NoLumenTouch); lower-case-only
tier argument (D-LowerCase); no `--observe-otlp`, no `--dry-run`,
no `--at`, no `--format=...`, no bulk migration; faithful to the
underlying idempotent same-tier API (D-Idempotent); locked test
files NOT modified (D-LockedTests).

---

## DD1: Function shape — Option A, returning `Result<(), Error>` with writer parameter

**Decision**: Add a new free function

```rust
pub fn migrate(
    tenant: &TenantId,
    data_dir: &Path,
    item_id: &str,
    to_tier_arg: &str,
    mut writer: impl Write,
) -> Result<(), Error>
```

in `crates/kaleidoscope-cli/src/lib.rs`. The function:

1. Parses `to_tier_arg` via the new private helper `parse_tier(s)`
   (DD3). On parse failure, returns `Err(Error::InvalidTier { value })`
   BEFORE opening any store.
2. Opens `FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))`,
   mapping open failure to `Err(Error::CinderOpen(_))`.
3. Calls `cinder.get_entry(tenant, &ItemId::new(item_id))` to read
   the `from` tier. If `None`, returns
   `Err(Error::CinderMigrate(MigrateError::UnknownItem { tenant: tenant.clone(), item: ItemId::new(item_id) }))`
   WITHOUT issuing the `migrate` call (no `place`, no mutation).
4. Calls `cinder.migrate(tenant, &ItemId::new(item_id), to_tier, SystemTime::now())`.
   Any `Err(MigrateError::*)` returned is lifted via
   `Err(Error::CinderMigrate(_))`. The `UnknownItem` case is
   structurally unreachable because step 3 already proved the entry
   exists, but the lift remains for completeness (covers
   `PersistenceFailed`).
5. Writes one literal line to `writer`:
   `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`,
   using `tier_lowercase` for both `<from>` and `<to>`.
6. Flushes `writer` and returns `Ok(())`.

The Lumen store is NEVER opened (D-NoLumenTouch). The Cinder
recorder is `NoopRecorder` (quiescent, D-OutOfScope-Observe).

**Function name choice**: `migrate`, parallel to `ingest`, `read`,
`stats`, `stats_with_tiers`. The free function is the fifth sibling
on the library's public surface.

**Rationale**:

1. **Option A (return `Result<(), Error>` with writer parameter)
   over Option B (keep entirely in `main.rs`)**. The
   `kaleidoscope-cli` library / binary split has held for five
   features in a row: `ingest`, `read`, `stats`,
   `stats_with_tiers`, and now `migrate`. Each library function
   takes a `writer: impl Write` so the acceptance tests can pipe
   `Vec<u8>` buffers in and assert the exact bytes emitted. Keeping
   `migrate` in `main.rs` would force the new acceptance test to
   spawn a subprocess (the five locked `tests/*.rs` files
   demonstrate the in-process pattern is the established norm) and
   would also leave the Cinder open / `get_entry` / `migrate`
   composition untested at the library boundary. **Option A is the
   precedent-consistent choice.**

2. **Returning `Result<(), Error>` rather than
   `Result<MigrateReport, Error>` with a typed struct**. The
   stdout bytes are the only consumer of the from/to information.
   No library caller asks for the report structure; no test
   destructures it (the acceptance test reads the captured stdout
   buffer and asserts on its bytes). A typed `MigrateReport` would
   be a premature abstraction — same argument as DD5 in
   `cli-stats-cinder-tier-distribution-v0` rejecting a
   `TierCounts` struct. **The writer parameter is the contract.**

3. **`item_id: &str` and `to_tier_arg: &str` rather than typed
   `ItemId` and `Tier`**. The function sits at the parse boundary;
   its job INCLUDES validating the raw argument strings. Passing
   typed `ItemId` and `Tier` would shift the parse responsibility
   to the caller (`main.rs`), splitting it across the binary /
   library boundary and complicating the test harness (which would
   need to construct typed values and lose coverage of the parse
   path). The `&str` shape mirrors how `parse_iso8601_utc_nanos`
   accepts a raw string at the boundary in the time-range features.

4. **Pre-flight `get_entry` to discover the `from` tier**. The
   `TieringStore::migrate(tenant, item, to_tier, migrated_at) -> Result<(), MigrateError>`
   signature at `crates/cinder/src/store.rs:93-99` returns `()` on
   success — it does NOT return the previous tier. To populate the
   `from=<from>` field of the stdout report, the function MUST
   read the entry before issuing the migrate call. The
   `get_entry(tenant, item) -> Option<TierEntry>` trait method at
   `crates/cinder/src/store.rs:89` is the canonical read API. Two
   calls (one read, one mutate) is the minimum honest shape.

5. **Race window between `get_entry` and `migrate`**. In a
   hypothetical multi-process world, another writer could mutate
   the entry between the two calls; the reported `from` would be
   stale. For v0 this is documented and accepted: the CLI is
   single-process (one operator at the terminal), the
   `FileBackedTieringStore` is opened twice across processes only
   if the operator runs two `migrate` commands concurrently
   (operationally unusual; no contract is violated — the second
   would see the post-first state via its own `get_entry`). A
   future multi-writer feature would need transactional semantics
   in the underlying store; v0 does not.

**Rejected alternative — Option B (keep entirely in `main.rs`)**:
breaks precedent set by five sibling features; forces the
acceptance test to spawn a subprocess (deviating from the
established in-process harness pattern); leaves the
open/get_entry/migrate composition untested at the library
boundary. **Rejected.**

**Rejected alternative — return a typed
`MigrateReport { from: Tier, to: Tier }` struct**: premature
abstraction (zero library callers consume the structure; stdout is
the only consumer). The function still has to render the bytes; if
the renderer moves into `main.rs` then the library function does
not own its output contract and acceptance testing fragments
across the boundary. **Rejected.**

**Rejected alternative — accept typed `ItemId` and `Tier`
arguments**: shifts parse responsibility to `main.rs`, splits
parse logic across the library / binary boundary, and removes the
acceptance test's ability to exercise the parse-failure paths
through the library entry point. **Rejected.**

---

## DD2: Pre-flight `get_entry` ordering and same-tier idempotence

**Decision**: The function calls `get_entry` BEFORE `migrate`, in
that strict order. The pseudocode:

```rust
let item = ItemId::new(item_id);
let from = cinder
    .get_entry(tenant, &item)
    .ok_or_else(|| Error::CinderMigrate(MigrateError::UnknownItem {
        tenant: tenant.clone(),
        item: item.clone(),
    }))?
    .tier;
cinder
    .migrate(tenant, &item, to_tier, SystemTime::now())
    .map_err(Error::CinderMigrate)?;
writeln!(writer, "migrated tenant={} item={} from={} to={}",
    tenant.0, item_id, tier_lowercase(from), tier_lowercase(to_tier))?;
writer.flush()?;
Ok(())
```

For the same-tier case (`from == to_tier`), the function writes
the line faithfully (`from=cold to=cold`) and exits Ok. NO
special case. The underlying `InMemoryTieringStore::migrate` is
idempotent for same-tier (`crates/cinder/src/store.rs:167-188`);
the file-backed adapter is expected to mirror this.

**Rationale**:

1. **`get_entry` returning `None` is the unknown-item discriminator
   on the read side.** A subsequent `migrate` against the same
   item would return `MigrateError::UnknownItem`, but issuing it
   would (a) be a wasted round-trip and (b) muddy the reporting
   pipeline — the function would have to discard a `()` success
   path AND the `Err(MigrateError)` path while also discarding the
   value returned by `get_entry`. Doing the discrimination on
   `get_entry` short-circuits cleanly.

2. **No `place`-on-missing.** The function never calls
   `TieringStore::place`; an unknown item id is reported as an
   error, not silently inserted. This is the OK2 fail-fast
   contract verbatim.

3. **Idempotent same-tier is documented behaviour, not a special
   case.** D-Idempotent in DISCUSS locks the contract: the CLI is
   faithful to the underlying API; the API is idempotent for
   same-tier; the CLI reports `from=X to=X` and exits 0. The
   `migrated_at` field is bumped (per the in-memory adapter's
   line 185); the wire-observable invariant is the stdout report
   + the `get_entry` result, both of which are correct.

4. **The pre-flight `get_entry` plus the post-migrate `migrate`
   call together discharge the OK1 oracle**: the test harness can
   call `cinder.get_entry(tenant, item)` AFTER the library call
   to assert that `entry.tier == to_tier`. This is the principal
   KPI verification path.

---

## DD3: Tier parser — `parse_tier(s: &str) -> Result<Tier, ()>` accepting lower-case only

**Decision**: Add a private free function

```rust
fn parse_tier(s: &str) -> Result<Tier, ()> {
    match s {
        "hot" => Ok(Tier::Hot),
        "warm" => Ok(Tier::Warm),
        "cold" => Ok(Tier::Cold),
        _ => Err(()),
    }
}
```

in `crates/kaleidoscope-cli/src/lib.rs`. The unit `Err(())` carries
no payload; the caller (the new `migrate` function) materialises
the parse failure as `Error::InvalidTier { value: to_tier_arg.to_string() }`
preserving the verbatim invalid input for the OK3 stderr contract.

**Rationale**:

1. **Three accepted spellings, exact match, no normalisation.**
   `s.eq_ignore_ascii_case("hot")` would let `HOT` / `Hot` / `hOt`
   through; D-LowerCase in DISCUSS forbids that. Bare `match`
   on the three literal lower-case strings IS the contract. The
   inverse of `tier_lowercase` (`crates/kaleidoscope-cli/src/lib.rs:389-395`),
   pinning the read/write asymmetry at zero — the renderer emits
   lower-case; the parser accepts lower-case.

2. **No leading/trailing whitespace trim.** The acceptance test
   under OK3 verifies that `" hot"` (leading space) and `"hot "`
   (trailing space) are rejected. A `trim()` call would silently
   accept both, breaking the OK3 invariant. The literal match
   has no trim.

3. **`Result<Tier, ()>` over `Option<Tier>` or `Result<Tier, &str>`**.
   The parser is a private helper; the caller does not destructure
   the error variant (it materialises `Error::InvalidTier` from
   the original `&str` argument, not from the parser's return).
   `Result<_, ()>` over `Option<_>` because the call-site reads
   more naturally with `.map_err(|()| ...)` than with
   `.ok_or_else(...)`. `Result<Tier, &str>` (carrying a borrow of
   the input) would constrain lifetimes for no caller benefit.

4. **Empty string, typos, mixed-case, upper-case all flow through
   the same `_ => Err(())` arm.** One source of rejection, one
   wire-observable failure mode.

**Rejected alternative — return `Option<Tier>`**: the call site
prefers `.map_err(|()| Error::InvalidTier { value })` over
`.ok_or_else(|| Error::InvalidTier { value })`; both work but
`Result<_, ()>` reads as "the parser explicitly signals failure"
rather than "the parser returned nothing." Minor style choice;
**rejected on consistency with the typed-error idiom used
elsewhere in the crate** (`parse_iso8601_utc_nanos` returns
`Result<u64, IsoParseError>`, not `Option<u64>`).

**Rejected alternative — accept upper-case and normalise via
`to_ascii_lowercase()`**: D-LowerCase in DISCUSS explicitly
forbids any spelling other than the three lower-case literals.
**Rejected.**

---

## DD4: Error variants — add TWO new variants (`InvalidTier`, `CinderMigrate`)

**Decision**: Add exactly two new variants to
`kaleidoscope_cli::Error` at `crates/kaleidoscope-cli/src/lib.rs:72-84`:

```rust
pub enum Error {
    // ... existing variants ...
    /// Invalid `<to_tier>` argument: any spelling other than the
    /// three accepted lower-case literals (`hot`, `warm`, `cold`).
    /// Carries the verbatim invalid input so the binary's
    /// `eprintln!("kaleidoscope-cli: {e}")` line shows the bytes
    /// the operator typed.
    InvalidTier { value: String },
    /// `TieringStore::migrate` (or the pre-flight `get_entry`
    /// unknown-item check) failed. Distinct from `CinderOpen`,
    /// which fires only when `FileBackedTieringStore::open` itself
    /// failed.
    CinderMigrate(MigrateError),
}
```

with matching `Display` impls:

```rust
Error::InvalidTier { value } =>
    write!(f, "<to_tier> {value:?}: expected one of hot, warm, cold"),
Error::CinderMigrate(e) =>
    write!(f, "cinder migrate: {e}"),
```

**Rationale**:

1. **`Error::InvalidTier { value }` is mandatory.** DISCUSS
   D-ErrorVariant explicitly lists it as the new variant for the
   parse-side fail-fast. The `value` field carries the verbatim
   invalid input so the binary's existing
   `eprintln!("kaleidoscope-cli: {e}")` line at
   `crates/kaleidoscope-cli/src/main.rs:69` shows BOTH the
   binary prefix AND the verbatim bad value, satisfying the OK3
   substring invariant ("stderr contains the verbatim invalid
   value"). The `{value:?}` debug formatting wraps the value in
   quotes so trailing whitespace is visible (the predecessor
   `parse_flag_iso` at `crates/kaleidoscope-cli/src/main.rs:219`
   uses the same `{value:?}` shape).

2. **`Error::CinderMigrate(MigrateError)` is DISCUSS's recommended
   (b) option, taken here.** DISCUSS D-ErrorVariant offered
   reusing `Error::CinderOpen(MigrateError)` (semantically loose
   but byte-equivalent on the wire) or introducing a new variant
   for "cleaner typing." The new variant is the more honest shape:
   the existing `Error::CinderOpen` already absorbs
   `FileBackedTieringStore::open` failures via the
   `.map_err(Error::CinderOpen)` site at
   `crates/kaleidoscope-cli/src/lib.rs:179-180` and the parallel
   site at `crates/kaleidoscope-cli/src/lib.rs:369-370`; treating
   "open failed" and "migrate failed" as the same variant would
   mean a future log analyser cannot distinguish the two. The new
   variant separates them cleanly.

3. **The `Display` impl for `Error::CinderMigrate(MigrateError::UnknownItem { tenant, item })`
   composes via `MigrateError`'s own `Display`** at
   `crates/cinder/src/store.rs:55-58` which renders
   `cannot migrate unknown item "<item.0>" for tenant <tenant>`.
   The complete stderr line then becomes
   `kaleidoscope-cli: cinder migrate: cannot migrate unknown item "acme/batch-00099" for tenant acme`.
   Contains the verbatim item id (OK2 invariant). Contains the
   prefix `cinder migrate:` so the operator can grep for the
   subcommand.

4. **The `Display` impl for `Error::InvalidTier { value: "HOT".to_string() }`**
   renders `<to_tier> "HOT": expected one of hot, warm, cold`,
   composed with the binary prefix becomes
   `kaleidoscope-cli: <to_tier> "HOT": expected one of hot, warm, cold`.
   Contains the verbatim invalid value (OK3 invariant). The
   `<to_tier>` token names the positional argument (mirroring
   `--since` / `--until` naming at
   `crates/kaleidoscope-cli/src/main.rs:219` for flag arguments).

5. **No further variants needed.** The
   `FileBackedTieringStore::open` failure mode is already covered
   by the existing `Error::CinderOpen` variant. The
   `writeln!(writer, ...)?` and `writer.flush()?` failure modes
   are covered by the existing `Error::Io(std::io::Error)` variant
   plus `From<std::io::Error> for Error` at
   `crates/kaleidoscope-cli/src/lib.rs:104-108`.

**Rejected alternative — reuse `Error::CinderOpen(MigrateError)`
for the migrate-call failure**: DISCUSS labelled this "semantically
loose." A future log analyser sees `cinder open: ...` for what is
actually a `cinder migrate: ...` event. Honest typing matters here.
**Rejected.**

**Rejected alternative — single new variant `Error::Migrate(MigrateError)`
covering both unknown-item AND parse-failure cases**: parse
failure has no `MigrateError` to wrap (it never reached the
underlying store). Forcing a `MigrateError` wrapper for a CLI-side
parse error would mean inventing a synthetic `MigrateError::UnknownItem`
with empty fields, which violates the type's contract.
**Rejected.**

---

## DD5: Stderr wording — `<to_tier> "HOT": expected one of hot, warm, cold` and `cinder migrate: cannot migrate unknown item "acme/batch-00099" for tenant acme`

**Decision**: The exact wording is locked by DD4's `Display` impls.
Both lines, when composed with the binary's existing
`eprintln!("kaleidoscope-cli: {e}")` wrapper at
`crates/kaleidoscope-cli/src/main.rs:69`, produce:

For OK2 (unknown item):
```text
kaleidoscope-cli: cinder migrate: cannot migrate unknown item "acme/batch-00099" for tenant acme
```

For OK3 (invalid tier):
```text
kaleidoscope-cli: <to_tier> "HOT": expected one of hot, warm, cold
```

**Rationale**:

1. The OK2 / OK3 acceptance contracts assert the SUBSTRING
   invariant (verbatim item id; verbatim invalid tier value). The
   exact wording above satisfies both invariants by construction.

2. The `cinder migrate:` prefix on OK2 mirrors the `cinder open:`
   prefix on `Error::CinderOpen` (`crates/kaleidoscope-cli/src/lib.rs:92`)
   and the `lumen open:` / `lumen ingest:` / `lumen query:` prefix
   pattern on the Lumen variants. The crate name is the prefix;
   the operation is the second token; the underlying error message
   is the suffix.

3. The `<to_tier>` token on OK3 mirrors the `--since` / `--until`
   token on the time-range parse errors (the binary's
   `parse_flag_iso` at `crates/kaleidoscope-cli/src/main.rs:219`
   names the flag explicitly). For the positional argument, the
   bracket-form `<to_tier>` reads as the documentation form (same
   shape used in the usage text at
   `crates/kaleidoscope-cli/src/main.rs:87`).

4. The `{value:?}` debug formatting wraps the invalid value in
   quotes (`"HOT"`, `""`, `"lukewarm"`). For empty-string or
   whitespace-only inputs the quotes are the only way to make the
   value visible.

---

## DD6: Reuse Analysis (RCA F-1 hard gate)

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `cinder_base(data_dir)` private helper | `crates/kaleidoscope-cli/src/lib.rs:122-124` | **REUSE** | Used by `ingest()` and `stats_with_tiers()`; `migrate()` becomes the third call site. |
| `FileBackedTieringStore::open` | `crates/kaleidoscope-cli/src/lib.rs:179-180` (ingest) and `:369-370` (stats_with_tiers) | **REUSE** | Same construction pattern. The third call site outside `ingest` / `stats_with_tiers`. |
| `cinder::NoopRecorder` (alias `CinderRecorder`) | `crates/kaleidoscope-cli/src/lib.rs:58, 173-174` | **REUSE** | Quiescent recorder for the Cinder side. Same construction pattern as `ingest()`'s no-flag arm and `stats_with_tiers()`. |
| `TieringStore::get_entry(tenant, item)` | `crates/cinder/src/store.rs:89` | **REUSE** | One call per `migrate` invocation; the read that discovers the `from` tier and discriminates `UnknownItem`. |
| `TieringStore::migrate(tenant, item, to_tier, migrated_at)` | `crates/cinder/src/store.rs:93-99` | **REUSE** | The mutation API. One call per `migrate` invocation. |
| `MigrateError::UnknownItem { tenant, item }` | `crates/cinder/src/store.rs:43, 55-58` | **REUSE** | Materialised by the pre-flight `get_entry` `None` branch (DD2). Display already prints the verbatim item id (`{item:?}`). |
| `cinder::ItemId` + `ItemId::new(s)` | `crates/cinder/src/tier.rs:51-62` | **REUSE** | Wraps the raw `item_id: &str` argument for the Cinder calls. |
| `cinder::Tier` enum | `crates/cinder/src/tier.rs:28-32` | **REUSE** | The `Tier::Hot` / `Tier::Warm` / `Tier::Cold` variants are pattern targets of `parse_tier` (DD3) and `tier_lowercase` (already private). |
| `tier_lowercase(tier)` private helper | `crates/kaleidoscope-cli/src/lib.rs:389-395` | **REUSE** | The `<from>` and `<to>` fields of the stdout report are rendered via this. The renderer's inverse is the new `parse_tier` (DD3). |
| `cinder::MigrateError` | `crates/cinder/src/store.rs:38-66` | **REUSE** | Wrapped by the new `Error::CinderMigrate` variant (DD4). Display composed via the variant's own Display impl. |
| `From<std::io::Error> for Error` | `crates/kaleidoscope-cli/src/lib.rs:104-108` | **REUSE** | Lifts `writeln!(writer, ...)?` and `writer.flush()?` failures. |
| `Error::CinderOpen(MigrateError)` variant | `crates/kaleidoscope-cli/src/lib.rs:77` | **REUSE** | Covers `FileBackedTieringStore::open` failure. Distinct from the new `Error::CinderMigrate` (DD4). |
| `parse_positional` helper | `crates/kaleidoscope-cli/src/main.rs:248-254` | **REUSE** | New `run_migrate` parses `args[2]` (tenant) and `args[3]` (data_dir) identically. The new positional `args[4]` (item_id) and `args[5]` (to_tier) parse inline in `run_migrate` because they are migrate-specific. |
| `aegis::TenantId` | reused as in every sibling | **REUSE** | The tenant argument; constructed identically to predecessor features. |
| Existing test harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`) | `crates/kaleidoscope-cli/tests/stats_subcommand.rs:77-118` and six siblings | **DUPLICATE INLINE AT V0** | DISCUSS D-NewTestFile confirms rule-of-three extraction deferred. This is the seventh test file in the cluster. |
| `parse_tier(s: &str) -> Result<Tier, ()>` private helper | n/a — new | **CREATE** | DD3. Inverse of `tier_lowercase`. Private to the crate. |
| `Error::InvalidTier { value: String }` variant | n/a — new | **CREATE** | DD4. Parse-side fail-fast carrier. |
| `Error::CinderMigrate(MigrateError)` variant | n/a — new | **CREATE** | DD4. Distinct from `CinderOpen`. |
| `pub fn migrate(...)` library function | n/a — new | **CREATE** | DD1. ~25 lines. The fifth public free function on the library surface. |
| `run_migrate` binary helper + dispatch arm + usage text | n/a — new in `main.rs` | **CREATE** | One new arm in the `match args.get(1)` block plus `run_migrate(&args)` plus a new paragraph in `write_usage`. ~15 lines total. |
| `Tier::all()` associated function | n/a — does not exist | **DO NOT CREATE** | Same reasoning as `stats_with_tiers` DD2: the parse-side does not iterate tiers; it pattern-matches on three literal strings. No `Tier::all()` needed. |
| New `MigrateReport` or `TierTransition` struct | n/a | **DO NOT CREATE** | DD1 rationale 2: stdout is the only consumer; premature abstraction. |

**Verdict**: **REUSE** (fourteen existing constructs:
`cinder_base`, `FileBackedTieringStore::open`, `NoopRecorder`,
`get_entry`, `migrate`, `MigrateError`, `ItemId`, `Tier`,
`tier_lowercase`, `From<io::Error>`, `Error::CinderOpen`,
`parse_positional`, `TenantId`, the test harness shape). **CREATE
NEW**: one private parser helper (`parse_tier`), two error
variants (`InvalidTier`, `CinderMigrate`), one public free function
(`migrate`), and the binary-side dispatch arm + `run_migrate`
helper + usage paragraph. **No new public type, no new trait, no
new module, no new external crate.** The change surface is two
files in `src/` (`lib.rs`, `main.rs`) plus one new test file plus
one new `[[test]]` block in `Cargo.toml`.

---

## DD7: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following
DISCUSS decisions:

1. **No `--observe-otlp` flag** (D-OutOfScope-Observe). Confirmed.
   The Cinder recorder is `NoopRecorder`.
2. **No `--dry-run` flag** (D-OutOfScope-Dryrun). Confirmed. Every
   successful invocation mutates the Cinder store.
3. **No `--at` / `--migrated-at` flag** (D-Timestamp). Confirmed.
   `SystemTime::now()` at the call site.
4. **No `--json` / `--csv` / `--format=...`** (D-OutOfScope-Json).
   Confirmed.
5. **No bulk migration** (D-OutOfScope-Bulk). Confirmed. One item
   per invocation.
6. **Cinder-only open, no Lumen touch** (D-NoLumenTouch). Confirmed.
   The `lumen_base(data_dir)` helper is NOT called; no
   `FileBackedLogStore::open` site is added.
7. **Lower-case tier argument only** (D-LowerCase). Confirmed.
   `parse_tier` matches three literal strings (DD3).
8. **Faithful to underlying idempotent same-tier API**
   (D-Idempotent). Confirmed. No `from == to` special case.
9. **Locked test files NOT modified** (D-LockedTests). Confirmed.
   `tests/stats_subcommand.rs`,
   `tests/stats_cinder_tier_distribution.rs`,
   `tests/observe_otlp_*.rs`, `tests/stats_time_range.rs` continue
   to pass green UNMODIFIED.
10. **No SSOT journey or `jobs.yaml` modification** (D-NoSSOT).
    Confirmed.
11. **No new ADR**. Same reasoning as `stats_with_tiers`'s DD6.10:
    the new `migrate` function is the fifth public free function
    on `kaleidoscope-cli`'s library surface, following the
    established shape. No new public type, no new abstraction, no
    new module, no new external dependency. ADR-0001 absorbs the
    addition without amendment.

---

## DEVOPS handoff annotation

Recipient: `nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1
    and L2; L3 explicitly skipped, reification conditions
    documented).
  - The new subsection appended to
    `docs/product/architecture/brief.md > ## Application Architecture
    — cli-migrate-subcommand-v0`.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-migrate-subcommand-v0/discuss/` (locked, not
    modified).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1 (principal —
    migrate-success correctness), OK2 (unknown-item fail-fast),
    OK3 (invalid-tier fail-fast), OK4 (idempotent same-tier
    guardrail).

- **Development paradigm for DELIVER**: Rust idiomatic per
  `CLAUDE.md`. Data + free functions + traits where genuinely
  needed. The new `migrate()` is a free function. `parse_tier`
  is a private free function. No new trait. No new struct. No new
  `dyn` boundary beyond the existing `Box<dyn CinderRec + Send + Sync>`
  at the recorder construction site (inherited from `ingest()`'s
  and `stats_with_tiers()`'s shapes).

- **External integrations**: **none**. No HTTP client, no webhook,
  no third-party API, no vendor SDK, no subprocess, no network I/O.
  Pure local mutation of the Cinder WAL+snapshot via the existing
  `TieringStore::migrate` trait method. The Lumen store is not
  opened. No contract-test recommendation applies.

- **External dependency footprint**: **no new external crate**.
  All used types (`TenantId`, `Tier`, `ItemId`, `MigrateError`,
  `FileBackedTieringStore`, `TieringStore`, `NoopRecorder` alias
  `CinderRecorder`) are already in `kaleidoscope-cli`'s use list
  at `crates/kaleidoscope-cli/src/lib.rs:55-59`. `Cargo.lock`
  churn is zero beyond what a recompile produces.

- **CI gates** (ADR-0005): the five existing workspace gates
  apply unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test migrate_subcommand`
  exits 0 as the OK1/OK2/OK3/OK4 acceptance probe under Gate 1
  (`cargo test --workspace`). The seven locked test files
  continue to pass green and collectively serve as the
  no-regression oracle for every shipped CLI feature. **No new
  gate is added.**

  Specifically on **Gate 5 (mutation testing)**: the existing
  `gate-5-mutants-kaleidoscope-cli` job is path-filtered on
  `crates/kaleidoscope-cli/**` via `--in-diff`. Any commit
  touching `crates/kaleidoscope-cli/src/lib.rs` or
  `crates/kaleidoscope-cli/src/main.rs` (this feature touches
  both) is automatically mutated by the existing job. The new
  `migrate()` body's branches (the `parse_tier` match, the
  `get_entry` `None` arm, the `migrate` `Err` arm, the
  `writeln!` line, the `flush()`), the new `parse_tier`'s four
  match arms, and the new `Error::InvalidTier` / `Error::CinderMigrate`
  `Display` impls all fall inside the same mutation surface.
  **No new Gate 5 job needed.**

- **Workspace changes**: no `Cargo.toml` additions at the
  workspace root. `crates/kaleidoscope-cli/Cargo.toml` gains
  exactly one new `[[test]]` block:

  ```toml
  [[test]]
  name = "migrate_subcommand"
  path = "tests/migrate_subcommand.rs"
  ```

  No new `[dependencies]` line; no new `[dev-dependencies]` line.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate
  5): scoped to `crates/kaleidoscope-cli/src/lib.rs` and
  `crates/kaleidoscope-cli/src/main.rs` (the two modified source
  files). Run after the DELIVER refactor pass. 100% kill rate.
  The changed code surface is small (the new `migrate()` function
  ~25 lines plus the new `parse_tier` ~8 lines plus the two new
  `Error` variants + Display arms ~6 lines plus the `main.rs`
  delta ~15 lines = ~54 lines of new production source).
  Mutation-testing budget should be modest.

- **Architectural-rule enforcement tooling** (Principle 11): no
  new tooling is recommended for this feature. The existing
  five-gate workspace contract already enforces every rule this
  feature touches. The "no `place()` / `evaluate_at()` / Lumen
  open in `migrate()`" property is structurally enforced by the
  acceptance test (D-NoLumenTouch byte-equivalence assertion on
  the `<data_dir>/lumen.*` directory before and after the call).

### Why no ADR change

The new `migrate` function introduces **no new public type, no
new abstraction, no new module, no new external dependency**. It
is the fifth public free function on `kaleidoscope-cli`'s library
surface, parallel to `ingest`, `read`, `stats`, `stats_with_tiers`,
following the same shape (tenant + data_dir + positional args +
writer → result). The two new `Error` variants are additive on
an existing public enum that already carries domain-specific
variants (`LumenOpen`, `LumenIngest`, `LumenQuery`, `CinderOpen`,
`Io`, `ParseRecord`, `SerialiseRecord`). The change is captured
within the established free-function-plus-typed-error pattern.
ADR-0001 absorbs the addition without amendment.
