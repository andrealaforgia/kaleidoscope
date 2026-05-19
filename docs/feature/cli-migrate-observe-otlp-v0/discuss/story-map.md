# Story Map: `cli-migrate-observe-otlp-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold --observe-otlp /tmp/audit.ndjson`,
the existing stdout report is unchanged
(`migrated tenant=acme item=acme/batch-00042 from=hot to=cold`) and a
single new NDJSON line appears in `/tmp/audit.ndjson` naming the
tenant, from-tier, and to-tier. Her existing sidecar tails the file and
forwards the line to her existing OTLP/HTTP collector without
configuration change; the dashboard gains a `kaleidoscope.cinder /
cinder.migrate.count` row next refresh. State-mutating operator actions
are no longer fire-and-forget.

## Backbone

The journey has exactly one activity: the operator runs `migrate` with
the optional flag and sees the audit line in her existing OTLP stream.
The activity is a thin end-to-end slice: a single
`kaleidoscope-cli migrate ... --observe-otlp ...` invocation produces
the stdout line + the OTLP line, which a sidecar reads, which a
collector ingests, which a dashboard displays. The thinness is the
point: the prior `cli-migrate-subcommand-v0` already shipped the
subcommand and the library function; the prior
`cinder-to-otlp-json-bridge-v0` already shipped the
`CinderToOtlpJsonWriter::record_migrate` wire contract; the prior
`cli-cinder-otlp-wiring-v0` already shipped the precedent for
constructing the OTLP writer at `FileBackedTieringStore::open` time on
the `ingest` path. This feature is the wire that connects them at one
site inside `kaleidoscope_cli::migrate`.

| Activity 1: operator sees manual migrations in OTLP audit stream |
|---|
| CLI's migrate path constructs `CinderToOtlpJsonWriter` against the operator-supplied file path (instead of `cinder::NoopRecorder`) when `--observe-otlp` is set, leaving stdout and the from→to transition contract untouched. The file gains one new line per successful migrate carrying `tenant_id`, `from`, and `to`. |

## Walking Skeleton

Per `wave-decisions.md` D2, the walking-skeleton concept is N/A: the
CLI already has the `migrate` subcommand (feature
`cli-migrate-subcommand-v0`), the `--observe-otlp` flag parsing
(commit `3af7e82`), and the `CinderToOtlpJsonWriter::record_migrate`
implementation (`cinder-to-otlp-json-bridge-v0`). This feature is a
thin extension that routes one already-shipped writer to a place that
previously held a `NoopRecorder`. There is no UI backbone to span;
there is one construction site to change.

Equivalent statement: **the smallest valuable change is to add
`otlp_log_path: Option<&Path>` to `kaleidoscope_cli::migrate(...)` and
replace the unconditional `Box::new(CinderRecorder)` at
`crates/kaleidoscope-cli/src/lib.rs:434` with a conditional
construction analogous to the `ingest` pattern at lines 155-184.**
Slice 01 ships exactly that.

## Release Slices

### Slice 01 — Manual migrations also land in the `--observe-otlp` file

- **Outcome**: A sidecar reading the operator's
  `--observe-otlp <path>` file sees one `cinder.migrate.count` line per
  CLI-driven successful `migrate()` call, carrying the right
  `tenant_id`, `from`, and `to` attributes; stdout remains
  byte-equivalent to today; the locked
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` test file
  continues to pass with no edits.
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that the
  store-open-time recorder construction site is the right one for the
  `migrate` path too. ADR-0039 §1 fixed the writer's public surface so
  it MUST be constructed before the `cinder.migrate(...)` call; the
  natural seam is `FileBackedTieringStore::open` (mirroring `ingest`).
  If a future refactor wanted to construct the writer at `migrate()`
  call time instead, it would have to hand it through the Cinder API
  surface, which ADR-0039 §1 forbids. This slice's wiring choice is
  therefore the only compile-tractable shape.
- **Production-data-equivalent AC**: an end-to-end test calls
  `kaleidoscope_cli::migrate` (the actual library function the binary
  calls — same entry point) with `otlp_log_path = Some(...)` against a
  real `tempfile::NamedTempFile`-style temp path, seeds one Cinder Hot
  placement for `acme/batch-00042`, runs the call with target tier
  `cold`, and reads back the sink to assert exactly one
  `cinder.migrate.count` line with the expected attributes. This is
  the same data path the operator's
  `kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold
  --observe-otlp /tmp/audit.ndjson` invocation will exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a terminal,
  seeds a Cinder placement via a short ingest, then runs
  `cargo run --bin kaleidoscope-cli -- migrate acme /tmp/kdata
  acme/batch-00042 cold --observe-otlp /tmp/audit.ndjson` in one pane,
  and `cat /tmp/audit.ndjson | jq .` in another. The second pane shows
  the one new line carrying `tenant_id=acme`, `from=hot`, `to=cold`,
  parsed by `jq` without error. The audit-line demo is the dogfood
  gate for the slice.
- **Effort**: well under 1 day. The change inside
  `kaleidoscope_cli::migrate` is structurally a copy of the `ingest`-
  side wiring already at lines 155-184 of
  `crates/kaleidoscope-cli/src/lib.rs` (open file in append mode, wrap
  in writer, box-up as `Box<dyn cinder::MetricsRecorder + Send + Sync>`);
  the new acceptance test mirrors the existing `migrate_subcommand.rs`
  harness with a `--observe-otlp` overlay.

## Priority Rationale

There is one slice and it is the only slice. The reference-class sizing
(this is the fourth small feature in a row in the
`kaleidoscope-cli` / `self-observe` cluster) means there is no benefit
from further splitting:

- Slice 01 carries the wiring change, the happy-path acceptance test
  (OK1), the no-flag scenario (OK2 partial), the unknown-item scenario
  (OK3), and the invalid-tier scenario (OK4) all together. Splitting
  any of these into separate slices would force two PRs for one wiring
  change — net negative for the reviewer.
- The principal KPI (OK1) is the wire-shape per successful migrate;
  OK2 is its no-flag guardrail (and is exercised by the locked
  `migrate_subcommand.rs` test file, unmodified); OK3 and OK4 are the
  error-path emission absence guarantees that pin the two short-
  circuit contracts (pre-flight `get_entry` and `parse_tier`
  respectively) inherited from `cli-migrate-subcommand-v0`. Shipping
  any of the four without the others is meaningless: OK1 alone leaves
  the operator unsure whether error paths pollute the audit stream;
  OK2 by itself is "we did nothing useful, but we didn't break
  anything", which is not a shippable outcome.

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the wiring change is one match-arm
substitution in `crates/kaleidoscope-cli/src/lib.rs:434`. There is no
sub-slice worth shipping in isolation.

## Cross-bridge alignment

This story-map intentionally mirrors the shape of
`docs/feature/cli-cinder-otlp-wiring-v0/discuss/story-map.md`. That
feature wired the OTLP writer onto the `ingest` path (driving
`cinder.place.count` lines); this feature wires the same writer onto
the `migrate` path (driving `cinder.migrate.count` lines). The
`record_migrate` trait method is already exercised in the library
feature's test
`crates/self-observe/tests/cinder_to_otlp_json.rs`; this feature is the
CLI surface for that already-validated wire shape.

The `evaluate` event is not yet exercised at the CLI level because the
CLI does not currently invoke `cinder.evaluate(...)`. A future
`kaleidoscope-cli tier evaluate` or similar subcommand would be a
separate feature with its own KPI and acceptance harness; the present
feature deliberately does NOT extend `--observe-otlp` to `evaluate`.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the wiring change is at
  one match-arm-equivalent and the new acceptance test lives in one
  new file).
- 1 modified file (`crates/kaleidoscope-cli/src/lib.rs` for the
  library function signature + the recorder construction site), 1 file
  touched at the binary boundary
  (`crates/kaleidoscope-cli/src/main.rs` for `run_migrate_with` to
  thread `parse_observe_otlp(args)` through and for the usage text
  update), 1 new file
  (`crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs`), 1
  line-level modification (`crates/kaleidoscope-cli/Cargo.toml` for
  the new `[[test]]` entry).
- 1 integration point (the existing migrate path inside
  `kaleidoscope_cli::migrate`, already exercised by the locked
  `migrate_subcommand.rs` tests).
- Estimated effort: well under 1 day for the crafter. The acceptance
  test's four scenarios are all simple library-direct or subprocess
  invocations with seed → call → assert shape.

The feature is right-sized. No splitting required, no thinning possible.
