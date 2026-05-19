# Wave Decisions — `cli-stats-time-range-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` (D1) | CLI plumbing inside `kaleidoscope_cli::stats_with_tiers` plus a tiny addition to `crates/kaleidoscope-cli/src/main.rs`'s `run_stats_with`. No new UI; reuses the existing positional + optional-flag idiom shipped at commit `3af7e82` and extended in `cli-read-observe-otlp-v0` and `cli-read-time-range-v0`. |
| `walking_skeleton` | `no` (D2) | The CLI already exists, the `stats` subcommand already exists, `lumen::TimeRange` already exists with the correct `[start, end)` semantics (`crates/lumen/src/record.rs:97-120`), `LogStore::query(tenant, TimeRange)` already exists, the binary already knows how to parse `--since` / `--until` via the shared `parse_time_range(args)` helper introduced for `read` in `cli-read-time-range-v0` (`crates/kaleidoscope-cli/src/main.rs:188-214`), and the ISO 8601 UTC parser (`kaleidoscope_cli::parse_iso8601_utc_nanos`) already exists at `crates/kaleidoscope-cli/src/lib.rs:528-647`. This feature threads the existing helper through the `stats` dispatcher and explicitly leaves the Cinder side alone (D-CinderScope below). |
| `research_depth` | `lightweight` (D3) | Single operator persona (Priya, inherited from the five prior CLI features). Single decision-class enabled (per-incident "how many records arrived in this window?" plus "what's the duration of the active window?"). The wiring shape is collapsed by precedent: `lumen::TimeRange::new(s, e)` is the exact type the storage layer wants; the flag values are ISO 8601 UTC strings (operator vernacular); the parser is the existing one from `cli-read-time-range-v0`. |
| `jtbd_analysis` | `no` (D4) | The job is obvious and singular: count and bracket a tenant's records by time window without dumping every record through `jq`. Persona, push, pull, anxiety, habit are direct mirrors of the five prior `kaleidoscope-cli` features and are direct extensions of the operator's everyday incident-response workflow. DIVERGE artefacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. Job statement implicit and singular: operator wants `records=N` / `earliest=` / `latest=` reflecting ONLY the half-open `[since, until)` window on the Lumen side, with the Cinder side unchanged. | DIVERGE skipped by Andrea's explicit instruction. The wiring has exactly one shape that compiles cleanly: `stats_with_tiers()` gains a way for its caller to drive a `TimeRange`, `run_stats_with` calls the existing `parse_time_range(args)` helper, and the Cinder-side calls remain untouched (D-CinderScope). |
| No formal JTBD workshop | LOW. Persona, push, pull, anxiety, habit are direct extensions of the five prior `kaleidoscope-cli` features. | Persona + emotional-arc inherited from `docs/feature/cli-read-time-range-v0/discuss/`. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the `lumen::TimeRange` API (which the CLI surface mirrors exactly) and by the existing `parse_time_range` precedent for optional-flag parsing on `read`. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D-Optional: Both `--since` and `--until` are optional; half-bounded supported

Both `--since X` and `--until Y` are optional. When only `--since X`
is set, the upper bound is `u64::MAX` (interval `[X, u64::MAX)`,
mirroring `TimeRange::all()`'s upper bound at
`crates/lumen/src/record.rs:111-114`). When only `--until Y` is set,
the lower bound is `0` (interval `[0, Y)`, mirroring `TimeRange::all()`'s
lower bound). When neither flag is set, the constructed `TimeRange`
is exactly `TimeRange::all()` and the call site is structurally
equivalent to today's at `crates/kaleidoscope-cli/src/lib.rs:359-361`.

This shape is non-negotiable because:

- "Records since 15:30" (operator runs `--since 2026-05-19T15:30:00Z`)
  and "records before yesterday" (operator runs
  `--until 2026-05-18T00:00:00Z`) are common incident-response
  queries.
- The no-flag default MUST be `TimeRange::all()` so the locked
  `stats_subcommand.rs` and `stats_cinder_tier_distribution.rs`
  test files continue to pass byte-equivalently (OK4).
- This matches the predecessor `read` feature's `D1` decision exactly
  — symmetry across `read` and `stats` reduces the operator's mental
  model.

### D-Interval: Interval semantics are `[since, until)` — closed-lower, open-upper

This is the only choice consistent with the underlying
`lumen::TimeRange` semantics. `TimeRange::contains(t)` at
`crates/lumen/src/record.rs:116-119` is:

```rust
observed_time_unix_nano >= self.start_unix_nano
    && observed_time_unix_nano < self.end_unix_nano
```

i.e. closed-lower, open-upper. A record with
`observed_time_unix_nano == since_ns` is INCLUDED; a record with
`observed_time_unix_nano == until_ns` is EXCLUDED. The CLI flag pair
MUST honour this exactly so the surface matches both the storage
semantics and the predecessor `read --since / --until` feature with
no second-guessing required at the boundary.

The half-open contract is also operationally desirable: it makes
contiguous windows trivially composable. Two successive `stats`
invocations with `[T0, T1)` then `[T1, T2)` count disjoint subsets
over `[T0, T2)` with no double-count at `T1`.

### D-CinderScope: Cinder lines are state-snapshot, time filter does NOT apply to them

**This is the DECISION point most likely to confuse reviewers and
the principal new design surface this feature introduces.**

The Cinder `hot=` / `warm=` / `cold=` lines reflect the CURRENT
per-tenant `TieringStore::list_by_tier(tenant, tier)` counts at the
call time. The `--since` / `--until` flag pair does NOT apply to
them. The flags filter the Lumen-side `lumen.query(tenant,
TimeRange)` call only; the Cinder side keeps emitting CURRENT
placements regardless of the time range.

**Alternative considered and rejected**: filter Cinder by
`placed_at` within the window.

Rationale for rejection:

- Cinder's `TierEntry` records the `placed_at` timestamp of each
  tier placement, but Cinder exposes NO "currently in tier at time
  T" query. The `TieringStore` trait offers `list_by_tier(tenant,
  tier)` (state-snapshot) and `evaluate_at(time)` (for tier
  migration decisions), neither of which fits the
  "what-was-the-tier-count-during-this-window" semantic naturally.
  An item placed Hot at T0 and migrated Warm at T1 would
  legitimately appear in BOTH the Hot count at T0 and the Warm
  count at T1; aggregating into a single window count requires a
  decision on tie-breaking (count once per tier touched? count
  only at the start of the window? count only at the end?) that
  has no obvious operator-visible right answer.
- The principal operator value of this feature is the
  `records=N` line — the count of Lumen records in the window. The
  Cinder lines today already serve a different operational purpose
  (the CURRENT tier distribution for the tenant, a state-snapshot
  signal used for tiering health, not for incident windowing). The
  two purposes have different temporal semantics, and conflating
  them under one flag pair would make the output harder to reason
  about, not easier.
- A time-bound Cinder query is a future feature candidate
  (potentially shipped as a separate `--cinder-at <ISO>` flag that
  takes an explicit snapshot time, or as a new `cinder-history`
  subcommand). Out-of-scope for this feature; documented here as a
  future candidate.

The acceptance test for this feature explicitly probes the
Cinder-invariance contract via OK3
(`outcome-kpis.md`): two invocations of `stats_with_tiers` with
two different `TimeRange` values against the same `(tenant,
data_dir)` pair produce byte-identical Cinder lines while the
Lumen lines differ.

### D-EmptyWindow: Empty bounded window mirrors the predecessor's empty-tenant contract

When the bounded `lumen.query(tenant, TimeRange::new(s, e))` returns
zero records, the Lumen-side output is exactly one line `records=0\n`
with no `earliest=` line and no `latest=` line. The Cinder lines
follow unchanged from their state-snapshot semantics per
D-CinderScope.

The existing `stats_with_tiers` body at
`crates/kaleidoscope-cli/src/lib.rs:362-369` already implements
this contract for the empty-tenant case (the `if let (Some(first),
Some(last)) = (records.first(), records.last())` arm only fires when
`records` is non-empty). Under the bounded-query extension, the
same arm fires on the windowed result instead of the
`TimeRange::all()` result; the empty-tenant contract carries over
to the empty-window case automatically because the code path is the
same.

This decision is deliberately documented (rather than left
implicit) because:

- A reviewer might propose emitting `earliest=null` / `latest=null`
  or `earliest=<since>` / `latest=<until>` lines for the empty-window
  case — both rejected because they would diverge from the
  predecessor's empty-tenant contract on which the locked OK4 oracle
  tests depend. Byte-equivalence under the no-flag default requires
  the empty-tenant contract to be preserved unchanged; the simplest
  way to ensure that is to carry the same contract through to the
  empty-window case.
- An operator who sees `records=0\n` followed by the Cinder lines
  has unambiguous information: "no Lumen records in this window,
  these are the current Cinder placements". An operator who sees
  `records=0\nearliest=null\nlatest=null` followed by the Cinder
  lines has the same information plus two distracting lines — the
  signal-to-noise ratio is strictly worse.

### D-NoNewError: Invalid ISO 8601 uses the same parse error path as `read`

The `--since` / `--until` flag values are parsed by the existing
`kaleidoscope_cli::parse_iso8601_utc_nanos`
(`crates/kaleidoscope-cli/src/lib.rs:528-647`) shipped in
`cli-read-time-range-v0`. The `parse_time_range(args)` /
`parse_flag_iso(args, flag)` helpers at
`crates/kaleidoscope-cli/src/main.rs:188-214` are REUSED unchanged
— they scan from `args.iter().skip(2)`, which is past the bin name
and the subcommand name, so they work identically for `read` and
`stats`.

Consequences:

- NO new parser code is added in this feature.
- NO new error code is introduced.
- NO new variant on `IsoParseError`
  (`crates/kaleidoscope-cli/src/lib.rs:465-483`) is added.
- The stderr error message format is byte-identical to what `read`
  produces today: `kaleidoscope-cli: --since "yesterday":
  invalid ISO 8601 length 9: expected 20 (no fraction) or 22..=30
  (with 1..=9 fractional digits)`.
- The exit code is 1 (`ExitCode::FAILURE`), same as `read`.
- The inline tests at
  `crates/kaleidoscope-cli/src/main.rs:510-553` already cover the
  helper-level invalid-input contract for both flag names; the
  `stats` side inherits this coverage automatically by sharing the
  helper.

### D-Locked-tests: Locked test files get mechanical signature-match update only

Two locked test files exist for the prior stats waves:

- `crates/kaleidoscope-cli/tests/stats_subcommand.rs` (OK4 oracle
  for `cli-stats-subcommand-v0`).
- `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  (OK4 oracle for `cli-stats-cinder-tier-distribution-v0`).

DESIGN's likely signature extension on `stats_with_tiers()` (a new
fourth `range: TimeRange` parameter, mirroring the `read()`
extension precedent) requires the existing call sites to be updated
to pass `TimeRange::all()` explicitly. This is the SAME
precedent as `observe_otlp_read_flag.rs` adopted in
`cli-read-time-range-v0`'s DELIVER wave: mechanical signature-match
update only, no assertion edits.

Note that `stats_subcommand.rs` exercises the LEGACY `stats()`
function (3-arg, not `stats_with_tiers`) at
`crates/kaleidoscope-cli/src/lib.rs:312-331`. This function is NOT
modified by this feature — it remains as-is, the byte-level OK4
oracle for the original `cli-stats-subcommand-v0` feature. So
`stats_subcommand.rs` likely needs NO signature update at all; only
`stats_cinder_tier_distribution.rs` needs the mechanical update.
DESIGN to confirm.

### D-Test-file: A new acceptance test file mirrors `stats_cinder_tier_distribution.rs`

New file: `crates/kaleidoscope-cli/tests/stats_time_range.rs`. The
harness pattern (`tenant`, `record`, `temp_root`, `cleanup`,
`ndjson`, `cinder_base`, `lumen_base`, `seed_cinder`,
`cinder_count` helpers) is duplicated inline at v0, mirroring the
rule-of-three deferral from `cli-read-observe-otlp-v0` D6 and
`cli-read-time-range-v0` D7. After this feature ships the
`kaleidoscope-cli` crate has seven test files using the same
harness. The rule-of-three extraction trigger has been past-due for
multiple features; the extraction itself is a separate refactoring
task and is NOT a deliverable of this DISCUSS wave.

### D-OutOfScope: Out-of-scope items

The following are explicitly out of scope for this feature:

1. **Cinder time-bound queries.** Cinder exposes no "currently in
   tier at time T" query; a time-bound Cinder projection is a
   future feature candidate (potentially `--cinder-at <ISO>` flag
   or a new `cinder-history` subcommand). See D-CinderScope.
2. **`--observe-otlp` on `stats`.** The `stats` subcommand does
   NOT support `--observe-otlp` today and this feature does NOT
   change that. If/when `stats` gains an OTLP-JSON emission for
   the `lumen.query.count` per-invocation, that is a separate
   feature.
3. **JSON output on `stats`.** Today `stats` emits plain-text
   `key=value\n` lines. A `--json` flag for machine-parseable
   output is a future feature candidate but is not part of this
   wave.
4. **Other Lumen query parameters** (severity filter, body
   substring filter, attribute filter). This feature ONLY wires
   the existing `TimeRange` parameter to the CLI surface.
5. **Composition with the legacy `stats()` function** (3-arg). The
   legacy `stats()` is NOT modified — it remains the byte-level OK4
   oracle for the original `cli-stats-subcommand-v0` feature.
   Only `stats_with_tiers()` is extended.
6. **Time format variants other than ISO 8601 with `Z` suffix.**
   Inherited from the `read` feature's D3 / D8: no Unix-timestamp
   input, no RFC 2822, no `YYYY-MM-DD` (date-only), no compact
   `T0930Z` form. The parser accepts exactly the two shapes
   `YYYY-MM-DDTHH:MM:SSZ` and `YYYY-MM-DDTHH:MM:SS.D..DZ` (1..=9
   fractional-second digits).
7. **SSOT journey and `jobs.yaml`** are NOT modified in this wave
   (same posture as `cli-read-time-range-v0` D9).

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate),
2 modified files (`src/lib.rs`, `src/main.rs`), 1 new test file
(`tests/stats_time_range.rs`), 1 manifest line-level change
(`Cargo.toml`), and 1 mechanical signature-match update across the
locked stats test files (no assertion edits). Estimated effort:
<= 1 day. PASSES the right-sized gate. STRICTLY THINNER than
`cli-read-time-range-v0` because the parser-side work is already
done in the prior wave; the only new design surface introduced is
D-CinderScope and its test-level guardrail (OK3) plus
D-EmptyWindow inherited from the predecessor's empty-tenant
contract.

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-stats-time-range-filter.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions:

1. Confirm the exact signature shape for the new `TimeRange`-driving
   control on `kaleidoscope_cli::stats_with_tiers` (strongly
   recommended: a new explicit `range: TimeRange` parameter, fourth
   in argument order, mirroring `read()`'s extension precedent).
   The acceptance test cares only that the caller can drive any
   `TimeRange::new(s, e)` into the underlying `lumen.query` call;
   the rest is DESIGN's choice. Default at the `run_stats_with`
   call site MUST be `TimeRange::all()` so the locked test files
   continue to pass.
2. Confirm whether the locked `stats_subcommand.rs` file needs ANY
   signature update. It exercises the legacy `stats()` function
   which this feature does NOT modify. The likely answer is NO
   mechanical update needed for `stats_subcommand.rs`, only for
   `stats_cinder_tier_distribution.rs`.
3. Confirm the `print_usage` block update for the `stats`
   subcommand documenting `--since` / `--until`, including the
   explicit D-CinderScope note (the time range applies to the
   Lumen lines only; the Cinder tier-distribution lines remain
   state-snapshot) and the D-EmptyWindow note (empty window emits
   exactly `records=0\n` with no `earliest=` / `latest=`).
