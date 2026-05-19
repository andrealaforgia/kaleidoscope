# KPI Instrumentation - `cli-list-items-subcommand-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

All three KPIs are wired through the SAME new acceptance test file
`crates/kaleidoscope-cli/tests/list_items_subcommand.rs`, executed
under ADR-0005 Gate 1 (`cargo test --workspace --all-targets
--locked`). No new collection infrastructure; no new dashboard; no
new alert on the project side. The CI exit code IS the KPI signal.
`list_by_tier` is a pure read with no operator-visible event to
record (Cinder's `MetricsRecorder` trait has no `record_list` method
per `crates/cinder/src/metrics.rs`).

## Per-KPI verification

### OK1 - CLI list-items correctness (principal / North Star)

| Aspect | Value |
|---|---|
| Probe | `tests/list_items_subcommand.rs` - three sub-scenarios: (a) happy-path `list_items_writes_lex_sorted_item_ids_one_per_line`, (b) determinism `two_successive_invocations_produce_byte_identical_stdout`, (c) empty-result `list_items_on_empty_tier_writes_empty_stdout`. |
| Mechanism | (a) Pre-place three items in Cold for `acme` in non-lex insertion order (`acme/batch-00099`, `acme/batch-00007`, `acme/batch-00041`) plus a decoy Hot item (`acme/batch-00050`) via direct `FileBackedTieringStore::open(...).place(...)` calls; invoke `kaleidoscope_cli::list_items(&acme, &dir, "cold", &mut sink)`; assert captured stdout EQUALS the bytes `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n` (lex-sorted; decoy Hot excluded). (b) Call the function TWICE in succession with the same arguments and two fresh sinks; assert both captured stdouts are byte-identical (this is the probe that pins DD2's `sort_unstable()` boundary sort against `HashMap` iteration randomness). (c) Call with `tier_arg = "warm"` against a tenant whose Warm tier has zero entries; assert captured stdout is empty (zero bytes). |
| Gate | Gate 1 (`cargo test`) + Gate 5 (`gate-5-mutants-kaleidoscope-cli`). |
| Alerting | CI failure on Gate 1 or Gate 5 is the alert (trunk-based; CI is feedback per project doctrine). |
| Dashboard | None on the project side. No operator-side dashboard either: `list_by_tier` emits no OTLP/metric. |

### OK2 - tenant isolation (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/list_items_subcommand.rs` - tenant-isolation scenario `list_items_does_not_surface_other_tenants_items`. |
| Mechanism | Pre-place `shared/batch-00042` in Cold for BOTH `acme` and `globex` in the same `data_dir` via two direct `FileBackedTieringStore::open(...).place(...)` calls; invoke `list_items(&acme, &dir, "cold", &mut sink)`; assert captured stdout contains exactly one line `shared/batch-00042\n` (only `acme`'s entry); follow-up: open a second `FileBackedTieringStore` handle and call `list_by_tier(&globex, Tier::Cold)`; assert the returned `Vec`, after lex sort, equals the pre-call state (containing `shared/batch-00042` for `globex`, unchanged). |
| Gate | Gate 1 + Gate 5. Specifically, if a future refactor short-circuits the per-tenant filter (e.g. by accidentally calling a `list_by_tier_all_tenants` variant), OK2 fails immediately. |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK3 - invalid-tier fail-fast (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/list_items_subcommand.rs` - invalid-tier scenarios `list_items_on_invalid_tier_argument_returns_invalid_tier_error` (sub-scenarios: `tier_arg = "COLD"` upper-case, `tier_arg = "lukewarm"` typo). |
| Mechanism | Each sub-scenario seeds at least one item in Hot for `acme` (so the Cinder store has content) and snapshots the pre-call `cinder.list_by_tier(&acme, Tier::Hot).len()`; invokes `list_items(&acme, &dir, "COLD", &mut sink)` (or `"lukewarm"`); asserts the call returns `Err(Error::InvalidTier { value })` with `value == "COLD"` (or `"lukewarm"`); asserts captured stdout is empty (zero bytes); asserts captured stderr (via the `Display` impl `kaleidoscope-cli: invalid tier "<value>": expected one of hot, warm, cold` per DD5) contains the verbatim invalid value as a substring; follow-up: re-opens the Cinder store and asserts `cinder.list_by_tier(&acme, Tier::Hot).len()` matches the pre-call snapshot (no mutation; `parse_tier` short-circuits BEFORE `FileBackedTieringStore::open` is called, per the parse-then-open ordering at `lib.rs:432-446` for `migrate`). |
| Gate | Gate 1 + Gate 5. Specifically, if a future refactor reorders `parse_tier(...)?` to run AFTER `FileBackedTieringStore::open(...)?`, OK3 fails because the store would have been opened (and read-only opens are observable as side effects in the Cinder WAL access pattern, even though the snapshot bytes remain unchanged). |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

## Why no dashboards or alerts (project-side)

This feature adds a pure read-only positional subcommand. There is no
project-side service to monitor, no project-side SLO to track, no
project-side error budget to burn, and no OTLP emission to feed any
operator-side dashboard either: `list_by_tier` is a pure read with
no `MetricsRecorder` trait method behind it. The operator's existing
sidecar (which tails the optional `--observe-otlp` sinks of `ingest`,
`read`, and `migrate`) does not see `list-items` activity, by design.
The CI gates are the only authority that this feature's contract
continues to hold.

This matches the posture of all nine prior `kaleidoscope-cli` DEVOPS
waves; no dashboard or alerting surface was created for any of them,
and none is needed here. This is the TENTH realisation of the same
posture.
