# Outcome KPIs — `cli-migrate-observe-otlp-v0`

## Feature

`cli-migrate-observe-otlp-v0` — extend the `kaleidoscope-cli migrate
<tenant> <data_dir> <item_id> <to_tier>` subcommand with an optional
`--observe-otlp <path>` flag. When set, every successful `migrate()`
call emits exactly one NDJSON OTLP-JSON line to `<path>` via the
already-shipped `CinderToOtlpJsonWriter`, carrying metric name
`cinder.migrate.count`, a `tenant_id` resource attribute, and point
attributes naming the `from` and `to` tiers. When absent, behaviour is
byte-equivalent to today (Cinder constructed with `NoopRecorder`; no
file created).

## Objective

A single `kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold
--observe-otlp /tmp/audit.ndjson` invocation leaves the existing stdout
report unchanged and appends exactly one new line to `/tmp/audit.ndjson`
carrying the `tenant_id`, `from`, and `to` attributes of the
transition. State-mutating operator actions are no longer fire-and-
forget: the operator's existing sidecar tails the file and forwards the
line to the existing OTLP/HTTP collector, populating an audit trail
queryable from the same dashboard chain already used for ingest
activity.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
guarantee is the **wire shape per successful migrate** (OK1): a single
line with the exact metric name, scope, resource attribute, and point
attributes mandated by ADR-0039 §2. The remaining KPIs frame OK1: OK2
is the no-flag byte-equivalence guardrail (the locked
`migrate_subcommand.rs` test file must continue to pass); OK3 and OK4
are the error-path emission absence guarantees (`UnknownItem` produces
no line; `InvalidTier` does not even create the file).

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-migrate-line-shape | Priya the platform operator, observed at the byte level on her configured `--observe-otlp <path>` sink for `migrate` invocations | Sees exactly one new non-empty line per successful `migrate()` call whose `scopeMetrics[0].metrics[0].name` is `cinder.migrate.count`, with `resource.attributes[0]` carrying `tenant_id` equal to the tenant passed on the command line, and point attributes carrying `from` and `to` set to the lowercase ASCII spellings of the source and target tiers; `sum.dataPoints[0].asInt == "1"`; line ends with `\n`; line parses as `serde_json::Value` | 100% of successful migrate calls produce one such line; zero deviation from the wire shape locked in ADR-0039 §2 | 0% today (the CLI's migrate path constructs Cinder with `cinder::NoopRecorder` — emits nothing) | New acceptance test `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` — happy-path scenario (`hot -> cold`) seeds one Cinder placement, calls `migrate(..., otlp_log_path = Some(<sink>))`, reads back the sink and asserts the full wire shape | Leading (operator-visible behaviour; principal KPI) |
| OK2-CLI-no-flag-byte-equiv | Priya the platform operator, observed on the stdout of `kaleidoscope-cli migrate` and at the byte level of all existing locked migrate tests | Sees stdout output byte-equivalent to the pre-feature behaviour (`migrated tenant=<t> item=<i> from=<f> to=<t>\n`) and no OTLP file created at any path when `--observe-otlp` is absent | 100% pass of every existing assertion in `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (all tests in that file continue to pass with no edit to their assertions); 0% files created at any path on no-flag invocations | n/a (baseline = current shipped behaviour at feature `cli-migrate-subcommand-v0`) | Existing test file `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` continues to pass green under `cargo test --package kaleidoscope-cli --test migrate_subcommand` after this feature ships, with no edits to the assertions in that file | Guardrail (non-regression on the prior `migrate` subcommand feature) |
| OK3-CLI-unknown-item-no-emission | Priya the platform operator, observed on the sink file after a typo'd item id | Sees zero `cinder.migrate.count` lines attributable to a CLI invocation whose pre-flight `get_entry` returned `None` (the `MigrateError::UnknownItem` path); the sink file may exist (opened by `OpenOptions::create(true)` before the pre-flight check) but contains no migrate line for this call; exit code non-zero; stderr names the verbatim item id | 100% of `UnknownItem`-path invocations leave no `cinder.migrate.count` line in the sink; 0% of `UnknownItem`-path invocations have any stdout content | n/a (no flag exists today; the pre-flight `get_entry` short-circuit is inherited from `cli-migrate-subcommand-v0`) | New acceptance test `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` — unknown-item scenario invokes the binary with `--observe-otlp <sink>` against a Cinder directory with no placement for the item, asserts non-zero exit, stderr substring, empty stdout, and that the sink contains no line with metric name `cinder.migrate.count` for this invocation | Guardrail (error-path emission absence; rests on the existing pre-flight `get_entry` contract) |
| OK4-CLI-invalid-tier-no-file | Priya the platform operator, observed on the filesystem after a tier typo | Sees no OTLP file created at the path supplied via `--observe-otlp`; the `parse_tier(...)` call short-circuits BEFORE `FileBackedTieringStore::open` runs, so the recorder construction site is unreachable on this path and no `OpenOptions::create(true)` runs against the sink path; exit code non-zero; stderr names the verbatim invalid tier value | 100% of `InvalidTier`-path invocations leave NO file at the supplied sink path; 0% of `InvalidTier`-path invocations have any stdout content | n/a (no flag exists today) | New acceptance test `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` — invalid-tier scenario invokes the binary with `--observe-otlp <sink>` and `to_tier = "LUKEWARM"`, asserts non-zero exit, stderr substring, empty stdout, and that the sink file does NOT exist after the call | Guardrail (error-path file-creation absence; pins the parse-before-open contract from `lib.rs:431`) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-migrate-line-shape** — the per-migrate wire
  shape is what makes the audit trail queryable. Without OK1, the
  feature's reason to exist (cross-process audit of state-mutating
  operator actions) is unmet. The other three KPIs frame OK1: OK2
  guarantees no regression on the existing migrate UX; OK3 and OK4
  guarantee that error paths do not pollute the audit stream with
  phantom lines or leave the operator confused by an unexpectedly-
  created sink file.
- **Leading Indicators**: none additional. The four KPIs together cover
  the success path (OK1), the no-flag path (OK2), and the two distinct
  error paths (OK3, OK4) the existing `migrate()` library function can
  take.
- **Guardrail Metrics**: OK2 (no-flag byte-equivalence on stdout + the
  locked `migrate_subcommand.rs` test file) is the principal guardrail.
  OK3 and OK4 are also guardrails in the sense that they guarantee
  error-path emission absence.

## Cross-bridge alignment

OK1 is the CLI surface for `CinderToOtlpJsonWriter::record_migrate`
already shipped in `cinder-to-otlp-json-bridge-v0` (locked at ADR-0039
§2: metric name `cinder.migrate.count`, point attrs `tenant_id`,
`from`, `to`). The library feature proved that the writer produces one
line per `record_migrate` call against an in-memory `SharedBuf`; this
feature proves that the same guarantee survives the move to a real
`File` opened with `O_APPEND` and driven by the CLI's `migrate`
subcommand.

OK2 is the analogue of OK8 in `cli-cinder-otlp-wiring-v0` (Lumen-side
non-regression) but on the migrate path: the existing
`migrate_subcommand.rs` test file is the byte-equivalence probe for
"migrate stdout + behaviour unchanged when the flag is absent".

| KPI | Library precedent | This feature |
|-----|-------------------|--------------|
| OK1 | `record_migrate` line shape against `SharedBuf` | one parseable line per CLI-driven successful `migrate()` against the operator's real file |
| OK2 | n/a | byte-equivalence on stdout + locked tests when `--observe-otlp` is absent |
| OK3 | n/a (library writer is not driven on the `UnknownItem` path because the CLI's pre-flight `get_entry` short-circuits before the trait call) | empirical assertion that the CLI honours the same short-circuit when the flag is set |
| OK4 | n/a | empirical assertion that `parse_tier` short-circuits BEFORE the sink file is ever created |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-migrate-line-shape | `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` — happy-path scenario | `cargo test --package kaleidoscope-cli --test migrate_observe_otlp_flag` exit code; the test seeds one Cinder Hot placement, calls `migrate(..., otlp_log_path = Some(<sink>))`, reads back the sink file, asserts (a) exactly one non-empty line with `metric.name == "cinder.migrate.count"`, (b) `tenant_id == "acme"`, (c) `from == "hot"`, (d) `to == "cold"`, (e) `asInt == "1"`, (f) file ends with `\n` | At every commit touching the CLI migrate path or the `self-observe` Cinder writer | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-no-flag-byte-equiv | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (unchanged) PLUS the no-flag scenario in the new test file | `cargo test --package kaleidoscope-cli --test migrate_subcommand` exit code for the locked file; an additional no-flag scenario in the new test invokes `migrate(..., otlp_log_path = None)` and asserts no file was created at any candidate path | Same | Same |
| OK3-CLI-unknown-item-no-emission | Same new test file — unknown-item scenario (subprocess) | Same `cargo test` invocation. The test spawns the binary with `migrate acme <data> ghost-item warm --observe-otlp <sink>`, asserts non-zero exit, stderr substring `ghost-item`, empty stdout, and that the sink (if it exists) contains no `cinder.migrate.count` line for this invocation | Same | Same |
| OK4-CLI-invalid-tier-no-file | Same new test file — invalid-tier scenario (subprocess) | Same `cargo test` invocation. The test spawns the binary with `migrate acme <data> item_id LUKEWARM --observe-otlp <sink>`, asserts non-zero exit, stderr substring `LUKEWARM`, empty stdout, and that the sink file does NOT exist after the call | Same | Same |

## Hypothesis

We believe that **adding `--observe-otlp <path>` to the
`kaleidoscope-cli migrate` subcommand, threading the resulting
`Option<&Path>` into `kaleidoscope_cli::migrate(...)`, and constructing
`CinderToOtlpJsonWriter::new(file)` at the Cinder store-open site
instead of `cinder::NoopRecorder` when the path is `Some`** for the
**platform operator (Priya)** will achieve **a queryable cross-process
audit trail of state-mutating operator actions, observable on her
existing sidecar + collector + dashboard chain with zero configuration
change, while leaving the migrate stdout and locked tests
byte-equivalent**.

We will know this is true when:

- The new acceptance test's happy-path scenario passes green, asserting
  the full wire shape per successful migrate (OK1).
- The existing `migrate_subcommand.rs` test file passes
  byte-equivalently with no edits to its assertions, AND the new no-flag
  scenario asserts no file is created at any path (OK2).
- The new acceptance test's unknown-item scenario passes green,
  asserting non-zero exit, stderr content, and no
  `cinder.migrate.count` line in the sink (OK3).
- The new acceptance test's invalid-tier scenario passes green,
  asserting non-zero exit, stderr content, and that the sink file does
  not exist (OK4).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The recorder-construction site contract (D-RecorderConstruction)**:
   the OTLP writer is constructed at `FileBackedTieringStore::open`
   time, NOT at `migrate()` call time. The
   `OpenOptions::new().create(true).append(true).open(path)` call
   produces the file handle that wraps `CinderToOtlpJsonWriter::new(file)`,
   mirroring the existing pattern in `ingest` at
   `crates/kaleidoscope-cli/src/lib.rs:155-184`.
2. **The cross-bridge metric-name contract from ADR-0039 §2**: the
   Cinder side of the wiring must produce lines whose `metric.name` is
   exactly `cinder.migrate.count`, with point attributes
   `{tenant_id, from, to}` in that order or any order — the order is
   not pinned at the CLI level because the library writer's order is
   pinned at the library level (`cinder_otlp_json.rs:263-285`) and the
   CLI consumes that surface unchanged.
3. **The migrate stdout untouched**: the chosen wiring must not alter
   the shape, count, or contents of the one-line transition report on
   stdout. The existing `migrate_subcommand.rs` test is the
   byte-equivalence probe.
4. **The parse-before-open contract**: `parse_tier(...)` MUST continue
   to run BEFORE `FileBackedTieringStore::open`, so that invalid-tier
   invocations short-circuit before the sink file is created. This is
   the contract that OK4 measures.
5. **The pre-flight `get_entry` contract**: the pre-flight `get_entry`
   short-circuit MUST continue to fire BEFORE `cinder.migrate(...)` is
   invoked, so that `UnknownItem` invocations do not produce a
   `cinder.migrate.count` line. This is the contract that OK3 measures.

The DESIGN wave should NOT:

- Introduce a new flag name (`--observe-cinder-otlp` or similar).
- Add bulk-migrate, `--dry-run`, or JSON-output variants.
- Wire `--observe-otlp` on the from-tier read (`get_entry`) — only the
  actual migrate emits.
- Modify the writer's public API (ADR-0039 §1 is locked).

## DEVOPS instrumentation needs

No new collection infrastructure. The writer appends to the same NDJSON
file shape the existing sidecar already tails for `ingest`; the
operator's existing dashboards extend by adding a
`kaleidoscope.cinder / cinder.migrate.count` panel (the
`kaleidoscope.cinder` scope is already populated by
`cli-cinder-otlp-wiring-v0` for `cinder.place.count`; this feature adds
a sibling metric in the same scope). The CI gate is the new acceptance
test's exit code, per ADR-0005 Gate 1.
