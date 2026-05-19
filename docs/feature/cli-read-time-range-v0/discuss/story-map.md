# Story Map: `cli-read-time-range-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli read acme /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`,
stdout receives ONLY the records whose `observed_time_unix_nano` lies
in the half-open interval `[1_747_526_400_000_000_000, 1_747_612_800_000_000_000)`
— yesterday's slice for tenant `acme`, computed by the storage layer's
`TimeRange` query at `crates/lumen/src/record.rs:97-120`, not by
client-side `jq` filtering of the full tenant dump. Half-bounded forms
(`--since X` alone or `--until Y` alone) work. Omitting both flags is
byte-equivalent to today (the existing
`crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` test still
passes). A typo on either flag exits non-zero with the offending flag
named in stderr.

## Backbone

The journey has exactly one activity: the operator filters a tenant's
records by an ISO 8601 time window via two new CLI flags. The activity
is a thin end-to-end slice: a single
`kaleidoscope-cli read <tenant> <data_dir> [--since X] [--until Y]`
invocation produces a bounded slice of records on stdout. The
underlying `lumen::TimeRange` data type, the storage-layer
`LogStore::query(tenant, TimeRange)` entry point, the binary's
positional argument parsing, the NDJSON-on-stdout serialisation
contract, and the precedent for adding optional flags after the
positional arguments (the prior `--observe-otlp <path>` flag at
`crates/kaleidoscope-cli/src/main.rs:130-144`) all already exist; this
feature is the wire that lets the operator drive a non-`all()`
`TimeRange` from the CLI surface and the parser that converts ISO 8601
text into the `u64` nanoseconds the storage layer wants.

| Activity 1: operator filters a tenant's records by ISO 8601 time window |
|---|
| CLI's `read` path constructs `TimeRange::new(since_ns, until_ns)` from the operator-supplied `--since` and `--until` flag values (instead of always calling `TimeRange::all()`). Stdout contains only the records in the half-open interval. The no-flag default is `TimeRange::all()` (byte-equivalent to today). Invalid ISO 8601 input on either flag fails fast with the offending flag name in stderr. |

## Walking Skeleton

Per `wave-decisions.md` D? (no explicit decision needed), the
walking-skeleton concept is N/A: the CLI already exists, the `read`
subcommand already exists, the `lumen::TimeRange` data type already
exists with the correct `[start, end)` semantics
(`crates/lumen/src/record.rs:97-120`), the `LogStore::query(tenant,
TimeRange)` entry point already exists, the binary's argument parser
already knows how to parse `--observe-otlp <path>` (the optional-flag
precedent at `crates/kaleidoscope-cli/src/main.rs:130-144`), and the
project already hand-rolls an ISO 8601 UTC formatter
(`crates/kaleidoscope-cli/src/lib.rs:410-420`). This feature extends
one subcommand with two optional flags and a hand-rolled parser that
is the inverse of the existing formatter.

Equivalent statement: **the smallest valuable change is to expose
`TimeRange` to `kaleidoscope_cli::read`'s caller (with
`TimeRange::all()` as the default), add `--since <value>` and
`--until <value>` flag parsing helpers to the binary's `run_read`
dispatcher mirroring the order-independent posture of
`parse_observe_otlp`, and write a hand-rolled ISO 8601 UTC parser
that is the inverse of `format_iso8601_utc_nanos`.** Slice 01 ships
exactly that.

## Release Slices

### Slice 01 — `read` filters by `--since` / `--until` time window

- **Outcome**: An operator running `kaleidoscope-cli read acme
  /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`
  sees ONLY the records whose `observed_time_unix_nano` lies in
  yesterday's half-open window (OK1). Half-bounded forms work (OK3).
  Omitting both flags is byte-equivalent to today (OK2). Typos fail
  fast (OK4).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that the hand-
  rolled ISO 8601 parser is harder than the hand-rolled formatter the
  project already ships. The formatter renders a fixed shape
  (`YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ`); the parser accepts that shape
  plus the no-fractional-digits variant (`YYYY-MM-DDTHH:MM:SSZ`).
  No timezone variants other than `Z` are accepted (D5 in
  `wave-decisions.md`). If the assumption holds (highly likely given
  the parser's input alphabet is `[0-9TZ:.-]`), the OK1 acceptance
  test passes green with a parser of approximately the same line
  count as the formatter. If it fails, the failure mode tells DESIGN
  whether the parser's surface needs to accept additional variants
  (e.g. `+00:00` instead of `Z`), in which case the System Constraint
  in `user-stories.md` is the place to amend.
- **Production-data-equivalent AC**: an end-to-end test invokes
  `kaleidoscope_cli::read` (the actual library function the binary
  calls) with a `TimeRange::new(s, e)` other than `TimeRange::all()`
  against a Lumen store pre-populated with witness records at known
  `observed_time_unix_nano` boundary values; asserts the stdout
  contents exactly match the expected half-open subset. This is the
  same data path the operator's `kaleidoscope-cli read acme /tmp/data
  --since X --until Y` invocation exercises.
- **Dogfood moment**: After the slice ships, Andrea opens a terminal,
  runs:

  ```text
  cargo run --bin kaleidoscope-cli -- ingest acme /tmp/kdata < some_records.ndjson
  cargo run --bin kaleidoscope-cli -- read acme /tmp/kdata \
    --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z \
    | jq -s 'length'
  ```

  and observes that the line count of the bounded read is strictly
  less than the line count of the unbounded read
  (`cargo run --bin kaleidoscope-cli -- read acme /tmp/kdata | jq -s 'length'`).
  Then `cargo run --bin kaleidoscope-cli -- read acme /tmp/kdata --since yesterday`
  exits non-zero with `kaleidoscope-cli: invalid --since value "yesterday"`
  on stderr. The two demonstrations together are the dogfood gate for
  the slice.
- **Effort**: well under 1 day. The change inside
  `kaleidoscope_cli::read` is one new parameter (or builder field, at
  DESIGN's choice) defaulted to `TimeRange::all()`; the change inside
  `crates/kaleidoscope-cli/src/main.rs` is two new
  `parse_flag_value(args, "--since")` / `parse_flag_value(args,
  "--until")` helpers mirroring `parse_observe_otlp` plus the
  ISO 8601 UTC parser invocation on each value; the new acceptance
  test mirrors the existing `observe_otlp_read_flag.rs` harness. No
  cross-writer concurrency probe, no second writer, no second
  subcommand to coordinate — strictly thinner than the predecessor
  feature `cli-read-observe-otlp-v0`.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the fifth consecutive small feature in the
`kaleidoscope-cli` / `self-observe` cluster, after
`cinder-to-pulse-bridge-v0`, `cinder-to-otlp-json-bridge-v0`,
`cli-cinder-otlp-wiring-v0`, and `cli-read-observe-otlp-v0`, and
comparable in size to the predecessor because the parser-side work
roughly balances the absence of cross-writer concurrency work) means
there is no benefit from further splitting:

- Slice 01 carries the wiring change (one new TimeRange-equivalent
  control on `read`, two new flag-parse helpers in `main.rs`, one
  new hand-rolled ISO 8601 UTC parser, one new `print_usage` block),
  the OK1 bounded-window test, the OK2 no-flag test, the OK3
  half-bounded tests (two: one for `--since`-only, one for
  `--until`-only), and the OK4 invalid-input tests (two: one for
  `--since`, one for `--until`) all together. Splitting any one of
  the four KPIs into a separate slice would force a second PR for
  trivially the same wiring — net negative for the reviewer.
- The principal KPI (OK1) is the bounded-window filter; OK2 is its
  no-flag-quiescence guardrail; OK3 is the half-bounded shape that
  matches common operator queries; OK4 is the failure-mode contract
  for typos. Shipping any without the others is meaningless: OK1
  alone gives the operator the bounded form but leaves the no-flag
  path unverified (regression risk); OK2 alone is "we did nothing
  useful, but we didn't break anything"; OK3 alone is structurally
  redundant without OK1's bounded form because the bounded form is
  the principal value; OK4 alone is structurally impossible without
  OK1 because OK4 asserts the failure mode of the parser that OK1
  exercises.

If schedule pressure ever forces a partial ship, the slice is
already as thin as it can be: the wiring change is one new control
on `read`, two new flag-parse helpers, one hand-rolled parser. The
hand-rolled parser is the largest chunk; an alternative (pull a
small ISO 8601 dep like `time` with default features disabled) was
considered and rejected in `wave-decisions.md` D5 because the
parser's input alphabet is narrow enough that hand-rolling is
strictly thinner than the dependency.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture of
`cli-read-observe-otlp-v0/discuss/story-map.md` and inherits its
persona (Priya), CLI surface (`kaleidoscope-cli read <tenant>
<data_dir> [flags]`), and stdout NDJSON contract. The principal
contractual difference is that this feature changes the SHAPE of the
query (it's the input-side, not the side-channel emission), so:

- there is no `--observe-otlp` interaction in this feature's AC
  (composition is out of scope per `wave-decisions.md` D6);
- there is no cross-writer or cross-subcommand probe (the change is
  on `read` only and affects one query call site);
- there IS a new failure mode (invalid ISO 8601 input), which has no
  precedent in the prior `kaleidoscope-cli` features because no
  prior flag accepted free-form parseable input — `--observe-otlp`'s
  value is a filesystem path that the underlying `OpenOptions::open`
  is responsible for validating, whereas `--since` / `--until`
  require explicit parsing before any I/O.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated <= 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the wiring change is
  at one call site in `lib.rs`, two flag-parse helpers + parser in
  `main.rs`, and the new acceptance test lives in one new file).
- 2 modified files (`crates/kaleidoscope-cli/src/lib.rs`,
  `crates/kaleidoscope-cli/src/main.rs`), 1 new file
  (`crates/kaleidoscope-cli/tests/read_time_range.rs`), 1 line-level
  modification (`crates/kaleidoscope-cli/Cargo.toml` for the new
  `[[test]]` entry).
- 1 integration point (the `lumen.query(tenant, TimeRange::new(s,
  e))` call site at `crates/kaleidoscope-cli/src/lib.rs:283-285`).
  The hand-rolled ISO 8601 UTC parser is a second, self-contained
  unit but it is a free function with no integration surface beyond
  its `&str -> Result<u64, ParseError>` shape.
- Estimated effort: <= 1 day for the crafter. The parser is the
  largest chunk and is bounded by the formatter's complexity (the
  formatter is ~30 lines; the parser is comparable). No concurrency
  test to write, no shared-handle ownership puzzle to solve.

The feature is right-sized. No splitting required, no thinning
possible.
