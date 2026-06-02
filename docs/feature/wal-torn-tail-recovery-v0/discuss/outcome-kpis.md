# Outcome KPIs — wal-torn-tail-recovery-v0

British English. No em dashes in body.

## Feature: wal-torn-tail-recovery-v0

### Objective

After an abrupt process death, a file-backed Kaleidoscope store comes back up and serves everything that was durably acked before the crash, instead of refusing to start because of the benign torn final WAL line a crash leaves behind, and it tells the operator out loud, in one structured warning, exactly what it dropped.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| K1 | Operators restarting a crashed file-backed store whose WAL ends in a torn final line | Bring the store up serving the full intact acked prefix | 100% of in-scope pillars (lumen, ray, cinder; pulse if confirmed) recover the prefix and start | 0% (any torn tail blocks the whole open today) | AC-1 acceptance tests per pillar (the verifier D04 intact-prefix path: open succeeds, read API returns the acked prefix) | Leading (Outcome) |
| K2 | Operators of file-backed stores | Avoid a "store will not start after crash" incident attributable to a torn WAL tail | Reduce torn-tail-attributable startup refusals to 0 | Every torn-tail crash refuses today | Count of `event="health.startup.refused"` (or `PersistenceFailed` at open) whose root cause is a torn final WAL line, across the in-scope pillars; target 0 post-feature | Leading (Outcome) |
| K3 | Operators recovering a crashed store | Confirm exactly one torn tail was dropped (not a silent mid-file gap) before trusting the recovered store | 100% of torn-tail drops emit exactly one structured WARN naming pillar, line, dropped bytes | No such signal exists today | AC-3 acceptance test asserting the structured WARN fields; manual `journalctl`/`docker logs`/`kubectl logs` inspection in the field | Leading (Secondary) |
| K4 | The platform (guardrail) | Continue refusing real corruption (mid-file, or newline-terminated malformed final line) | 100% of mid-file and newline-terminated-malformed cases stay fail-closed; 0 false tolerations | Fail-closed today (the property to preserve) | AC-5 and AC-6 negative acceptance tests; mutation testing on the three guard conditions at 100% kill (AC-10) | Guardrail |
| K5 | The project's structural-honesty thesis (guardrail) | Ship no module doc claiming a robustness the code lacks | 0 false robustness claims in the cinder recovery docs | 1 false claim today (`crates/cinder/src/file_backed.rs:36-38`) | AC-7: read the corrected doc against AC-1 through AC-6; the claim now matches the code | Guardrail |

### Metric Hierarchy

- **North Star**: K1 — the share of crashed-with-torn-tail stores that recover their intact acked prefix and start successfully. This is the one metric that captures the durability promise the feature exists to honour.
- **Leading Indicators**: K2 (torn-tail-attributable refusals fall to zero) directly predicts K1; K3 (the warning fires on every drop) is the operator-trust signal that lets them act on a recovered store.
- **Guardrail Metrics**: K4 (mid-file and newline-terminated-malformed corruption stays fail-closed; the tolerance never widens) and K5 (no doc claims a robustness the code lacks). K4 must NOT degrade: a recovery that silently swallowed mid-file corruption would be strictly worse than today's fail-closed behaviour, so K4 is co-equal with K1, not subordinate.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| K1 | Per-pillar acceptance tests (the AC-1 intact-prefix path) | `cargo test` in DELIVER; black-box verifier expectation D04 on landing | Per CI run; once on landing | crafter (DELIVER), verifier (landing) |
| K2 | Process stderr / startup logs in the field; acceptance tests for the open-succeeds path | Count startup refusals whose root cause is a torn tail; expect 0 post-feature | Per crash event in the field; per CI run | operator (field), crafter (CI) |
| K3 | Process stderr structured `tracing` output | AC-3 acceptance test asserts the WARN fields; field operators read the WARN line | Per CI run; per recovery in the field | crafter (CI), operator (field) |
| K4 | AC-5 / AC-6 negative acceptance tests; `cargo mutants` on the guard conditions | `cargo test` plus each pillar's `gate-5-mutants-*` job via `--in-diff` at 100% kill | Per CI run | crafter (DELIVER) |
| K5 | The cinder module and `open` doc comments | AC-7 doc review against the actual behaviour | Once, at feature close; thereafter on any cinder recovery-path change | reviewer (DISCUSS), crafter (DELIVER) |

### Hypothesis

We believe that tolerating a torn FINAL WAL line on open (last line, no trailing newline), recovering the intact acked prefix, and emitting a structured warning, while keeping every other parse failure fail-closed, for operators restarting a crashed file-backed collector, will achieve the durability promise the WAL recovery discipline (ADR-0040) and the fsync-honest write path (ADR-0049) already make on the write side. We will know this is true when 100% of crashed-with-torn-tail stores across the in-scope pillars recover their intact acked prefix and start successfully (K1), torn-tail-attributable startup refusals fall to zero (K2), and zero mid-file or newline-terminated-malformed corruptions are ever silently tolerated (K4).

### Handoff to DEVOPS (platform-architect)

- **Data collection requirements**: the structured WARN event (`event="wal.recovery.torn_tail_dropped"` plus pillar, line, dropped-bytes fields, names pinned in ADR-0059) should be capturable by whatever log pipeline already captures `event="health.startup.refused"` and `event="listener_bound"`. No new metric, no new dashboard required at v0; the event rides the existing structured `tracing` stream.
- **Alerting thresholds**: none new mandated at v0. An operator MAY choose to alert on a torn-tail warning frequency above 1 per restart (more than one torn tail per open would itself be anomalous, since a clean crash leaves at most one), but this is operator policy, not a feature requirement.
- **Baseline measurement**: none required before release; the baseline (0% recovery, every torn tail refuses) is established by reading the four existing `file_backed.rs` replay loops.
