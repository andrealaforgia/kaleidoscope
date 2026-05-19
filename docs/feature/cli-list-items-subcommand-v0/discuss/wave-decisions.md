# Wave Decisions — `cli-list-items-subcommand-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus
decisions recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| D1 `feature_type` | `backend` (CLI subcommand) | New positional subcommand on the existing `kaleidoscope-cli` binary: `list-items <tenant_id> <data_dir> <tier>`. Adds one new dispatch arm to `main.rs`'s match block (lines 52-66), one new free function in `lib.rs`, and one new acceptance test file. No new persona, no new crate, no new external dependency. |
| D2 `walking_skeleton` | `no` | The CLI exists, four subcommands (`ingest`, `read`, `stats`, `migrate`) work, the `TieringStore` trait already exposes `list_by_tier(tenant, tier)` returning `Vec<ItemId>` per `crates/cinder/src/store.rs:102`. This method is ALREADY used in production by `stats_with_tiers` at `crates/kaleidoscope-cli/src/lib.rs:383` for the per-tier count lines. This feature is a thin extension on top of an existing substrate. |
| D3 `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the seven predecessor features). Single decision-set: enumerate every item the tenant has in a given tier. Output shape collapsed by precedent: one bare item id per line on stdout, terminated by `\n`, sorted lexicographically. The natural consumer is a shell pipeline (`xargs -I {} migrate ... {} ...`) which expects one record per line. |
| D4 `jtbd_analysis` | `no` | The job is obvious and singular: tier enumeration. Three operationally distinct decisions (manual rebalancing follow-up / sanity check / scripted pipeline) all collapse to the same primitive ("show me every item this tenant has in this tier"). Persona inherited from the seven predecessor features; the natural follow-on to `stats` (which surfaces COUNTS) is `list-items` (which surfaces IDS). |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. The job statement is implicit and singular: an operator wants `list-items <tenant> <data_dir> <tier>` to call `TieringStore::list_by_tier(...)` and print one item id per line on stdout. | DIVERGE skipped by Andrea's explicit instruction. The output shape (one bare id per line, lex-sorted, exit-code-based pass/fail) has exactly one reasonable shape that operators can `grep`, `xargs`, and `sort` over. |
| No formal JTBD workshop | LOW. Persona, push (operator can't answer "which 47 cold items?" from the CLI today), pull (one-shot CLI enumeration that pipes into `xargs`), anxiety (fail-fast on invalid tier; deterministic stdout across runs; no Cinder mutation; no Lumen-side touch), habit (operator already runs `stats` and `migrate` daily). | Persona + emotional-arc inherited from the seven reference features under `docs/feature/cli-*/discuss/` (`cli-migrate-subcommand-v0` most directly). |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the existing `TieringStore::list_by_tier` API surface (already used by `stats_with_tiers`) and by the established CLI conventions (positional arguments, lower-case tier rendering, plain-text stdout, stderr-on-failure with verbatim offending value). | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D-LowerCase: Tier arguments accepted in lower-case only

The `<tier>` positional argument is accepted ONLY in lower-case
as one of `hot` / `warm` / `cold`. Upper-case (`HOT`, `WARM`,
`COLD`), mixed-case (`Hot`, `Warm`, `Cold`), or any other
spelling (typo `lukewarm`, empty string, leading/trailing
whitespace) is REJECTED with a non-zero exit and a single
stderr line containing the verbatim invalid value.

Rationale:

- This mirrors the established lower-case tier convention
  enforced by `migrate` at
  `crates/kaleidoscope-cli/src/lib.rs:432-434, :475-482` and
  by the rendering side at `lib.rs:489-495`. Accepting
  upper-case would create asymmetry across the four CLI
  subcommands and break operator muscle memory.
- The OTLP-JSON serialisation in `self_observe::cinder_bridge`
  also serialises tier values in lower-case. The cross-boundary
  contract is "tier names are lower-case everywhere on the
  wire"; this feature preserves that contract.
- Lower-case is a one-token convention; mixed-case acceptance
  multiplies the surface area for typos and bug reports without
  any user benefit. The operator types four characters
  (`cold`); typing `Cold` saves zero keystrokes.

### D-ReadOnly: `list-items` performs only `list_by_tier`

The new library function calls ONLY
`TieringStore::list_by_tier(&tenant, tier)` on the Cinder
store. It does NOT call `place`, `migrate`, or `evaluate_at`.
The Cinder WAL+snapshot under `<data_dir>/cinder.*` is
BYTE-EQUIVALENT before and after every `list-items` subcommand
invocation, including the success path AND the invalid-tier
failure path.

Rationale:

- The job is purely a read of currently-placed items. No
  mutation is needed at any branch.
- This is verifiable by the acceptance test: it asserts the
  presence/absence/byte-content of `cinder_base(data_dir)`
  state is unchanged across the call (via a follow-up
  `list_by_tier(...).len()` snapshot comparison).
- This matches the read-only invariant already preserved by
  `stats` and `stats_with_tiers` in the cluster.

### D-NoLumenTouch: `list-items` opens ONLY the Cinder store

The new library function opens ONLY
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`.
It does NOT open
`FileBackedLogStore::open(lumen_base(data_dir), recorder)`.

The Lumen WAL+snapshot under `<data_dir>/lumen.*` is
BYTE-EQUIVALENT before and after every `list-items` subcommand
invocation, including the success path AND the invalid-tier
failure path.

Rationale:

- The job is purely a Cinder-side enumeration. The Lumen side
  has no role to play (no records are read or mutated).
- Opening the Lumen store would do nothing observable but
  would add cost (WAL replay, snapshot deserialisation) AND
  would require the Lumen directory to exist, which is a
  precondition the list-items subcommand has no reason to
  require.
- Inherits the same posture from `migrate`'s D-NoLumenTouch
  decision (`cli-migrate-subcommand-v0/discuss/wave-decisions.md`).
  Verifiable by the acceptance test.

### D-Sort: Lexicographic sort at the CLI boundary

The library function sorts the `Vec<ItemId>` returned by
`cinder.list_by_tier(&tenant, tier)` BEFORE writing it to
stdout. The sort is `Vec::sort()` (or equivalent), which sorts
by `ItemId`'s natural `Ord` impl — lexicographic byte order on
the inner `String`.

Rationale:

- `cinder::InMemoryTieringStore::list_by_tier` at
  `crates/cinder/src/store.rs:190-198` iterates a
  `HashMap<(TenantId, ItemId), TierEntry>` and collects
  matching entries into a `Vec<ItemId>`. `HashMap` iteration
  order is randomised per process (since Rust 1.7+, via
  `RandomState`), so two invocations in different processes
  produce items in different orders.
- For the operator, byte-identical stdout across runs is the
  deterministic CLI experience that scripted pipelines depend
  on (`comm`, `diff`, `sort -c`). Without the boundary sort,
  the operator's `kaleidoscope-cli list-items ... | diff -
  expected.txt` would spuriously fail with reordered lines.
- The sort cost is `O(N log N)` over a `Vec<ItemId>` whose
  cardinality is bounded by the per-tenant tier count
  (typically tens to hundreds). This is negligible compared
  to the Cinder store open + `list_by_tier` cost.
- Documented as an explicit AC in `user-stories.md` (the
  determinism scenario) and as the OK1 determinism
  sub-property in `outcome-kpis.md`.

### D-OutputShape: One bare item id per line — no key=value, no header

The stdout shape is one bare item id per line, each terminated
by `\n`. There is NO `key=value` formatting (unlike `stats` and
`migrate`), NO header line, NO trailing summary line on
stdout, NO colour codes.

Rationale:

- The natural consumer is a shell pipeline (`xargs -I {}
  kaleidoscope-cli migrate ... {} ...`) that expects ONE
  RECORD PER LINE on stdin. A `key=value` shape (e.g.
  `item=acme/batch-00007`) would force operators to `cut -d=
  -f2` to extract the bare id, adding shell ceremony with no
  upside.
- The `stats` and `migrate` subcommands use `key=value`
  because their output is a SINGLE EVENT report (one or three
  lines per invocation). `list-items` output is structurally
  different: ZERO TO N records per invocation. The
  one-bare-id-per-line shape is the natural representation
  for a list of records.
- Empty stdout for N=0 (the tenant has no items in the
  queried tier) is the natural shell-pipeline signal for
  "nothing to iterate": `... | wc -l` reports `0`, `... |
  xargs ...` is a no-op, `... | grep ...` exits non-zero (per
  `grep` semantics). There is no "0 items" placeholder line
  and no header because their absence IS the result.

### D-FunctionShape: Library function shape — NAMED but NOT designed

DESIGN owns the exact signature. The task brief and the
predecessor wave precedent suggest:

- A new free function in `crates/kaleidoscope-cli/src/lib.rs`,
  likely
  `pub fn list_items(tenant: &TenantId, data_dir: &Path, tier_arg: &str, writer: impl Write) -> Result<usize, Error>`,
  where the function does the parse + open + list_by_tier +
  sort + writeln loop, returning `Ok(N)` on success (N = the
  number of lines written) and an `Error` variant on any
  failure branch (parse error, store-open error, I/O error).

The choice of returning `usize` (the count) vs `()` (unit) is
DESIGN's call; either way the stdout bytes are the same and
the binary's `run_list_items` helper can either emit a
`list-items ok: items=N` stderr summary line (using the count)
or NOT (returning unit). See D-StderrSummary below.

### D-StderrSummary: Optional stderr summary line — DESIGN's call

DESIGN may choose either:

- (a) Emit a `list-items ok: items=N` stderr summary line on
  success, mirroring the `stats ok: records=N` line at
  `crates/kaleidoscope-cli/src/main.rs:261` and the `read ok:
  records=N` line at `lib.rs:201`. The library function
  returns `Ok(N)` for this to be possible.
- (b) Emit no stderr summary line at all on success. Stderr is
  empty for the happy path. The library function returns
  `Ok(N)` regardless (the binary just ignores N).

The acceptance test asserts the STDOUT contract only. Whether
stderr carries a summary line is observable but not part of
the OK1 contract; both choices satisfy the wire-observable
invariants in `user-stories.md`. DESIGN locks the choice.

### D-ErrorVariant: NO new Error variants needed

The new library function reuses the existing variants in
`kaleidoscope_cli::Error` at
`crates/kaleidoscope-cli/src/lib.rs:72-88` without adding any
new variant:

- `Error::InvalidTier { value: String }` (already at lines
  79-81) — for the invalid-tier parse-side fail-fast. The
  `Display` impl at lines 98-100 already prints the verbatim
  invalid value to satisfy the OK3 stderr-naming contract.
- `Error::CinderOpen(MigrateError)` (already at line 77) — for
  any `FileBackedTieringStore::open` failure (filesystem
  error, corrupted snapshot, etc.).
- `Error::Io(std::io::Error)` (already at line 82) — for any
  `writeln!` I/O failure when writing to the supplied writer.

This is STRICTLY THINNER than the predecessor
(`cli-migrate-subcommand-v0`) which introduced one new
variant (`Error::CinderMigrate(MigrateError)` at lib.rs:78 for
the unknown-item case). `list-items` introduces none.

### D-StderrWording: Exact stderr wording is DESIGN's call

The OK3 (invalid-tier) contract requires:

- Non-zero exit code.
- Empty stdout.
- A single stderr line that CONTAINS the verbatim offending
  value (the invalid tier value).

The exact byte-level wording of the line (prefix, suffix,
punctuation, the `kaleidoscope-cli: ` binary-name prefix
already emitted by `main.rs:71`) is DESIGN's call. The
acceptance test asserts the SUBSTRING invariant, not byte-
exact equality, to give DESIGN flexibility.

For reference, the existing fail-fast wording for the same
`Error::InvalidTier` variant at the `migrate` site emits:

```text
kaleidoscope-cli: invalid tier "COLD": expected one of hot, warm, cold
```

which the existing `Display` impl at
`crates/kaleidoscope-cli/src/lib.rs:98-100` already produces.
The natural choice for `list-items` is to reuse this exact
wording verbatim because the same variant is reused.

### D-NewTestFile: New acceptance test file — DO NOT modify the locked test files

New file:
`crates/kaleidoscope-cli/tests/list_items_subcommand.rs`.
Mirrors the harness pattern in
`crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (the
predecessor's locked file) — which itself mirrors
`tests/stats_subcommand.rs` and the six other predecessor test
files. The `tenant`, `record`, `temp_root`, `cleanup`, `ndjson`
helpers are duplicated inline at v0 — the rule-of-three
extraction was deferred in the predecessor waves and remains a
separate refactoring task. This is now the EIGHTH `tests/*.rs`
file in the cluster using the same harness shape.

### D-LockedTests: Do NOT modify any locked test file

The existing locked acceptance test files MUST NOT be
modified:

- `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
- `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
- `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`
- (any other `tests/observe_otlp_*.rs` files in the cluster)

Each of these locks the byte-level contract for an earlier
shipped feature. The new `list-items` subcommand is a pure
ADDITION to the binary's dispatch (new arm, new helper, new
library function); it does NOT modify the behaviour of any
existing subcommand. Therefore every locked test file MUST
continue to pass green UNMODIFIED under `cargo test --package
kaleidoscope-cli` after this feature ships.

This is the supplementary oracle for the wave's "no
regression" invariant.

### D-OutOfScope-Json: No `--json` / `--csv` / `--format=...`

v0 ships exactly the plain-text one-bare-id-per-line stdout
shape. No `--json`, no `--csv`, no `--format=...`.

Rationale: same as the predecessor `migrate`'s
D-OutOfScope-Json. Machine-parseable contracts become a v1
concern once the v0 shape proves it is the right thing to
make machine-parseable. The one-bare-id-per-line shape is
ALREADY machine-parseable by every Unix text tool (`grep`,
`wc -l`, `xargs`, `sort`, `comm`, `diff`).

### D-OutOfScope-Observe: No `--observe-otlp` on `list-items`

The `list-items` subcommand does NOT accept the `--observe-otlp
<path>` flag. The Cinder recorder is hard-wired to the
quiescent `CinderRecorder` at the call site.

Rationale: `list_by_tier` is a pure read with no operator-
visible event to record. Cinder's `MetricsRecorder` trait at
`crates/cinder/src/metrics.rs` has NO `record_list` method.
There is nothing for the OTLP-JSON writer to emit on a
`list-items` invocation. Adding a `--observe-otlp` flag that
produces zero output lines would be operationally hostile
flag-surface with no clear operator workflow.

### D-OutOfScope-CrossTenant: No cross-tenant aggregate

The CLI takes exactly one `<tenant_id>` per invocation. There
is NO multi-tenant shape (`list-items <data_dir> <tier>`
without `<tenant_id>`, `list-items "acme,globex" <data_dir>
<tier>`, etc.).

Rationale: v0 ships the per-tenant primitive that mirrors the
underlying API at `crates/cinder/src/store.rs:102`. The
operator's natural workaround for the cross-tenant case is a
shell loop:

```bash
for tenant in acme globex initech; do
  kaleidoscope-cli list-items "$tenant" /tmp/data cold | \
    awk -v t="$tenant" '{print t"\t"$0}'
done
```

A genuine cross-tenant aggregate would require either a new
trait method (`list_by_tier_all_tenants(tier) ->
Vec<(TenantId, ItemId)>`) or a `list_tenants()` method (also
not exposed). Both belong to follow-up features.

### D-OutOfScope-Historical: No time-bound historical state

The CLI returns the CURRENT state of `list_by_tier(tenant,
tier)`. There is NO `--at <timestamp>` flag, no
`--since`/`--until` window, no historical reconstruction.

Rationale: deferred to ADR-0039 §7 future feature. The current
Cinder store has no historical reconstruction primitive
(the `TierEntry` records `placed_at` and `migrated_at` but the
store does not snapshot intermediate state). Introducing time-
bound enumeration would require a separate large feature.

### D-OutOfScope-Pagination: No `--limit` / `--offset`

The CLI returns ALL items in the queried tier. There is NO
pagination, no `--limit N`, no `--offset N`.

Rationale: cardinality is small in v0 (operator deployments
have on the order of tens to hundreds of items per tier per
tenant). A one-line-per-item shape with no pagination fits
the operator's `xargs` pipeline naturally. Pagination is a v1
concern once cardinality grows large enough to matter.

### D-NoSSOT: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as the seven predecessor features in the cluster.
The SSOT operator-incident-response journey is incident-time
focused; this manual tier-enumeration extension serves the
orthogonal "operator manually enumerates items in a tier"
workflow, which is operationally useful but does not rise to
the level of an SSOT journey modification. The feature-local
artefacts produced in this wave are NOT promoted to
`docs/product/journeys/` or `docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli`
crate), 2 modified files in `src/` (`lib.rs` for the new
function; `main.rs` for the new dispatch arm + new
`run_list_items` helper + `print_usage` update), 1 new test
file (`tests/list_items_subcommand.rs`), 1 manifest line-
level change (`Cargo.toml` for the new `[[test]]` entry).
Estimated effort: well under 1 day. PASSES the right-sized
gate. STRICTLY THINNER than the predecessor
(`cli-migrate-subcommand-v0`): no `get_entry` pre-flight, no
`from`/`to` resolution, no mutation, no `SystemTime::now()`
argument, no new `Error` variant.

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs
delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-list-items-subcommand-enumerates-tier.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions to lock:

- **D-FunctionShape**: the exact signature of the new
  `list_items(...)` library function in `lib.rs` (returning
  `Result<usize, Error>` with the writer as a parameter, vs
  returning `Result<(), Error>` and ignoring the count).
- **D-StderrSummary**: whether to emit a `list-items ok:
  items=N` stderr summary line on success (mirroring
  `stats`/`read`) or to leave stderr empty on the happy
  path.
- **`parse_tier` visibility**: whether to promote the
  existing private `parse_tier` helper at
  `crates/kaleidoscope-cli/src/lib.rs:475-482` to
  `pub(crate)` for direct reuse by `list_items`, or to
  duplicate the four-line `match` inline.
- **D-StderrWording**: the exact byte-level wording of the
  stderr line on OK3 (invalid tier). The contract is the
  SUBSTRING invariant (the invalid value appears in the
  line); the wording is DESIGN's call. The natural choice is
  to reuse the existing `Error::InvalidTier` Display impl
  verbatim (`crates/kaleidoscope-cli/src/lib.rs:98-100`),
  which already prints `invalid tier "<value>": expected one
  of hot, warm, cold`.
