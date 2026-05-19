# Wave Decisions — `cli-read-time-range-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` (D1) | CLI plumbing inside `kaleidoscope_cli::read`. No new UI; reuses the existing positional + flag idiom shipped at commit `3af7e82` and extended in `cli-read-observe-otlp-v0`. |
| `walking_skeleton` | `no` (D2) | The CLI already exists, `read` already exists, `lumen::TimeRange` already exists with the correct `[start, end)` semantics (`crates/lumen/src/record.rs:97-120`), `LogStore::query(tenant, TimeRange)` already exists, and the binary already knows how to parse optional flags after positional arguments (`parse_observe_otlp` at `crates/kaleidoscope-cli/src/main.rs:130-144`). This feature extends one subcommand with two optional flags and a hand-rolled parser. |
| `research_depth` | `lightweight` (D3) | Single operator persona (Priya, inherited from the four prior CLI features). Single decision-class enabled (per-incident "what arrived in this window?" queries). The wiring shape is collapsed by precedent: `lumen::TimeRange::new(s, e)` is the exact type the storage layer wants; the flag values are ISO 8601 UTC strings (operator vernacular); the parser is the inverse of the existing hand-rolled `format_iso8601_utc_nanos` at `crates/kaleidoscope-cli/src/lib.rs:410-420`. |
| `jtbd_analysis` | `no` (D4) | The job is obvious and singular: filter a tenant's records by time window without streaming the full tenant dump. Persona, push, pull, anxiety, habit mirror-image of the four prior `kaleidoscope-cli` features and are direct extensions of the operator's everyday incident-response workflow. DIVERGE artefacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. Job statement implicit and singular: operator wants ONLY records in `[since, until)` on stdout, instead of the full tenant dump. | DIVERGE skipped by Andrea's explicit instruction. The wiring has exactly one shape that compiles cleanly: the `read()` library function gains a way for its caller to drive a `TimeRange`, the `main.rs` `run_read` dispatcher gains two `--since` / `--until` flag-parse helpers structurally identical to `parse_observe_otlp`, and the parser is the inverse of the existing formatter. |
| No formal JTBD workshop | LOW. Persona, push, pull, anxiety, habit are direct extensions of the four prior `kaleidoscope-cli` features (already validated by ship at commits `3af7e82` and predecessors). | Persona + emotional-arc inherited from `docs/feature/cli-read-observe-otlp-v0/discuss/`. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the `lumen::TimeRange` API (which the CLI surface mirrors exactly) and by the existing `parse_observe_otlp` precedent for optional-flag parsing. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D1: Both `--since` and `--until` flags are optional; half-bounded supported

Both `--since X` and `--until Y` are optional. When only `--since X`
is set, the upper bound is `u64::MAX` (interval `[X, u64::MAX)`,
mirroring `TimeRange::all()`'s upper bound at
`crates/lumen/src/record.rs:111-114`). When only `--until Y` is set,
the lower bound is `0` (interval `[0, Y)`, mirroring `TimeRange::all()`'s
lower bound). When neither flag is set, the constructed `TimeRange`
is exactly `TimeRange::all()` and the call site is structurally
equivalent to today's at `crates/kaleidoscope-cli/src/lib.rs:283-285`.

This shape is non-negotiable because:

- "Last 90 minutes" (operator runs `--since 2026-05-19T15:30:00Z`)
  and "everything before yesterday" (operator runs
  `--until 2026-05-18T00:00:00Z`) are common incident-response
  queries; requiring both flags would force the operator to invent
  a sentinel upper/lower bound by hand, which is the exact
  paper-cut this feature exists to remove.
- The no-flag default MUST be `TimeRange::all()` so the existing
  locked test files (`observe_otlp_read_flag.rs` and
  `observe_otlp_flag.rs`) continue to pass byte-equivalently (OK2).

### D2: Interval semantics are `[since, until)` — closed-lower, open-upper

This is the only choice consistent with the underlying
`lumen::TimeRange` semantics. `TimeRange::contains(t)` at
`crates/lumen/src/record.rs:116-119` is:

```rust
observed_time_unix_nano >= self.start_unix_nano
    && observed_time_unix_nano < self.end_unix_nano
```

i.e. closed-lower, open-upper. A record with
`observed_time_unix_nano == since_ns` is INCLUDED in the result; a
record with `observed_time_unix_nano == until_ns` is EXCLUDED. The
CLI flag pair MUST honour this exactly so the surface matches the
storage semantics with no second-guessing required at the boundary.

The doc-comment on `TimeRange` itself confirms this
(`crates/lumen/src/record.rs:94-96`):

> Half-open time range `[start, end)` in nanoseconds since the
> Unix epoch. A record matches when
> `start <= observed_time_unix_nano < end`.

The half-open contract is also operationally desirable: it makes
contiguous windows trivially composable. `[T0, T1)` followed by
`[T1, T2)` produces the disjoint union over `[T0, T2)` with no
double-count at `T1`.

### D3: ISO 8601 UTC only, `Z` suffix only (no other timezone variants)

The accepted shapes are exactly:

- `YYYY-MM-DDTHH:MM:SSZ` (no fractional seconds)
- `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` (1..=9 fractional-second digits;
  fewer than 9 digits are left-padded to 9 internally before the
  nanosecond conversion)

No other timezone forms are accepted at v0: not `+00:00`, not
`+0000`, not `UTC`, not bare `YYYY-MM-DD` (no time-of-day), not
RFC 2822 forms. The parser fails fast on any deviation per OK4.

Rationale:

- The CLI's own `stats` subcommand emits ISO 8601 UTC with the `Z`
  suffix (`crates/kaleidoscope-cli/src/lib.rs:419`). The accepted
  parser input is the exact superset that round-trips with the
  formatter (the formatter always emits 9 fractional digits; the
  parser accepts 0..=9 digits and the no-`.` form for symmetry with
  hand-typed operator input).
- Restricting to `Z` removes an entire category of parser-complexity
  and an entire category of operator-error mode (offset arithmetic
  mistakes).
- A future extension to accept `+00:00` is non-breaking: any
  currently-accepted shape remains accepted under the extended
  parser. So the `Z`-only constraint is a v0 simplification, not a
  forward-incompatible choice.

### D4: Hand-rolled parser — no `chrono`, no `time`

The parser is hand-rolled. Decision criteria:

- The project already hand-rolls the inverse direction
  (`format_iso8601_utc_nanos` at
  `crates/kaleidoscope-cli/src/lib.rs:410-420`, using
  Howard Hinnant's `civil_from_days` algorithm at lines 426-438).
  The parser direction needs the inverse `days_from_civil` (also
  Howard Hinnant, also public domain) plus a small state machine
  that walks the fixed-shape input `YYYY-MM-DDTHH:MM:SS[.FFF..FFF]Z`.
- The accepted input alphabet is `[0-9TZ:.-]`. The shape is fully
  fixed (positions of separators are known a priori; no whitespace
  tolerance; no AM/PM; no day-of-week; no timezone offsets). This
  is the easy direction of ISO 8601 parsing.
- Pulling `chrono` adds ~100 transitive dependencies and the C
  `time` crate (`libc` on Unix) into the binary's dependency graph;
  the `kaleidoscope-cli` binary currently has a small, principled
  dep tree.
- Pulling `time` adds 5-10 transitive dependencies and includes
  parsing modes for many variants the operator MUST NOT use under
  D3.

The parser's mutation-killing unit tests live alongside the
formatter's already-present tests at
`crates/kaleidoscope-cli/src/lib.rs:457-651`.

Note this is the INVERSE-direction case of the `stats` feature's
hand-rolled FORMATTER. The PARSER is structurally harder (it must
detect and reject invalid input, not just emit valid output) but
the shape is still small: a fixed-position digit reader, a
`days_from_civil` arithmetic helper (Hinnant inverse), and a
combine-into-`u64`-nanoseconds tail. Estimated parser line count
is comparable to the formatter's ~30-line body.

### D5: Out of scope — `--observe-otlp` interaction

This feature does NOT exercise the composition of `--since` /
`--until` with `--observe-otlp` in its acceptance test. The
`--observe-otlp` flag remains independently usable on `read` (it was
shipped in `cli-read-observe-otlp-v0`); the natural behaviour of
combining all three flags is that the OTLP file receives one
`lumen.query.count` line whose `asInt` equals the bounded match
count under the supplied `TimeRange`. A dedicated composition test
is deferred to a follow-up feature if it ever becomes operationally
warranted — at present, the operator value of the composition is
not large enough to justify the additional test surface and the
two features are independently demonstrable without the cross-test.

Rationale:

- Each flag is independently testable and independently valuable.
- The wiring inside `read()` is structurally orthogonal: the
  `TimeRange` parameter controls which `lumen.query` shape is
  called; the `otlp_log_path` parameter controls which `MetricsRecorder`
  is constructed. They do not interact except through the `asInt`
  field of the emitted line (which is automatically correct because
  the recorder is invoked AFTER the bounded query returns).
- A composition test would couple the two test suites in a way that
  reduces their isolation under the locked-test posture (changes to
  `--observe-otlp` would force re-running this feature's
  acceptance, and vice versa).

### D6: Out of scope — other Lumen query parameters

`lumen::LogStore::query` may accept additional parameters in future
(e.g. severity filter, body substring filter, attribute filter).
This feature ONLY wires the existing `TimeRange` parameter to the
CLI surface. Other query parameters are out of scope and have no
flag presence in this feature.

### D7: A new acceptance test file mirrors `observe_otlp_read_flag.rs`

New file: `crates/kaleidoscope-cli/tests/read_time_range.rs`.
The harness pattern (`tenant`, `record`, `temp_root`, `cleanup`,
`ndjson` helpers) is duplicated inline at v0, mirroring the
rule-of-three deferral from `cli-read-observe-otlp-v0` D6. After
this feature ships the `kaleidoscope-cli` crate has four test files
using the same harness (`observe_otlp_flag.rs`,
`observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, and
this feature's `read_time_range.rs`). The rule-of-three extraction
trigger has been past-due for two features; the extraction itself is
a separate refactoring task and is NOT a deliverable of this DISCUSS
wave — extracting a shared `tests/common/mod.rs` module is a
DESIGN/DELIVER concern and may be done in a follow-up fix-forward
commit rather than inside this feature's slice.

### D8: Out of scope — time format variants other than ISO 8601 with `Z` suffix

Restated for emphasis (also covered in D3): no Unix timestamp input
(integer-seconds or integer-nanoseconds shapes are rejected), no
RFC 2822 form, no `YYYY-MM-DD` (date-only) form, no `T0930Z`
(compact) form. The parser accepts exactly the two shapes documented
in D3 and rejects everything else with the OK4 fail-fast contract.

### D9: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as `cli-read-observe-otlp-v0` D7. The SSOT
operator-incident-response journey is incident-time focused; this
feature serves the orthogonal "operator narrows a tenant dump to a
time window" journey, which directly supports incident response but
is a per-CLI-flag affordance rather than a journey-shape change.
The feature-local artefacts produced in this wave (user-stories.md,
story-map.md, outcome-kpis.md, slice brief, wave-decisions.md,
dor-validation.md) are NOT promoted to `docs/product/journeys/` or
`docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate),
2 modified files (`src/lib.rs`, `src/main.rs`), 1 new test file
(`tests/read_time_range.rs`), 1 manifest line-level change
(`Cargo.toml`). Estimated effort: <= 1 day. PASSES the right-sized
gate. Comparable in size to `cli-read-observe-otlp-v0` because the
parser-side work roughly balances the absence of cross-writer
concurrency work.

## Handoff

Next wave: DESIGN (nw-solution-architect). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-read-time-range-filter.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions:

1. Confirm the exact signature shape for the new `TimeRange`-driving
   control on `kaleidoscope_cli::read` (likely a new explicit
   `time_range: TimeRange` parameter; the acceptance test cares only
   that the caller can drive any `TimeRange::new(s, e)` into the
   underlying `lumen.query` call).
2. Confirm the placement of the new hand-rolled ISO 8601 UTC parser
   (likely inline in `crates/kaleidoscope-cli/src/main.rs` alongside
   the flag-parse helpers, OR in `src/lib.rs` next to its inverse
   `format_iso8601_utc_nanos`).
3. Confirm the exact behaviour for the no-`.` form of the parser
   input (accept `YYYY-MM-DDTHH:MM:SSZ` as exactly 0 nanoseconds-of-
   second; accept 1..=9 fractional-second digits with left-padding
   to 9).
