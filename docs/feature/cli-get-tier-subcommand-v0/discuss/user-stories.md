<!-- markdownlint-disable MD024 -->

# User Stories — `cli-get-tier-subcommand-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. This change introduces ONE new
  CLI subcommand (`get-tier`) and ONE new free function in the
  `kaleidoscope_cli` library; no new trait is introduced and no
  existing trait is modified. The existing
  `cinder::TieringStore::get_tier(tenant, &ItemId) -> Option<Tier>`
  method (`crates/cinder/src/store.rs:85`) is the only Cinder API
  consumed.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions
  with `// Given / // When / // Then` comment blocks, not Gherkin
  `.feature` files. The Given/When/Then text in the UAT Scenarios
  sections below is the specification; DISTILL translates it into
  `#[test]` functions in
  `crates/kaleidoscope-cli/tests/get_tier_subcommand.rs` (NEW file,
  per `wave-decisions.md` D-NewTestFile). The harness mirrors the
  pattern already in `crates/kaleidoscope-cli/tests/list_items_subcommand.rs`
  and `tests/migrate_subcommand.rs`.
- Output-shape contract: stdout is exactly ONE line on success, the
  literal string `tier=<lowercase>\n`, where `<lowercase>` is `hot`
  / `warm` / `cold` rendered through the same lower-case mapping as
  `tier_lowercase` at `crates/kaleidoscope-cli/src/lib.rs:564-570`.
  No header, no JSON, no CSV, no colour codes. Exit code 0.
  (`wave-decisions.md` D-OutputShape.)
- Stderr-on-failure contract: on `get_tier(tenant, &item) -> None`,
  exit code is non-zero and stderr carries a single line containing
  the substring `unknown item`, the verbatim item id, and the
  verbatim tenant string — mirroring the
  `MigrateError::UnknownItem` `Display` impl at
  `crates/cinder/src/store.rs:55-58`. Stdout is empty. The exact
  byte-level wording is DESIGN-locked per `wave-decisions.md`
  D-StderrWording; the contract here is the SUBSTRING invariant
  only.
- Read-only contract: this subcommand opens ONLY the Cinder store
  under `<data_dir>/cinder.*`. It does NOT open the Lumen store
  under `<data_dir>/lumen.*`. The Cinder WAL+snapshot AND the Lumen
  WAL+snapshot are byte-equivalent before and after every
  invocation (success or unknown-item failure). The `get_tier`
  trait method does not mutate state (`crates/cinder/src/store.rs:154-160`).
  (`wave-decisions.md` D-NoLumenTouch.)
- Tenant-isolation contract: `get-tier acme /tmp/data acme/batch-00042`
  MUST NOT read or report `globex`'s tier metadata even if `globex`
  has placed an item with the same `ItemId` string
  (`acme/batch-00042`) into the same `data_dir`. Inherited from
  `cinder::TieringStore`'s per-tenant isolation invariant
  (`crates/cinder/src/store.rs:71-72`).
- No-flag posture: the `get-tier` subcommand accepts NO optional
  flags in v0. No `--observe-otlp` (`get_tier` is a read with no
  `MetricsRecorder` hook — see `crates/cinder/src/store.rs:154-160`
  — so there is no operationally meaningful OTLP signal to
  attach), no `--json`, no `--format=...`. The positional argument
  shape is fixed: `get-tier <tenant> <data_dir> <item_id>` —
  three positional arguments, in that order.

---

## US-01: Operator confirms a single item's current tier before manual action

### Elevator Pitch

- **Before**: Priya the platform operator runs a multi-tenant
  Kaleidoscope deployment. After `kaleidoscope-cli list-items acme
  /tmp/data cold` shows "47 items in Cold", and
  `kaleidoscope-cli stats acme /tmp/data` reports `hot=5 warm=12
  cold=47`, the natural next question on a specific item is "what
  tier is item `acme/batch-00042` in right now?". Today the only
  way to answer it from the CLI is:

  ```bash
  kaleidoscope-cli list-items acme /tmp/data hot  | grep acme/batch-00042 ||
  kaleidoscope-cli list-items acme /tmp/data warm | grep acme/batch-00042 ||
  kaleidoscope-cli list-items acme /tmp/data cold | grep acme/batch-00042
  ```

  That is THREE subprocess invocations, THREE
  `FileBackedTieringStore::open` calls, THREE full per-tier scans
  via `list_by_tier`, THREE pipes to `grep`. The `||` short-circuit
  also makes the exit code semantics confusing (`grep` returns
  non-zero on no-match, so the final exit code reflects the LAST
  list-items invocation's grep, not "did we find the item
  anywhere"). For a tier-decision question that is operationally
  one read — "look up the tier for one `(tenant, item_id)` pair" —
  this is operator-hostile.

- **After**: Priya runs:

  ```text
  kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042
  ```

  Stdout, in milliseconds, prints exactly one line:

  ```text
  tier=hot
  ```

  Exit code 0. One subprocess invocation. One
  `FileBackedTieringStore::open` call. One `get_tier(tenant, &item)`
  call returning `Option<Tier>` in O(1). She can pipe it into a
  scripted assertion:

  ```bash
  test "$(kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042)" = "tier=hot"
  ```

  When she queries an item that was never placed —
  `kaleidoscope-cli get-tier acme /tmp/data acme/batch-00099` —
  she gets a non-zero exit and a stderr line containing
  `unknown item "acme/batch-00099" for tenant acme` (mirroring
  `migrate`'s text). Stdout is empty. The Cinder store is
  byte-equivalent before and after (the underlying `get_tier` is
  read-only by construction).

- **Decision enabled**: Priya can decide and execute three
  operationally distinct uses from one CLI invocation:
  1. "Before I run `migrate acme /tmp/data acme/batch-00042 cold`,
     confirm that the item is in Hot or Warm (not already in Cold)"
     — pre-flight check for the manual migrate workflow.
  2. "In my deployment pipeline, assert that `acme/batch-00042` is
     in Cold before declaring the migration job successful" —
     scripted assertion in CI / a runbook.
  3. "An alert just fired naming `acme/batch-00042`. Audit the
     specific item: what tier is it in right now?" — incident-time
     read on one item without a full per-tier scan.

### Problem

Priya the platform operator already uses six shipped CLI
subcommands (`ingest`, `read`, `stats`, `list-items`, `place`,
`migrate`) daily. The CLI's gap, today, is that it has no
single-invocation surface for the question "what tier is item X in
right now for tenant Y?". Three operationally distinct decisions
all reduce to the same primitive:

1. **Pre-flight before manual migrate**: "Before I run `migrate
   acme /tmp/data acme/batch-00042 cold`, confirm that the item is
   in a non-Cold tier." Today this requires three `list-items`
   invocations + greps.
2. **Scripted assertion in a pipeline**: "After the migration job
   runs, assert that `acme/batch-00042` is in Cold." Today this
   requires the same three-invocation grep chain, plus operators
   have to disambiguate the final exit code (which reflects
   `grep`'s no-match, not "did we find the item").
3. **Incident-time audit on a specific item**: "An alert fired
   naming `acme/batch-00042`. What tier is it in?" Today this
   requires the same chain — three scans even though the operator's
   question is about one item.

All three reduce to "give me one CLI invocation that calls
`TieringStore::get_tier(tenant, &ItemId)` and tells me, on stdout,
the tier (or fails fast on unknown item)." The underlying API
(`cinder::TieringStore::get_tier(...)` at
`crates/cinder/src/store.rs:85`, returning `Option<Tier>`) is
already adequate; the gap is the missing CLI surface.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli ingest`,
`kaleidoscope-cli read`, `kaleidoscope-cli stats`,
`kaleidoscope-cli list-items`, `kaleidoscope-cli place`,
`kaleidoscope-cli migrate` daily (inherited from the predecessor
features) | wants to confirm one specific Cinder item's current
tier without writing Rust, without running three subprocess
`list-items` invocations, and without disambiguating `grep`'s
no-match exit code | uses standard Unix text tools (`grep`,
`cut`, `awk`, `test`) on stdout output, not JSON parsers |
expects fail-fast behaviour (non-zero exit + descriptive stderr)
on unknown item ids | does NOT expect the subcommand to touch
the Lumen store.

### Solution

Add a new positional subcommand to the existing CLI binary:

```text
kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>
```

The dispatch arm in `crates/kaleidoscope-cli/src/main.rs` gains a
new `Some("get-tier")` branch that calls a new library function in
`kaleidoscope_cli`. The library function:

1. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
   recorder)` with a quiescent `CinderRecorder` (the same
   pattern `list_items` uses at `lib.rs:534`). It does NOT open
   the Lumen store.
2. Builds `ItemId::new(item_id_arg.to_string())`.
3. Calls `cinder.get_tier(&tenant, &item)`. The trait method
   returns `Option<Tier>` per `crates/cinder/src/store.rs:85`.
4. If `Some(tier)`: writes ONE line to stdout —
   `tier=<lowercase>\n` — where `<lowercase>` is the same
   `tier_lowercase` rendering as `migrate`'s stdout report. Exits
   0.
5. If `None`: returns a typed error mirroring
   `MigrateError::UnknownItem { tenant, item }`. The CLI
   dispatcher converts this to a non-zero exit + stderr line
   containing the verbatim item id and tenant string and the
   token `unknown item`. Stdout is empty.

The output report uses the same `key=value` aesthetic as `stats`
but with ONE field on a single line (rather than three) because
the report answers exactly ONE question.

The function signature is DESIGN-locked per `wave-decisions.md`
D-FunctionShape. Likely:

```rust
pub fn get_tier(
    tenant: &TenantId,
    data_dir: &Path,
    item_id: &str,
    writer: impl Write,
) -> Result<(), Error>
```

— mirroring `list_items`'s shape at `lib.rs:525-542`.

The `Error` type either reuses the existing
`Error::CinderMigrate(MigrateError)` variant (constructing
`MigrateError::UnknownItem` off-label at the call site) or
introduces a new dedicated variant. The choice is DESIGN's per
`wave-decisions.md` D-ErrorVariant.

### Domain Examples

#### 1. Happy path — Priya confirms `acme/batch-00042` is in Hot before migrating it to Cold

Priya is about to run `migrate acme /tmp/data acme/batch-00042
cold`. She wants to confirm the item is currently in Hot (not
already in Cold, which would make the migrate a no-op faithful
to the underlying API per `wave-decisions.md` D-Idempotent of
`cli-migrate-subcommand-v0`). She runs:

```text
kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042
```

Stdout contains exactly one line:

```text
tier=hot
```

Exit code 0. Stderr is empty. She proceeds with the migrate.

#### 2. Happy path (cold) — Priya audits `acme/batch-00007` after an alert mentions it

An alert fires naming `acme/batch-00007`. The runbook says "if the
item is in Cold and the alert is about a hot-path latency
regression, the alert is a false positive; close it." Priya
runs:

```text
kaleidoscope-cli get-tier acme /tmp/data acme/batch-00007
```

Stdout:

```text
tier=cold
```

Exit code 0. She closes the alert as a false positive.

#### 3. Scripted assertion (warm) — Priya pipes into `test`

In her deployment pipeline she has a CI step asserting that a
specific item was migrated to Warm by the auto-tiering job. She
writes:

```bash
expected="tier=warm"
actual=$(kaleidoscope-cli get-tier acme /tmp/data acme/batch-00050)
if [ "$actual" != "$expected" ]; then
  echo "ASSERTION FAILED: $actual != $expected" >&2
  exit 1
fi
```

Stdout from `get-tier` is the literal `tier=warm` line; the
shell-side `[ "$actual" != "$expected" ]` check passes on the
exact byte match because both strings end with the lower-case
tier name and no trailing whitespace beyond the line terminator
(which `$(...)` strips).

#### 4. Error case (unknown item) — Priya fat-fingers the item id

Priya types `acme/batch-00099` when she meant `acme/batch-00009`:

```text
kaleidoscope-cli get-tier acme /tmp/data acme/batch-00099
```

Stdout is empty. Stderr contains a single line containing the
substring `unknown item`, the verbatim item id
`acme/batch-00099`, and the verbatim tenant `acme`. Exit code is
non-zero. The Cinder store under `/tmp/data/cinder.*` is
byte-equivalent before and after the call (the underlying
`get_tier` is read-only; the `None` arm at
`crates/cinder/src/store.rs:154-160` returns without any
side-effect).

#### 5. Tenant-isolation case — Priya queries `acme/batch-00042` for `acme`, `globex`'s same-named item is not visible

Priya runs a deployment where `acme` and `globex` both have an
item id `acme/batch-00042` placed in Cinder under the same
`/tmp/data` (Hot for `acme`, Warm for `globex`). The placement key
is `(TenantId, ItemId)` per `crates/cinder/src/store.rs:119`. She
runs:

```text
kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042
```

Stdout:

```text
tier=hot
```

The `globex` placement (`acme/batch-00042` in Warm) is not
referenced; if Priya immediately re-runs the command with
`tenant=globex`, stdout reads `tier=warm`. The two reads return
different tiers for the same `ItemId` string because the
placement key is `(TenantId, ItemId)`.

### UAT Scenarios (BDD)

#### Scenario: Operator queries an item placed in Hot — sees `tier=hot` on stdout

```text
Given Priya has placed item acme/batch-00042 into Cinder under /tmp/k-data with tier Hot for tenant acme (via a direct FileBackedTieringStore::open + place call)
When Priya invokes the get-tier subcommand with arguments (acme, /tmp/k-data, acme/batch-00042) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `tier=hot\n`
And the captured stderr is empty
And exit code is 0
And cinder.get_tier(acme, acme/batch-00042) at /tmp/k-data returns Some(Tier::Hot) (unchanged from the pre-call state)
And the Lumen store under /tmp/k-data/lumen.* is byte-equivalent before and after the call
```

#### Scenario: Operator queries an item placed in Warm — sees `tier=warm` on stdout

```text
Given Priya has placed item acme/batch-00050 into Cinder under /tmp/k-data with tier Warm for tenant acme
When Priya invokes the get-tier subcommand with arguments (acme, /tmp/k-data, acme/batch-00050) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `tier=warm\n`
And the captured stderr is empty
And exit code is 0
And cinder.get_tier(acme, acme/batch-00050) at /tmp/k-data returns Some(Tier::Warm)
```

#### Scenario: Operator queries an item placed in Cold — sees `tier=cold` on stdout

```text
Given Priya has placed item acme/batch-00007 into Cinder under /tmp/k-data with tier Cold for tenant acme
When Priya invokes the get-tier subcommand with arguments (acme, /tmp/k-data, acme/batch-00007) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `tier=cold\n`
And the captured stderr is empty
And exit code is 0
And cinder.get_tier(acme, acme/batch-00007) at /tmp/k-data returns Some(Tier::Cold)
```

#### Scenario: Operator queries an item that was never placed — fail-fast with stderr naming the missing item

```text
Given the Cinder store under /tmp/k-data has NO placement for the (tenant=acme, item=acme/batch-00099) pair (no prior place call for that item id under that tenant)
And the Cinder store under /tmp/k-data has at least one OTHER placement for tenant acme (e.g. acme/batch-00001 in Hot) so the store opens cleanly
When Priya invokes the get-tier subcommand with arguments (acme, /tmp/k-data, acme/batch-00099) and a captured stdout sink and a captured stderr sink
Then the call returns Err
And the captured stdout is empty
And the captured stderr contains the substring `unknown item`
And the captured stderr contains the substring `acme/batch-00099`
And the captured stderr contains the substring `acme`
And exit code is non-zero
And the Cinder store at /tmp/k-data is byte-equivalent before and after the call
```

#### Scenario: Tenant isolation — querying `acme/batch-00042` for `acme` does not surface `globex`'s same-named item

```text
Given Priya has placed item acme/batch-00042 in Cinder under /tmp/k-data with tier Hot for tenant acme
And Priya has placed item acme/batch-00042 in Cinder under /tmp/k-data with tier Warm for tenant globex
When Priya invokes the get-tier subcommand with arguments (acme, /tmp/k-data, acme/batch-00042) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `tier=hot\n`
And subsequently invoking the get-tier subcommand with arguments (globex, /tmp/k-data, acme/batch-00042) returns Ok with stdout `tier=warm\n`
And the two reads return DIFFERENT tiers for the same ItemId string because the placement key is (TenantId, ItemId)
```

### Acceptance Criteria

- [ ] When tenant `acme` has item `acme/batch-00042` placed in tier
  Hot in `/tmp/k-data/cinder.*`, invoking the get-tier subcommand
  with `(acme, /tmp/k-data, acme/batch-00042)` produces exactly the
  stdout line `tier=hot\n`, empty stderr, and exit code 0.
- [ ] When tenant `acme` has item `acme/batch-00050` placed in tier
  Warm, the same invocation shape produces exactly the stdout line
  `tier=warm\n`, empty stderr, and exit code 0.
- [ ] When tenant `acme` has item `acme/batch-00007` placed in tier
  Cold, the same invocation shape produces exactly the stdout line
  `tier=cold\n`, empty stderr, and exit code 0.
- [ ] When tenant `acme` has NO item `acme/batch-00099` placed in
  `/tmp/k-data/cinder.*`, invoking the get-tier subcommand with
  `(acme, /tmp/k-data, acme/batch-00099)` produces empty stdout, a
  non-empty stderr line containing the substrings `unknown item`,
  `acme/batch-00099`, and `acme`, and a non-zero exit code. The
  Cinder store is byte-equivalent before and after.
- [ ] When tenants `acme` and `globex` both have an item id
  `acme/batch-00042` placed in the same `/tmp/k-data` (Hot for
  `acme`, Warm for `globex`), invoking the get-tier subcommand
  with `(acme, /tmp/k-data, acme/batch-00042)` produces stdout
  `tier=hot\n` and a subsequent invocation with `(globex, ...)`
  produces stdout `tier=warm\n`.
- [ ] The Lumen store under `<data_dir>/lumen.*` is byte-equivalent
  before and after every `get-tier` subcommand invocation (success
  or unknown-item failure). The subcommand never opens the Lumen
  store (`wave-decisions.md` D-NoLumenTouch).
- [ ] The Cinder store under `<data_dir>/cinder.*` is byte-
  equivalent before and after every `get-tier` subcommand
  invocation. `get_tier` is a read; no mutation occurs.
- [ ] The new acceptance test file
  `crates/kaleidoscope-cli/tests/get_tier_subcommand.rs` is added
  (NEW file, mirroring the harness pattern of
  `tests/list_items_subcommand.rs` / `tests/migrate_subcommand.rs`)
  with assertions covering the five UAT scenarios above.
- [ ] The existing locked acceptance test files
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/list_items_subcommand.rs`, `tests/migrate_subcommand.rs`,
  `tests/place_subcommand.rs`, `tests/observe_otlp_*.rs`) continue
  to pass green UNMODIFIED under `cargo test --package
  kaleidoscope-cli`.
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs` is
  updated to document the new `get-tier` subcommand's positional
  argument shape. (Optional — DESIGN's call on exact wording.)
- [ ] No new external crate dependency is added to
  `crates/kaleidoscope-cli/Cargo.toml`. The only new
  `Cargo.toml` change is one `[[test]]` entry for the new test
  file.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the stdout
  byte level on the new
  `kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>` CLI
  invocation.
- **Does what**: receives a one-line stdout report of the form
  `tier=<lowercase>\n` on success, or a non-zero exit with a
  stderr line containing the substring `unknown item` plus the
  verbatim item id plus the verbatim tenant on the unknown-item
  case, on a single CLI invocation, without writing Rust code,
  without three `list-items` invocations, and without
  disambiguating `grep`'s no-match exit code.
- **By how much**: 100% of `get-tier` invocations against placed
  items produce the exact stdout report line for the tier
  returned by `cinder.get_tier(tenant, &item)` (OK1); 100% of
  invocations against unplaced item ids produce a non-zero exit +
  stderr containing the verbatim item id + an unchanged Cinder
  store (OK2); 100% of invocations preserve the per-tenant
  isolation invariant — same `ItemId` string under different
  tenants returns the respective per-tenant tier (OK3); 100% of
  invocations (success or failure) produce a byte-equivalent
  Lumen store before and after.
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/get_tier_subcommand.rs` covering
  the three KPIs across the five UAT scenarios above. PLUS the
  existing locked acceptance test files continuing to pass green
  UNMODIFIED.
- **Baseline**: 0% — today operators answer the same question via
  three `list-items` invocations and a `grep` chain, with
  ambiguous final exit code semantics.

Maps to OK1-CLI-get-tier-success (principal),
OK2-CLI-get-tier-unknown-item-fail-fast, and
OK3-CLI-get-tier-tenant-isolation in `outcome-kpis.md`.

### Technical Notes

- The exact library function shape is DESIGN-locked per
  `wave-decisions.md` D-FunctionShape. Likely a new free
  function in `crates/kaleidoscope-cli/src/lib.rs` with a
  signature like
  `pub fn get_tier(tenant: &TenantId, data_dir: &Path, item_id: &str, writer: impl Write) -> Result<(), Error>`.
- The function internally constructs
  `FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))`
  identically to the pattern at
  `crates/kaleidoscope-cli/src/lib.rs:534`. It calls
  `cinder.get_tier(&tenant, &item)` once. On `Some(tier)` it
  writes `tier=<lowercase>\n` and returns Ok. On `None` it
  returns the typed unknown-item error.
- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — new
  free function `get_tier(...)`, possibly a new `Error` variant
  (`CinderUnknownItem { tenant: TenantId, item: ItemId }` —
  DESIGN's call per `wave-decisions.md` D-ErrorVariant).
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — new
  `Some("get-tier") => run_get_tier(&args)` dispatch arm; new
  `run_get_tier(...)` function mirroring the shape of
  `run_list_items(...)`. `print_usage` gains a
  `kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>`
  block.
- New test file:
  `crates/kaleidoscope-cli/tests/get_tier_subcommand.rs`.
  Mirrors the harness pattern of `tests/list_items_subcommand.rs`
  and `tests/migrate_subcommand.rs`.
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "get_tier_subcommand", path =
  "tests/get_tier_subcommand.rs"`. The `cinder` dependency is
  already present.
- DO NOT modify any locked test file. This is a hard constraint
  from the task brief and is restated as `wave-decisions.md`
  D-LockedTests.
- Slice tag: not `@infrastructure` — this story directly enables
  three operator-visible uses on a real CLI surface
  (`kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>`).

### Dependencies

- `cinder::TieringStore::get_tier(tenant, item) -> Option<Tier>`
  already exists at `crates/cinder/src/store.rs:85`.
- `cinder::FileBackedTieringStore` already implements
  `TieringStore` and is already constructed by `list_items()` at
  `crates/kaleidoscope-cli/src/lib.rs:534`.
- `cinder::CinderRecorder` is the quiescent recorder used by
  `list_items`; already a `kaleidoscope-cli` dependency.
- `cinder::Tier` enum already imported.
- `cinder::ItemId::new(id)` already imported.
- The existing `cinder_base(data_dir)` helper at
  `crates/kaleidoscope-cli/src/lib.rs:122-124` is reused
  unchanged.
- The existing `tier_lowercase(tier) -> &'static str` helper at
  `crates/kaleidoscope-cli/src/lib.rs:564-570` is reused unchanged
  (lifted to `pub(crate)` if needed — DESIGN's call).
- `aegis::TenantId` already a dependency.
- No `std::time::SystemTime` call needed (this is a read).
- No new external dependencies. No new internal crate
  dependencies.

### Slice

`slices/slice-01-get-tier-subcommand-reports-current-tier.md`
