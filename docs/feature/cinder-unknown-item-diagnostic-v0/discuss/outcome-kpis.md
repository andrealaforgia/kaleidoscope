# Outcome KPIs: cinder-unknown-item-diagnostic-v0

## Feature: cinder-unknown-item-diagnostic-v0

### Objective

When an operator names a Cinder item id that does not exist, the
diagnostic names that id back exactly as typed (quoted), so the operator
classifies the failure as a not-found at a glance — and the message
matches the contract the CLI `--help` documents. Closed when zero
internal-type-name leaks remain in the unknown-item diagnostic and the
doc-vs-code mismatch is resolved.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Platform SREs operating Cinder via `kaleidoscope-cli` | correctly read an unknown-item failure as a not-found (re-run with corrected id) instead of an internal fault (escalate) | internal-type-name leaks in the unknown-item diagnostic: 1 -> 0 | `ItemId("ghost")` emitted today | acceptance assertion: stderr contains the quoted bare id AND does NOT contain `ItemId(` | Leading |
| 2 | The contract surface (CLI help vs code) | agree on the unknown-item wording | doc-vs-code mismatches on this message: 1 -> 0 | help promises `"ghost"`, code emits `ItemId("ghost")` | byte-comparison of emitted message to documented help shape; verifier K18 (UC-TIER-008/009) flips GREEN | Leading |

### Metric Hierarchy

- **North Star**: zero internal-type-name leaks in the unknown-item
  diagnostic (the `ItemId(` substring never appears in operator-facing
  stderr for this error).
- **Leading Indicators**: emitted message is byte-equal to the documented
  CLI-help shape with the bare quoted id substituted; verifier K18 GREEN.
- **Guardrail Metrics**: exit code on unknown item stays 1 (fail-closed
  unchanged); known-item success path stdout unchanged
  (`migrated tenant=... item=... from=... to=...`); no OTHER cinder
  diagnostic message changes; ADR-0005 five gates stay GREEN; per-feature
  mutation kill rate = 100% on the modified line(s).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| Leak count (north star) | acceptance test stderr assertions | `cargo test` (must-contain quoted id; must-not-contain `ItemId(`) | every CI run | DELIVER |
| Doc-vs-code match | acceptance test + verifier K18 | byte-comparison to help shape; UC-TIER-008/009 | every CI run + verifier batch | DISTILL / Verifier |
| Exit-code guardrail | acceptance test | assert non-zero on unknown, zero on known | every CI run | DELIVER |
| No-regression guardrail | render-site audit + mutation testing | grep `ItemId` render sites; `cargo mutants` scoped to modified files | per feature | DELIVER |

### Hypothesis

We believe that rendering the unknown-item diagnostic as the bare quoted
id (`"ghost"`) for both the `migrate` and `get-tier` paths will let
Platform SREs read the failure as a not-found and match the documented
CLI-help contract. We will know this is true when the operator-facing
stderr contains the quoted bare id and never the `ItemId(` newtype text,
the emitted message is byte-equal to the help shape, and verifier K18
(UC-TIER-008/009) is GREEN — with exit codes and the known-item success
path unchanged.
