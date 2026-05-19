# Wave Decisions — `cli-read-time-range-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE. The DISCUSS artefacts collapse most of the design
space: D1-D9 of `discuss/wave-decisions.md` lock the half-open
`[since, until)` semantics (D2, inherited from `lumen::TimeRange`),
the ISO 8601 UTC `Z`-only input shape (D3), the hand-rolled parser
posture (D4), and the new acceptance test file
`crates/kaleidoscope-cli/tests/read_time_range.rs` (D7). DESIGN's
load-bearing job is to lock the exact `read()` signature evolution,
to decide where the parser lives (library, binary, or both), to lock
the parser scope (fractional-digit range, calendar validation), and
to discharge the Reuse Analysis hard gate. No new ADR (the parser is
the symmetric inverse of the already-shipped formatter — same
architectural decision class as DD1 of `cli-stats-subcommand-v0`).

Scope inherited from DISCUSS (locked, not re-litigated): both flags
optional with half-bounded support (D1); half-open `[since, until)`
matching `lumen::TimeRange::contains` (D2); ISO 8601 UTC with `Z`
suffix only, two accepted shapes
`YYYY-MM-DDTHH:MM:SSZ` and `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` (D3);
hand-rolled parser, no `chrono`/`time`/`jiff` (D4); composition with
`--observe-otlp` out of scope for this feature's acceptance test
(D5); other `LogStore::query` parameters out of scope (D6); new test
file `tests/read_time_range.rs` with the harness duplicated inline
(D7); SSOT journey and `jobs.yaml` not modified (D9).

Confirmed environment state (Earned Trust probe — Principle 12): a
workspace grep for `use chrono|use jiff|use time::` across all
`*.rs` returned zero matches; a workspace grep for `chrono`, `jiff`,
or a `time =` dependency line across all `Cargo.toml` returned zero
matches. The "no time-crate" posture inherited from
`cli-stats-subcommand-v0` is still in force at the commit this
DESIGN wave is written against.

---

## DD1: `read()` signature evolution — Option C, explicit `range: TimeRange` as 5th parameter

**Decision**: extend `kaleidoscope_cli::read` from its current 4-arg
shape

```rust
pub fn read(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<usize, Error>
```

to a 5-arg shape

```rust
pub fn read(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
    range: TimeRange,
) -> Result<usize, Error>
```

with `range` placed AFTER `otlp_log_path` so the new parameter sits
at the end of the argument list (least-disruptive append). The
parameter is the explicit `lumen::TimeRange` value type already
re-exported into `kaleidoscope_cli`'s `use` list at
`crates/kaleidoscope-cli/src/lib.rs:60-63`. The call-site change
inside `read()` is a single token swap:
`lumen.query(tenant, TimeRange::all())` becomes
`lumen.query(tenant, range)`.

Every in-tree caller of `read()` is updated to pass an explicit
`TimeRange`. The no-flag CLI default is `TimeRange::all()`, mirroring
`crates/lumen/src/record.rs:111-114`. The byte-equivalent OK2 contract
holds because `TimeRange::all()` is structurally what today's
`read()` body constructs at the call site
(`crates/kaleidoscope-cli/src/lib.rs:284`).

**Internal call sites that must be updated** (verified by grep):

| Site | File | New argument |
|---|---|---|
| `main.rs::run_read_with` | `crates/kaleidoscope-cli/src/main.rs:162` | `time_range` parsed from `--since` / `--until` (DD3), defaulting to `TimeRange::all()` |
| Inline test `run_read_with_writes_records_to_stdout_and_summary_to_stderr` | `crates/kaleidoscope-cli/src/main.rs:283` | n/a — calls through `run_read_with`, not `read` directly |
| `tests/observe_otlp_read_flag.rs` and `tests/observe_otlp_flag.rs` | locked OK2-protection tests | n/a — they call through the binary subprocess, not the library directly. Their argv lists do NOT mention `--since` / `--until`, so they hit the no-flag default and remain byte-equivalent. |
| `tests/ingest_and_read_roundtrip.rs` (if it calls `read` directly) | `crates/kaleidoscope-cli/tests/` | adds `TimeRange::all()` as the fifth argument |

**Rationale**:

1. **Option C over Option A (`range: TimeRange` 5th arg, breaks
   callers)**: Option A and Option C have the same external shape;
   the difference is just whether the DECISION is framed as "break
   callers" or "explicitly update callers". Framing it as "update
   callers" is honest. Every in-tree caller is updated as part of
   this feature's slice. The two locked test files
   (`observe_otlp_read_flag.rs`, `observe_otlp_flag.rs`) call the
   binary as a subprocess, not the library function; they need no
   edits.

2. **Option C over Option B (`range: Option<TimeRange>` with None →
   all())**: `Option<TimeRange>` introduces a second null-state
   semantic on top of `TimeRange::all()` itself. The library port
   already has a "give me everything" sentinel (`TimeRange::all()`);
   adding `None` as a second one violates "make illegal states
   unrepresentable". Callers should construct the `TimeRange` they
   want; `TimeRange::all()` is the no-flag idiom and is one line at
   the call site. **Rejected.**

3. **Option C over Option D (new `read_with_range(...)` parallel
   to `read()`)**: this is the shape `cli-stats-cinder-tier-
   distribution-v0` DD1 chose (parallel `stats_with_tiers` sibling).
   That precedent applied because the locked test file for `stats`
   called `kaleidoscope_cli::stats` by name (so the rename would
   have been a breaking change). For `read()`, neither of the locked
   test files imports the library function by name — they invoke
   the binary via a subprocess. So the structural force that made
   Option A (parallel sibling) the right answer for `stats` does
   not apply here. A parallel `read_with_range` would leave an
   unreachable `read()` dead-end on the public surface (no caller
   would ever use it). **Rejected.**

4. **Parameter ordering — `range` last, not interleaved.** Placing
   the new parameter at the end is the standard "additive change"
   shape. Interleaving `range` before `otlp_log_path` would
   permute the existing-caller argv order and is gratuitous
   churn.

5. **Public API surface**: this is an additive public-surface
   change on `kaleidoscope_cli`. Same shape as the precedent
   `otlp_log_path` addition shipped in `cli-read-observe-otlp-v0`
   (which itself extended `read()` from a 3-arg to a 4-arg shape).
   No new ADR; the precedent set by that feature governs this one.

**Rejected alternative — builder pattern (`ReadOptions::new().with_range(..).with_otlp(..)`)**:
two optional knobs do not earn a builder. Two positional parameters
are clearer at the call site (`read(&tenant, &dir, sink, otlp.as_deref(), range)`)
than a builder chain. **Rejected.**

**Rejected alternative — caller pre-constructs `lumen::FileBackedLogStore`
and calls `query` directly**: would push every `read()` caller into
opening the Lumen store, wiring the recorder, and invoking the
query — the entire purpose of the `read()` library function is to
encapsulate that wiring. **Rejected.**

---

## DD2: ISO 8601 parser placement — Option C, library core + binary-side wrapper

**Decision**: the parser lives in TWO places, each with a different
responsibility.

1. **In `crates/kaleidoscope-cli/src/lib.rs`**, a new private free
   function next to its inverse `format_iso8601_utc_nanos`:

   ```rust
   fn parse_iso8601_utc_nanos(s: &str) -> Result<u64, IsoParseError>;
   ```

   The library function knows about nanos. It does NOT know which
   CLI flag the value came from; it does NOT format the error
   message that the operator sees on stderr. It returns a typed
   `IsoParseError` value the caller wraps.

2. **In `crates/kaleidoscope-cli/src/main.rs`**, a new private
   helper next to `parse_observe_otlp`:

   ```rust
   fn parse_time_range(args: &[String])
       -> Result<TimeRange, Box<dyn std::error::Error>>;
   ```

   This helper does a single-pass argv scan (mirroring
   `parse_observe_otlp` at `crates/kaleidoscope-cli/src/main.rs:130-144`),
   pulls the `--since` and `--until` values if present, calls
   `parse_iso8601_utc_nanos` on each, and on failure constructs the
   stderr error message that names which flag carried the bad value
   AND the verbatim bad value. On success it constructs the
   `TimeRange` (using `0` for absent `--since`, `u64::MAX` for absent
   `--until`).

The new `IsoParseError` type is a typed sum (variant per failure
mode: bad length, bad separator at position N, non-digit in digit
slot, out-of-range field). It does NOT embed the flag name. The
binary-side wrapper adds the flag-name context to the stderr
message.

**Rationale**:

1. **Single responsibility — the library knows nanos, the binary
   knows flag names.** This is the same split shape the project
   already uses for `parse_observe_otlp` (binary-only, knows the
   flag name) vs the `Option<&Path>` parameter on `read()` /
   `ingest()` (library, knows the path but not the flag). The
   parser-direction symmetry is the SAME split: typed
   parser-in-lib + flag-aware-wrapper-in-bin.

2. **Round-trip property pins the placement.** AC says
   `parse(format(ns)) == ns` for every `ns`. The formatter lives in
   `lib.rs` (line 410). Putting the parser next to it makes the
   round-trip property a single-file local check. The inline
   mutation-killing unit tests for the parser sit in the same
   `#[cfg(test)] mod tests` block at the bottom of `lib.rs`
   (already at lines 457-651 for the formatter); the parser tests
   join that block.

3. **The binary-side wrapper is small.** Two `--since` / `--until`
   argv scans (DRY-fied into one helper); two calls to the typed
   library parser; two error-message constructions naming each
   flag. The wrapper is also where the fail-fast invariant lives:
   it runs BEFORE `read()` is called, so a typo never opens the
   Lumen store (DISCUSS handoff item 5).

4. **The typed `IsoParseError` enables future contextless callers.**
   If a future feature ever needs the parser outside the CLI
   (e.g. a library test, a future config-file loader), it gets
   typed errors without an `--since`-shaped message stamped on
   them.

**Rejected alternative — Option A (library only, parser takes flag
name)**: would force the library to know about CLI flag names. The
`lumen` crate has no flag-name vocabulary; pushing flag names into
`kaleidoscope_cli`'s library layer ties the library to one binary's
flag shape. If a future tool ever wanted to parse the same ISO
8601 timestamp under a differently-named flag, it would inherit
the wrong message. **Rejected.**

**Rejected alternative — Option B (binary only, parser inline in
`main.rs`)**: would put the parser body and its mutation-killing
unit tests in `main.rs` instead of next to its inverse formatter
in `lib.rs`. Two cohesive pieces of arithmetic (the formatter and
its inverse parser) would be split across two files, complicating
the round-trip property AC and the `#[cfg(test)]` test placement.
**Rejected.**

---

## DD3: Parser scope — accept 0..=9 fractional digits, reject all else

**Decision**: `parse_iso8601_utc_nanos(s)` accepts exactly two
shapes, fail-fast on any deviation:

| Shape | Fractional digits | Note |
|---|---|---|
| `YYYY-MM-DDTHH:MM:SSZ` | 0 (no `.` at all) | Round-trips to/from `format_iso8601_utc_nanos` ONLY when the formatter would emit `.000000000` — for human-typed input where the operator omits subsecond precision. |
| `YYYY-MM-DDTHH:MM:SS.DDDDDDDDDZ` | 1..=9 (`.` followed by 1..=9 ASCII digits) | Round-trip with the formatter requires exactly 9 digits. For values with fewer than 9 digits (1..=8), the parser left-pads to 9 internally before the nanosecond multiplication (e.g. `.123Z` → `.123000000Z` → `123_000_000` nanos-of-second). |

**Validation contract** (all must hold or the parser returns
`Err(IsoParseError::...)`):

- Total length is `20` (no `.`) or `22..=30` (with `.` and 1..=9
  digits).
- Bytes at positions `4, 7` are `-`; byte at position `10` is `T`;
  bytes at positions `13, 16` are `:`; the trailing byte is `Z`
  (capital, ASCII).
- All other bytes in the date and time slots are ASCII digits
  `0..=9`.
- If a `.` is present, it sits at position `19` and is followed by
  1..=9 ASCII digits before the `Z`.
- Parsed field ranges: `month ∈ [1, 12]`; `day ∈ [1, days_in_month(year, month)]`
  (so `2026-02-29` rejects because 2026 is not a leap year, but
  `2024-02-29` accepts); `hour ∈ [0, 23]`; `minute ∈ [0, 59]`;
  `second ∈ [0, 59]` (no leap-second support — symmetric with the
  formatter, which does not encode them either).
- `year ∈ [1970, 9999]` (lower bound: u64 nanoseconds since the
  Unix epoch cannot represent pre-1970 timestamps without
  wraparound, and `lumen::LogRecord::observed_time_unix_nano`
  is a `u64` by definition; upper bound: the formatter pad is
  `{year:04}` so any year requiring 5+ digits round-trips to a
  different shape than it parsed from).

  > **Correction landed during peer review (Atlas HIGH item 2)**:
  > the year range was initially `[0000, 9999]`. Atlas correctly
  > noted that years < 1970 would produce a negative `days` value
  > whose `as u64` cast silently wraps to a huge positive number,
  > breaking the round-trip property and producing wildly incorrect
  > nanosecond values. Since the storage layer's
  > `observed_time_unix_nano` is `u64` (non-negative-by-definition
  > nanoseconds since `UNIX_EPOCH`), pre-1970 timestamps have no
  > sensible representation. The parser therefore rejects them
  > with the same OK4 fail-fast contract.

**Days-from-civil arithmetic**: reuse Howard Hinnant's inverse
algorithm `days_from_civil(y, m, d) -> i64`.

  > **Provenance and licence (Atlas HIGH item 1)**: published by
  > Howard Hinnant at
  > <https://howardhinnant.github.io/date_algorithms.html> (in
  > section "days_from_civil"), released under the explicit
  > public-domain statement at the top of that page ("the source
  > code is dedicated to the public domain"). The same algorithm
  > family is used in the existing `civil_from_days` (the inverse,
  > shipped during `cli-stats-subcommand-v0` at commit `75f15a6`,
  > `crates/kaleidoscope-cli/src/lib.rs`). The DELIVER wave's
  > Rust source comment MUST cite this URL and the public-domain
  > statement. `LICENSING.md` gained a `## Third-party algorithms`
  > section as part of this feature's commit, attributing the
  > Hinnant date algorithms by name with the URL and the
  > public-domain dedication; this section will be referenced by
  > any future feature that touches the same algorithm family.

It is the inverse of the already-shipped `civil_from_days` at
`crates/kaleidoscope-cli/src/lib.rs:426-438`. Round-trip
`days_from_civil(civil_from_days(z)) == z` is a property test
candidate.

**Final arithmetic**:

```text
days       = days_from_civil(year, month, day)         // i64
sec_of_day = hour*3600 + minute*60 + second            // u64
total_sec  = days as u64 * 86_400 + sec_of_day         // u64
nanos      = total_sec * 1_000_000_000 + nanos_of_second  // u64
```

(The `days as u64` cast is safe under the year range
`[1970, 9999]`: for years `>= 1970`, `days` is non-negative — the
minimum is `days_from_civil(1970, 1, 1) == 0`. The year range
check happens BEFORE the cast, so pre-1970 inputs never reach this
arithmetic.)

**Rationale**:

1. **0..=9 fractional-digit support is the exact superset of the
   formatter output AND human input.** The formatter emits 9 digits
   always; the operator typing `2026-05-18T00:00:00Z` expects it to
   parse (no operator hand-counts to 9 zeros). DISCUSS D3 explicitly
   accepts 1..=9 digits with left-padding; the 0-digit case (no `.`)
   is the symmetric round-trip for `format(0) = "1970-01-01T00:00:00.000000000Z"`
   vs the convenience shape `1970-01-01T00:00:00Z`.

2. **Calendar validation rejects malformed dates at the parser
   boundary, not at the storage layer.** `2026-13-32T25:99:99Z`
   would, if accepted, propagate through Hinnant's
   `days_from_civil` and yield SOME nanosecond integer. The result
   would be valid `u64` but operationally nonsense. Rejecting at
   the parser boundary is the OK4 contract; pushing the rejection
   to `TimeRange::contains` would mean an invalid value would be
   accepted by the CLI surface.

3. **Year range `[0000, 9999]` matches the formatter's `{year:04}`
   format-string contract.** Years outside this range would format
   to a different shape than they parse from, breaking the
   round-trip property AC.

4. **Leap-year handling is delegated to `days_from_civil`.** The
   Hinnant algorithm already encodes the Gregorian leap-year rule
   (divisible by 4 and not by 100, or divisible by 400). A separate
   `days_in_month` helper covers the day-range check for parser
   validation; the helper consults the same leap-year predicate as
   Hinnant.

**Rejected alternative — accept `+00:00` and `+0000` offset
forms in addition to `Z`**: DISCUSS D3 explicitly defers this to a
future extension. Forward-compatible because any currently-accepted
shape remains accepted under an extended parser. **Rejected for
v0.**

**Rejected alternative — accept lowercase `z` as the timezone
suffix**: the formatter emits uppercase `Z`; round-trip with the
formatter requires uppercase. Accepting lowercase would be a
silent-symmetry break. **Rejected.**

**Rejected alternative — accept missing `Z` (assume UTC)**: would
silently re-interpret operator-typed local-time strings as UTC,
which is exactly the silent-failure class DISCUSS D3 was written
to prevent. **Rejected.**

**Rejected alternative — accept Unix epoch integer-seconds or
integer-nanoseconds**: DISCUSS D8 explicitly rejects this. **Out
of scope.**

---

## DD4: Reuse Analysis (RCA F-1 hard gate)

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `format_iso8601_utc_nanos` formatter | `crates/kaleidoscope-cli/src/lib.rs:410-420` | **REUSE (as the symmetric inverse oracle).** | The parser inverts the formatter; the round-trip property AC makes the formatter the parser's primary correctness oracle. No formatter change. |
| `civil_from_days` Hinnant helper | `crates/kaleidoscope-cli/src/lib.rs:426-438` | **REUSE (and add inverse `days_from_civil`).** | The parser needs the inverse direction. Both helpers are public-domain Hinnant. The pair is the canonical idiom and they cohabit. |
| `lumen::TimeRange` value type | `crates/lumen/src/record.rs:97-120` | **REUSE.** | Already imported into `kaleidoscope_cli`'s use list at `crates/kaleidoscope-cli/src/lib.rs:60-63`. The new `range` parameter on `read()` uses this exact type. **DO NOT modify** the lumen crate. |
| `TimeRange::all()` sentinel | `crates/lumen/src/record.rs:111-114` | **REUSE.** | The no-flag CLI default constructs `TimeRange::all()`. Existing call site at `crates/kaleidoscope-cli/src/lib.rs:284` becomes `lumen.query(tenant, range)` where the caller passes `TimeRange::all()` when no flags supplied. |
| `parse_observe_otlp` argv-scan idiom | `crates/kaleidoscope-cli/src/main.rs:130-144` | **REUSE THE SHAPE (not the function).** | The new `parse_time_range` helper mirrors the single-pass argv scan with `.iter().skip(2)` and `arg == "--since"` / `arg == "--until"` branches. Order-independent. |
| `Error::Io` and `From<std::io::Error> for Error` | `crates/kaleidoscope-cli/src/lib.rs:78, 104-108` | **REUSE.** | The library `read()` body is unchanged on the I/O side. The new parser is a pure-string function; it does not perform I/O. |
| `kaleidoscope_cli::read` 4-arg signature | `crates/kaleidoscope-cli/src/lib.rs:261-294` | **EXTEND.** | One new parameter (`range: TimeRange`); one one-token call-site swap (`TimeRange::all()` → `range`). No body restructuring. |
| `run_read_with` dispatcher body | `crates/kaleidoscope-cli/src/main.rs:155-165` | **EXTEND.** | One new line that calls `parse_time_range(args)?` and threads the result into `read(..)`. |
| `print_usage` text in `write_usage` | `crates/kaleidoscope-cli/src/main.rs:79-109` | **EXTEND.** | The `read` subcommand line gains `[--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>]`. The half-open `[since, until)` contract is mentioned. |
| Existing test files `observe_otlp_read_flag.rs` and `observe_otlp_flag.rs` | `crates/kaleidoscope-cli/tests/` | **DO NOT MODIFY** (OK2 locked). | They invoke the binary via subprocess without `--since` / `--until`; they hit the no-flag default which is `TimeRange::all()`; byte-equivalent. |
| Existing test harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`) | `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` | **DUPLICATE INLINE** at v0 per DISCUSS D7. | Fourth test file in the cluster using the same harness shape; rule-of-three extraction is a separate refactoring task and is NOT a deliverable of this feature. |
| `chrono`, `time`, `jiff` external crates | n/a — not in workspace | **DO NOT INTRODUCE.** | Verified by grep at design time (0 matches across all `Cargo.toml` and `*.rs`). Inherits the no-time-crate posture from `cli-stats-subcommand-v0` DD1. |
| New `IsoParseError` type | n/a | **CREATE NEW** | Private to `lib.rs`. Typed sum (bad length / bad separator / non-digit / out-of-range). Carries position and field info; does NOT carry flag-name info (DD2). |
| New `parse_iso8601_utc_nanos` library function | n/a | **CREATE NEW** | Inverse of the existing formatter. Lives next to `format_iso8601_utc_nanos`. Private. |
| New `days_from_civil` library helper | n/a | **CREATE NEW** | Inverse of the existing `civil_from_days`. Lives next to it. Private. Public-domain Hinnant. |
| New `parse_time_range` binary helper | n/a | **CREATE NEW** | Single-pass argv scan in `main.rs`, parallel to `parse_observe_otlp`. Private. |
| New optional 5th parameter on `read()` | n/a | **CREATE NEW** | Additive public-surface change on `kaleidoscope_cli`. Same shape as the precedent `otlp_log_path` addition in `cli-read-observe-otlp-v0`. |

**Verdict**: **EXTEND** (`read()`'s signature; `run_read_with`'s
body; `write_usage`'s text) + **REUSE** (eight existing constructs:
the formatter, `civil_from_days`, `TimeRange`, `TimeRange::all()`,
the `parse_observe_otlp` argv-scan shape, the `Error::Io` / `From`
pair, the existing `read()` body, the locked test files) +
**CREATE NEW** (one private typed error, one private library
parser function, one private library helper `days_from_civil`,
one private binary `parse_time_range` helper, one new optional
parameter on the public `read()` signature). **No new public type,
no new trait, no new module, no new external dependency.**

---

## DD5: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following
DISCUSS decisions:

1. **Half-open `[since, until)` semantics** (D2). Confirmed: the
   CLI surface passes the values straight into `TimeRange::new(s, e)`
   and inherits the contract from `crates/lumen/src/record.rs:116-119`.
2. **ISO 8601 UTC with `Z` suffix only** (D3, D8). Confirmed: DD3
   pins the parser's accepted alphabet and rejects all other
   shapes.
3. **Hand-rolled parser, no `chrono`/`time`/`jiff`** (D4). Confirmed
   by DD3 and DD4. Workspace grep at design time confirmed zero
   matches for these crates anywhere.
4. **`--observe-otlp` composition out of scope for this feature's
   acceptance test** (D5). Confirmed: `parse_time_range` and
   `parse_observe_otlp` are independent helpers; the binary
   composes them in `run_read_with` but the acceptance test for
   this feature does not exercise the cross-flag path.
5. **Other Lumen query parameters (severity, body substring,
   attributes) are out of scope** (D6). Confirmed: only the
   existing `TimeRange` parameter is wired through.
6. **Test harness rule-of-three extraction is deferred** (D7).
   Confirmed: the new `tests/read_time_range.rs` duplicates the
   harness inline; extraction to `tests/common/mod.rs` is a
   separate refactoring task.
7. **No SSOT journey or `jobs.yaml` modification** (D9). Confirmed.
8. **No new ADR**. The parser is the symmetric inverse of the
   already-shipped formatter (same architectural decision class as
   `cli-stats-subcommand-v0` DD1, which introduced the formatter
   without an ADR). The 5th-parameter addition on `read()` follows
   the same shape as the 4th-parameter addition shipped in
   `cli-read-observe-otlp-v0`. ADR-0001 (`kaleidoscope-cli` public
   API surface) absorbs both as additive parameter / inverse-helper
   changes that do NOT introduce new public types, traits, or
   modules.

---

## DEVOPS handoff annotation

Recipient: `nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1 + L2;
    L3 explicitly skipped with reification conditions documented).
  - The new subsection appended to
    `docs/product/architecture/brief.md > ## Application Architecture
    — cli-read-time-range-v0`.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-read-time-range-v0/discuss/` (locked, not
    modified).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1 (principal —
    bounded-window filter), OK2 (no-flag byte equivalence), OK3
    (half-bounded support), OK4 (invalid-input fail-fast).

- **Development paradigm for DELIVER**: Rust idiomatic per
  `CLAUDE.md`. Data + free functions + traits where genuinely
  needed. The new `parse_iso8601_utc_nanos` is a free function. The
  new `days_from_civil` is a free function. The new
  `parse_time_range` is a free function. The new `IsoParseError` is
  a typed enum sum. **No new trait. No new `dyn` boundary. No new
  module.** No new `Box<dyn ...>` indirection.

- **External integrations**: **none**. Pure-string parser plus an
  additive parameter on a local function. No HTTP client, no
  webhook, no third-party API, no vendor SDK, no subprocess, no
  network I/O. **No contract-test recommendation applies.**

- **External dependency footprint**: **no new external crate**.
  All used types (`lumen::TimeRange`, `aegis::TenantId`, std I/O
  traits) are already in `kaleidoscope-cli`'s use list at
  `crates/kaleidoscope-cli/src/lib.rs:55-65`. `Cargo.lock` churn
  is zero beyond what a recompile produces.

- **CI gates** (ADR-0005): the five existing workspace gates apply
  unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test read_time_range`
  exits 0 as the OK1/OK2/OK3/OK4 acceptance probe under Gate 1
  (`cargo test --workspace`). The locked
  `cargo test --package kaleidoscope-cli --test observe_otlp_read_flag`
  and `--test observe_otlp_flag` continue to pass green and serve
  as the byte-level oracles for OK2. **No new gate is added.**

  Specifically on **Gate 5 (mutation testing)**: the existing
  `gate-5-mutants-kaleidoscope-cli` job is path-filtered on
  `crates/kaleidoscope-cli/**` via `--in-diff`. Any commit
  touching `crates/kaleidoscope-cli/src/lib.rs` or
  `crates/kaleidoscope-cli/src/main.rs` (this feature touches
  both) is automatically mutated by the existing job. The new
  parser body's branches (length check, separator check, digit
  check, field-range check, `days_from_civil` arithmetic,
  fractional-digit left-pad, `parse_time_range` argv scan,
  half-bounded `0`/`u64::MAX` defaults) fall inside the same
  mutation surface. The parser is **mutation-rich** (more
  arithmetic and branch logic than the formatter); inline unit
  tests in the `#[cfg(test)] mod tests` block in `lib.rs` MUST be
  authored to discharge the 100% kill rate, in the same style as
  the existing formatter tests at
  `crates/kaleidoscope-cli/src/lib.rs:457-651`. **No new Gate 5
  job needed.**

- **Workspace changes**: no `Cargo.toml` additions at the workspace
  root. `crates/kaleidoscope-cli/Cargo.toml` gains exactly one
  new `[[test]]` block:

  ```toml
  [[test]]
  name = "read_time_range"
  path = "tests/read_time_range.rs"
  ```

  No new `[dependencies]` line; no new `[dev-dependencies]` line.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate 5):
  scoped to `crates/kaleidoscope-cli/src/lib.rs` and
  `crates/kaleidoscope-cli/src/main.rs`. Run after the DELIVER
  refactor pass. 100% kill rate. The changed code surface is
  small-medium (parser ~50 source lines, `days_from_civil` ~15
  lines, `parse_time_range` ~25 lines, `read()` signature delta ~2
  lines, `write_usage` text delta ~3 lines). Mutation-testing
  budget should be moderate (the parser is the rich surface) and
  well under the 30-minute timeout in the existing job.

- **Architectural-rule enforcement tooling** (Principle 11): no
  new tooling is recommended for this feature. The existing
  five-gate workspace contract already enforces every rule this
  feature touches. The "no `chrono`/`time`/`jiff` dependency"
  property is enforced structurally: the existing Cargo manifest
  has no such dependency line, and adding one would surface as an
  unjustified diff in PR review plus a `Cargo.lock` churn beyond
  the expected recompile.

- **Earned Trust posture** (Principle 12): the parser is a pure
  function over a `&str` input — no external substrate, no
  vendor SDK, no filesystem. Earned Trust applies to the parser's
  round-trip property (`parse(format(ns)) == ns` for every `ns`)
  rather than to environmental lies. The mutation-killing unit
  test block at `crates/kaleidoscope-cli/src/lib.rs:457-651`
  already encodes this discipline for the formatter direction;
  the parser direction inherits the same test-shape contract.
  The round-trip property test (DISCUSS AC item 6) is the probe.

### Why no ADR change

The new ISO 8601 UTC parser is the **symmetric inverse** of the
already-shipped `format_iso8601_utc_nanos` formatter (DD1 of
`cli-stats-subcommand-v0`). That formatter shipped without an ADR;
its inverse counterpart inherits that decision class. The new
`days_from_civil` helper is the inverse of the existing
`civil_from_days` helper, which also shipped without an ADR. The
5th-parameter addition on `read()` follows the same shape as the
4th-parameter addition shipped in `cli-read-observe-otlp-v0`,
which also shipped without an ADR (additive parameter on an
existing public function, no new public type or trait).

If a future feature ever extends the parser to accept timezone
offsets other than `Z` (DD3 forward-compatibility hook), OR
introduces a `ReadOptions` builder (DD1 rejected alternative),
THAT feature would warrant a new ADR. This feature does not.
