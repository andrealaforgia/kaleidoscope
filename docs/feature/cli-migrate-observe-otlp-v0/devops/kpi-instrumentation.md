# KPI Instrumentation - `cli-migrate-observe-otlp-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

All four KPIs are wired through the SAME new acceptance test file
`crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs`,
executed under ADR-0005 Gate 1 (`cargo test --workspace
--all-targets --locked`). OK2 additionally rests on the locked
`tests/migrate_subcommand.rs` (mechanical signature-match edits
only; assertions byte-untouched). No new collection infrastructure;
no new dashboard; no new alert on the project side. The CI exit
code IS the KPI signal.

## Per-KPI verification

### OK1 - CLI migrate line shape (principal / North Star)

| Aspect | Value |
|---|---|
| Probe | `tests/migrate_observe_otlp_flag.rs` - happy-path scenario `migrate_with_observe_otlp_writes_one_cinder_migrate_count_line`. |
| Mechanism | Pre-place item `acme/batch-00042` for tenant `acme` in tier Hot via a direct `FileBackedTieringStore::open(...).place(...)` call; invoke `kaleidoscope_cli::migrate(&acme, &dir, "acme/batch-00042", "cold", &mut sink, Some(&otlp_path))`; read back the OTLP sink file and assert exactly one non-empty line whose `serde_json::Value` parse exposes `scopeMetrics[0].metrics[0].name == "cinder.migrate.count"`, `resource.attributes[0]` carrying `tenant_id == "acme"`, point attributes carrying `from == "hot"` and `to == "cold"`, `sum.dataPoints[0].asInt == "1"`, and a trailing `\n`. |
| Gate | Gate 1 (`cargo test`) + Gate 5 (`gate-5-mutants-kaleidoscope-cli`). |
| Alerting | CI failure on Gate 1 or Gate 5 is the alert (trunk-based; CI is feedback per project doctrine). |
| Dashboard | None on the project side. Operator-side: existing sidecar extends the existing `kaleidoscope.cinder` dashboard with a `cinder.migrate.count` panel (no project action). |

### OK2 - no-flag byte-equivalence (guardrail)

| Aspect | Value |
|---|---|
| Probe | (a) The locked `tests/migrate_subcommand.rs` continues to pass green under `cargo test --package kaleidoscope-cli --test migrate_subcommand` after the four mechanical `, None` suffixes (DD5 #3-#6) are applied; (b) a new no-flag scenario in `tests/migrate_observe_otlp_flag.rs` invokes `migrate(..., None)` against a candidate sink path and asserts no file is created at that path. |
| Mechanism | (a) Diff-time: zero assertion edits in `migrate_subcommand.rs`. Runtime: `cargo test --package kaleidoscope-cli --test migrate_subcommand` exits 0. (b) The new no-flag scenario seeds `acme/batch-00042` in Hot, picks a candidate path under `tempdir`, calls `migrate(..., None)`, asserts `Ok(())`, asserts stdout equals the byte-equivalent `migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`, then asserts `std::fs::metadata(&candidate).is_err()` (the file MUST NOT exist). |
| Gate | Gate 1 + Gate 5 + review-time diff inspection (any non-mechanical edit to `migrate_subcommand.rs` auto-rejects review per DEVOPS constraints). |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK3 - UnknownItem -> no emission (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/migrate_observe_otlp_flag.rs` - unknown-item scenario `migrate_with_observe_otlp_on_unknown_item_emits_no_line`. |
| Mechanism | Open a fresh `data_dir`; place `acme/batch-00001` in Hot to prove the store opens cleanly; spawn the binary via the `bin` helper with `migrate acme <data> ghost-item warm --observe-otlp <sink>`; assert non-zero exit code; assert stderr substring contains `ghost-item`; assert stdout is empty; if the sink file exists (it MAY have been created by an `OpenOptions::create(true)` ordering, which the test does not over-specify), assert that it contains zero lines whose parsed `scopeMetrics[0].metrics[0].name == "cinder.migrate.count"`. The asymmetry between "file may exist" and "no line written" pins the pre-flight `get_entry` short-circuit: the file open is allowed, but the recorder is never invoked because `get_entry` returns `None` BEFORE `cinder.migrate(...)` runs. |
| Gate | Gate 1 + Gate 5. |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK4 - InvalidTier -> no file created (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/migrate_observe_otlp_flag.rs` - invalid-tier scenario `migrate_with_observe_otlp_on_invalid_tier_does_not_create_sink_file`. |
| Mechanism | Open a fresh `data_dir`; place `acme/batch-00042` in Hot; pick a candidate sink path under `tempdir`; spawn the binary with `migrate acme <data> acme/batch-00042 LUKEWARM --observe-otlp <sink>`; assert non-zero exit; assert stderr contains the verbatim invalid tier value `LUKEWARM`; assert stdout is empty; assert `std::fs::metadata(&sink).is_err()` (the file MUST NOT exist after the call). This pins the parse-before-open contract from DESIGN DD2 (the `OpenOptions::open(path)?` call inside the `Some` arm is reached only AFTER `parse_tier(to_tier_arg)?` has succeeded). |
| Gate | Gate 1 + Gate 5. Specifically, if a future refactor moves the `OpenOptions::open(path)?` call site to BEFORE `parse_tier`, OK4 fails immediately. |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

## Why no dashboards or alerts (project-side)

This feature adds an opt-in NDJSON sink to the existing
`kaleidoscope-cli migrate` subcommand. There is no project-side
service to monitor, no project-side SLO to track, no project-side
error budget to burn. The operator's existing sidecar tails the
sink file and forwards lines to the operator's existing OTLP/HTTP
collector populating the operator's existing dashboard chain.
Project-side: CI gates are the only authority that this feature's
contract continues to hold.

This matches the posture of all eight prior `kaleidoscope-cli`
DEVOPS waves; no dashboard or alerting surface was created for any
of them, and none is needed here. This is the NINTH realisation of
the same posture.
