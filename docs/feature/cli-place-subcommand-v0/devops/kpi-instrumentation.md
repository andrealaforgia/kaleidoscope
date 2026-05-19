# KPI Instrumentation - `cli-place-subcommand-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

All four KPIs are wired through the SAME new acceptance test file
`crates/kaleidoscope-cli/tests/place_subcommand.rs`, executed under
ADR-0005 Gate 1 (`cargo test --workspace --all-targets --locked`).
No new collection infrastructure; no new dashboard; no new alert
on the project side. The CI exit code IS the KPI signal. The
operator-side dashboard chain (`--observe-otlp` sidecar -> OTLP/HTTP
collector -> dashboard) already absorbs the `cinder.place.count`
line shape (byte-identical to `ingest`'s emission of the same line
via the same `CinderToOtlpJsonWriter`).

## Per-KPI verification

### OK1 - CLI place-success correctness (principal / North Star)

| Aspect | Value |
|---|---|
| Probe | `tests/place_subcommand.rs` - happy-path scenario `place_writes_one_line_report_and_post_call_get_entry_matches`. |
| Mechanism | Fresh `data_dir`; invoke `kaleidoscope_cli::place(&acme, &dir, "acme/bootstrap-00001", "hot", &mut stdout_sink, None)`; assert captured stdout EQUALS the bytes `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`; assert captured stderr empty; assert return value `Ok(())`; follow-up: open a fresh `FileBackedTieringStore::open(cinder_base(&dir), CinderRecorder)` handle and assert `get_entry(&acme, &ItemId::new("acme/bootstrap-00001".to_string())).unwrap().tier == Tier::Hot`. |
| Gate | Gate 1 (`cargo test`) + Gate 5 (`gate-5-mutants-kaleidoscope-cli`). |
| Alerting | CI failure on Gate 1 or Gate 5 is the alert (trunk-based; CI is feedback per project doctrine). |
| Dashboard | None on the project side. Operator-side dashboard inherits `cinder.place.count` panel from `cli-cinder-otlp-wiring-v0` / `cli-migrate-observe-otlp-v0`; nothing new to add. |

### OK2 - overwrite-semantics fidelity (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/place_subcommand.rs` - overwrite scenario `place_over_existing_item_overwrites_tier_and_returns_ok`. |
| Mechanism | Pre-place `acme/bootstrap-00007` in Hot for `acme` via a direct `FileBackedTieringStore::open(...).place(&acme, &ItemId::new("acme/bootstrap-00007".to_string()), Tier::Hot, fixed_placed_at)` call; invoke `place(&acme, &dir, "acme/bootstrap-00007", "cold", &mut stdout_sink, None)`; assert captured stdout EQUALS `placed tenant=acme item=acme/bootstrap-00007 tier=cold\n`; assert captured stderr empty; assert return value `Ok(())`; follow-up: open a fresh handle and assert `get_entry(...).unwrap().tier == Tier::Cold` (the new tier, overwriting the previous Hot). The faithful overwrite is the assertion; no special-case CLI guard is invented (no `AlreadyPlaced` branch). |
| Gate | Gate 1 + Gate 5. Specifically, if a future refactor introduces a pre-flight `get_entry` guard that rejects already-placed items, OK2 fails immediately (the call would return `Err` instead of `Ok` and stdout would be empty instead of carrying the new tier). |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK3 - invalid-tier fail-fast (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/place_subcommand.rs` - invalid-tier scenarios `place_on_invalid_tier_argument_returns_invalid_tier_error` (sub-scenarios: `tier_arg = "HOT"` upper-case, `tier_arg = "lukewarm"` typo). |
| Mechanism | Each sub-scenario seeds at least one item in Hot for `acme` (so the Cinder store has content) and snapshots the pre-call `cinder.get_entry(&acme, &ItemId::new("seed".to_string()))`; invokes `place(&acme, &dir, "acme/bootstrap-00001", "HOT", &mut stdout_sink, None)` (or `"lukewarm"`); asserts the call returns `Err(Error::InvalidTier { value })` with `value == "HOT"` (or `"lukewarm"`); asserts captured stdout is empty (zero bytes); asserts captured stderr (via the `Display` impl `kaleidoscope-cli: invalid tier "<value>": expected one of hot, warm, cold` per DESIGN DD5 §10) contains the verbatim invalid value as a substring; follow-up: re-opens the Cinder store and asserts the seeded item's `get_entry(...).tier` matches the pre-call snapshot (no mutation; `parse_tier` short-circuits BEFORE `FileBackedTieringStore::open` is called, per the parse-then-open ordering at `lib.rs:432-446` for `migrate` mirrored at the new `place` site). |
| Gate | Gate 1 + Gate 5. Specifically, if a future refactor reorders `parse_tier(...)?` to run AFTER `FileBackedTieringStore::open(...)?`, OK3 fails because the store would have been opened (and the bug would be observable as a different Cinder access pattern in the test harness, even if the snapshot bytes still happen to match). |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK4 - `--observe-otlp` emission (leading)

| Aspect | Value |
|---|---|
| Probe | `tests/place_subcommand.rs` - observe-otlp scenarios `place_with_observe_otlp_appends_one_cinder_place_count_line` (flag present) and `place_without_observe_otlp_creates_no_sidecar_file` (flag absent). |
| Mechanism | Flag-present: fresh `data_dir` and a fresh `observe_log_path` (does NOT pre-exist); invoke `place(&acme, &dir, "acme/bootstrap-00001", "hot", &mut stdout_sink, Some(&observe_log_path))`; assert `Ok(())`; assert the file at `observe_log_path` exists; read its bytes; assert exactly one line (one `\n` count); assert that one line contains the substrings `cinder.place.count` (the metric name), `acme` (the tenant id resource attribute, lower-case rendering), and `hot` (the tier point attribute, lower-case rendering). Flag-absent: invoke the same call with `otlp_log_path = None`; assert `Ok(())`; assert no file exists at the candidate path (the path the test would have passed if the flag had been set) - verifying no implicit file creation. |
| Gate | Gate 1 + Gate 5. Specifically, if a future refactor swaps the `Some(path)` / `None` arms in the recorder match, OK4's flag-absent sub-scenario fails (a file would be created); if a future refactor replaces `CinderToOtlpJsonWriter::new(file)` with `CinderRecorder` in the `Some(path)` arm, OK4's flag-present substring assertion fails (no `cinder.place.count` line would be emitted since `CinderRecorder` is the no-op alias for `cinder::NoopRecorder`); if a future refactor wraps the `place` call in a loop, OK4's "exactly one line" assertion fails. |
| Alerting | CI failure is the alert. Operator-side, the existing `cinder.place.count` panel on the dashboard inherits this emission; no new panel needed. |
| Dashboard | None on the project side. Operator-side dashboard panel for `cinder.place.count` already exists (`cli-cinder-otlp-wiring-v0` introduced it via `ingest`'s batch flush emission; `cli-migrate-observe-otlp-v0` reused it for `migrate --observe-otlp`'s `cinder.migrate.count` sibling). This feature adds a third caller emitting the same line shape; the panel auto-aggregates. |

## Why no dashboards or alerts (project-side)

This feature adds a positional subcommand whose principal effect is
the same as `ingest`'s batch flush at the byte level: one
`TieringStore::place` call, optionally producing one
`cinder.place.count` OTLP-JSON line on the operator-supplied
sidecar. The line shape is byte-identical to what `ingest` already
emits via the same `CinderToOtlpJsonWriter`. The operator's
dashboard chain (sidecar -> OTLP/HTTP collector -> dashboard)
already absorbs this line; this feature adds a new emission site,
not a new line shape. No project-side service to monitor, no
project-side SLO to track, no project-side error budget to burn.
The CI gates are the only authority that this feature's contract
continues to hold.

This matches the posture of all ten prior `kaleidoscope-cli`
DEVOPS waves; no project-side dashboard or alerting surface was
created for any of them, and none is needed here. This is the
ELEVENTH realisation of the same posture.
