# Wave Decisions — `cli-get-tier-subcommand-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| D1 `feature_type` | `backend` (CLI subcommand) | New positional subcommand on the existing `kaleidoscope-cli` binary: `get-tier <tenant> <data_dir> <item_id>`. Adds one new dispatch arm to `main.rs`, one new free function in `lib.rs`, and one new acceptance test file. No new persona, no new crate, no new external dependency. |
| D2 `walking_skeleton` | `no` | The CLI exists, six subcommands (`ingest`, `read`, `stats`, `list-items`, `place`, `migrate`) work, the `TieringStore` trait already exposes `get_tier(tenant, item) -> Option<Tier>` (`crates/cinder/src/store.rs:85`). This feature is a thin extension on top of an existing substrate. |
| D3 `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the predecessor features). Single decision-set: confirm a single item's current tier from one CLI invocation. Output shape collapsed by precedent: `tier=<lowercase>\n` mirrors the `stats` `key=value` convention. |
| D4 `jtbd_analysis` | `no` | The job is obvious and singular: "what tier is item X in right now?". Three operationally distinct triggers (confirm before manual migrate / scripted pipeline assertions / audit specific item from an alert) all collapse to the same primitive ("read the tier for one (tenant, item_id) pair"). Persona inherited; forces are direct mirror-images of the operator's daily `stats` / `list-items` / `migrate` workflow. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. The job statement is implicit and singular: an operator wants `get-tier <tenant> <data_dir> <item_id>` to call `TieringStore::get_tier(...)` and report the tier on stdout. | DIVERGE skipped by Andrea's explicit instruction. The output shape (one literal stdout line on success, one literal stderr line on failure, exit-code-based pass/fail) has exactly one reasonable shape that operators can `grep` and pipe to `awk`/`cut`. |
| No formal JTBD workshop | LOW. Persona, push (operator runs three `list-items` invocations and greps each, today), pull (one CLI invocation returns the answer), anxiety (fail-fast on unknown item; no silent default tier), habit (operator already runs `kaleidoscope-cli stats` / `list-items` / `migrate` daily). | Persona + emotional-arc inherited from the six reference features under `docs/feature/cli-*/discuss/`. |
| No standalone Three Amigos session | LOW. The shape is doubly constrained: by the existing `TieringStore::get_tier` API surface and by the established CLI conventions (positional arguments, lower-case tier rendering, plain-text stdout, stderr-on-failure with verbatim offending value). | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D-OutputShape: `tier=<lowercase>\n` (mirrors stats convention)

Stdout on success is exactly ONE line:

```text
tier=<lowercase>\n
```

where `<lowercase>` is `hot` / `warm` / `cold` rendered through the
same lower-case mapping as `crates/kaleidoscope-cli/src/lib.rs`
`tier_lowercase` (lines 564-570). No header, no JSON, no CSV, no
colour codes. Exit code 0.

Rationale:

- Mirrors the `key=value` aesthetic of `stats` (`hot=N`, `warm=N`,
  `cold=N`). The single `tier=<lowercase>` line slots into the same
  operator's grep/awk/cut pipeline without disruption
  (`kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042 |
  cut -d= -f2`).
- Lower-case tier rendering is the established cross-boundary
  contract (read-side at lines 564-570 in `lib.rs`, write-side
  enforced by `migrate`'s `parse_tier` at the same file, OTLP-JSON
  serialisation in `self_observe::cinder_bridge`).
- One line per fact: this subcommand answers exactly one question
  ("what tier?"); the output has exactly one field; the line has
  exactly one `key=value` pair.

### D-StderrWording: Mirror `migrate`'s `UnknownItem` text

On `get_tier(tenant, &item)` returning `None`, exit non-zero and
emit a single stderr line that contains the text
`unknown item "<item_id>" for tenant <tenant>`. This mirrors the
`MigrateError::UnknownItem` `Display` impl at
`crates/cinder/src/store.rs:55-58`:

```text
cannot migrate unknown item "<item_id>" for tenant <tenant>
```

The exact byte-level prefix (`kaleidoscope-cli:` binary-name prefix
from `main.rs`, "cinder " prefix or not, "cannot migrate" vs plain
"unknown item") is DESIGN's call. The contract here is the
SUBSTRING invariant: the stderr line MUST contain the verbatim
item id AND the verbatim tenant string AND the literal `unknown
item` token. The acceptance test asserts the substring invariant,
not byte-exact equality, to give DESIGN flexibility on the wording.

Rationale:

- `get-tier` is the read-side counterpart of `migrate`'s
  pre-flight `get_entry` check; the operator should see the same
  "unknown item" language whether they query the tier or attempt
  to mutate it. Read-side / write-side symmetry on the wording.
- The `MigrateError::UnknownItem` `Display` text is already a
  shipped contract (locked in `tests/migrate_subcommand.rs:319`
  via `stderr.contains("unknown item")`). Re-using the phrase
  preserves operator muscle memory.

### D-NoLumenTouch: `get-tier` opens ONLY the Cinder store

The new library function opens ONLY
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`.
It does NOT open
`FileBackedLogStore::open(lumen_base(data_dir), recorder)`.

The Lumen WAL+snapshot under `<data_dir>/lumen.*` is BYTE-
EQUIVALENT before and after every `get-tier` subcommand invocation,
including the success path and the unknown-item failure path.

Rationale: same as `migrate`'s D-NoLumenTouch. The job is purely
a Cinder-side tier read. The Lumen side has no role.

### D-ReadOnly: No MetricsRecorder hook, no `--observe-otlp`

`get_tier` is a READ. It does not call any `record_*` recorder
method (see `crates/cinder/src/store.rs:154-160` — the
`InMemoryTieringStore::get_tier` impl does NOT touch
`self.recorder`). Therefore there is NO operationally meaningful
OTLP signal a `--observe-otlp <path>` flag could attach to.

The subcommand REJECTS any `--observe-otlp` flag (or just does not
accept it; DESIGN's call on whether a parse error or a usage error
is the right surface). v0 ships with no flags at all on this
subcommand.

The Cinder recorder constructed at the call site is a quiescent
`CinderRecorder` (the `NoopRecorder` equivalent, mirroring the
no-flag arm of `list_items` at line 534 of `lib.rs`). Any recorder
calls that happen (there are none, since `get_tier` does not
record) would be quiescent.

Rationale: same as `cli-stats-cinder-tier-distribution-v0`'s OTLP
decision and the task brief's pre-decided D2 (no WS) and
out-of-scope list.

### D-Tenant-Isolation: Inherited from `TieringStore`

`get-tier acme /tmp/data acme/batch-00042` MUST NOT read or report
`globex`'s tier metadata even if `globex` has placed an item with
the same `ItemId` string under the same `data_dir`. The placement
key in `cinder::InMemoryTieringStore` is `(TenantId, ItemId)` per
`crates/cinder/src/store.rs:119`; the trait's per-tenant isolation
invariant (`crates/cinder/src/store.rs:71-72`) is inherited by
this read.

Verified by the acceptance test's tenant-isolation scenario:
seed `acme/batch-00042` in Hot for `acme` AND
`acme/batch-00042` in Warm for `globex`; assert `get-tier acme
... acme/batch-00042` returns `tier=hot` and the post-call
`get_tier(globex, &item)` STILL returns `Some(Tier::Warm)`.

### D-FunctionShape: Library function shape — NAMED but NOT designed

DESIGN owns the exact signature. Likely:

```rust
pub fn get_tier(
    tenant: &TenantId,
    data_dir: &Path,
    item_id: &str,
    writer: impl Write,
) -> Result<(), Error>
```

— mirroring `list_items`'s shape at `lib.rs:525-542` (the
closest precedent: read-only, four-positional, no recorder hook,
no flag surface).

The exact field layout (writes directly to writer + returns
`Result<(), Error>` vs returns a typed `Option<Tier>` and lets
`main.rs` render the bytes) is DESIGN's call. Either way the
stdout bytes on success are identical.

### D-ErrorVariant: Reuse `Error::CinderOpen` or add a new variant

For the unknown-item case (`get_tier(...) -> None`), DESIGN may
choose either:

- (a) Reuse the existing `Error::CinderMigrate(MigrateError)`
  variant from `lib.rs:97` by constructing
  `MigrateError::UnknownItem { tenant, item }` directly at the
  `get_tier` call site even though no migrate was attempted. This
  is semantically off-label (the error did NOT come from a
  migrate) but byte-equivalent to the OK2 stderr contract because
  the Display already prints the canonical "unknown item ... for
  tenant ..." phrase.
- (b) Introduce a NEW `Error::CinderUnknownItem { tenant: TenantId,
  item: ItemId }` variant whose `Display` impl emits
  `unknown item "<item>" for tenant <tenant>` (without the
  "cannot migrate" prefix, since no migrate was attempted). This
  is the more honest typing choice.

Either way the wire-observable contract (stderr line contains
`unknown item`, the verbatim item id, and the verbatim tenant) is
satisfied. DESIGN locks the variant naming.

### D-NewTestFile: New acceptance test file — DO NOT modify the locked test files

New file:
`crates/kaleidoscope-cli/tests/get_tier_subcommand.rs`.
Mirrors the harness pattern in
`crates/kaleidoscope-cli/tests/list_items_subcommand.rs` and
`tests/migrate_subcommand.rs`. The `tenant`, `temp_root`,
`cleanup`, helpers are duplicated inline at v0 — the rule-of-three
extraction was deferred in prior waves and remains a separate
refactoring task. This is now the next `tests/*.rs` file in the
cluster using the same harness shape.

### D-LockedTests: Do NOT modify any locked test file

The existing locked acceptance test files MUST NOT be modified:

- `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
- `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
- `crates/kaleidoscope-cli/tests/list_items_subcommand.rs`
- `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`
- `crates/kaleidoscope-cli/tests/place_subcommand.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_*.rs` (all)

Each locks the byte-level contract for an earlier shipped feature.
The new `get-tier` subcommand is a pure ADDITION to the binary's
dispatch (new arm, new helper, new library function); it does NOT
modify the behaviour of any existing subcommand. Therefore every
locked test file MUST continue to pass green UNMODIFIED under
`cargo test --package kaleidoscope-cli`.

### D-OutOfScope-Bulk: No bulk get-tier

The CLI takes exactly one `<item_id>` per invocation. No
`get-tier <tenant> <data_dir> <item_ids_file>` shape. Bulk lookup
is a reasonable v1; the operator's natural workaround today is a
shell loop:

```bash
for item in acme/batch-00001 acme/batch-00002 acme/batch-00003; do
  kaleidoscope-cli get-tier acme /tmp/data "$item"
done
```

### D-OutOfScope-Json: No `--json` / `--csv` / `--format=...`

v0 ships exactly the plain-text key-value-on-one-line stdout
shape: `tier=<lowercase>\n`. No `--json`, no `--csv`, no
`--format=...`. Same posture as `stats`, `list-items`, `migrate`,
`place`.

### D-OutOfScope-Observe: No `--observe-otlp` on `get-tier`

Restated from D-ReadOnly above: the subcommand does NOT accept a
`--observe-otlp <path>` flag. `get_tier` is a read; no
`MetricsRecorder` hook exists for it (`crates/cinder/src/store.rs:154-160`).
There is no operationally meaningful OTLP signal to attach.

### D-OutOfScope-FullEntry: No `get-entry` shape (placed_at, migrated_at)

The operator's question for this feature is the narrow one:
"what tier is this item in?". The richer question ("what tier,
when was it placed, when was it last migrated?") would surface
`TieringStore::get_entry(...) -> Option<TierEntry>` returning the
full `TierEntry { tier, placed_at, migrated_at }` triple. That
shape is OUT OF SCOPE for v0. It belongs to a future feature
(`cli-get-entry-subcommand-v0` or equivalent) that ships once
the v0 `get-tier` primitive is validated and the operator has
articulated the need for the richer view.

### D-NoSSOT: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as the predecessor features in the cluster. The
SSOT operator-incident-response journey is incident-time focused;
this read-side single-item tier-query extension serves the
orthogonal "operator confirms a single item's tier before manual
action" workflow, which is operationally useful but does not
rise to the level of an SSOT journey modification.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli`
crate), 2 modified files in `src/` (`lib.rs` for the new
function; `main.rs` for the new dispatch arm + new `run_get_tier`
helper + `print_usage` update), 1 new test file
(`tests/get_tier_subcommand.rs`), 1 manifest line-level change
(`Cargo.toml` for the new `[[test]]` entry). Estimated effort:
well under 1 day. PASSES the right-sized gate. Strictly thinner
than `migrate` because (a) no tier-argument parser needed
(`get-tier` takes no tier argument), (b) no `from`/`to` pair to
render, (c) the underlying `get_tier` API returns
`Option<Tier>` directly so there is no need for a pre-flight
`get_entry` call.

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-get-tier-subcommand-reports-current-tier.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions to lock:

- **D-FunctionShape**: the exact signature of the new
  `get_tier(...)` library function in `lib.rs`.
- **D-ErrorVariant**: whether to reuse
  `Error::CinderMigrate(MigrateError::UnknownItem)` semantically
  off-label, or introduce a dedicated
  `Error::CinderUnknownItem` variant.
- **D-StderrWording**: the exact byte-level wording of the
  stderr line on the unknown-item path. The contract is the
  SUBSTRING invariant (`unknown item`, the verbatim item id,
  the verbatim tenant); the wording is DESIGN's call.
