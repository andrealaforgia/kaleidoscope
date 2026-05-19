# KPI Instrumentation - `cli-migrate-subcommand-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

All four KPIs are wired through the SAME new acceptance test file
`crates/kaleidoscope-cli/tests/migrate_subcommand.rs`, executed
under ADR-0005 Gate 1 (`cargo test --workspace --all-targets
--locked`). No new collection infrastructure; no new dashboard; no
new alert. The CI exit code IS the KPI signal.

## Per-KPI verification

### OK1 - migrate-success correctness (principal / North Star)

| Aspect | Value |
|---|---|
| Probe | `tests/migrate_subcommand.rs` - `happy_path_hot_to_warm_writes_report_and_mutates_tier` scenario. |
| Mechanism | Pre-place item `acme/batch-00042` for tenant `acme` in tier Hot via a direct `FileBackedTieringStore::open(...).place(...)` call; invoke `kaleidoscope_cli::migrate(&acme, &dir, "acme/batch-00042", "warm", &mut sink)`; assert captured stdout equals the exact bytes `migrated tenant=acme item=acme/batch-00042 from=hot to=warm\n`; assert return is `Ok(())`; assert post-call `get_entry(&acme, &ItemId::new("acme/batch-00042")).unwrap().tier == Tier::Warm`. |
| Gate | Gate 1 (`cargo test`) + Gate 5 (`gate-5-mutants-kaleidoscope-cli`). |
| Alerting | CI failure on Gate 1 or Gate 5 is the alert (trunk-based; CI is feedback per project doctrine). |
| Dashboard | None (no service surface). |

### OK2 - unknown-item fail-fast (leading)

| Aspect | Value |
|---|---|
| Probe | `tests/migrate_subcommand.rs` - `unknown_item_returns_error_and_leaves_store_unchanged` scenario. |
| Mechanism | Open a fresh `data_dir`; place `acme/batch-00001` in Hot to prove the store opens cleanly; snapshot `list_by_tier(&acme, Hot/Warm/Cold).len()` as a triple; invoke `migrate(&acme, &dir, "acme/batch-00099", "warm", &mut sink)`; assert return is `Err(Error::CinderMigrate(MigrateError::UnknownItem { .. }))`; assert captured stdout is empty; format the error via `format!("kaleidoscope-cli: {e}")` and assert the substring `acme/batch-00099` appears; re-snapshot the triple and assert byte-identity with the pre-call triple. |
| Gate | Gate 1 + Gate 5. |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK3 - invalid-tier fail-fast (leading)

| Aspect | Value |
|---|---|
| Probe | `tests/migrate_subcommand.rs` - `invalid_tier_returns_error_and_leaves_store_unchanged` scenario, parameterised over inputs `{"HOT", "Hot", "hOt", "lukewarm", "", " hot", "hot "}`. |
| Mechanism | Place `acme/batch-00042` in Hot; for each invalid-tier input, invoke `migrate(&acme, &dir, "acme/batch-00042", input, &mut sink)`; assert return is `Err(Error::InvalidTier { value: input.to_string() })`; assert captured stdout is empty; format the error and assert substring contains the verbatim invalid value (the `{value:?}` debug format wraps in quotes so trailing whitespace is visible); assert post-call `get_entry(&acme, &ItemId::new("acme/batch-00042")).unwrap().tier == Tier::Hot` (unchanged). |
| Gate | Gate 1 + Gate 5. |
| Alerting | CI failure is the alert. Specifically, if a future change introduces `.eq_ignore_ascii_case()` or `.trim()` in `parse_tier`, OK3 fails immediately on the relevant sub-scenarios. |
| Dashboard | None. |

### OK4 - idempotent same-tier (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/migrate_subcommand.rs` - `same_tier_migrate_is_idempotent_and_reports_from_equals_to` scenario. |
| Mechanism | Pre-place `acme/batch-00007` in Cold; invoke `migrate(&acme, &dir, "acme/batch-00007", "cold", &mut sink)`; assert captured stdout equals the exact bytes `migrated tenant=acme item=acme/batch-00007 from=cold to=cold\n`; assert return is `Ok(())`; assert post-call `get_entry(&acme, &ItemId::new("acme/batch-00007")).unwrap().tier == Tier::Cold` (unchanged). Code-side guard: review-time grep MUST confirm zero occurrences of `if from == to_tier` or any equivalent same-tier short-circuit in the new `migrate` body (D-Idempotent / DESIGN DD2). |
| Gate | Gate 1 - failure of the scenario OR review-time discovery of a same-tier short-circuit is OK4 violation. |
| Alerting | CI failure on Gate 1 is the alert. |
| Dashboard | None. |

## Why no dashboards or alerts

This feature adds a new mutation subcommand to `kaleidoscope-cli`.
There is no service to monitor, no SLO to track, no error budget to
burn. The CLI binary's operator runs it on their own host; if the
binary mis-behaves, the operator sees it directly in their
terminal. The CI gates are the only authority that this feature's
contract continues to hold across the codebase's life.

This matches the posture of all six prior `kaleidoscope-cli`
DEVOPS waves; no dashboard or alerting surface was created for any
of them, and none is needed here. This is the seventh realisation
of the same posture.
