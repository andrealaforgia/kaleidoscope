<!-- markdownlint-disable MD024 -->

# User Stories — `cli-migrate-subcommand-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. This change introduces ONE new
  CLI subcommand (`migrate`) and ONE new free function in the
  `kaleidoscope_cli` library; no new trait is introduced and no
  existing trait is modified. The existing
  `cinder::TieringStore::migrate(tenant, item, to_tier, migrated_at)`
  method (`crates/cinder/src/store.rs:93-99`) is the only Cinder
  mutating API consumed; `TieringStore::get_entry(tenant, item)`
  (`crates/cinder/src/store.rs:89`) is the only Cinder read API
  consumed, used to discover the `from` tier for the stdout report.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions
  with `// Given / // When / // Then` comment blocks, not Gherkin
  `.feature` files. The Given/When/Then text in the UAT Scenarios
  sections below is the specification; DISTILL translates it into
  `#[test]` functions in
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (NEW file,
  per `wave-decisions.md` D-NewTestFile). The harness mirrors the
  pattern already in `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
  and the five `tests/observe_otlp_*.rs` files.
- Output-shape contract: stdout is exactly ONE line on success, the
  literal string
  `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`,
  where `<from>` and `<to>` render as the lower-case tier names
  `hot` / `warm` / `cold` (mirroring the `tier_lowercase` helper at
  `crates/kaleidoscope-cli/src/lib.rs:389-395`). No header, no JSON,
  no CSV, no colour codes. Exit code 0.
- Stderr-on-failure contract: on `MigrateError::UnknownItem` or
  invalid `<to_tier>` argument, exit code is non-zero and stderr
  carries a single line naming the missing item (UnknownItem case)
  or the invalid value (parse case). Stdout is empty. The exact
  stderr wording is DESIGN-locked per `wave-decisions.md` D-StderrWording;
  the contract here is the OBSERVABLE invariants only (non-zero exit,
  stderr contains the offending item id or tier value, stdout empty).
- Tier-argument shape: `<to_tier>` is accepted ONLY in lower-case as
  one of `hot` / `warm` / `cold`. Upper-case (`HOT` / `Hot`),
  mixed-case (`Hot`), or any other spelling is rejected with a
  non-zero exit and a stderr line naming the invalid value
  (`wave-decisions.md` D-LowerCase). This mirrors the established
  lower-case tier rendering convention from
  `cli-stats-cinder-tier-distribution-v0` (output side) at
  `crates/kaleidoscope-cli/src/lib.rs:389-395`.
- Timestamp contract: the `migrated_at: SystemTime` argument passed
  to `TieringStore::migrate` is `SystemTime::now()` evaluated at the
  call site. There is NO `--at` / `--migrated-at` flag in v0
  (`wave-decisions.md` D-Timestamp). The acceptance test asserts
  observable wire-level invariants (the stdout report content, the
  post-call `get_entry` tier equals `to_tier`), not the exact
  recorded `migrated_at` value.
- Read+mutate contract: this subcommand opens ONLY the Cinder store
  under `<data_dir>/cinder.*`. It does NOT open the Lumen store
  under `<data_dir>/lumen.*`. The Lumen WAL+snapshot is byte-
  equivalent before and after the call (`wave-decisions.md`
  D-NoLumenTouch).
- Tenant-isolation contract: `migrate acme /tmp/data acme/batch-00042 cold`
  MUST NOT touch `globex`'s tier metadata even if `globex` happens
  to have placed an item with the same `ItemId` string
  (`acme/batch-00042`) into the same `data_dir`. Inherited from
  `cinder::TieringStore`'s per-tenant isolation invariant
  (`crates/cinder/src/store.rs:71-72`).
- No-flag posture: the `migrate` subcommand accepts NO optional
  flags in v0. No `--observe-otlp`, no `--dry-run`, no `--at`, no
  `--format=...`. The positional argument shape is fixed:
  `migrate <tenant> <data_dir> <item_id> <to_tier>` — four
  positional arguments, in that order.

---

## US-01: Operator manually moves a single item between Cinder tiers without writing Rust

### Elevator Pitch

- **Before**: Priya the platform operator runs a multi-tenant
  Kaleidoscope deployment. After `ingest`, every batch lands one
  Cinder Hot item via `flush()`
  (`crates/kaleidoscope-cli/src/lib.rs:243-244`). She knows the
  current distribution from
  `kaleidoscope-cli stats acme /tmp/data` (the previous wave). What
  she canNOT do today is move a single item — e.g.
  `acme/batch-00042` — between tiers from the CLI. To "pull a batch
  back to Warm" after an over-aggressive auto-tiering policy, or to
  "force a specific batch to Cold for rebalancing", she has to
  write a Rust harness: open `FileBackedTieringStore::open`, build
  an `ItemId`, call `migrate(tenant, &item, to_tier, SystemTime::now())`,
  match on `MigrateError`. She also has to remember which `from`
  tier the item was in, because the typed
  `MigrateError::UnknownItem` does not carry the previous tier and
  there is no operator-visible "before/after" record otherwise.
  Result: tier rebalancing is a Rust task, lifecycle testing is a
  Rust task, and the operator's mental model "I move item X from
  Hot to Cold" has no direct CLI surface.

- **After**: Priya runs:

  ```text
  kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold
  ```

  Stdout, in milliseconds, prints exactly one line:

  ```text
  migrated tenant=acme item=acme/batch-00042 from=hot to=cold
  ```

  Exit code 0. The line tells her TWO things she did not previously
  have from a single CLI invocation: (1) which tier the item was
  in BEFORE the call (`from=hot` — the previous tier, read from
  `get_entry` before the `migrate` call), and (2) which tier it is
  in AFTER the call (`to=cold` — the argument she typed, reflected
  back as confirmation). She can immediately re-run
  `kaleidoscope-cli stats acme /tmp/data` and see the `hot=` /
  `warm=` / `cold=` distribution reflect the move (one fewer in
  the old tier, one more in the new tier — assuming no other
  concurrent operator action).

  When she fat-fingers the item id —
  `kaleidoscope-cli migrate acme /tmp/data acme/batch-00099 cold`
  for an item that was never placed — she gets a non-zero exit and
  a stderr line that names `acme/batch-00099` (the missing item).
  Stdout is empty. The Cinder store is unchanged (no `place()`
  call, no `migrate()` call succeeded). She fixes the id and
  retries; this is the fail-fast contract she expects from a CLI.

  When she types an invalid tier name —
  `kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 HOT`
  (upper-case) or `... lukewarm` (typo) — she gets a non-zero
  exit and a stderr line that names the invalid value she typed.
  Stdout is empty. The Cinder store is unchanged.

- **Decision enabled**: Priya can decide and execute three
  operationally distinct moves from one CLI invocation:
  1. "Move this specific batch from Hot to Cold because we're
     rebalancing" (manual tier rebalancing without a Rust harness).
  2. "Auto-tiering policy moved this item too aggressively; pull
     it back to Warm" (compensating action against
     `evaluate_at(now, policy)` decisions made earlier).
  3. "Test lifecycle behaviour by manually walking an item through
     Hot → Warm → Cold" (operator-driven lifecycle smoke test).

### Problem

Priya the platform operator already uses the four shipped CLI
subcommands (`ingest`, `read`, `stats`, `--help`) daily. The CLI's
gap, today, is that it has no surface for triggering a Cinder tier
transition. Three operationally distinct decisions all reduce to
the same primitive ("move item X for tenant Y from its current
tier to tier T"):

1. **Manual rebalancing**: "I want to move this specific batch
   from Hot to Cold because we're rebalancing capacity." Today
   this requires either writing a Rust harness or running an
   off-cycle `evaluate_at` against a hand-tuned policy.
2. **Compensating a policy decision**: "The auto-tiering policy
   moved this item too aggressively; pull it back to Warm."
   Today this requires writing a Rust harness (no CLI surface).
3. **Lifecycle testing**: "Walk this item through Hot → Warm →
   Cold to confirm the policy is wired correctly." Today this
   requires a Rust harness with three sequential `migrate` calls.

All three reduce to "give me one CLI invocation that calls
`TieringStore::migrate(tenant, &ItemId, to_tier, SystemTime::now())`
and tells me, on stdout, what the from→to transition was." The
underlying API
(`cinder::TieringStore::migrate(...)` at
`crates/cinder/src/store.rs:93-99`, returning
`Result<(), MigrateError>`) is already adequate; the gap is the
missing CLI surface and the missing operator-visible "from/to"
report.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli ingest`,
`kaleidoscope-cli read`, `kaleidoscope-cli stats` daily (inherited
from the predecessor and the five reference features) | wants to
move one specific Cinder item between tiers without writing Rust
or running a policy-evaluation harness | uses standard Unix text
tools (`grep`, `cut`, `awk`) on stdout output, not JSON parsers |
expects fail-fast behaviour (non-zero exit + descriptive stderr)
on unknown item ids and invalid tier names | does NOT expect the
subcommand to touch the Lumen store.

### Solution

Add a new positional subcommand to the existing CLI binary:

```text
kaleidoscope-cli migrate <tenant_id> <data_dir> <item_id> <to_tier>
```

The dispatch arm in `crates/kaleidoscope-cli/src/main.rs` (the
match block at lines 50-64) gains a new `Some("migrate")` branch
that calls a new library function in `kaleidoscope_cli`. The
library function:

1. Opens `FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
   with a quiescent `cinder::NoopRecorder` (the same pattern
   `ingest`'s no-flag arm uses at
   `crates/kaleidoscope-cli/src/lib.rs:170-174`). It does NOT open
   the Lumen store.
2. Parses `<to_tier>` from its argv form to a `cinder::Tier`
   value using a `tier_from_lowercase(&str) -> Result<Tier, _>`
   shape that accepts ONLY `hot` / `warm` / `cold`. Any other
   spelling (upper-case `HOT`, mixed-case `Hot`, typo `lukewarm`)
   yields a parse error containing the offending value verbatim.
3. Builds `ItemId::new(item_id_arg.to_string())`.
4. Reads the current tier via `cinder.get_entry(&tenant, &item)`.
   If `None`, the item is unknown — the function returns a typed
   error mirroring `MigrateError::UnknownItem` (the CLI dispatcher
   converts to a non-zero exit + stderr line naming the item).
   If `Some(entry)`, captures `entry.tier` as the `from` value
   for the stdout report.
5. Calls `cinder.migrate(&tenant, &item, to_tier, SystemTime::now())`.
   Bubbles any `MigrateError` to the CLI dispatcher.
6. On success, writes ONE line to stdout:
   `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`,
   where `<from>` and `<to>` render as `hot` / `warm` / `cold` via
   the same lower-case mapping (likely the existing
   `tier_lowercase` helper at
   `crates/kaleidoscope-cli/src/lib.rs:389-395`, lifted to
   `pub(crate)` if needed — DESIGN's call).
7. Exits 0.

The output report uses the same `key=value` aesthetic as `stats`
but with FOUR fields on a single line (rather than one field per
line) because the report is a SINGLE atomic event ("this move
happened, from this tier to that tier") and the operator's
natural grep is `kaleidoscope-cli migrate ... 2>/dev/null | grep
from=`. No multi-line render is needed for a single move report.

The function signature is DESIGN-locked per `wave-decisions.md`
D-FunctionShape. Two plausible shapes:

1. A new free function
   `migrate(tenant: &TenantId, data_dir: &Path, item_id: &str, to_tier_arg: &str, writer: impl Write) -> Result<MigrateReport, Error>`
   returning a typed report struct with `from: Tier, to: Tier` so
   the binary's stdout sink writes the report bytes.
2. A new free function with the same shape but writing directly
   to the supplied writer and returning the `from`/`to` pair
   (or unit) — the binary collects the bytes via the writer.

DESIGN locks the choice. The wire-observable contract (stdout
content on success, stderr content on failure, exit code, no
Lumen touch) is what matters here.

The `Error` type adds at most ONE new variant to
`kaleidoscope_cli::Error` for the "invalid tier argument" case
(`wave-decisions.md` D-ErrorVariant). The "unknown item" case is
already representable via the existing
`Error::CinderOpen(MigrateError)` variant at
`crates/kaleidoscope-cli/src/lib.rs:77` — but DESIGN may choose to
introduce a dedicated `CinderMigrate(MigrateError)` variant to
separate "store-open failure" from "migrate-call failure" cleanly.
The choice is DESIGN's.

### Domain Examples

#### 1. Happy path — Priya pulls `acme/batch-00042` back from Hot to Warm

Priya has ingested seven days of `acme` traffic via the CLI's
existing `ingest` subcommand. Each `ingest` invocation placed
one Hot Cinder item per batch (per the `flush()` at
`crates/kaleidoscope-cli/src/lib.rs:243-244`). She runs
`kaleidoscope-cli stats acme /tmp/k-data` and sees `hot=42 warm=3
cold=0` — the auto-tiering job has not yet run, so almost
everything is in Hot. She wants to manually move
`acme/batch-00042` (the latest Hot item) into Warm as a
rebalancing experiment:

```text
kaleidoscope-cli migrate acme /tmp/k-data acme/batch-00042 warm
```

Stdout contains exactly one line:

```text
migrated tenant=acme item=acme/batch-00042 from=hot to=warm
```

Exit code 0. Stderr is empty. She immediately re-runs
`kaleidoscope-cli stats acme /tmp/k-data` and sees the new
distribution `hot=41 warm=4 cold=0` — confirming the move. The
Lumen side (`/tmp/k-data/lumen.*`) is byte-equivalent before and
after the migrate call.

#### 2. Edge case (idempotent same-tier migrate) — Priya migrates `acme/batch-00007` from Cold to Cold

Priya has been walking `acme/batch-00007` through the tier
lifecycle (Hot → Warm → Cold). The item is currently in Cold. She
accidentally re-issues the cold migration:

```text
kaleidoscope-cli migrate acme /tmp/k-data acme/batch-00007 cold
```

Stdout contains exactly one line:

```text
migrated tenant=acme item=acme/batch-00007 from=cold to=cold
```

Exit code 0. The underlying `TieringStore::migrate` API at
`crates/cinder/src/store.rs:167-188` IS idempotent for the
same-tier case: it overwrites `entry.tier = to_tier` (line 184)
and updates `entry.migrated_at = migrated_at` (line 185)
regardless of whether `from == to`. There is no
`if from != to_tier` guard in the `InMemoryTieringStore`
implementation, so the migrate succeeds and the
`record_migrate(tenant, from, to_tier)` recorder call fires with
`from == to == Cold` (line 186).

The CLI report mirrors this honestly: `from=cold to=cold`. The
operator can see that nothing logically changed (the line
itself signals the no-op), but the timestamp metadata
(`migrated_at`) WAS bumped in the underlying store. This is
documented as the operationally expected behaviour: no special
case in the CLI; the underlying API is idempotent, the CLI
faithfully reports the call's outcome.

#### 3. Error case (unknown item) — Priya fat-fingers the item id

Priya types `acme/batch-00099` when she meant `acme/batch-00009`:

```text
kaleidoscope-cli migrate acme /tmp/k-data acme/batch-00099 cold
```

Stdout is empty. Stderr contains a single line that names the
missing item id `acme/batch-00099`. Exit code is non-zero. The
Cinder store under `/tmp/k-data/cinder.*` is byte-equivalent
before and after the call (the
`MigrateError::UnknownItem { tenant, item }` arm at
`crates/cinder/src/store.rs:179-182` returns early without any
mutation). Priya can verify by re-running
`kaleidoscope-cli stats acme /tmp/k-data` and observing the
distribution is unchanged.

#### 4. Error case (invalid tier argument) — Priya types upper-case `HOT`

Priya types `HOT` instead of `hot`:

```text
kaleidoscope-cli migrate acme /tmp/k-data acme/batch-00042 HOT
```

Stdout is empty. Stderr contains a single line that names the
invalid value `HOT`. Exit code is non-zero. The Cinder store
is byte-equivalent before and after the call (the parse error
fires BEFORE any `migrate` call is issued, so the store is
never mutated; this is also the natural behaviour because the
parse error short-circuits the dispatch path before
`FileBackedTieringStore::open` is even called — DESIGN's call on
the exact ordering).

The same outcome obtains for any non-`hot`/`warm`/`cold` value:
`Hot` (mixed-case), `WARM`, `cool`, `lukewarm`, `frozen`, empty
string, leading/trailing whitespace. The acceptance test
exercises one representative upper-case value plus one typo.

#### 5. Tenant-isolation case — Priya migrates `acme/batch-00042`, `globex`'s same-named item is untouched

Priya runs a deployment where `acme` and `globex` both have an
item id `acme/batch-00042` placed in Cinder under the same
`/tmp/k-data` (the `ItemId` namespace is global across tenants
but the placement key is `(TenantId, ItemId)` per
`crates/cinder/src/store.rs:119`). She runs:

```text
kaleidoscope-cli migrate acme /tmp/k-data acme/batch-00042 cold
```

Stdout contains exactly one line:

```text
migrated tenant=acme item=acme/batch-00042 from=hot to=cold
```

After the call, `globex`'s `acme/batch-00042` entry is byte-
equivalent to its pre-call state: it is STILL in its original
tier (e.g. Warm) and its `migrated_at` is unchanged. This is the
tenant-isolation invariant inherited from
`cinder::TieringStore`'s per-tenant key
(`crates/cinder/src/store.rs:71-72, :142`).

### UAT Scenarios (BDD)

#### Scenario: Operator moves an item from Hot to Warm — sees the from/to transition on stdout

```text
Given Priya has placed item acme/batch-00042 into Cinder under /tmp/k-data with tier Hot for tenant acme (via a prior ingest invocation or direct FileBackedTieringStore::open + place call)
When Priya invokes the migrate subcommand with arguments (acme, /tmp/k-data, acme/batch-00042, warm) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `migrated tenant=acme item=acme/batch-00042 from=hot to=warm\n`
And the captured stderr is empty
And exit code is 0
And cinder.get_entry(acme, acme/batch-00042) at /tmp/k-data returns Some(entry) with entry.tier == Tier::Warm
And the Lumen store under /tmp/k-data/lumen.* is byte-equivalent before and after the call
```

#### Scenario: Operator re-issues the current tier as the target — idempotent same-tier migrate succeeds

```text
Given Priya has placed item acme/batch-00007 into Cinder under /tmp/k-data with tier Cold for tenant acme (via direct FileBackedTieringStore::open + place call)
When Priya invokes the migrate subcommand with arguments (acme, /tmp/k-data, acme/batch-00007, cold) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `migrated tenant=acme item=acme/batch-00007 from=cold to=cold\n`
And the captured stderr is empty
And exit code is 0
And cinder.get_entry(acme, acme/batch-00007) at /tmp/k-data returns Some(entry) with entry.tier == Tier::Cold
```

#### Scenario: Operator types an item id that was never placed — fail-fast with stderr naming the missing item

```text
Given the Cinder store under /tmp/k-data has NO placement for the (tenant=acme, item=acme/batch-00099) pair (no prior place call for that item id under that tenant)
When Priya invokes the migrate subcommand with arguments (acme, /tmp/k-data, acme/batch-00099, cold) and a captured stdout sink and a captured stderr sink
Then the call returns Err
And the captured stdout is empty
And the captured stderr contains a line that contains the substring `acme/batch-00099`
And exit code is non-zero
And the Cinder store at /tmp/k-data is byte-equivalent before and after the call (no place call, no migrate call mutated state)
```

#### Scenario: Operator types an invalid tier value — fail-fast with stderr naming the invalid value

```text
Given the Cinder store under /tmp/k-data has the item acme/batch-00042 placed for tenant acme in tier Hot
When Priya invokes the migrate subcommand with arguments (acme, /tmp/k-data, acme/batch-00042, HOT) and a captured stdout sink and a captured stderr sink
Then the call returns Err
And the captured stdout is empty
And the captured stderr contains a line that contains the substring `HOT`
And exit code is non-zero
And cinder.get_entry(acme, acme/batch-00042) at /tmp/k-data returns Some(entry) with entry.tier == Tier::Hot (unchanged from the pre-call state)
```

#### Scenario: Tenant isolation — migrating `acme/batch-00042` for `acme` does not touch `globex`'s same-named item

```text
Given Priya has placed item acme/batch-00042 in Cinder under /tmp/k-data with tier Hot for tenant acme
And Priya has placed item acme/batch-00042 in Cinder under /tmp/k-data with tier Warm for tenant globex
When Priya invokes the migrate subcommand with arguments (acme, /tmp/k-data, acme/batch-00042, cold) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`
And cinder.get_entry(acme, acme/batch-00042) at /tmp/k-data returns Some(entry) with entry.tier == Tier::Cold
And cinder.get_entry(globex, acme/batch-00042) at /tmp/k-data returns Some(entry) with entry.tier == Tier::Warm (unchanged from the pre-call state)
```

### Acceptance Criteria

- [ ] When tenant `acme` has item `acme/batch-00042` placed in tier
  Hot in `/tmp/k-data/cinder.*`, invoking the migrate subcommand
  with `(acme, /tmp/k-data, acme/batch-00042, warm)` produces
  exactly the stdout line
  `migrated tenant=acme item=acme/batch-00042 from=hot to=warm\n`,
  empty stderr, and exit code 0.
- [ ] After the successful call, `cinder.get_entry(acme,
  acme/batch-00042)` at `/tmp/k-data` returns `Some(entry)` with
  `entry.tier == Tier::Warm`.
- [ ] When tenant `acme` has item `acme/batch-00007` placed in tier
  Cold in `/tmp/k-data/cinder.*`, invoking the migrate subcommand
  with `(acme, /tmp/k-data, acme/batch-00007, cold)` produces
  exactly the stdout line
  `migrated tenant=acme item=acme/batch-00007 from=cold to=cold\n`,
  empty stderr, and exit code 0. The underlying API is idempotent
  per `crates/cinder/src/store.rs:167-188`; no special case in the
  CLI.
- [ ] When tenant `acme` has NO item `acme/batch-00099` placed in
  `/tmp/k-data/cinder.*`, invoking the migrate subcommand with
  `(acme, /tmp/k-data, acme/batch-00099, cold)` produces empty
  stdout, a non-empty stderr line containing the substring
  `acme/batch-00099`, and a non-zero exit code. The Cinder store
  is byte-equivalent before and after.
- [ ] When the `<to_tier>` argument is any spelling other than
  exactly `hot` / `warm` / `cold` — at minimum the upper-case
  value `HOT` and the typo `lukewarm` are tested — the subcommand
  produces empty stdout, a non-empty stderr line containing the
  invalid value verbatim, and a non-zero exit code. The Cinder
  store is byte-equivalent before and after.
- [ ] When tenants `acme` and `globex` both have an item id
  `acme/batch-00042` placed in the same `/tmp/k-data` (Hot for
  `acme`, Warm for `globex`), invoking the migrate subcommand
  with `(acme, /tmp/k-data, acme/batch-00042, cold)` moves only
  `acme`'s item to Cold; `globex`'s item remains in Warm. The
  pre-call and post-call `get_entry(globex, ...)` results have
  identical `tier`.
- [ ] The Lumen store under `<data_dir>/lumen.*` is byte-
  equivalent before and after every `migrate` subcommand
  invocation (success, unknown-item failure, invalid-tier
  failure). The subcommand never opens the Lumen store
  (`wave-decisions.md` D-NoLumenTouch).
- [ ] The new acceptance test file
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` is added
  (NEW file, mirroring the harness pattern of
  `tests/stats_subcommand.rs`) with assertions covering the five
  UAT scenarios above.
- [ ] The existing locked acceptance test files
  (`crates/kaleidoscope-cli/tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/observe_otlp_*.rs`) continue to pass green UNMODIFIED
  under `cargo test --package kaleidoscope-cli`.
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs`
  (lines 75-129) is updated to document the new `migrate`
  subcommand's positional argument shape and lower-case tier
  contract. (Optional — DESIGN's call on exact wording.)
- [ ] No new external crate dependency is added to
  `crates/kaleidoscope-cli/Cargo.toml`. The only new
  `Cargo.toml` change is one `[[test]]` entry for the new test
  file.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the stdout
  byte level on the new
  `kaleidoscope-cli migrate <tenant> <data_dir> <item_id> <to_tier>`
  CLI invocation.
- **Does what**: receives a one-line stdout report of the form
  `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`
  on success, or a non-zero exit with a stderr line naming the
  offending item id (unknown-item case) or the offending tier
  value (invalid-tier case) on failure, on a single CLI
  invocation, without writing Rust code or opening a Rust
  REPL.
- **By how much**: 100% of `migrate` invocations against placed
  items with valid lower-case tier arguments produce the exact
  stdout report line and post-call `get_entry().tier == to_tier`
  (OK1); 100% of invocations against unplaced item ids produce a
  non-zero exit + stderr containing the missing item id + an
  unchanged Cinder store (OK2); 100% of invocations with non-
  lower-case-`hot`/`warm`/`cold` tier arguments produce a non-zero
  exit + stderr containing the invalid value + an unchanged
  Cinder store (OK3); 100% of invocations (success or failure)
  produce a byte-equivalent Lumen store before and after, and a
  byte-equivalent same-tenant-id-cross-tenant Cinder state for
  any other tenant in the same `data_dir` (OK4).
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` covering
  the four KPIs across the five UAT scenarios above. PLUS the
  existing locked acceptance test files continuing to pass
  green UNMODIFIED.
- **Baseline**: 0% — today there is no CLI surface for tier
  migration at all; operators answer the same question by
  writing Rust harnesses against `cinder::FileBackedTieringStore`
  or by running off-cycle `evaluate_at` against a hand-tuned
  policy.

Maps to OK1-CLI-migrate-success (principal),
OK2-CLI-migrate-unknown-item-fail-fast,
OK3-CLI-migrate-invalid-tier-fail-fast, and
OK4-CLI-migrate-idempotent-same-tier in `outcome-kpis.md`.

### Technical Notes

- The exact library function shape is DESIGN-locked per
  `wave-decisions.md` D-FunctionShape. Likely a new free
  function in `crates/kaleidoscope-cli/src/lib.rs` with a
  signature like
  `pub fn migrate(tenant: &TenantId, data_dir: &Path, item_id: &str, to_tier_arg: &str, writer: impl Write) -> Result<(), Error>`.
- The function internally constructs
  `FileBackedTieringStore::open(cinder_base(data_dir), Box::new(NoopRecorder))`
  identically to the pattern at
  `crates/kaleidoscope-cli/src/lib.rs:170-174`. It calls
  `cinder.get_entry(&tenant, &item)` once to read the `from`
  tier (returning early with the unknown-item error if `None`),
  then `cinder.migrate(&tenant, &item, to_tier,
  SystemTime::now())`.
- The tier-argument parse is a new free function in `lib.rs`,
  likely `tier_from_lowercase(s: &str) -> Result<Tier, Error>`
  (the exact shape is DESIGN's). It is the inverse of the
  existing `tier_lowercase` helper at
  `crates/kaleidoscope-cli/src/lib.rs:389-395`.
- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — new
  free function `migrate(...)` and new free function
  `tier_from_lowercase(...)` (or equivalent), plus possibly a
  new `Error` variant (`CinderMigrate(MigrateError)` or
  `InvalidTier { value: String }` — DESIGN's call per
  `wave-decisions.md` D-ErrorVariant).
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — new
  `Some("migrate") => run_migrate(&args)` dispatch arm in the
  match at lines 50-64; new `run_migrate(...)` function
  mirroring the shape of `run_stats(...)` at lines 226-246
  (parse the four positional arguments via `parse_positional`
  + two extra `args.get(N)` calls, call the library function,
  write the stdout report). `print_usage` (lines 75-129)
  gains a `kaleidoscope-cli migrate <tenant_id> <data_dir>
  <item_id> <to_tier>` block.
- New test file:
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`.
  Mirrors the harness pattern of
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` (the
  predecessor wave's locked file).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "migrate_subcommand", path =
  "tests/migrate_subcommand.rs"`. The `cinder` dependency is
  already present.
- DO NOT modify any locked test file
  (`crates/kaleidoscope-cli/tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/observe_otlp_*.rs`). This is a hard constraint from
  the task brief and is restated as `wave-decisions.md`
  D-LockedTests.
- Slice tag: not `@infrastructure` — this story directly
  enables three operator-visible decisions on a real CLI
  surface (`kaleidoscope-cli migrate <tenant> <data_dir>
  <item_id> <to_tier>`).

### Dependencies

- `cinder::TieringStore::migrate(tenant, item, to_tier,
  migrated_at)` already exists and returns `Result<(),
  MigrateError>` per `crates/cinder/src/store.rs:93-99`.
- `cinder::TieringStore::get_entry(tenant, item)` already
  exists and returns `Option<TierEntry>` per
  `crates/cinder/src/store.rs:89`.
- `cinder::MigrateError::UnknownItem { tenant, item }`
  already exists per `crates/cinder/src/store.rs:43`.
- `cinder::FileBackedTieringStore` already implements
  `TieringStore` and is already constructed by `ingest()` at
  `crates/kaleidoscope-cli/src/lib.rs:179-180`.
- `cinder::NoopRecorder` is the quiescent recorder used by
  `ingest`'s no-flag arm at
  `crates/kaleidoscope-cli/src/lib.rs:170-174`; already a
  `kaleidoscope-cli` dependency.
- `cinder::Tier` enum (`Hot`, `Warm`, `Cold`) at
  `crates/cinder/src/tier.rs:28-32`; already imported at
  `crates/kaleidoscope-cli/src/lib.rs:58`.
- `cinder::ItemId::new(id)` at `crates/cinder/src/tier.rs:54-56`;
  already imported.
- The existing `cinder_base(data_dir)` helper at
  `crates/kaleidoscope-cli/src/lib.rs:122-124` is reused
  unchanged.
- The existing `parse_positional` helper at
  `crates/kaleidoscope-cli/src/main.rs:248-254` is reused for
  the first two positional arguments (`<tenant_id>`,
  `<data_dir>`); the third and fourth positional arguments
  (`<item_id>`, `<to_tier>`) are read via additional
  `args.get(N)` calls in the new `run_migrate` helper.
- `aegis::TenantId` already a dependency.
- `std::time::SystemTime::now()` for the `migrated_at`
  argument.
- No new external dependencies. No new internal crate
  dependencies.

### Slice

`slices/slice-01-migrate-subcommand-moves-item.md`
