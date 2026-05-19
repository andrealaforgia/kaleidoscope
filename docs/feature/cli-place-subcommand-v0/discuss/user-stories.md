<!-- markdownlint-disable MD024 -->

# User Stories — `cli-place-subcommand-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits
  where polymorphism is genuinely needed. This change introduces
  ONE new CLI subcommand (`place`) and ONE new free function in
  the `kaleidoscope_cli` library; no new trait is introduced and
  no existing trait is modified. The existing
  `cinder::TieringStore::place(tenant, &ItemId, tier, placed_at)`
  method (`crates/cinder/src/store.rs:78-81`) is the only Cinder
  mutating API consumed; the existing
  `cinder::TieringStore::get_entry(tenant, &ItemId)` method
  (`crates/cinder/src/store.rs:89`) is consumed ONLY in the
  acceptance test as the post-call oracle (the production code
  path does NOT call `get_entry`).
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]`
  functions with `// Given / // When / // Then` comment blocks,
  not Gherkin `.feature` files. The Given/When/Then text in the
  UAT Scenarios sections below is the specification; DISTILL
  translates it into `#[test]` functions in
  `crates/kaleidoscope-cli/tests/place_subcommand.rs` (NEW file,
  per `wave-decisions.md` D-NewTestFile). The harness mirrors the
  pattern already in `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`
  and the seven other `tests/*_subcommand.rs` / `tests/observe_otlp_*.rs`
  files.
- Output-shape contract: stdout is exactly ONE line on success,
  the literal string `placed tenant=<tenant> item=<item_id>
  tier=<tier>\n`, where `<tier>` renders as the lower-case tier
  name `hot` / `warm` / `cold` (mirroring the `tier_lowercase`
  helper at `crates/kaleidoscope-cli/src/lib.rs:519-525`). No
  header, no JSON, no CSV, no colour codes. Exit code 0.
- Stderr-on-failure contract: on invalid `<tier>` argument, exit
  code is non-zero and stderr carries a single line containing
  the invalid value verbatim. Stdout is empty. The exact stderr
  wording is the inherited `Error::InvalidTier` Display impl at
  `crates/kaleidoscope-cli/src/lib.rs:98-100` (`invalid tier
  "<value>": expected one of hot, warm, cold`), prefixed by the
  binary-name prefix `kaleidoscope-cli: ` from `main.rs:73-77`.
  The contract here is the OBSERVABLE invariants only (non-zero
  exit, stderr contains the offending tier value, stdout empty).
- Tier-argument shape: `<tier>` is accepted ONLY in lower-case as
  one of `hot` / `warm` / `cold`. Upper-case (`HOT` / `Hot`),
  mixed-case (`Hot`), or any other spelling is rejected with a
  non-zero exit and a stderr line naming the invalid value
  (`wave-decisions.md` D-LowerCase). This mirrors the established
  lower-case tier rendering convention from
  `cli-stats-cinder-tier-distribution-v0` and the established
  parse-side convention from `cli-migrate-subcommand-v0` /
  `cli-list-items-subcommand-v0`.
- Timestamp contract: the `placed_at: SystemTime` argument passed
  to `TieringStore::place` is `SystemTime::now()` evaluated at
  the call site. There is NO `--placed-at` / `--at` flag in v0
  (`wave-decisions.md` D-Timestamp / D-OutOfScope-PlacedAt). The
  acceptance test asserts observable wire-level invariants (the
  stdout report content, the post-call `get_entry().tier ==
  tier`), not the exact recorded `placed_at` value.
- Overwrite-semantics contract: `TieringStore::place` overwrites
  any prior placement for the same `(tenant, item)` key per the
  trait docstring at `crates/cinder/src/store.rs:78-81` and the
  in-memory body at `crates/cinder/src/store.rs:140-152` (which
  unconditionally `state.entries.insert(key, ...)`). The CLI MUST
  NOT verify existence (`wave-decisions.md` D-OutOfScope-ExistsCheck)
  and MUST NOT introduce a special case (`wave-decisions.md`
  D-Overwrite). The stdout report shows the NEW tier; the post-
  call `get_entry().tier` equals the NEW tier.
- Read+mutate contract: this subcommand opens ONLY the Cinder
  store under `<data_dir>/cinder.*`. It does NOT open the Lumen
  store under `<data_dir>/lumen.*`. The Lumen WAL+snapshot is
  byte-equivalent before and after the call (`wave-decisions.md`
  D-NoLumenTouch / D-OutOfScope-LumenMutation).
- Tenant-isolation contract: `place acme /tmp/data acme/bootstrap-00001 hot`
  MUST NOT touch `globex`'s tier metadata even if `globex` happens
  to have placed an item with the same `ItemId` string
  (`acme/bootstrap-00001`) into the same `data_dir`. Inherited
  from `cinder::TieringStore`'s per-tenant isolation invariant
  (`crates/cinder/src/store.rs:71-72`).
- Single-optional-flag posture: the `place` subcommand accepts
  EXACTLY ONE optional flag: `--observe-otlp <path>`
  (`wave-decisions.md` D-ObserveOtlp). No `--dry-run`, no
  `--placed-at`, no `--format=...`. The positional argument shape
  is fixed: `place <tenant> <data_dir> <item_id> <tier>` — four
  positional arguments, in that order, with the optional
  `--observe-otlp <path>` accepted anywhere after the subcommand.
- `--observe-otlp <path>` contract: when set, exactly ONE
  `cinder.place.count` OTLP-JSON line per place call is appended
  to `<path>`. The file is opened once with
  `OpenOptions::create(true).append(true)` per ADR-0039 §8. The
  line's wire shape is byte-identical to the `cinder.place.count`
  lines `ingest` emits via the same `CinderToOtlpJsonWriter` at
  `crates/kaleidoscope-cli/src/lib.rs:172-174`. When the flag is
  NOT set, no on-disk OTLP file is created.

---

## US-01: Operator manually bootstraps a Cinder item without writing Rust or running ingest

### Elevator Pitch

- **Before**: Priya the platform operator runs a multi-tenant
  Kaleidoscope deployment. After the seven predecessor features
  shipped, she can `ingest` (which side-effects one Hot Cinder
  item per batch via `flush()` at
  `crates/kaleidoscope-cli/src/lib.rs:251-253`), `read` records
  back, `stats` to see per-tier counts, `list-items` to enumerate
  a tier, and `migrate` to move an item between tiers. What she
  canNOT do today is `place` a Cinder item DIRECTLY — without
  running `ingest` first. Three operationally distinct gaps fall
  out of this:

  1. **Bootstrap items that exist outside the Lumen ingest flow**.
     Some items live in object storage / on tape / in another
     log pipeline; their existence in Cinder should be recorded
     without forcing a fake `ingest` of synthetic records into
     Lumen.
  2. **Set up a controlled test scenario**: "place 10 items in
     Hot, run `evaluate-policy`, observe the migrations". Today
     she has to write a Rust harness or fabricate 10 NDJSON
     records and `ingest` them.
  3. **Recover the Cinder catalog from a manifest**: after a
     Cinder snapshot corruption, she has a JSON manifest of
     `(tenant, item_id, tier)` triples but no way to feed them
     back into Cinder without writing Rust.

  All three reduce to "give me one CLI invocation that calls
  `TieringStore::place(tenant, &ItemId, tier, SystemTime::now())`
  and tells me, on stdout, that the placement happened." The
  underlying API
  (`cinder::TieringStore::place(...)` at
  `crates/cinder/src/store.rs:78-81`, returning `()` — overwrite-
  semantics, no failure modes) is already adequate; the gap is
  the missing CLI surface.

- **After**: Priya runs:

  ```text
  kaleidoscope-cli place acme /tmp/data acme/bootstrap-00001 hot
  ```

  Stdout, in milliseconds, prints exactly one line:

  ```text
  placed tenant=acme item=acme/bootstrap-00001 tier=hot
  ```

  Exit code 0. The line confirms three things on a single
  invocation: (1) WHICH tenant the placement was recorded for
  (`acme`), (2) WHICH item id was placed (`acme/bootstrap-00001`,
  reflected back as confirmation), and (3) WHICH tier the item
  now lives in (`hot`). She can immediately re-run
  `kaleidoscope-cli list-items acme /tmp/data hot` and see
  `acme/bootstrap-00001` in the output. She can re-run
  `kaleidoscope-cli stats acme /tmp/data` and see the `hot=`
  count incremented by 1.

  When she wants to record the place call to an OTLP-JSON sidecar
  (e.g. tail it from another shell for live monitoring during a
  bulk-bootstrap shell loop), she adds the flag:

  ```text
  kaleidoscope-cli place acme /tmp/data acme/bootstrap-00001 hot \
    --observe-otlp /tmp/cinder.otlp.json
  ```

  Stdout is byte-identical to the no-flag case. The file at
  `/tmp/cinder.otlp.json` gains exactly one new line: a
  `cinder.place.count` OTLP-JSON metric with `tenant_id=acme`
  resource attribute and `tier=hot` point attribute. The byte
  shape is identical to the `cinder.place.count` lines `ingest`
  already emits — she can `tail -f` the same file across her
  `ingest` and `place` invocations.

  When she places an item that ALREADY exists for the same tenant
  — `kaleidoscope-cli place acme /tmp/data acme/bootstrap-00001 cold`
  after the earlier Hot placement — she gets exit 0 and the line:

  ```text
  placed tenant=acme item=acme/bootstrap-00001 tier=cold
  ```

  The underlying API is overwrite-semantics per
  `crates/cinder/src/store.rs:78-81` ("Overwrites any prior
  placement for the same key"). The CLI faithfully reflects this:
  the post-call `get_entry().tier` is `Cold`, the previous Hot
  entry is gone, no error is raised. This is the contract Priya
  needs for the "recover from manifest" use case where the same
  `(tenant, item_id)` may appear with a different tier on a
  second pass.

  When she types an invalid tier name —
  `kaleidoscope-cli place acme /tmp/data acme/bootstrap-00001 HOT`
  (upper-case) or `... lukewarm` (typo) — she gets a non-zero
  exit and a stderr line that names the invalid value she typed.
  Stdout is empty. The Cinder store is unchanged.

- **Decision enabled**: Priya can decide and execute three
  operationally distinct placements from one CLI invocation:
  1. "Bootstrap an item that lives outside Lumen — e.g. an
     archival batch in cold storage I want Cinder to track —
     without running a fake `ingest`" (catalog bootstrap).
  2. "Set up the test scenario: 10 items in Hot, then `evaluate-
     policy` to observe migrations" (controlled test setup
     without a Rust harness).
  3. "Recover the Cinder catalog from a manifest file after a
     snapshot corruption" (operator-driven catalog rebuild,
     overwrite-safe on re-runs).

### Problem

Priya the platform operator already uses the seven shipped CLI
subcommands (`ingest`, `read`, `stats`, `migrate`, `list-items`,
plus `--help` / `--version`) daily. The CLI's gap, today, is that
it has no surface for direct Cinder placement. Three
operationally distinct decisions all reduce to the same primitive
("place item X for tenant Y in tier T, now"):

1. **Bootstrap outside the ingest flow**: items that live in
   another storage system (cold-storage object store, archival
   tape, an external log pipeline) need to be tracked in Cinder
   without forcing a fake `ingest` of synthetic records into
   Lumen. Today this requires writing a Rust harness against
   `cinder::FileBackedTieringStore::open(...).place(...)` or
   fabricating NDJSON records.
2. **Controlled test scenarios**: "place 10 items in Hot, run
   `evaluate-policy`, observe the migrations" — a routine
   operator workflow for validating that a new policy behaves as
   expected. Today this requires writing a Rust harness or
   fabricating 10 NDJSON records and `ingest`ing them (which
   places them in Hot but also writes Lumen records the operator
   doesn't need).
3. **Catalog recovery**: after a Cinder snapshot corruption, the
   operator has a JSON manifest of `(tenant, item_id, tier)`
   triples (e.g. from a daily backup) but no CLI to feed them
   back into Cinder. Today this requires writing a Rust harness.

All three reduce to "give me one CLI invocation that calls
`TieringStore::place(tenant, &ItemId::new(item_id), tier,
SystemTime::now())` and tells me, on stdout, that the placement
happened." The underlying API
(`cinder::TieringStore::place(...)` at
`crates/cinder/src/store.rs:78-81`, returning `()`) is already
adequate; the gap is the missing CLI surface.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli ingest`,
`kaleidoscope-cli stats`, `kaleidoscope-cli migrate`,
`kaleidoscope-cli list-items` daily (inherited from the seven
predecessor features) | wants to record one specific Cinder
placement without writing Rust or running a fake `ingest` | uses
standard Unix text tools (`grep`, `cut`, `awk`) on stdout output,
not JSON parsers | expects fail-fast behaviour (non-zero exit +
descriptive stderr) on invalid tier names | does NOT expect the
subcommand to verify the item doesn't already exist (overwrite-
semantics is the documented underlying API behaviour) | does NOT
expect the subcommand to touch the Lumen store | runs the
subcommand inside `for item in ...; do ... done` shell loops for
bulk bootstrap, optionally piping the OTLP-JSON sidecar to a
`tail -f` for live monitoring.

### Solution

Add a new positional subcommand to the existing CLI binary:

```text
kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>]
```

The dispatch arm in `crates/kaleidoscope-cli/src/main.rs` (the
match block at lines 54-69) gains a new `Some("place")` branch
that calls a new library function in `kaleidoscope_cli`. The
library function:

1. Parses `<tier>` from its argv form to a `cinder::Tier` value
   using the existing `parse_tier(&str) -> Result<Tier, ()>`
   private helper at `crates/kaleidoscope-cli/src/lib.rs:505-512`
   (DESIGN's call: lift to `pub(crate)` and reuse, or duplicate
   the four-line match inline). Only literal lowercase `hot` /
   `warm` / `cold` are accepted. Any other spelling yields
   `Error::InvalidTier { value }` (existing variant at lines
   79-81), carrying the verbatim invalid input.
2. Constructs the Cinder recorder via the same `match
   otlp_log_path { Some(path) => CinderToOtlpJsonWriter::new(file),
   None => CinderRecorder }` shape used by `migrate()` at
   `crates/kaleidoscope-cli/src/lib.rs:435-444`. ADR-0039 §8
   file-open contract: `OpenOptions::create(true).append(true)`
   once per call.
3. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
   recorder)` (mirroring `migrate()` at
   `crates/kaleidoscope-cli/src/lib.rs:445-446`). It does NOT
   open the Lumen store.
4. Builds `ItemId::new(item_id_arg.to_string())`.
5. Calls `cinder.place(&tenant, &item, tier, SystemTime::now())`.
   The trait method returns `()`; there is no `Result` to match
   on at this step.
6. On success, writes ONE line to stdout:
   `placed tenant=<tenant> item=<item_id> tier=<tier>\n`, where
   `<tier>` renders as `hot` / `warm` / `cold` via the same
   lower-case mapping (the existing `tier_lowercase` helper at
   `crates/kaleidoscope-cli/src/lib.rs:519-525`).
7. Exits 0.

The output report uses the same `key=value` aesthetic as `stats`
and `migrate` but with THREE fields on a single line (rather than
`migrate`'s four `from=`/`to=` fields) because `place` records
ONE atomic event ("this item now lives in this tier"). No
multi-line render is needed for a single placement report.

The function signature is DESIGN-locked per `wave-decisions.md`
D-FunctionShape. Likely:

```rust
pub fn place(
    tenant: &TenantId,
    data_dir: &Path,
    item_id: &str,
    tier_arg: &str,
    writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<(), Error>
```

DESIGN locks the choice. The wire-observable contract (stdout
content on success, stderr content on failure, exit code, no
Lumen touch, OTLP emission shape) is what matters here.

No new `Error` variant is needed: `Error::InvalidTier { value }`
already exists at `crates/kaleidoscope-cli/src/lib.rs:79-81` for
the parse-side failure; `Error::CinderOpen(MigrateError)` at line
77 covers any store-open failure; `Error::Io(std::io::Error)` at
line 82 covers any `--observe-otlp` file-open failure. The
underlying `TieringStore::place` itself returns `()` (no failure
modes per the trait docstring).

### Domain Examples

#### 1. Happy path — Priya bootstraps `acme/bootstrap-00001` in Hot

Priya wants to track an external archival batch in Cinder. The
batch lives in cold storage; she doesn't want to ingest it into
Lumen. She runs:

```text
kaleidoscope-cli place acme /tmp/k-data acme/bootstrap-00001 hot
```

Stdout contains exactly one line:

```text
placed tenant=acme item=acme/bootstrap-00001 tier=hot
```

Exit code 0. Stderr is empty. She immediately re-runs
`kaleidoscope-cli list-items acme /tmp/k-data hot` and sees
`acme/bootstrap-00001` in the output. She re-runs
`kaleidoscope-cli stats acme /tmp/k-data` and sees the new
distribution `hot=1 warm=0 cold=0`. The Lumen side
(`/tmp/k-data/lumen.*`) does not exist (or, if it pre-existed
from a previous `ingest`, is byte-equivalent before and after the
place call).

#### 2. Edge case (overwrite-semantics) — Priya re-places `acme/bootstrap-00007` from Hot to Cold

Priya has bootstrapped `acme/bootstrap-00007` into Hot earlier.
Reading her manifest a second time (e.g. after a partial restore
restart), she places the same item with a different tier:

```text
kaleidoscope-cli place acme /tmp/k-data acme/bootstrap-00007 cold
```

Stdout contains exactly one line:

```text
placed tenant=acme item=acme/bootstrap-00007 tier=cold
```

Exit code 0. The underlying `TieringStore::place` API at
`crates/cinder/src/store.rs:78-81` is overwrite-semantics ("Records
`(tenant, item)` as living in `tier` at `placed_at`. Overwrites any
prior placement for the same key."). The in-memory body at
`crates/cinder/src/store.rs:140-152` confirms by unconditionally
`state.entries.insert(key, TierEntry { tier, placed_at,
migrated_at: placed_at })`.

The CLI report mirrors this honestly: the line shows `tier=cold`
(the NEW tier), not `tier=hot` (the old tier). The post-call
`get_entry(acme, acme/bootstrap-00007).unwrap().tier == Tier::Cold`.
The previous Hot entry is gone. This is documented as the
operationally expected behaviour: no special case in the CLI; the
underlying API is overwrite-semantics, the CLI faithfully reports
the call's outcome.

#### 3. Error case (invalid tier argument) — Priya types upper-case `HOT`

Priya types `HOT` instead of `hot`:

```text
kaleidoscope-cli place acme /tmp/k-data acme/bootstrap-00001 HOT
```

Stdout is empty. Stderr contains a single line that names the
invalid value `HOT`:

```text
kaleidoscope-cli: invalid tier "HOT": expected one of hot, warm, cold
```

Exit code is non-zero. The Cinder store under `/tmp/k-data/cinder.*`
is byte-equivalent before and after the call (the parse error fires
BEFORE any `place` call is issued, so the store is never mutated).

The same outcome obtains for any non-`hot`/`warm`/`cold` value:
`Hot` (mixed-case), `WARM`, `cool`, `lukewarm`, `frozen`, empty
string, leading/trailing whitespace. The acceptance test exercises
one representative upper-case value (`HOT`) plus one typo
(`lukewarm`).

#### 4. Observability case — Priya tail-watches placement events via `--observe-otlp`

Priya runs a bulk-bootstrap shell loop and wants live OTLP-JSON
monitoring. In one terminal:

```text
tail -f /tmp/cinder.otlp.json
```

In another, she runs:

```text
for n in 0001 0002 0003; do
  kaleidoscope-cli place acme /tmp/k-data "acme/bootstrap-$n" hot \
    --observe-otlp /tmp/cinder.otlp.json
done
```

Stdout shows three lines (one per place call):

```text
placed tenant=acme item=acme/bootstrap-0001 tier=hot
placed tenant=acme item=acme/bootstrap-0002 tier=hot
placed tenant=acme item=acme/bootstrap-0003 tier=hot
```

The file at `/tmp/cinder.otlp.json` gains exactly three new
`cinder.place.count` OTLP-JSON lines, one per place call. Each
line carries `tenant_id=acme` as a resource attribute and
`tier=hot` as a point attribute. The byte shape is identical to
the `cinder.place.count` lines `ingest --observe-otlp` already
emits via the same `CinderToOtlpJsonWriter` at
`crates/kaleidoscope-cli/src/lib.rs:172-174`. Priya's `tail -f`
terminal shows each line as it lands.

#### 5. Tenant-isolation case — Priya places `acme/bootstrap-00001` for `acme`; `globex`'s same-named item is untouched

Priya runs a deployment where `acme` and `globex` both have an
item id `acme/bootstrap-00001` placed in Cinder under the same
`/tmp/k-data` (the `ItemId` namespace is global across tenants
but the placement key is `(TenantId, ItemId)` per
`crates/cinder/src/store.rs:119`). `acme`'s item is currently in
Warm; `globex`'s item is in Cold. She re-places `acme`'s item in
Hot:

```text
kaleidoscope-cli place acme /tmp/k-data acme/bootstrap-00001 hot
```

Stdout contains exactly one line:

```text
placed tenant=acme item=acme/bootstrap-00001 tier=hot
```

After the call:

- `get_entry(acme, acme/bootstrap-00001).unwrap().tier == Tier::Hot`
  (the new tier — overwrite-semantics).
- `get_entry(globex, acme/bootstrap-00001).unwrap().tier == Tier::Cold`
  (byte-equivalent to its pre-call state).

This is the tenant-isolation invariant inherited from
`cinder::TieringStore`'s per-tenant key
(`crates/cinder/src/store.rs:71-72, :142`).

### UAT Scenarios (BDD)

#### Scenario: Operator bootstraps a fresh item in Hot — sees the placement on stdout

```text
Given the Cinder store under /tmp/k-data has NO placement for the (tenant=acme, item=acme/bootstrap-00001) pair (fresh data_dir or directory without that key)
When Priya invokes the place subcommand with arguments (acme, /tmp/k-data, acme/bootstrap-00001, hot, &mut stdout, None) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`
And the captured stderr is empty
And exit code is 0
And a freshly-opened FileBackedTieringStore::open(cinder_base(/tmp/k-data), ...).get_entry(acme, acme/bootstrap-00001) returns Some(entry) with entry.tier == Tier::Hot
And the Lumen directory under /tmp/k-data/lumen.* either does not exist OR is byte-equivalent to its pre-call state
```

#### Scenario: Operator re-places an existing item with a different tier — overwrite succeeds

```text
Given Priya has placed item acme/bootstrap-00007 into Cinder under /tmp/k-data with tier Hot for tenant acme (via a direct FileBackedTieringStore::open + place call with placed_at = a fixed SystemTime value)
When Priya invokes the place subcommand with arguments (acme, /tmp/k-data, acme/bootstrap-00007, cold, &mut stdout, None) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `placed tenant=acme item=acme/bootstrap-00007 tier=cold\n`
And the captured stderr is empty
And exit code is 0
And a freshly-opened FileBackedTieringStore::open(cinder_base(/tmp/k-data), ...).get_entry(acme, acme/bootstrap-00007) returns Some(entry) with entry.tier == Tier::Cold (the NEW tier — overwrite-semantics)
```

#### Scenario: Operator types an invalid tier value — fail-fast with stderr naming the invalid value

```text
Given the Cinder store under /tmp/k-data is fresh (or, separately, has the item acme/seed-00001 placed for tenant acme in tier Hot to prove the parse error short-circuits before any store mutation)
When Priya invokes the place subcommand with arguments (acme, /tmp/k-data, acme/bootstrap-00001, HOT, &mut stdout, None) and a captured stdout sink and a captured stderr sink
Then the call returns Err
And the captured stdout is empty
And the captured stderr contains a line that contains the substring `HOT`
And exit code is non-zero
And the Cinder store at /tmp/k-data is byte-equivalent before and after the call (no place call mutated state; the seed item acme/seed-00001 is still in tier Hot)
And a second sub-scenario with tier_arg = "lukewarm" produces an Err whose captured stderr contains the substring `lukewarm`
```

#### Scenario: Operator invokes `--observe-otlp` — one `cinder.place.count` line is appended per call

```text
Given the Cinder store under /tmp/k-data is fresh
And the path /tmp/cinder.otlp.json does not exist before the call
When Priya invokes the place subcommand with arguments (acme, /tmp/k-data, acme/bootstrap-00001, hot, &mut stdout, Some(/tmp/cinder.otlp.json)) and a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly the line `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`
And the file at /tmp/cinder.otlp.json exists after the call
And the file contains exactly one line
And that line contains the substrings `cinder.place.count`, `acme`, and `hot`
And a second sub-scenario invoking the place library function with otlp_log_path = None asserts no file is created at /tmp/cinder.otlp.json (the file's pre-call non-existence is preserved)
```

#### Scenario: Tenant isolation — placing `acme/bootstrap-00001` for `acme` does not touch `globex`'s same-named item

```text
Given Priya has placed item acme/bootstrap-00001 in Cinder under /tmp/k-data with tier Warm for tenant acme (via a direct FileBackedTieringStore::open + place call)
And Priya has placed item acme/bootstrap-00001 in Cinder under /tmp/k-data with tier Cold for tenant globex (via a direct FileBackedTieringStore::open + place call)
When Priya invokes the place subcommand with arguments (acme, /tmp/k-data, acme/bootstrap-00001, hot, &mut stdout, None) and a captured stdout sink and a captured stderr sink
And the call returns Ok
Then the captured stdout contains exactly the line `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`
And cinder.get_entry(acme, acme/bootstrap-00001) at /tmp/k-data returns Some(entry) with entry.tier == Tier::Hot (the new tier)
And cinder.get_entry(globex, acme/bootstrap-00001) at /tmp/k-data returns Some(entry) with entry.tier == Tier::Cold (unchanged from the pre-call state)
```

### Acceptance Criteria

- [ ] When tenant `acme` has NO item `acme/bootstrap-00001` placed
  in `/tmp/k-data/cinder.*`, invoking the place subcommand with
  `(acme, /tmp/k-data, acme/bootstrap-00001, hot)` produces exactly
  the stdout line
  `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`, empty
  stderr, and exit code 0.
- [ ] After the successful happy-path call, `cinder.get_entry(acme,
  acme/bootstrap-00001)` at `/tmp/k-data` returns `Some(entry)`
  with `entry.tier == Tier::Hot`.
- [ ] When tenant `acme` has item `acme/bootstrap-00007` placed in
  tier Hot in `/tmp/k-data/cinder.*`, invoking the place subcommand
  with `(acme, /tmp/k-data, acme/bootstrap-00007, cold)` produces
  exactly the stdout line
  `placed tenant=acme item=acme/bootstrap-00007 tier=cold\n`, empty
  stderr, and exit code 0. After the call, `get_entry(acme,
  acme/bootstrap-00007).unwrap().tier == Tier::Cold` (overwrite-
  semantics per `crates/cinder/src/store.rs:78-81`; no special case
  in the CLI).
- [ ] When the `<tier>` argument is any spelling other than exactly
  `hot` / `warm` / `cold` — at minimum the upper-case value `HOT`
  and the typo `lukewarm` are tested — the subcommand produces
  empty stdout, a non-empty stderr line containing the invalid
  value verbatim, and a non-zero exit code. The Cinder store is
  byte-equivalent before and after.
- [ ] When `--observe-otlp <path>` is supplied, the file at `<path>`
  gains exactly ONE new `cinder.place.count` OTLP-JSON line per
  successful place call. The line contains the substrings
  `cinder.place.count`, the tenant id, and the lower-case tier
  name. When `--observe-otlp` is NOT supplied, NO file is created
  at any path.
- [ ] When tenants `acme` and `globex` both have an item id
  `acme/bootstrap-00001` placed in the same `/tmp/k-data` (Warm for
  `acme`, Cold for `globex`), invoking the place subcommand with
  `(acme, /tmp/k-data, acme/bootstrap-00001, hot)` overwrites only
  `acme`'s item to Hot; `globex`'s item remains in Cold. The pre-
  call and post-call `get_entry(globex, ...)` results have
  identical `tier`.
- [ ] The Lumen store under `<data_dir>/lumen.*` is byte-equivalent
  before and after every `place` subcommand invocation (success,
  invalid-tier failure, observe-otlp success). The subcommand
  never opens the Lumen store (`wave-decisions.md` D-NoLumenTouch).
- [ ] The new acceptance test file
  `crates/kaleidoscope-cli/tests/place_subcommand.rs` is added
  (NEW file, mirroring the harness pattern of
  `tests/migrate_subcommand.rs` and `tests/migrate_observe_otlp_flag.rs`)
  with assertions covering the five UAT scenarios above.
- [ ] The existing locked acceptance test files
  (`crates/kaleidoscope-cli/tests/migrate_subcommand.rs`,
  `tests/migrate_observe_otlp_flag.rs`,
  `tests/list_items_subcommand.rs`,
  `tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/stats_time_range.rs`,
  `tests/read_time_range.rs`,
  `tests/ingest_and_read_roundtrip.rs`,
  `tests/cli_binary_smoke.rs`,
  `tests/observe_otlp_*.rs`) continue to pass green UNMODIFIED
  under `cargo test --package kaleidoscope-cli`.
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs`
  (lines 80-157) is updated to document the new `place` subcommand's
  positional argument shape, the lower-case tier contract, the
  overwrite-semantics contract, and the optional `--observe-otlp
  <path>` flag. (Optional — DESIGN's call on exact wording.)
- [ ] No new external crate dependency is added to
  `crates/kaleidoscope-cli/Cargo.toml`. The only new `Cargo.toml`
  change is one `[[test]]` entry for the new test file.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the stdout byte
  level on the new
  `kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>]`
  CLI invocation.
- **Does what**: receives a one-line stdout report of the form
  `placed tenant=<tenant> item=<item_id> tier=<tier>\n` on success,
  or a non-zero exit with a stderr line naming the offending tier
  value (invalid-tier case) on failure, on a single CLI invocation,
  without writing Rust code or opening a Rust REPL. When
  `--observe-otlp` is set, an OTLP-JSON sidecar file gains one
  `cinder.place.count` line per place call.
- **By how much**: 100% of `place` invocations with valid lower-case
  tier arguments produce the exact stdout report line AND post-call
  `get_entry().tier == tier` (OK1); 100% of overwrite invocations
  succeed and the post-call `get_entry().tier` equals the NEW tier
  (OK2 — faithful to overwrite-semantics, no CLI special case);
  100% of invocations with non-lower-case-`hot`/`warm`/`cold` tier
  arguments produce a non-zero exit + stderr containing the invalid
  value + an unchanged Cinder store (OK3); 100% of `--observe-otlp`
  invocations produce exactly one new `cinder.place.count` OTLP-JSON
  line per call (OK4); 100% of invocations (success or failure)
  produce a byte-equivalent Lumen store before and after, and a
  byte-equivalent same-tenant-id-cross-tenant Cinder state for any
  other tenant in the same `data_dir`.
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/place_subcommand.rs` covering the
  four KPIs across the five UAT scenarios above. PLUS the existing
  locked acceptance test files continuing to pass green UNMODIFIED.
- **Baseline**: 0% — today there is no CLI surface for direct Cinder
  placement at all; items only enter Cinder as a side effect of
  `ingest`'s batch flush at
  `crates/kaleidoscope-cli/src/lib.rs:251-253`. Operators answer
  the same question by writing Rust harnesses against
  `cinder::FileBackedTieringStore::place(...)` or by fabricating
  NDJSON records and running `ingest`.

Maps to OK1-CLI-place-success (principal),
OK2-CLI-place-overwrite-semantics (guardrail),
OK3-CLI-place-invalid-tier-fail-fast, and
OK4-CLI-place-observe-otlp-emission in `outcome-kpis.md`.

### Technical Notes

- The exact library function shape is DESIGN-locked per
  `wave-decisions.md` D-FunctionShape. Likely a new free function
  in `crates/kaleidoscope-cli/src/lib.rs` with the signature
  `pub fn place(tenant: &TenantId, data_dir: &Path, item_id: &str,
  tier_arg: &str, writer: impl Write, otlp_log_path: Option<&Path>)
  -> Result<(), Error>`.
- The function internally:
  1. Calls the existing `parse_tier(tier_arg)` helper at
     `crates/kaleidoscope-cli/src/lib.rs:505-512` (DESIGN's call:
     lift to `pub(crate)` or duplicate the four-line match).
  2. Constructs the Cinder recorder via the same
     `match otlp_log_path { Some(path) => CinderToOtlpJsonWriter::new(file),
     None => CinderRecorder }` shape as `migrate()` at lines
     435-444. ADR-0039 §8 file-open contract.
  3. Calls `FileBackedTieringStore::open(cinder_base(data_dir),
     recorder).map_err(Error::CinderOpen)?`.
  4. Builds `ItemId::new(item_id.to_string())` (mirroring `ingest`
     at line 251 and `migrate` at line 447).
  5. Calls `cinder.place(&tenant, &item, tier, SystemTime::now())`.
     This call returns `()`; no error matching needed.
  6. Calls `writeln!(writer, "placed tenant={} item={} tier={}",
     tenant.0, item_id, tier_lowercase(tier))?`.
- The tier-argument parse reuses `parse_tier(s: &str) ->
  Result<Tier, ()>` at lines 505-512; the `Error::InvalidTier {
  value: String }` variant at lines 79-81 wraps the unit error.
  Both already exist — no new code at the parse-helper level.
- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — new free
  function `place(...)`. Possibly promotion of `parse_tier` to
  `pub(crate)` (DESIGN's call). No new `Error` variant.
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — new
  `Some("place") => run_place(&args)` dispatch arm in the match
  at lines 54-69; new `run_place(...)` function and inner
  `run_place_with<O: Write>(...)` mirroring the shape of
  `run_migrate` / `run_migrate_with` at lines 276-306 (parse the
  four positional arguments via `parse_positional` + two extra
  `args.get(N)` calls, parse `--observe-otlp` via the existing
  `parse_observe_otlp` helper at lines 178-192, call the library
  function, propagate any error). `print_usage` (lines 80-157)
  gains a `kaleidoscope-cli place <tenant_id> <data_dir>
  <item_id> <tier> [--observe-otlp <path>]` block.
- New test file: `crates/kaleidoscope-cli/tests/place_subcommand.rs`.
  Mirrors the harness pattern of `tests/migrate_subcommand.rs`
  (the predecessor's locked file) and
  `tests/migrate_observe_otlp_flag.rs` (the --observe-otlp sibling).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "place_subcommand", path =
  "tests/place_subcommand.rs"`. The `cinder` and `self-observe`
  dependencies are already present.
- DO NOT modify any locked test file. This is a hard constraint
  from the task brief and is restated as `wave-decisions.md`
  D-LockedTests.
- Slice tag: not `@infrastructure` — this story directly enables
  three operator-visible decisions on a real CLI surface
  (`kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier>
  [--observe-otlp <path>]`).

### Dependencies

- `cinder::TieringStore::place(tenant, item, tier, placed_at)`
  already exists per `crates/cinder/src/store.rs:78-81`, returning
  `()` (no failure modes at the trait level).
- `cinder::TieringStore::get_entry(tenant, item)` already exists
  per `crates/cinder/src/store.rs:89` (used as the post-call
  oracle in the acceptance test; NOT called in production code).
- `cinder::FileBackedTieringStore` already implements
  `TieringStore` and is already constructed by `ingest()`,
  `migrate()`, `list_items()`, `stats_with_tiers()`.
- `cinder::CinderRecorder` (quiescent) already used by the
  no-flag arm of `migrate()` at
  `crates/kaleidoscope-cli/src/lib.rs:443`.
- `self_observe::CinderToOtlpJsonWriter` already used by the
  `--observe-otlp` arm of `ingest()` at
  `crates/kaleidoscope-cli/src/lib.rs:172-174` and `migrate()`
  at lines 441.
- `cinder::Tier` enum (`Hot`, `Warm`, `Cold`); already imported
  at `crates/kaleidoscope-cli/src/lib.rs:58`.
- `cinder::ItemId::new(id)`; already imported.
- The existing `cinder_base(data_dir)` helper at
  `crates/kaleidoscope-cli/src/lib.rs:130-132` is reused
  unchanged.
- The existing `parse_positional` helper at
  `crates/kaleidoscope-cli/src/main.rs:328-334` is reused for the
  first two positional arguments (`<tenant>`, `<data_dir>`); the
  third and fourth positional arguments (`<item_id>`, `<tier>`)
  are read via additional `args.get(N)` calls in the new
  `run_place` helper.
- The existing `parse_observe_otlp` helper at
  `crates/kaleidoscope-cli/src/main.rs:178-192` is reused for the
  optional `--observe-otlp <path>` flag.
- The existing `parse_tier` helper at
  `crates/kaleidoscope-cli/src/lib.rs:505-512` is reused (DESIGN's
  call on promotion to `pub(crate)` or duplication).
- The existing `tier_lowercase` helper at
  `crates/kaleidoscope-cli/src/lib.rs:519-525` is reused for the
  stdout `tier=` field rendering.
- The existing `Error::InvalidTier { value: String }` variant at
  lines 79-81 (and its Display at 98-100) is reused for the
  invalid-tier fail-fast.
- `aegis::TenantId` already a dependency.
- `std::time::SystemTime::now()` for the `placed_at` argument.
- No new external dependencies. No new internal crate
  dependencies.

### Slice

`slices/slice-01-place-subcommand-bootstraps-item.md`
