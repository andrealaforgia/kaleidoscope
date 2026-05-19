<!-- markdownlint-disable MD024 -->

# User Stories — `cli-list-items-subcommand-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits
  where polymorphism is genuinely needed. This change introduces
  ONE new CLI subcommand (`list-items`) and ONE new free function
  in the `kaleidoscope_cli` library; no new trait is introduced
  and no existing trait is modified. The existing
  `cinder::TieringStore::list_by_tier(tenant, tier)` method
  (`crates/cinder/src/store.rs:102`, used in production at
  `crates/kaleidoscope-cli/src/lib.rs:383` inside
  `stats_with_tiers`) is the only Cinder API consumed.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]`
  functions with `// Given / // When / // Then` comment blocks,
  not Gherkin `.feature` files. The Given/When/Then text in the
  UAT Scenarios sections below is the specification; DISTILL
  translates it into `#[test]` functions in
  `crates/kaleidoscope-cli/tests/list_items_subcommand.rs` (NEW
  file, per `wave-decisions.md` D-NewTestFile). The harness
  mirrors the pattern already in
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` and the
  six other `tests/*.rs` files in the cluster.
- Output-shape contract: on success, stdout is exactly N lines,
  one item id per line in lower-case ASCII sort order (lexicographic
  byte comparison via `slice::sort`), each terminated by `\n`. N
  equals `cinder.list_by_tier(tenant, tier).len()` at call time.
  When N is 0 (the tenant has no items in the queried tier), stdout
  is empty (no header, no trailing newline, no "0 items" placeholder
  line). No header, no JSON, no CSV, no colour codes. Exit code 0.
- Stderr-on-failure contract: on invalid `<tier>` argument (any
  spelling other than literal lower-case `hot` / `warm` / `cold`),
  exit code is non-zero and stderr carries a single line containing
  the invalid value verbatim. Stdout is empty. The Cinder store is
  never opened on this branch (the tier-parse error short-circuits
  before `FileBackedTieringStore::open` is called). This mirrors
  the established fail-fast posture of the `migrate` subcommand's
  invalid-tier handling (`crates/kaleidoscope-cli/src/lib.rs:432-434`).
- Tier-argument shape: `<tier>` is accepted ONLY in lower-case as
  one of `hot` / `warm` / `cold`. Upper-case (`HOT` / `Hot`),
  mixed-case, or any other spelling is rejected with a non-zero
  exit and a stderr line naming the invalid value
  (`wave-decisions.md` D-LowerCase). Same set the rendering side
  and the `migrate` subcommand already enforce
  (`crates/kaleidoscope-cli/src/lib.rs:475-482, :489-495`).
- Read-only contract: this subcommand opens ONLY the Cinder store
  under `<data_dir>/cinder.*` and performs ONLY a read
  (`list_by_tier`). It does NOT call `place`, `migrate`, or
  `evaluate_at`. The Cinder WAL+snapshot is byte-equivalent before
  and after every invocation, success or failure
  (`wave-decisions.md` D-ReadOnly). It does NOT open the Lumen
  store under `<data_dir>/lumen.*`; the Lumen WAL+snapshot is
  byte-equivalent before and after every call
  (`wave-decisions.md` D-NoLumenTouch).
- Tenant-isolation contract: `list-items acme /tmp/data cold` MUST
  NOT include items belonging to any other tenant in the output,
  even if those tenants share an item id string with `acme` items.
  Inherited from `cinder::TieringStore`'s per-tenant isolation
  invariant (`crates/cinder/src/store.rs:71-72`) and from the
  `list_by_tier(tenant, tier)` filter at
  `crates/cinder/src/store.rs:194-196`.
- Determinism contract: the same `(tenant, data_dir, tier)` tuple
  invoked twice in succession produces byte-identical stdout. This
  matters because `cinder::InMemoryTieringStore::list_by_tier`
  at `crates/cinder/src/store.rs:190-198` iterates a `HashMap`
  whose iteration order is randomised per process. The CLI sorts
  the returned `Vec<ItemId>` at the boundary so the operator sees
  a deterministic, lexicographically-sorted list across runs and
  across machines (`wave-decisions.md` D-Sort).
- No-flag posture: the `list-items` subcommand accepts NO optional
  flags in v0. No `--observe-otlp` (the Cinder
  `MetricsRecorder` trait at `crates/cinder/src/metrics.rs` has no
  `record_list` method — `list_by_tier` is a pure read with
  nothing operator-visible to record), no `--json`, no
  `--format=...`, no pagination, no cross-tenant aggregate. The
  positional argument shape is fixed: `list-items <tenant>
  <data_dir> <tier>` — three positional arguments in that order
  (`wave-decisions.md` D-OutOfScope-*).

---

## US-01: Operator lists every item Cinder currently has in a given tier for a given tenant

### Elevator Pitch

- **Before**: Priya the platform operator has just run
  `kaleidoscope-cli stats acme /tmp/data` and seen the Cinder
  tier distribution:

  ```text
  records=5234
  earliest=2026-05-12T08:00:00.000000000Z
  latest=2026-05-19T13:00:00.000000000Z
  hot=5 warm=12 cold=47
  ```

  The next natural operator question is "which items are those
  47 cold ones?" Today that question is unanswerable from the
  CLI. The `stats` subcommand surfaces COUNTS via
  `list_by_tier(tenant, tier).len()` (`crates/kaleidoscope-cli/src/lib.rs:383`),
  but it does NOT surface the item ids themselves. To answer
  "which items?", Priya has to write a Rust harness that opens
  `FileBackedTieringStore`, calls `list_by_tier(&tenant, Tier::Cold)`,
  iterates the returned `Vec<ItemId>`, and prints each one. Three
  operationally distinct decisions all reduce to that same
  primitive ("show me every item this tenant has in this tier"):
  1. "Of the 47 cold items, which ones should I manually migrate
     back to warm?" (manual rebalancing follow-up).
  2. "Are these the items I'm worried about?" (sanity check against
     the tenant manifest — does the set of cold items match the
     batches the operator EXPECTED to be cold?).
  3. "Pipe this list into `migrate` so a shell loop walks each item
     between tiers" (scripted pipelines, e.g. `... list-items acme
     /tmp/data cold | xargs -I {} kaleidoscope-cli migrate acme
     /tmp/data {} warm`).

- **After**: Priya runs:

  ```text
  kaleidoscope-cli list-items acme /tmp/data cold
  ```

  Stdout, in milliseconds, prints exactly N lines (where N is the
  count `stats` already reported as `cold=N`), each line carrying
  one ItemId, sorted lexicographically. For example, with 3 cold
  items:

  ```text
  acme/batch-00007
  acme/batch-00041
  acme/batch-00099
  ```

  Exit code 0. Stderr is empty. The lines are deterministically
  sorted (lexicographic byte comparison) so a second invocation of
  the same command produces byte-identical stdout, regardless of
  the underlying `HashMap` iteration order which is randomised per
  process (`crates/cinder/src/store.rs:194-196`). The output is
  immediately pipeable: `kaleidoscope-cli list-items acme
  /tmp/data cold | xargs -I {} kaleidoscope-cli migrate acme
  /tmp/data {} warm` walks each cold item back to warm without
  ever leaving the shell.

  When the tenant has zero items in the queried tier (e.g.
  `list-items acme /tmp/data hot` when `stats` already showed
  `hot=` was absent because the count was zero — recall that
  `stats_with_tiers` at
  `crates/kaleidoscope-cli/src/lib.rs:382-387` emits no line for
  zero-count tiers), stdout is empty, exit code 0. There is no
  "0 items" placeholder line because empty-stdout is the natural
  shell-pipeline signal for "nothing to iterate" (the downstream
  `xargs` is a no-op).

  When Priya types an invalid tier name — `kaleidoscope-cli
  list-items acme /tmp/data COLD` (upper-case) or `... lukewarm`
  (typo) — she gets a non-zero exit and a stderr line that names
  the invalid value she typed. Stdout is empty. The Cinder store
  is unchanged (the parse error fires BEFORE
  `FileBackedTieringStore::open` is called).

- **Decision enabled**: Priya can decide and execute three
  operationally distinct workflows from one CLI invocation:
  1. "Show me every cold item for `acme` so I can identify which
     ones to manually migrate back to warm" (manual rebalancing
     follow-up to `stats`).
  2. "List every warm item for `acme` and diff it against the
     tenant manifest I have in my notes" (sanity check).
  3. "Pipe every cold item through `migrate` to walk them back to
     warm in one shell loop" (scripted pipeline:
     `kaleidoscope-cli list-items acme /tmp/data cold | xargs -I
     {} kaleidoscope-cli migrate acme /tmp/data {} warm`).

### Problem

Priya the platform operator already uses the five shipped CLI
subcommands (`ingest`, `read`, `stats`, `migrate`, `--help`)
daily. The CLI's gap, today, is that after `stats` shows a
non-zero count for a tier (e.g. `cold=47`), there is no surface
for enumerating the specific item ids in that tier. Three
operationally distinct decisions all reduce to the same
primitive ("show me every item this tenant has in this tier"):

1. **Manual rebalancing follow-up**: "Of the 47 cold items,
   which ones should I manually migrate back to warm?" Today
   this requires writing a Rust harness or grovelling through
   the Cinder snapshot file format directly.
2. **Sanity check against tenant manifest**: "Are these the
   items I'm worried about?" — the operator has an out-of-band
   list of batches she EXPECTS to be cold (from a deployment
   spreadsheet, a runbook, or a tenant-specific monitoring
   alert) and wants to diff it against what Cinder actually
   reports. No CLI surface today.
3. **Scripted pipelines**: `... list-items acme /tmp/data cold
   | xargs -I {} kaleidoscope-cli migrate acme /tmp/data {}
   warm` — drive `migrate` from the output of an enumeration.
   No CLI surface today.

All three reduce to "give me one CLI invocation that calls
`TieringStore::list_by_tier(tenant, tier)`, sorts the returned
`Vec<ItemId>`, and prints each item id on its own line to
stdout." The underlying API
(`cinder::TieringStore::list_by_tier(tenant, tier)` at
`crates/cinder/src/store.rs:102`, returning `Vec<ItemId>`) is
already adequate; it is the precise method `stats_with_tiers`
calls at `crates/kaleidoscope-cli/src/lib.rs:383` for the
`hot=N` / `warm=N` / `cold=N` count lines. The gap is the
missing CLI surface that exposes the SAME read to the
operator at the item-id granularity.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli
ingest`, `kaleidoscope-cli read`, `kaleidoscope-cli stats`,
`kaleidoscope-cli migrate` daily (inherited from the seven
predecessor features in the `kaleidoscope-cli` cluster) | wants
to enumerate every item a tenant has in a given Cinder tier
without writing Rust or parsing Cinder snapshot files directly
| uses standard Unix text tools (`grep`, `wc -l`, `xargs`,
`sort`, `comm`, `diff`) on stdout output, not JSON parsers |
expects fail-fast behaviour (non-zero exit + descriptive
stderr) on invalid tier names | expects determinism (the same
invocation produces byte-identical stdout across runs,
regardless of `HashMap` iteration order) | does NOT expect the
subcommand to touch the Lumen store or mutate any Cinder state.

### Solution

Add a new positional subcommand to the existing CLI binary:

```text
kaleidoscope-cli list-items <tenant_id> <data_dir> <tier>
```

The dispatch arm in `crates/kaleidoscope-cli/src/main.rs` (the
match block at lines 52-66) gains a new `Some("list-items")`
branch that calls a new library function in `kaleidoscope_cli`.
The library function:

1. Parses `<tier>` from its argv form to a `cinder::Tier`
   value using the SAME tier-from-lowercase parser shape
   already used by `migrate` at
   `crates/kaleidoscope-cli/src/lib.rs:475-482` (the existing
   `parse_tier` helper). Accepts ONLY `hot` / `warm` / `cold`.
   Any other spelling (upper-case `HOT`, mixed-case `Hot`,
   typo `lukewarm`) yields a parse error
   (`Error::InvalidTier { value }`) containing the offending
   value verbatim. The parse runs BEFORE the Cinder store is
   opened — invalid tier values never touch the filesystem.
2. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
   recorder)` with a quiescent `cinder::NoopRecorder`
   (`CinderRecorder` in the existing code — same pattern
   `stats_with_tiers` uses at
   `crates/kaleidoscope-cli/src/lib.rs:377-378`). It does NOT
   open the Lumen store.
3. Calls `cinder.list_by_tier(&tenant, tier)` to obtain a
   `Vec<ItemId>` of every item this tenant currently has in
   the given tier.
4. Sorts the returned vector lexicographically by item id byte
   sequence (`vec.sort()` — `ItemId` implements `Ord` via its
   inner `String`). This is the DETERMINISM step: without it,
   `cinder::InMemoryTieringStore::list_by_tier` at
   `crates/cinder/src/store.rs:190-198` returns items in
   `HashMap` iteration order which is randomised per process,
   so two invocations on the same data would produce stdout
   with the same lines in different orders.
5. Writes one line per item id to the supplied writer:
   `writeln!(writer, "{}", item_id)` per `ItemId` in the sorted
   vec. For an empty vec (N=0), nothing is written: stdout is
   empty.
6. Exits 0.

The output uses one-line-per-item-id (NO key=value, NO header)
because the natural operator consumer is a shell pipeline
(`xargs -I {} kaleidoscope-cli migrate ... {} ...`) that
expects one record per line on stdin. A `key=value` shape
would force operators to `cut -d= -f2` to extract the bare item
id, adding shell ceremony with no upside. The `migrate` and
`stats` subcommands use `key=value` because their output is a
SINGLE event report (one line per invocation) — `list-items`
output is structurally different (one line per RECORD).

The function signature is DESIGN-locked per
`wave-decisions.md` D-FunctionShape. Likely a new free function:

```text
pub fn list_items(
    tenant: &TenantId,
    data_dir: &Path,
    tier_arg: &str,
    writer: impl Write,
) -> Result<usize, Error>
```

returning `Ok(N)` where N is the number of lines written (the
binary's `run_list_items` helper writes a `stderr` summary line
similar to `stats`'s `stats ok: records=N` and `read`'s `read ok:
records=N` — DESIGN's call on whether to emit such a line for
this subcommand, per D-StderrSummary). The wire-observable
contract (stdout content on success, stderr content on failure,
exit code, no mutation, no Lumen touch) is what matters here.

The `Error` type adds NO new variants. The invalid-tier case
reuses the existing `Error::InvalidTier { value: String }`
variant at `crates/kaleidoscope-cli/src/lib.rs:79-81`; the
store-open error reuses the existing
`Error::CinderOpen(MigrateError)` variant at
`crates/kaleidoscope-cli/src/lib.rs:77`; the I/O error from
`writeln!` reuses the existing `Error::Io(std::io::Error)`
variant at `crates/kaleidoscope-cli/src/lib.rs:82`.

### Domain Examples

#### 1. Happy path — Priya enumerates the three cold items for `acme`

Priya has ingested seven days of `acme` traffic via the CLI's
existing `ingest` subcommand and has manually migrated three
specific items to Cold via `migrate`:

```text
kaleidoscope-cli migrate acme /tmp/data acme/batch-00007 cold
kaleidoscope-cli migrate acme /tmp/data acme/batch-00099 cold
kaleidoscope-cli migrate acme /tmp/data acme/batch-00041 cold
```

She runs `kaleidoscope-cli stats acme /tmp/data` and sees the
final line `cold=3` (per `stats_with_tiers` at
`crates/kaleidoscope-cli/src/lib.rs:382-387`). She wants to
enumerate the three cold items by id:

```text
kaleidoscope-cli list-items acme /tmp/data cold
```

Stdout contains exactly three lines, each one item id, sorted
lexicographically:

```text
acme/batch-00007
acme/batch-00041
acme/batch-00099
```

Exit code 0. Stderr is empty. Note: the order on stdout is
`00007` then `00041` then `00099` — lexicographic sort, NOT the
insertion order (`00007`, `00099`, `00041`). A second invocation
of the same command produces byte-identical stdout. The
underlying `cinder.list_by_tier(acme, Tier::Cold)` returned the
same three items but in some `HashMap` iteration order, which
the CLI sorted at the boundary.

The Lumen side (`/tmp/data/lumen.*`) is byte-equivalent before
and after the call (no Lumen open). The Cinder side
(`/tmp/data/cinder.*`) is byte-equivalent before and after the
call (read-only — no `place`, no `migrate`, no `evaluate_at`).

#### 2. Edge case (empty result) — Priya lists items for a tier that has none

Priya has ingested data but never migrated any item to Warm.
Her `stats` output therefore omits a `warm=` line (per
`stats_with_tiers` at
`crates/kaleidoscope-cli/src/lib.rs:382-387`: only tiers with a
non-zero count emit a line). She still types the list-items
command, perhaps because she's confirming the tenant manifest
contains no warm items:

```text
kaleidoscope-cli list-items acme /tmp/data warm
```

Stdout is EMPTY (zero bytes, no trailing newline). Exit code 0.
Stderr is empty. The empty-stdout signal is the natural
shell-pipeline behaviour for "nothing to iterate": `... | xargs
-I {} migrate ...` is a no-op, `... | wc -l` reports `0`, `...
| grep ...` exits non-zero (per `grep` semantics, which is
discriminating for downstream scripts).

There is no "0 items" placeholder line and no header. The
absence of bytes IS the result.

#### 3. Edge case (single item, ordering trivial) — Priya lists the only hot item for `globex`

A fresh deployment has placed exactly one hot item for tenant
`globex`:

```text
kaleidoscope-cli list-items globex /tmp/data hot
```

Stdout contains exactly one line:

```text
globex/batch-00000
```

Exit code 0. The "sort" step at the boundary is a no-op for a
single-element vector, but the contract is still satisfied: one
line per item, lexicographic sort (trivial for N=1).

#### 4. Edge case (tenant isolation) — Priya lists `acme`'s cold items, `globex`'s same-named items are absent

Priya runs a deployment where `acme` and `globex` both have an
item id `shared/batch-00042` placed in Cinder under the same
`/tmp/data`. `acme/shared/batch-00042` is Cold; `globex/shared/batch-00042`
is also Cold. She runs:

```text
kaleidoscope-cli list-items acme /tmp/data cold
```

Stdout contains a line `shared/batch-00042` (and any other cold
items for `acme`, sorted lex), but does NOT contain
`globex/shared/batch-00042` even though that string is
identical. The `list_by_tier(tenant, tier)` filter at
`crates/cinder/src/store.rs:194-196` restricts to entries whose
key is `(acme, *)`; `globex`'s entries are excluded by
construction.

This is the tenant-isolation invariant inherited from
`cinder::TieringStore`'s per-tenant key
(`crates/cinder/src/store.rs:71-72, :119`).

#### 5. Error case (invalid tier argument) — Priya types upper-case `COLD`

Priya types `COLD` instead of `cold`:

```text
kaleidoscope-cli list-items acme /tmp/data COLD
```

Stdout is empty. Stderr contains a single line that names the
invalid value `COLD`. Exit code is non-zero. The Cinder store
is byte-equivalent before and after the call (the parse error
fires BEFORE `FileBackedTieringStore::open` is called — the
parse precedes the open, identically to the
`migrate` subcommand's parse-then-open ordering at
`crates/kaleidoscope-cli/src/lib.rs:432-446`).

The same outcome obtains for any non-`hot`/`warm`/`cold` value:
`Hot` (mixed-case), `WARM`, `cool`, `lukewarm`, `frozen`, empty
string, leading/trailing whitespace. The acceptance test
exercises one representative upper-case value plus one typo.

### UAT Scenarios (BDD)

#### Scenario: Operator lists every cold item for `acme` — sees N lines on stdout, sorted lex

```text
Given Priya has placed items acme/batch-00099, acme/batch-00007, and acme/batch-00041 into Cinder under /tmp/data with tier Cold for tenant acme (via direct FileBackedTieringStore::open + place calls, intentionally NOT in lexicographic order to exercise the sort step)
And Priya has placed item acme/batch-00050 into Cinder under /tmp/data with tier Hot for tenant acme (a hot item that MUST NOT appear in the cold list)
When Priya invokes the list-items subcommand with arguments (acme, /tmp/data, cold) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the bytes `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`
And the captured stderr is empty (or contains only a `list-items ok: items=3` summary line if DESIGN chooses to emit one — DESIGN's call per D-StderrSummary; the AC is on stdout shape, not stderr summary presence)
And exit code is 0
And cinder.list_by_tier(acme, Tier::Cold) at /tmp/data after the call returns the same three items (in any order), confirming the Cinder store was not mutated
And the Cinder store under /tmp/data/cinder.* is byte-equivalent before and after the call
And the Lumen store under /tmp/data/lumen.* is byte-equivalent before and after the call
```

#### Scenario: Operator lists items for an empty tier — sees empty stdout, exit 0

```text
Given Priya has placed item acme/batch-00050 into Cinder under /tmp/data with tier Hot for tenant acme (so the Cinder store opens cleanly with at least one entry)
And Priya has NOT placed any item for tenant acme in tier Warm
When Priya invokes the list-items subcommand with arguments (acme, /tmp/data, warm) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout is empty (zero bytes)
And exit code is 0
And cinder.list_by_tier(acme, Tier::Warm) at /tmp/data returns an empty Vec
And the Cinder store under /tmp/data/cinder.* is byte-equivalent before and after the call
```

#### Scenario: Operator types an invalid tier value — fail-fast with stderr naming the invalid value

```text
Given the Cinder store under /tmp/data has item acme/batch-00042 placed for tenant acme in tier Hot
When Priya invokes the list-items subcommand with arguments (acme, /tmp/data, COLD) and a captured stdout sink and a captured stderr sink
Then the call returns Err
And the captured stdout is empty
And the captured stderr contains a line that contains the substring `COLD`
And exit code is non-zero
And cinder.list_by_tier(acme, Tier::Hot) at /tmp/data still returns a Vec containing acme/batch-00042 (unchanged from the pre-call state, demonstrating no mutation occurred)
```

#### Scenario: Tenant isolation — listing `acme`'s cold items does not surface `globex`'s same-named cold items

```text
Given Priya has placed item shared/batch-00042 in Cinder under /tmp/data with tier Cold for tenant acme
And Priya has placed item shared/batch-00042 in Cinder under /tmp/data with tier Cold for tenant globex
When Priya invokes the list-items subcommand with arguments (acme, /tmp/data, cold) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `shared/batch-00042\n` (one line, one item id, the only entry because only acme has cold items — globex's same-named item is filtered out by the per-tenant key)
And exit code is 0
And cinder.list_by_tier(globex, Tier::Cold) at /tmp/data still returns a Vec containing shared/batch-00042 (unchanged from the pre-call state)
```

#### Scenario: Determinism — the same invocation produces byte-identical stdout across two runs

```text
Given Priya has placed items acme/batch-00099, acme/batch-00007, and acme/batch-00041 into Cinder under /tmp/data with tier Cold for tenant acme (in non-lex insertion order to exercise the sort step)
When Priya invokes the list-items subcommand twice in succession with arguments (acme, /tmp/data, cold) and a captured stdout sink for each call
Then both captured stdouts contain exactly the bytes `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n` (byte-identical between the two runs, demonstrating that the lexicographic sort at the boundary masks the HashMap iteration order randomisation in cinder::InMemoryTieringStore::list_by_tier)
And exit code is 0 for both invocations
```

### Acceptance Criteria

- [ ] When tenant `acme` has items `acme/batch-00099`,
  `acme/batch-00007`, `acme/batch-00041` placed in tier Cold in
  `/tmp/data/cinder.*` (in any insertion order), invoking the
  list-items subcommand with `(acme, /tmp/data, cold)` produces
  stdout exactly equal to the bytes
  `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`
  (lex-sorted, one item id per line, trailing newline on each
  line including the last), and exit code 0.
- [ ] When tenant `acme` has at least one item in Cinder but
  ZERO items in tier Warm, invoking the list-items subcommand
  with `(acme, /tmp/data, warm)` produces empty stdout
  (zero bytes), and exit code 0.
- [ ] When the `<tier>` argument is any spelling other than
  exactly `hot` / `warm` / `cold` — at minimum the upper-case
  value `COLD` and the typo `lukewarm` are tested — the
  subcommand produces empty stdout, a non-empty stderr line
  containing the invalid value verbatim, and a non-zero exit
  code. The Cinder store is byte-equivalent before and after
  (the parse error short-circuits before
  `FileBackedTieringStore::open` is called).
- [ ] When tenants `acme` and `globex` both have an item id
  `shared/batch-00042` placed in tier Cold in the same
  `/tmp/data`, invoking the list-items subcommand with
  `(acme, /tmp/data, cold)` produces stdout that contains
  `shared/batch-00042` exactly once, and the post-call
  `list_by_tier(globex, Tier::Cold)` still contains
  `shared/batch-00042` (unchanged).
- [ ] Two successive invocations of the list-items subcommand
  with the same `(tenant, data_dir, tier)` tuple produce
  byte-identical stdout. This demonstrates the lex-sort step
  at the CLI boundary masks the `HashMap` iteration order
  randomisation in `cinder::InMemoryTieringStore::list_by_tier`
  at `crates/cinder/src/store.rs:190-198`.
- [ ] The Cinder store under `<data_dir>/cinder.*` is
  byte-equivalent before and after every list-items invocation
  (success path AND invalid-tier failure path). The subcommand
  performs only `list_by_tier` (read-only) and never calls
  `place`, `migrate`, or `evaluate_at`.
- [ ] The Lumen store under `<data_dir>/lumen.*` is
  byte-equivalent before and after every list-items invocation.
  The subcommand never opens the Lumen store
  (`wave-decisions.md` D-NoLumenTouch).
- [ ] The new acceptance test file
  `crates/kaleidoscope-cli/tests/list_items_subcommand.rs` is
  added (NEW file, mirroring the harness pattern of
  `tests/migrate_subcommand.rs`) with assertions covering the
  five UAT scenarios above.
- [ ] The existing locked acceptance test files
  (`crates/kaleidoscope-cli/tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/migrate_subcommand.rs`, `tests/observe_otlp_*.rs`)
  continue to pass green UNMODIFIED under
  `cargo test --package kaleidoscope-cli`.
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs`
  (lines 83-145) is updated to document the new `list-items`
  subcommand's positional argument shape and lower-case tier
  contract. (Optional — DESIGN's call on exact wording.)
- [ ] No new external crate dependency is added to
  `crates/kaleidoscope-cli/Cargo.toml`. The only new
  `Cargo.toml` change is one `[[test]]` entry for the new test
  file.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the stdout byte
  level on the new
  `kaleidoscope-cli list-items <tenant> <data_dir> <tier>` CLI
  invocation.
- **Does what**: receives N stdout lines (one item id per line,
  sorted lexicographically) representing every item the queried
  tenant currently has in the queried tier, where N equals the
  count `stats` reports for that tier. On invalid tier value,
  receives non-zero exit with a stderr line naming the offending
  value and empty stdout. On a single CLI invocation, without
  writing Rust code or opening a Rust REPL.
- **By how much**: 100% of `list-items` invocations against a
  valid lower-case tier produce stdout containing exactly the
  set of item ids returned by
  `cinder.list_by_tier(tenant, tier)`, one per line, in
  lexicographic order (OK1); 100% of invocations against the
  same tenant from another tenant's perspective (e.g.
  `list-items globex` does not surface `acme`'s items) preserve
  the tenant-isolation invariant (OK2); 100% of invocations with
  non-lower-case-`hot`/`warm`/`cold` tier arguments produce a
  non-zero exit + stderr containing the invalid value + an
  unchanged Cinder store (OK3); 100% of invocations are
  read-only (no `place`, no `migrate`, no `evaluate_at` calls).
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/list_items_subcommand.rs`
  covering the three KPIs across the five UAT scenarios above.
  PLUS the existing locked acceptance test files continuing to
  pass green UNMODIFIED.
- **Baseline**: 0% — today there is no CLI surface for tier
  enumeration; operators answer the same question by writing
  Rust harnesses against `cinder::FileBackedTieringStore` or
  by grovelling through the Cinder snapshot file format.

Maps to OK1-CLI-list-items-correctness (principal),
OK2-CLI-list-items-tenant-isolation, and
OK3-CLI-list-items-invalid-tier-fail-fast in `outcome-kpis.md`.

### Technical Notes

- The exact library function shape is DESIGN-locked per
  `wave-decisions.md` D-FunctionShape. Likely a new free
  function in `crates/kaleidoscope-cli/src/lib.rs` with a
  signature like
  `pub fn list_items(tenant: &TenantId, data_dir: &Path, tier_arg: &str, writer: impl Write) -> Result<usize, Error>`,
  returning `Ok(N)` where N is the number of lines written
  (mirrors the `stats`/`stats_with_tiers` shape that returns
  the matched record count).
- The function internally:
  1. Calls the existing `parse_tier` helper at
     `crates/kaleidoscope-cli/src/lib.rs:475-482` to parse
     `<tier>` (or, if `parse_tier` remains private, an
     inline `match` reproducing it — DESIGN's call).
  2. On parse failure, returns `Error::InvalidTier { value }`
     (already exists at `lib.rs:79-81`) BEFORE opening any
     store.
  3. On parse success, constructs
     `FileBackedTieringStore::open(cinder_base(data_dir),
     Box::new(CinderRecorder))` (same quiescent recorder
     pattern as `stats_with_tiers` at
     `lib.rs:377-378`).
  4. Calls `cinder.list_by_tier(&tenant, tier)` to obtain a
     `Vec<ItemId>`.
  5. Sorts the vec lexicographically (`vec.sort()` —
     `ItemId(String)` derives `Ord`).
  6. For each item id, writes `writeln!(writer, "{}", item_id.0)`
     (or the equivalent `Display` impl path).
  7. Returns `Ok(items.len())`.
- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — new
  free function `list_items(...)`. NO new `Error` variant
  needed — the existing `InvalidTier`, `CinderOpen`, and `Io`
  variants cover the three error paths.
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — new
  `Some("list-items") => run_list_items(&args)` dispatch arm
  in the match at lines 52-66; new `run_list_items(...)`
  function mirroring the shape of `run_migrate(...)` at lines
  264-294 (parse the three positional arguments via
  `parse_positional` + one extra `args.get(N)` call for
  `<tier>`, call the library function, propagate). `print_usage`
  (lines 83-145) gains a `kaleidoscope-cli list-items
  <tenant_id> <data_dir> <tier>` block.
- New test file:
  `crates/kaleidoscope-cli/tests/list_items_subcommand.rs`.
  Mirrors the harness pattern of
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (the
  predecessor wave's locked file).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "list_items_subcommand", path =
  "tests/list_items_subcommand.rs"`. The `cinder` dependency
  is already present.
- DO NOT modify any locked test file
  (`crates/kaleidoscope-cli/tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/migrate_subcommand.rs`, `tests/observe_otlp_*.rs`).
  This is a hard constraint from the task brief and is
  restated as `wave-decisions.md` D-LockedTests.
- Slice tag: not `@infrastructure` — this story directly
  enables three operator-visible decisions on a real CLI
  surface (`kaleidoscope-cli list-items <tenant> <data_dir>
  <tier>`).

### Dependencies

- `cinder::TieringStore::list_by_tier(tenant, tier)` already
  exists and returns `Vec<ItemId>` per
  `crates/cinder/src/store.rs:102`. It is the exact method
  `stats_with_tiers` calls at
  `crates/kaleidoscope-cli/src/lib.rs:383` for the
  per-tier count lines.
- `cinder::FileBackedTieringStore` already implements
  `TieringStore` and is already constructed by `stats_with_tiers`
  at `crates/kaleidoscope-cli/src/lib.rs:377-378` and by
  `migrate` at `lib.rs:445-446`.
- `cinder::CinderRecorder` (the quiescent recorder) is already
  used by `stats_with_tiers` at `lib.rs:377` and by `migrate`'s
  no-flag arm at `lib.rs:443`; already a `kaleidoscope-cli`
  dependency.
- `cinder::Tier` enum (`Hot`, `Warm`, `Cold`); already imported
  at `crates/kaleidoscope-cli/src/lib.rs` (used by
  `stats_with_tiers` and `migrate`).
- `cinder::ItemId` already imported and used by `migrate` at
  `lib.rs:447`.
- The existing `parse_tier` helper at
  `crates/kaleidoscope-cli/src/lib.rs:475-482` is reused (or
  reproduced inline — DESIGN's call). It is currently a private
  free function; promoting it to `pub(crate)` or duplicating
  the four-line match is the trivial choice.
- The existing `cinder_base(data_dir)` helper at
  `crates/kaleidoscope-cli/src/lib.rs:130-132` is reused.
- The existing `parse_positional` helper at
  `crates/kaleidoscope-cli/src/main.rs:296-302` is reused for
  the first two positional arguments (`<tenant_id>`,
  `<data_dir>`); the third positional argument (`<tier>`) is
  read via an additional `args.get(4)` call in the new
  `run_list_items` helper.
- `aegis::TenantId` already a dependency.
- No `std::time::SystemTime` call (no mutation, no timestamp
  needed).
- No new external dependencies. No new internal crate
  dependencies.

### Slice

`slices/slice-01-list-items-subcommand-enumerates-tier.md`
