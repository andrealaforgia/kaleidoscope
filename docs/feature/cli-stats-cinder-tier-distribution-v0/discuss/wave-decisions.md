# Wave Decisions — `cli-stats-cinder-tier-distribution-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` | Extension of an existing CLI subcommand (`stats`, shipped in `cli-stats-subcommand-v0`). Adds three new stdout lines (`hot=H` / `warm=W` / `cold=C`) derived from `cinder::TieringStore::list_by_tier(tenant, tier).len()`. No new subcommand, no new flag, no new persona. |
| `walking_skeleton` | `no` | The CLI exists, `stats` exists (`crates/kaleidoscope-cli/src/lib.rs:313-332` from `cli-stats-subcommand-v0`), and Cinder's `TieringStore::list_by_tier` already returns the per-tenant per-tier item ids (`crates/cinder/src/store.rs:101-102`). This feature is a thin extension on top of an existing substrate. |
| `research_depth` | `lightweight` | Single operator persona inherited (Priya). Single decision enabled per invocation set: "what is the tier distribution for this tenant?" — a direct mirror of the existing stats decision ("what is the time window for this tenant?"). Output shape collapsed by precedent: three new key=value lines on stdout, mirroring the existing `records=` / `earliest=` / `latest=` shape. |
| `jtbd_analysis` | `no` | The job is obvious and singular: a platform operator wants tier visibility — "how many items does this tenant have in each tier right now" — without writing Rust code or inspecting `cinder.*` snapshot files by hand. Persona inherited from the predecessor feature; forces are direct mirror-images of the existing post-ingest smoke-test workflow. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. The job statement is implicit and singular: an operator wants `hot=H` / `warm=W` / `cold=C` for one tenant on stdout, in one CLI invocation, without parsing Cinder snapshot files. | DIVERGE skipped by Andrea's explicit instruction. The output shape (three additional plain-text key=value lines on stdout) has exactly one reasonable shape that operators can `grep` and pipe to `awk` or `cut`. |
| No formal JTBD workshop | LOW. Persona, push (operator inspects Cinder by writing Rust today), pull (one-shot tier counts), anxiety (no risk of mutating Cinder placements — pure query), habit (operator already runs `kaleidoscope-cli stats ...` after ingests per the predecessor feature). | Persona + emotional-arc inherited from the five reference features under `docs/feature/cli-*/discuss/` (`cli-stats-subcommand-v0` most directly). |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the existing `stats` subcommand contract (locked OK1/OK2/OK3 from the predecessor) and by the existing `TieringStore::list_by_tier` API that already returns per-tenant per-tier item id vectors. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D1: Scope is THREE additional stdout lines from the existing `stats` subcommand

The change extends the existing `stats` subcommand (shipped in commit
`75f15a6`) so that the same invocation:

```text
kaleidoscope-cli stats <tenant_id> <data_dir>
```

…ALSO emits, after the existing `records=` / `earliest=` / `latest=`
lines, up to three additional key=value lines:

```text
hot=H
warm=W
cold=C
```

…where `H`, `W`, `C` are the lengths of
`cinder::TieringStore::list_by_tier(tenant, Tier::Hot)`,
`list_by_tier(tenant, Tier::Warm)`, and
`list_by_tier(tenant, Tier::Cold)` respectively against a
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)` opened
in the same call.

No new subcommand. No new flag. No change to the positional arguments.
Output goes to **stdout** (inherited from the predecessor's stream
contract). The new keys are exactly `hot`, `warm`, `cold` — lower-case,
matching the tier enum variants from `crates/cinder/src/tier.rs:28-32`
rendered in lower-case.

### D-EmptyRender: Option B — emit Cinder lines selectively (only for non-zero tier counts)

**Chosen: Option B.** The empty-render contract is:

- For tenants with `records > 0` (the populated-Lumen case): the
  existing `records=N` / `earliest=` / `latest=` lines appear FIRST,
  followed by `hot=H` / `warm=W` / `cold=C` lines **only for tiers
  where the count is non-zero**.
- For tenants with `records == 0` (the empty-Lumen case): the existing
  contract holds — `records=0\n` is the first line — followed by
  `hot=H` / `warm=W` / `cold=C` lines **only for tiers where the count
  is non-zero**.

Concrete renderings under Option B:

| Lumen state | Cinder state | Output |
|-------------|--------------|--------|
| 7 records | hot=3, warm=2, cold=0 | `records=7\nearliest=...\nlatest=...\nhot=3\nwarm=2\n` (no `cold=` line) |
| 7 records | hot=0, warm=0, cold=0 | `records=7\nearliest=...\nlatest=...\n` (byte-equivalent to predecessor) |
| 7 records | hot=5, warm=4, cold=2 | `records=7\nearliest=...\nlatest=...\nhot=5\nwarm=4\ncold=2\n` |
| 0 records | hot=0, warm=0, cold=0 | `records=0\n` (byte-equivalent to predecessor — OK3 unchanged) |
| 0 records | hot=2, warm=0, cold=1 | `records=0\nhot=2\ncold=1\n` (no `warm=` line, no timestamp lines) |

#### Rationale for Option B over A and C

- **Option A (strict OK3 compat — never emit Cinder lines for an empty
  Lumen tenant)** is rejected because the situation "tenant has 0
  Lumen records but non-zero Cinder placements" is operationally
  meaningful — it surfaces orphan tier metadata, which is exactly the
  thing an operator who runs `stats` wants to learn about. Suppressing
  the Cinder lines for that case would silently hide a real
  operational issue.
- **Option C (always emit all six lines, with explicit zeros)** is
  rejected because explicit `hot=0\nwarm=0\ncold=0\n` lines on every
  populated-Lumen tenant that has no Cinder placements would BREAK the
  byte-equivalent backwards-compatibility for the
  predecessor (`cli-stats-subcommand-v0`). Every existing operator
  shell pipeline that asserts `wc -l == 3` on a populated `stats`
  output for a tenant with no Cinder placements would silently start
  failing. Option C also forces operators to grep-distinguish "tier is
  genuinely empty for this tenant" from "tier has items" by parsing
  the value side of the line, which is a strictly worse interface
  than letting the line's absence be the signal (the same posture as
  the predecessor's D5 for the empty-tenant case).
- **Option B (selective emission of non-zero tier lines)** gives the
  operator a `grep`-friendly answer for the cases they care about
  (`grep ^hot= | cut -d= -f2` returns the hot count, or returns
  nothing if hot is zero — same `wc -l = 0` disambiguation pattern as
  the predecessor's empty-tenant case for `earliest=`/`latest=`).
  Crucially, Option B preserves byte-equivalence for the most common
  legacy case: a tenant that has Lumen records but no Cinder
  placements gets the SAME three-line output as before the feature
  shipped. The empty-Lumen-empty-Cinder case (OK3 from the
  predecessor) gets the SAME one-line output as before. The Option B
  contract differs only when there are Cinder placements to surface.

This decision is documented in OK3 and OK4 (`outcome-kpis.md`) and
restated as a System Constraint in `user-stories.md`.

### D-Backwards-compat: byte-equivalent stdout for tenants without Cinder placements

The predecessor wave (`cli-stats-subcommand-v0`) locked three contracts
on `stats()`'s stdout. This wave preserves all three for tenants
without Cinder placements:

| Predecessor invariant | This wave's posture |
|-----------------------|---------------------|
| OK1 (populated tenant: `records=N` line equals what `read()` returns) | UNCHANGED. The `records=` line is still derived from `lumen.query(tenant, TimeRange::all()).len()`. Cinder is not consulted for the `records=` value. |
| OK2 (populated tenant: `earliest=` / `latest=` lines equal min/max `observed_time_unix_nano`) | UNCHANGED. The timestamp lines are still derived from the Lumen record set. Cinder is not consulted for timestamps. |
| OK3 (empty tenant: exactly `records=0\n`, no timestamp lines) | EXTENDED (per Option B): if the empty-Lumen tenant ALSO has no Cinder placements (the common case for a never-ingested tenant), the output is byte-equivalent to the predecessor — exactly `records=0\n`. If the empty-Lumen tenant has Cinder placements (the unusual but possible case), the output adds the non-zero `hot=` / `warm=` / `cold=` lines after `records=0`. |

The new invariant (OK4, this feature) is the backwards-compatibility
contract: **for any tenant whose Cinder placements are all zero,
`stats()`'s stdout is byte-equivalent to the predecessor's
output for the same `(tenant, data_dir)` pair**. The locked
`tests/stats_subcommand.rs` test file is the byte-level oracle for
this invariant — if every test in that file continues to pass green
after this feature ships, OK4 is satisfied. The new
`tests/stats_cinder_tier_distribution.rs` test file adds the new
assertions for the Cinder-positive cases.

### D2: Out of scope — Cinder placement triggering

This subcommand does NOT trigger any Cinder placement, migration, or
policy evaluation. It is a pure read over the existing
`cinder.*` WAL+snapshot. The `recorder` for the
`FileBackedTieringStore::open` call is a quiescent `NoopRecorder` (the
same pattern `ingest`'s no-flag arm uses at
`crates/kaleidoscope-cli/src/lib.rs:173`). No `place()` calls, no
`migrate()` calls, no `evaluate_at()` calls. The Cinder store under
`<data_dir>/cinder.*` is unchanged after the call.

### D3: Out of scope — policy evaluation

`TieringStore::evaluate_at(now, policy)` is NOT called. The `stats`
subcommand does not need to know about the tier policy; it reports the
CURRENT distribution as the operator sees it, not the projected
distribution under any policy. Operators who want to know "what would
happen if I evaluated the policy at time T" get that from a follow-up
feature (or from existing test harnesses).

### D4: Out of scope — per-item dump

No `--items` / `--list-items` / `--per-item` flag. The output is the
COUNT per tier (`H`, `W`, `C`), not the item ids themselves. An
operator who wants the item ids can write a follow-up feature; the
operationally useful answer for the v0 of this feature is the
distribution count, not the enumeration. The
`list_by_tier(tenant, tier).len()` reduction is exactly the operator's
quesion.

### D5: Out of scope — JSON / CSV output

No `--json`, no `--csv`, no `--format=...`. v0 ships exactly the
key=value text shape (`hot=H`, `warm=W`, `cold=C`) on stdout. Same
rationale as the predecessor's D4: machine-parseable contracts become
a v1 concern once the v0 output shape proves it is the right thing to
make machine-parseable. The new lines slot into the operator's
existing `grep ^hot= | cut -d= -f2` pipeline without disruption.

### D6: Out of scope — Cinder-only mode

No `--cinder-only` / `--no-lumen` flag. The output ALWAYS starts with
the existing Lumen lines (`records=N`, optional `earliest=` / `latest=`)
and the Cinder lines come AFTER. Operators who want only the Cinder
lines can `grep -E '^(hot|warm|cold)='` on the stdout output. The
Lumen lines are cheap to produce (the same `lumen.query(...)` call that
the predecessor already runs) and removing them would break the
byte-equivalence contract with the predecessor for the existing legacy
shape.

### D7: Tier key naming — lower-case, exactly `hot` / `warm` / `cold`

The `Tier` enum variants are `Hot`, `Warm`, `Cold`
(`crates/cinder/src/tier.rs:28-32`). The stdout keys are the lower-case
forms: `hot`, `warm`, `cold`. Rationale:

- Lower-case keys are the established convention for the existing
  stats output keys (`records`, `earliest`, `latest`).
- Operators pipe through `grep` / `awk` / `cut`, which are case-
  sensitive by default; matching the established lower-case convention
  preserves their muscle memory.
- The mapping is unambiguous and reversible (`hot` ↔ `Tier::Hot`,
  etc.).

### D8: Tier key ordering — `hot`, then `warm`, then `cold`

When more than one tier line is emitted, the order is always
`hot` → `warm` → `cold`. Rationale:

- The `Tier` enum has a documented forward lifecycle:
  `Hot.next_forward() == Some(Warm)`, `Warm.next_forward() == Some(Cold)`
  (`crates/cinder/src/tier.rs:38-44`). Walking the lifecycle in order
  is the operator's natural mental model.
- Deterministic key ordering is required by the test oracle (the new
  acceptance test asserts byte-exact lines in order).

### D9: Library function shape — NAMED but NOT designed

DESIGN owns the exact signature. The task brief offers two plausible
shapes:

- Extend the existing `stats()` to take a third optional capability
  (e.g. an `Option<&dyn TieringStore>` parameter), and have the binary
  pass `Some(&cinder_store)` when invoking.
- Add a parallel `stats_with_cinder()` function with the same
  signature shape as `stats()` plus the Cinder store opening, leaving
  the existing `stats()` untouched.

A third reasonable shape (not requested in the brief but worth
flagging): modify `stats()` in-place to ALWAYS open the
`FileBackedTieringStore` at `cinder_base(data_dir)` and emit the
Cinder lines per Option B (D-EmptyRender). The existing
`tests/stats_subcommand.rs` test file would then need to NOT break,
which Option B guarantees as long as the test fixtures place no
Cinder items for the tenants they exercise (verify against the
locked test file: the predecessor's tests call `ingest()` which DOES
place Cinder Hot items per batch via the existing `flush()` at
`crates/kaleidoscope-cli/src/lib.rs:243-244`, so the locked tests
WOULD break under the in-place modification).

This rules out the third shape and confirms the task brief's two
shapes as the candidate set. DESIGN locks the choice. The
wire-observable contract (the byte-exact stdout produced) is what
matters for the DISCUSS wave's acceptance criteria; the signature is
a DESIGN concern.

### D10: Acceptance test file location — NEW file, do NOT touch the predecessor's file

New file:
`crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`.
Mirrors the harness pattern in
`crates/kaleidoscope-cli/tests/stats_subcommand.rs` (which itself
mirrors `observe_otlp_flag.rs`, `observe_otlp_read_flag.rs`). The
`tenant`, `record`, `temp_root`, `cleanup`, `ndjson` helpers are
duplicated inline at v0 — the rule-of-three extraction was deferred
in the predecessor wave (`cli-stats-subcommand-v0/discuss/wave-decisions.md`
D9) and the extraction remains a separate refactoring task. This is
now the SIXTH `tests/*.rs` file in the cluster using the same harness
shape.

The existing `tests/stats_subcommand.rs` MUST NOT be modified. The
predecessor wave locked OK1/OK2/OK3 byte-for-byte against the
assertions in that file; any modification would be a regression on the
predecessor's contract. The new test file holds the NEW assertions for
the Cinder-positive cases (and a regression test that re-asserts the
predecessor's empty-tenant invariant in the new
no-Lumen-and-no-Cinder case).

### D11: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as the predecessor. The SSOT operator-incident-response
journey is incident-time focused; this tier-distribution extension
serves the orthogonal "operator confirms tier distribution and detects
migration progress" workflow, which is operationally useful but does
not rise to the level of an SSOT journey modification. The
feature-local artefacts produced in this wave are NOT promoted to
`docs/product/journeys/` or `docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate),
no new file under `src/` (the change is structurally a modification of
`stats()` or a new sibling function in `lib.rs`), 1 new test file
(`tests/stats_cinder_tier_distribution.rs`), 1 manifest line-level
change (`Cargo.toml` for the new `[[test]]` entry). Estimated effort:
well under 1 day. PASSES the right-sized gate. Strictly comparable in
size to `cli-stats-subcommand-v0` (the predecessor); structurally
SMALLER because the Lumen-side work is reused unchanged and only the
Cinder-side computation (three `list_by_tier(...).len()` calls plus
three conditional `writeln!`s) is new.

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-stats-includes-cinder-tier-distribution.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decision: lock the function shape (D9). The
two plausible shapes are (a) extend `stats()` with an optional third
capability, or (b) add a parallel `stats_with_cinder()` function. The
third in-place modification shape is ruled out by D9 because it would
break the predecessor's locked test file (the predecessor's
`ingest()`-based fixtures DO place Cinder items per batch).
