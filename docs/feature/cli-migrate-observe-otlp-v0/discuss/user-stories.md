<!-- markdownlint-disable MD024 -->

# User Stories — `cli-migrate-observe-otlp-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. The Cinder trait-port boundary
  (`cinder::MetricsRecorder`) is already in place from
  `cinder-to-otlp-json-bridge-v0`; this feature replaces one runtime
  construction site, not a trait.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions with
  `// Given / // When / // Then` comment blocks, not Gherkin `.feature`
  files. The Given/When/Then text in the UAT Scenarios sections below is
  the specification; DISTILL translates it into `#[test]` functions in
  `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` mirroring
  the pattern already in `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`
  and `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs`.
- Cross-bridge contract (locked in ADR-0039 §2 and inherited from
  `cinder-to-otlp-json-bridge-v0`): the Cinder migrate metric name on
  the wire MUST be exactly `cinder.migrate.count`. The point-attribute
  shape MUST be `{tenant_id, from, to}` per ADR-0039 §2. Drift between
  this feature's observed wire output and the library precedent is a
  review failure.
- Locked tests non-regression: every assertion in
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` MUST continue
  to pass after this feature ships, with no edits to that file's
  assertions. The migrate happy path / idempotent / unknown-item /
  invalid-tier / tenant-isolation invariants are preserved
  byte-equivalently. This is OK2.
- Recorder construction site (D-RecorderConstruction): the OTLP writer
  is constructed at `FileBackedTieringStore::open` time inside
  `kaleidoscope_cli::migrate(...)` — mirroring the pattern in
  `kaleidoscope_cli::ingest(...)` at
  `crates/kaleidoscope-cli/src/lib.rs:155-184`. The same
  `OpenOptions::new().create(true).append(true).open(path)` open call
  produces the file handle that wraps `CinderToOtlpJsonWriter::new(file)`.
- Scope is the `migrate` event only. `place` and `evaluate` are out of
  scope: `place` belongs to the `ingest` path already wired by
  `cli-cinder-otlp-wiring-v0`; `evaluate` has no CLI call site today.
- Scope is the `migrate` subcommand only. No bulk-migrate, no
  `--dry-run`, no JSON output, no `--observe-otlp` on the from-tier read.

---

## US-01: Manual tier migrations are visible on the operator's OTLP audit stream

### Elevator Pitch

- **Before**: Priya the platform operator runs
  `kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold` during
  an incident to move a stuck batch out of Hot. The command succeeds
  and prints `migrated tenant=acme item=acme/batch-00042 from=hot to=cold`
  to stdout. The next morning the SRE asks "who moved
  `acme/batch-00042` to Cold yesterday, and from what tier?" Priya has
  no audit trail beyond her own shell history; the CLI's `migrate` path
  constructs Cinder with `cinder::NoopRecorder`
  (`crates/kaleidoscope-cli/src/lib.rs:434`), so the OTLP collector
  Priya already runs for Lumen sees zero `cinder.migrate.count` lines
  for this manual action. She cannot answer the audit question from her
  standard tooling.
- **After**: Priya runs:
  `kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold --observe-otlp /tmp/audit.ndjson`.
  Stdout is byte-equivalent to today
  (`migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`).
  The sink file `/tmp/audit.ndjson` gains exactly one new line with
  `scopeMetrics[0].metrics[0].name == "cinder.migrate.count"`,
  `resource.attributes[0]` carrying `tenant_id="acme"`, and point
  attributes carrying `from="hot"` and `to="cold"`. Her sidecar tails
  the file and forwards the line to the existing OTLP/HTTP collector
  without configuration change; the dashboard gains a
  `kaleidoscope.cinder / cinder.migrate.count` row next refresh.
- **Decision enabled**: Priya can answer "who moved
  `acme/batch-00042` to Cold yesterday, and from what?" by querying the
  collector for `cinder.migrate.count` lines with
  `tenant_id="acme"` and `to="cold"`, and can audit "how many manual
  tier migrations did we run during the incident window?" — all from
  the same cross-process dashboard she already uses for ingest activity,
  without patching the binary and without changing her sidecar or
  collector configuration.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope deployment.
The CLI's `migrate` subcommand exists
(`crates/kaleidoscope-cli/src/lib.rs:424-456`) and lets her manually
move a Cinder item from one tier to another during incident response.
But the subcommand has no `--observe-otlp` flag and constructs Cinder
with `cinder::NoopRecorder`. State-mutating operator actions therefore
leave no audit trail beyond the operator's shell history.

Priya finds it operationally hostile to answer "who moved this item
yesterday?" or "how many manual migrations did we run during the
incident window?" from her cross-process collector, because the data
does not exist there. Her only workarounds today are (a) parsing shell
history files across the operator team (fragile, incomplete), or (b)
reading the Cinder snapshot file directly and reconstructing the
transitions by diffing pre- and post-incident states (defeats the
purpose of running a cross-process collector).

The `CinderToOtlpJsonWriter::record_migrate(...)` implementation already
exists in `crates/self-observe/src/cinder_otlp_json.rs:263-285` and
emits the right wire shape (`cinder.migrate.count`, point attrs
`tenant_id`, `from`, `to`). The CLI's `migrate` path just does not
construct it.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `--observe-otlp` on `ingest`
for Lumen observability via an existing sidecar + OTLP/HTTP collector +
dashboard chain | now wants the same chain to capture state-mutating
operator actions (manual tier migrations) without infrastructure
additions and without remembering a different flag name | already
familiar with `kaleidoscope-cli migrate ...` (the subcommand shipped at
feature `cli-migrate-subcommand-v0`).

### Solution

Inside `kaleidoscope_cli::migrate(...)`
(`crates/kaleidoscope-cli/src/lib.rs:424-456`), accept an optional
`otlp_log_path: Option<&Path>` parameter. When set, construct a
`CinderToOtlpJsonWriter` against a file opened via
`OpenOptions::new().create(true).append(true).open(path)` at the same
point where `FileBackedTieringStore::open` is called (line 434), and
pass it as Cinder's recorder instead of `cinder::NoopRecorder`. When
absent (`None`), Cinder continues to be constructed with
`cinder::NoopRecorder` exactly as today.

At the binary boundary, extend `run_migrate_with(...)` in
`crates/kaleidoscope-cli/src/main.rs:272-281` to call the existing
`parse_observe_otlp(args)` helper and pass the resulting
`Option<PathBuf>` through. The `--observe-otlp <path>` flag parsing is
already implemented for `ingest` and `read`; this feature reuses it.

The DESIGN wave picks any compile-tractable mechanism for the writer
construction; the operator-visible invariant is OK1 (the wire shape per
successful migrate) plus the no-flag byte-equivalence guarantee (OK2).

### Domain Examples

#### 1. Happy path — Priya sees one audit line per manual migration

Priya runs, during an incident at 14:32 UTC:

```text
kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold \
  --observe-otlp /tmp/audit.ndjson
```

where `/tmp/data/cinder.*` already contains `acme/batch-00042` placed
in Hot. The command exits 0. Stdout contains exactly one line, byte-for-
byte identical to the pre-flag behaviour:

```text
migrated tenant=acme item=acme/batch-00042 from=hot to=cold
```

The file `/tmp/audit.ndjson` (created if it did not exist) gains
exactly one new non-empty line:

```text
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.migrate.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}},{"key":"from","value":{"stringValue":"hot"}},{"key":"to","value":{"stringValue":"cold"}}],"timeUnixNano":"...","asInt":"1"}]}}]}]}
```

The line parses as `serde_json::Value`. The line ends with `\n`. Priya's
sidecar forwards it to the OTLP/HTTP collector; her dashboard gains a
`kaleidoscope.cinder / cinder.migrate.count` row for `acme` with
`from=hot, to=cold` next refresh.

#### 2. Flag absent — byte-equivalent to today

Priya runs (no `--observe-otlp`):

```text
kaleidoscope-cli migrate acme /tmp/data acme/batch-00007 hot
```

where `/tmp/data/cinder.*` already contains `acme/batch-00007` placed
in Hot (the idempotent same-tier case). Exit code 0. Stdout is exactly
one line:

```text
migrated tenant=acme item=acme/batch-00007 from=hot to=hot
```

No file is created at any path. The Cinder recorder is constructed as
`cinder::NoopRecorder` — exactly today's behaviour. Every assertion in
`crates/kaleidoscope-cli/tests/migrate_subcommand.rs` continues to pass
byte-equivalently (the locked test file is unmodified). This is the
no-flag byte-equivalence guarantee.

#### 3. Unknown item with flag set — no emission, file may exist but contains no line

Priya runs, with a typo in the item id:

```text
kaleidoscope-cli migrate acme /tmp/data ghost-item warm \
  --observe-otlp /tmp/audit.ndjson
```

where `/tmp/data/cinder.*` has NO placement for `ghost-item`. Exit code
non-zero. Stderr contains the verbatim `ghost-item` substring and the
canonical `MigrateError::UnknownItem` Display fragment ("unknown
item"). Stdout is empty (no transition was reported because no
transition happened). The file `/tmp/audit.ndjson` may exist (opened
with `OpenOptions::new().create(true).append(true)` before the
pre-flight `get_entry` check returns `None`) but contains zero non-empty
lines whose metric name equals `cinder.migrate.count` for this CLI
invocation. The pre-flight `get_entry` short-circuits BEFORE
`cinder.migrate(...)` is invoked, so the `record_migrate` writer method
is never called and no OTLP line is emitted.

#### 4. Invalid tier with flag set — no file created, no emission

Priya runs, with a tier typo:

```text
kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 LUKEWARM \
  --observe-otlp /tmp/audit.ndjson
```

Exit code non-zero. Stderr contains the verbatim `LUKEWARM` substring
and the canonical `Error::InvalidTier` Display fragment
("expected one of hot, warm, cold"). Stdout is empty. The
`parse_tier(...)` call at `lib.rs:431` short-circuits BEFORE
`FileBackedTieringStore::open` runs (per the contract DD step 1 at
`lib.rs:411`), so no Cinder store is opened and the OTLP writer is
never constructed. Therefore `/tmp/audit.ndjson` is NOT created by this
CLI invocation (the `OpenOptions::create(true)` call sits inside the
store-open arm of the recorder construction, which is unreachable on
this path).

### UAT Scenarios (BDD)

#### Scenario: Manual migration with `--observe-otlp` emits one `cinder.migrate.count` line

```text
Given Priya has placed item `acme/batch-00042` under tenant `acme` in Hot at `<data>/cinder.*`
And `<sink>` is an empty NDJSON path
When Priya calls `kaleidoscope_cli::migrate(&acme, &data, "acme/batch-00042", "cold", &mut buf, otlp_log_path = Some(<sink>))`
Then the call returns Ok(())
And `buf` is exactly `migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`
And exactly one non-empty line in `<sink>` has `scopeMetrics[0].metrics[0].name == "cinder.migrate.count"`
And that line has `resource.attributes[0].value.stringValue == "acme"`
And that line has `scopeMetrics[0].scope.name == "kaleidoscope.cinder"`
And that line has `sum.dataPoints[0].asInt == "1"`
And that line's point attributes include `{"key":"from","value":{"stringValue":"hot"}}`
And that line's point attributes include `{"key":"to","value":{"stringValue":"cold"}}`
And `<sink>` ends with `\n`
```

#### Scenario: Migration with `--observe-otlp` absent is byte-equivalent to today

```text
Given Priya has placed item `acme/batch-00007` under tenant `acme` in Hot
When Priya calls `kaleidoscope_cli::migrate(&acme, &data, "acme/batch-00007", "hot", &mut buf, otlp_log_path = None)`
Then the call returns Ok(())
And `buf` is exactly `migrated tenant=acme item=acme/batch-00007 from=hot to=hot\n`
And no OTLP sink file is created at any path
And the post-call entry for (`acme`, `acme/batch-00007`) still has tier Hot
```

#### Scenario: Existing `migrate_subcommand.rs` tests pass byte-equivalently

```text
Given the existing test file `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` is unmodified
When `cargo test --package kaleidoscope-cli --test migrate_subcommand` runs after this feature ships
Then all tests in that file pass green
And no assertion in that file is edited
```

#### Scenario: Unknown item with `--observe-otlp` set leaves no `cinder.migrate.count` line

```text
Given a fresh `<data>` directory with NO placement for (`acme`, `ghost-item`)
And `<sink>` is an empty NDJSON path
When Priya invokes the binary with `migrate acme <data> ghost-item warm --observe-otlp <sink>`
Then exit code is non-zero
And stderr contains the substring `ghost-item`
And stderr contains the substring `unknown item`
And stdout is empty
And no non-empty line in `<sink>` (if it exists) has metric name `cinder.migrate.count` for this invocation
```

#### Scenario: Invalid tier with `--observe-otlp` set creates no OTLP file

```text
Given a fresh `<data>` directory
And `<sink>` is a path that does NOT exist before the call
When Priya invokes the binary with `migrate acme <data> item_id LUKEWARM --observe-otlp <sink>`
Then exit code is non-zero
And stderr contains the substring `LUKEWARM`
And stdout is empty
And the file at `<sink>` does NOT exist after the call
```

### Acceptance Criteria

- [ ] When `kaleidoscope_cli::migrate` is invoked with `otlp_log_path = Some(path)` and a successful migrate is performed, exactly one non-empty line is appended to the file at `path` whose `scopeMetrics[0].metrics[0].name` equals `cinder.migrate.count`.
- [ ] That line has `resource.attributes[0].value.stringValue` equal to the tenant id passed to `migrate`.
- [ ] That line has `scopeMetrics[0].scope.name` equal to `kaleidoscope.cinder`.
- [ ] That line has `sum.dataPoints[0].asInt` equal to `"1"`.
- [ ] That line's point attributes contain `{"key":"from","value":{"stringValue":"<from>"}}` where `<from>` is the lowercase ASCII spelling of the tier the item was in before the call.
- [ ] That line's point attributes contain `{"key":"to","value":{"stringValue":"<to>"}}` where `<to>` is the lowercase ASCII spelling of the requested target tier.
- [ ] Stdout is byte-equivalent to the pre-flag behaviour: exactly `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`.
- [ ] When `kaleidoscope_cli::migrate` is invoked with `otlp_log_path = None`, no file is created at any path; the Cinder recorder is constructed as `cinder::NoopRecorder`.
- [ ] The existing test file `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` continues to pass green under `cargo test --package kaleidoscope-cli --test migrate_subcommand` with no edits to its assertions.
- [ ] On `MigrateError::UnknownItem` (pre-flight `get_entry` returns `None`), the sink file may exist (opened with `create(true)`) but contains no `cinder.migrate.count` line attributable to this invocation; stdout is empty; exit code is non-zero.
- [ ] On `Error::InvalidTier` (parse fails before any store-open), the sink file does NOT exist after the call; stdout is empty; exit code is non-zero.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the byte level on her configured `--observe-otlp <path>` sink for `migrate` invocations
- **Does what**: sees one `cinder.migrate.count` line per successful manual tier migration with the correct `tenant_id`, `from`, and `to` attributes, while stdout remains byte-equivalent to the pre-flag behaviour and locked tests continue to pass
- **By how much**: 100% of successful `migrate()` calls produce exactly one line (OK1); 100% no-flag byte-equivalence on stdout and locked tests (OK2); 0% emission on unknown-item / invalid-tier error paths (OK3, OK4)
- **Measured by**: new test `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` (OK1, OK3, OK4) and existing `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (OK2)
- **Baseline**: 0% migrate lines today (NoopRecorder on the migrate path); existing migrate stdout shape as shipped at feature `cli-migrate-subcommand-v0`

Maps to OK1-CLI-migrate-line-shape (principal), OK2-CLI-no-flag-byte-equiv, OK3-CLI-unknown-item-no-emission, OK4-CLI-invalid-tier-no-file in `outcome-kpis.md`.

### Technical Notes

- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — `migrate(...)` gains an `otlp_log_path: Option<&Path>` parameter. The construction site at line 434 (`FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))`) becomes conditional on `otlp_log_path`, mirroring the `ingest` pattern at lines 155-184.
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — `run_migrate_with(...)` at lines 272-281 calls `parse_observe_otlp(args)?` and threads the result through. The existing `parse_observe_otlp` helper at lines 161-175 is reused as-is.
- New test file: `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs`. Mirrors the harness shape in `migrate_subcommand.rs` (helpers `tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`, `read_entry`, `bin`) — duplicated inline at v0 per the same rule-of-three deferral used in sibling feature `cli-cinder-otlp-wiring-v0` D4.
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new `[[test]]` entry `name = "migrate_observe_otlp_flag", path = "tests/migrate_observe_otlp_flag.rs"`. The `self-observe` dependency already re-exports `CinderToOtlpJsonWriter`; no new dependency required.
- Usage text in `main.rs` `write_usage(...)` is updated to document `[--observe-otlp <path>]` on the `migrate` subcommand. The existing `migrate_subcommand.rs` tests do not assert on usage text and remain green.
- Concurrency model: `migrate(...)` is invoked once per CLI process; there is no in-process concurrency on this path (unlike `ingest`'s batch loop). The writer's `Mutex<W>` + `write_all + b"\n" + flush` triple inherited from `cinder_otlp_json.rs:226-241` is sufficient for the single-call shape.
- Slice tag: not `@infrastructure` — this story directly enables an operator-visible audit-trail decision on a real CLI surface.

### Dependencies

- `cinder-to-otlp-json-bridge-v0` shipped (`CinderToOtlpJsonWriter::record_migrate` and `cinder.migrate.count` wire contract).
- `cli-migrate-subcommand-v0` shipped (the `migrate` subcommand and library function exist).
- Prior `--observe-otlp` flag parsing shipped (commit `3af7e82` / sibling feature `cli-cinder-otlp-wiring-v0`).
- `aegis` (already a `kaleidoscope-cli` dependency).
- No new external crates required.

### Slice

`slices/slice-01-migrate-observe-otlp.md`
