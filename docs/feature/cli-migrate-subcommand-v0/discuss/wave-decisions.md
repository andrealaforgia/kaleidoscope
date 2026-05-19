# Wave Decisions — `cli-migrate-subcommand-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| D1 `feature_type` | `backend` (CLI subcommand) | New positional subcommand on the existing `kaleidoscope-cli` binary: `migrate <tenant_id> <data_dir> <item_id> <to_tier>`. Adds one new dispatch arm to `main.rs`'s match block (lines 50-64), one new free function in `lib.rs`, and one new acceptance test file. No new persona, no new crate, no new external dependency. |
| D2 `walking_skeleton` | `no` | The CLI exists, three subcommands (`ingest`, `read`, `stats`) work, the `TieringStore` trait already exposes `migrate(tenant, item, to_tier, migrated_at)` (`crates/cinder/src/store.rs:93-99`), `get_entry(tenant, item)` (`crates/cinder/src/store.rs:89`), and `MigrateError::UnknownItem` (`crates/cinder/src/store.rs:43`). This feature is a thin extension on top of an existing substrate. |
| D3 `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the six predecessor features). Single decision-set: trigger one Cinder tier transition per CLI invocation. Output shape collapsed by precedent: one literal stdout line in plain-text key=value-on-one-line form, mirroring the established `key=value` aesthetic of `stats` but with FOUR fields on a single line because the report is a SINGLE atomic event ("this move happened, from this tier to that tier"). |
| D4 `jtbd_analysis` | `no` | The job is obvious and singular: manual tier migration. Three operationally distinct decisions (manual rebalance / compensating policy decision / lifecycle test) all collapse to the same primitive ("move item X from current tier to target tier"). Persona inherited from the six predecessor features; forces are direct mirror-images of the operator's daily `ingest` / `stats` workflow. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. The job statement is implicit and singular: an operator wants `migrate <tenant> <data_dir> <item_id> <to_tier>` to call `TieringStore::migrate(...)` and report the from→to transition on stdout. | DIVERGE skipped by Andrea's explicit instruction. The output shape (one literal stdout line on success, one literal stderr line on failure, exit-code-based pass/fail) has exactly one reasonable shape that operators can `grep` and pipe to `awk` or `cut`. |
| No formal JTBD workshop | LOW. Persona, push (operator triggers tier moves by writing Rust today), pull (one-shot CLI tier move with from→to report), anxiety (fail-fast on unknown item ids; no silent insert; no Lumen-side mutation), habit (operator already runs `kaleidoscope-cli ingest` / `read` / `stats` daily). | Persona + emotional-arc inherited from the six reference features under `docs/feature/cli-*/discuss/` (`cli-stats-cinder-tier-distribution-v0` most directly). |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the existing `TieringStore::migrate` API surface and by the established CLI conventions (positional arguments, lower-case tier rendering, plain-text stdout, stderr-on-failure with verbatim offending value). | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D-LowerCase: Tier arguments accepted in lower-case only

The `<to_tier>` positional argument is accepted ONLY in lower-case
as one of `hot` / `warm` / `cold`. Upper-case (`HOT`, `WARM`,
`COLD`), mixed-case (`Hot`, `Warm`, `Cold`), or any other spelling
(typo `lukewarm`, empty string, leading/trailing whitespace) is
REJECTED with a non-zero exit and a single stderr line containing
the verbatim invalid value.

Rationale:

- This mirrors the established lower-case tier rendering convention
  from the read side at
  `crates/kaleidoscope-cli/src/lib.rs:389-395` (`tier_lowercase`)
  and from the predecessor `cli-stats-cinder-tier-distribution-v0`
  (which writes `hot=H` / `warm=W` / `cold=C` lines to stdout, all
  lower-case keys). Accepting upper-case on the write side would
  create read-write asymmetry and break operator muscle memory.
- The OTLP-JSON serialisation in `self_observe::cinder_bridge`
  (per the task brief's reference) also serialises tier values in
  lower-case. The cross-boundary contract is "tier names are
  lower-case everywhere on the wire"; this feature preserves that
  contract on the CLI write side.
- Lower-case is a one-token convention; mixed-case acceptance
  multiplies the surface area for typos and bug reports without
  any user benefit. The operator types four characters
  (`cold`); typing `Cold` saves zero keystrokes.

### D-Timestamp: `SystemTime::now()` at call site, no `--at` flag

The `migrated_at: SystemTime` argument passed to
`TieringStore::migrate` is `SystemTime::now()` evaluated at the
call site inside the new library `migrate` function. There is NO
`--at <timestamp>` / `--migrated-at <timestamp>` flag in v0.

Rationale:

- The operator's natural use case is "I want to record THIS move
  as happening NOW"; the `SystemTime::now()` is the correct
  default and the only one a non-test caller ever needs.
- Deterministic-time testing belongs to a separate `TestKit` /
  spike feature: the existing `cinder::InMemoryTieringStore`
  already accepts an arbitrary `SystemTime` per call (see
  `crates/cinder/src/store.rs:140` for `place` and `:172` for
  `migrate`), so unit tests of the underlying trait can use a
  controlled clock. The CLI surface does NOT need a `--at` flag
  to support those unit tests.
- The acceptance test for the new CLI subcommand asserts the
  WIRE-OBSERVABLE invariants only: the stdout report content
  (which does not include the `migrated_at` value), the post-
  call `get_entry(tenant, item).tier == to_tier`, and the post-
  call list-by-tier counts. The exact `entry.migrated_at` value
  recorded in the Cinder store is NOT part of the wire-observable
  contract for this feature — DESIGN MUST NOT introduce a `--at`
  flag for testing.

### D-NoLumenTouch: `migrate` opens ONLY the Cinder store

The new library function opens ONLY
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`.
It does NOT open
`FileBackedLogStore::open(lumen_base(data_dir), recorder)`.

The Lumen WAL+snapshot under `<data_dir>/lumen.*` is BYTE-
EQUIVALENT before and after every `migrate` subcommand invocation,
including the success path, the unknown-item failure path, and the
invalid-tier failure path.

Rationale:

- The job is purely a Cinder-side tier transition. The Lumen side
  has no role to play (no records are mutated, no records are
  read).
- Opening the Lumen store would do nothing observable but would
  add cost (WAL replay, snapshot deserialisation) AND would
  require the Lumen directory to exist, which is a precondition
  the migrate subcommand has no reason to require. An operator
  who has a Cinder snapshot inherited from a previous deployment
  but no Lumen snapshot (because the Lumen side was pruned for
  retention) should still be able to migrate items.
- This is verifiable by the acceptance test: it asserts the
  presence/absence/byte-content of `lumen_base(data_dir)` is
  unchanged across the migrate call.

### D-FunctionShape: Library function shape — NAMED but NOT designed

DESIGN owns the exact signature. The task brief and the
predecessor wave precedent suggest:

- A new free function in `crates/kaleidoscope-cli/src/lib.rs`,
  likely
  `pub fn migrate(tenant: &TenantId, data_dir: &Path, item_id: &str, to_tier_arg: &str, writer: impl Write) -> Result<(), Error>`,
  where the function does the parse + open + get_entry + migrate
  + writeln, returning `Ok(())` on success and an `Error` variant
  on any failure branch (parse error, store-open error, unknown
  item, migrate error).

The exact field layout of the report and whether the function
returns a typed `MigrateReport { from: Tier, to: Tier }` struct or
unit (`()`) is DESIGN's call; either way the stdout bytes are the
same.

### D-ErrorVariant: Error type modifications

The function adds AT MOST one new `Error` variant to
`kaleidoscope_cli::Error` at
`crates/kaleidoscope-cli/src/lib.rs:72-84`:

- `InvalidTier { value: String }` — for the parse-side fail-fast
  on a non-lower-case-`hot`/`warm`/`cold` argument. The `Display`
  impl prints the verbatim invalid value to satisfy the OK3
  stderr-naming contract.

For the unknown-item case (`MigrateError::UnknownItem` returned by
`TieringStore::migrate`), DESIGN may choose either:

- (a) Reuse the existing `Error::CinderOpen(MigrateError)`
  variant at `crates/kaleidoscope-cli/src/lib.rs:77`. This is
  semantically loose (the error did NOT come from open) but
  byte-equivalent to the OK2 stderr contract because
  `MigrateError::UnknownItem`'s `Display` already prints the
  item id verbatim (per `crates/cinder/src/store.rs:55-58`).
- (b) Introduce a new variant `Error::CinderMigrate(MigrateError)`
  cleanly separating "store-open failure" from "migrate-call
  failure". This is the more honest typing choice and is
  recommended (but not mandated) by this wave.

The wire-observable contract (stderr line on UnknownItem contains
the verbatim item id) is satisfied by EITHER choice. DESIGN locks
the variant naming.

### D-StderrWording: Exact stderr wording is DESIGN's call

The OK2 (unknown-item) and OK3 (invalid-tier) contracts require:

- Non-zero exit code.
- Empty stdout.
- A single stderr line that CONTAINS the verbatim offending value
  (the item id for OK2; the invalid tier value for OK3).

The exact byte-level wording of the line (prefix, suffix,
punctuation, the `kaleidoscope-cli: ` binary-name prefix already
emitted by `main.rs:69`) is DESIGN's call. The acceptance test
asserts the SUBSTRING invariant, not byte-exact equality, to give
DESIGN flexibility on the wording.

For reference, the existing fail-fast wording at
`crates/kaleidoscope-cli/src/main.rs:219`:

```text
kaleidoscope-cli: --since "not-an-iso": invalid ISO 8601 ...
```

A natural mirror for OK2 would be:

```text
kaleidoscope-cli: cinder migrate: cannot migrate unknown item "acme/batch-00099" for tenant acme
```

(The string `cannot migrate unknown item ... for tenant ...` is
the existing `MigrateError::UnknownItem` Display impl at
`crates/cinder/src/store.rs:55-58`.) And for OK3:

```text
kaleidoscope-cli: <to_tier> "HOT": expected one of hot, warm, cold
```

But DESIGN may choose other phrasings.

### D-NewTestFile: New acceptance test file — DO NOT modify the locked test files

New file:
`crates/kaleidoscope-cli/tests/migrate_subcommand.rs`.
Mirrors the harness pattern in
`crates/kaleidoscope-cli/tests/stats_subcommand.rs` (the
predecessor's locked file) — which itself mirrors
`tests/observe_otlp_flag.rs`, `tests/observe_otlp_read_flag.rs`,
and the four other predecessor test files. The `tenant`,
`record`, `temp_root`, `cleanup`, `ndjson` helpers are duplicated
inline at v0 — the rule-of-three extraction was deferred in the
predecessor waves (`cli-stats-subcommand-v0/discuss/wave-decisions.md`
D9 and `cli-stats-cinder-tier-distribution-v0/discuss/wave-decisions.md`
D10) and the extraction remains a separate refactoring task. This
is now the SEVENTH `tests/*.rs` file in the cluster using the
same harness shape.

### D-LockedTests: Do NOT modify any locked test file

The existing locked acceptance test files MUST NOT be modified:

- `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
- `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`
- (any other `tests/observe_otlp_*.rs` files in the cluster)

Each of these locks the byte-level contract for an earlier
shipped feature. The new `migrate` subcommand is a pure ADDITION
to the binary's dispatch (new arm, new helper, new library
function); it does NOT modify the behaviour of any existing
subcommand. Therefore every locked test file MUST continue to
pass green UNMODIFIED under `cargo test --package
kaleidoscope-cli` after this feature ships.

This is the supplementary oracle for the wave's "no regression"
invariant.

### D-OutOfScope-Bulk: No bulk migration

The CLI takes exactly one `<item_id>` per invocation. There is
NO multi-item shape (`migrate <tenant> <data_dir>
<item_ids_file>`, `migrate <tenant> <data_dir>
acme/batch-00001,acme/batch-00002`, etc.).

Rationale: v0 ships the single-item primitive. Bulk migration is
a reasonable v1 once the single-item contract is validated; the
operator's natural workaround today is a shell loop:

```bash
for item in acme/batch-00001 acme/batch-00002 acme/batch-00003; do
  kaleidoscope-cli migrate acme /tmp/data "$item" cold || break
done
```

### D-OutOfScope-Dryrun: No `--dry-run` flag

There is NO `--dry-run` flag. Every successful invocation of the
`migrate` subcommand mutates the Cinder store.

Rationale: the operator's natural dry-run today is to invoke
`kaleidoscope-cli stats <tenant> <data_dir>` BEFORE and AFTER
the migrate call and compare the per-tier counts. A genuine
no-mutation preview would require either (a) a separate trait
method (`would_migrate(tenant, item, to_tier) ->
Option<(Tier, Tier)>`) which the Cinder crate does not expose,
or (b) a transactional rollback shape which is overkill for v0.
Both belong to follow-up features.

### D-OutOfScope-Observe: No `--observe-otlp` on `migrate`

The `migrate` subcommand does NOT accept the `--observe-otlp
<path>` flag. The Cinder recorder is hard-wired to the quiescent
`cinder::NoopRecorder` at the call site.

Rationale: the predecessor wave
(`cli-stats-cinder-tier-distribution-v0`) made the same call for
the `stats` extension, and the underlying argument is the same
here: the per-migration OTLP-JSON line would add operationally
hostile flag-surface for a v0 single-item primitive, with no
clear operator workflow that benefits. If a follow-up wave
introduces bulk migration or scheduled migration, OTLP wiring
becomes more interesting then.

### D-OutOfScope-Json: No `--json` / `--csv` / `--format=...`

v0 ships exactly the plain-text key-value-on-one-line stdout
shape: `migrated tenant=<tenant> item=<item_id> from=<from>
to=<to>\n`. No `--json`, no `--csv`, no `--format=...`.

Rationale: same as the predecessor wave's D5 (no JSON output on
`stats`). Machine-parseable contracts become a v1 concern once
the v0 shape proves it is the right thing to make machine-
parseable. The new line slots into the operator's existing
`grep`/`cut`/`awk` pipeline without disruption (`kaleidoscope-cli
migrate ... | grep -o 'from=[a-z]*'`).

### D-Idempotent: Faithful to underlying API for same-tier migrate

The CLI MUST NOT introduce a special case for the same-tier
migrate. Per `crates/cinder/src/store.rs:167-188`, the underlying
`InMemoryTieringStore::migrate` is idempotent for the same-tier
case: it overwrites `entry.tier = to_tier` (line 184), updates
`entry.migrated_at = migrated_at` (line 185), and records a
metric `record_migrate(tenant, from, to_tier)` with `from == to`
(line 186). The file-backed adapter implements the same trait
and is expected to mirror the in-memory behaviour.

The CLI faithfully reports `from=X to=X` for a same-tier migrate
and exits 0. Documented in `user-stories.md` Domain Example 2
and asserted by the
`migrate_existing_item_to_its_current_tier_succeeds_idempotently`
acceptance test.

### D-NoSSOT: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as the six predecessor features in the cluster. The
SSOT operator-incident-response journey is incident-time focused;
this manual tier-migration extension serves the orthogonal
"operator manually moves a single item between tiers" workflow,
which is operationally useful but does not rise to the level of
an SSOT journey modification. The feature-local artefacts
produced in this wave are NOT promoted to
`docs/product/journeys/` or `docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli`
crate), 2 modified files in `src/` (`lib.rs` for the new
function + tier parser; `main.rs` for the new dispatch arm + new
`run_migrate` helper + `print_usage` update), 1 new test file
(`tests/migrate_subcommand.rs`), 1 manifest line-level change
(`Cargo.toml` for the new `[[test]]` entry). Estimated effort:
well under 1 day. PASSES the right-sized gate. Comparable in
size to the predecessor (`cli-stats-cinder-tier-distribution-v0`);
the structural surface area is the same (one new dispatch arm +
one new library function + one new acceptance test + one new
`[[test]]` manifest entry). The substantive difference is that
this feature MUTATES the Cinder side (one `migrate` call per
invocation) whereas the predecessor was read-only (three
`list_by_tier` calls per invocation).

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-migrate-subcommand-moves-item.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions to lock:

- **D-FunctionShape**: the exact signature of the new
  `migrate(...)` library function in `lib.rs` (returning
  `Result<(), Error>` with the writer as a parameter, vs
  returning a typed `MigrateReport { from: Tier, to: Tier }`
  the binary then renders).
- **D-ErrorVariant**: whether to introduce a new
  `Error::CinderMigrate(MigrateError)` variant or reuse the
  existing `Error::CinderOpen(MigrateError)` for the unknown-
  item case. The new variant is recommended for clarity but not
  mandated.
- **D-StderrWording**: the exact byte-level wording of the
  stderr line on OK2 (unknown item) and OK3 (invalid tier).
  The contract is the SUBSTRING invariant; the wording is
  DESIGN's call.
