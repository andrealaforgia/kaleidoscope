# ADR-0036 — Beacon SLO MWMBR synthesis

**Status**: Accepted
**Date**: 2026-05-11
**Author**: Bea (autonomous DESIGN dispatch)

## Context

Slice 05 of Beacon v0 synthesises a five-rule alert set from one
CUE SLO declaration, per Google SRE workbook §14.4 "Alerting
Significant Events" Table 14-3 (the multi-window-multi-burn-rate
methodology). The contract is byte-equal firing decisions to a
hand-authored reference on a 24-hour synthetic trace.

The MWMBR methodology balances three concerns:

1. Catch fast-burn errors quickly (page on rapid budget exhaustion)
2. Catch slow-burn errors before the monthly budget is gone
   (ticket on sustained low-rate burns)
3. Avoid false pages on transient blips (multi-window requires both
   the long and short windows to exceed the threshold)

## Decision

The synthesis function is:

```rust
pub fn synthesise_slo(slo: &Slo) -> Vec<Rule> {
    let mut rules = Vec::with_capacity(4);
    for &(severity, page_ticket, threshold, long, short) in &MWMBR_TABLE {
        rules.push(synthesise_mwmbr_rule(slo, severity, page_ticket, threshold, long, short));
    }
    rules
}

const MWMBR_TABLE: &[(Severity, &str, f64, &str, &str)] = &[
    // (severity, page/ticket, burn_rate_threshold, long_window, short_window)
    (Severity::Critical, "page",   14.4, "1h",  "5m"),
    (Severity::Critical, "page",    6.0, "6h",  "30m"),
    (Severity::Warning,  "ticket",  3.0, "1d",  "2h"),
    (Severity::Warning,  "ticket",  1.0, "3d",  "6h"),
];
```

The values are taken verbatim from Google SRE workbook §14.4 Table
14-3 (the canonical multi-window-multi-burn-rate configuration for
a 30-day budget). The workbook URL is cited in the constant's
comment:

```rust
// Google SRE workbook §14.4 Table 14-3, multi-window-multi-burn-rate
// for a 30-day error budget. https://sre.google/workbook/alerting-on-slos/
```

## Synthesised rule shape

For each row of the table, the synthesised `Rule` is:

```rust
Rule {
    name: format!("{}_{}_{}_{}", slo.service, page_ticket, long, short),
    query: format!(
        "(\
            (sum(rate({total}[{long}])) - sum(rate({good}[{long}]))) / sum(rate({total}[{long}])) > ({budget} * {threshold}) \
            and \
            (sum(rate({total}[{short}])) - sum(rate({good}[{short}]))) / sum(rate({total}[{short}])) > ({budget} * {threshold}) \
        )",
        good = slo.sli_good_events,
        total = slo.sli_total_events,
        budget = 1.0 - slo.target_availability,
        threshold = threshold,
        long = long_window,
        short = short_window,
    ),
    for_duration: "0m".to_string(),  // multi-window IS the dwell time
    interval: "30s".to_string(),
    severity: severity,
    labels: {
        let mut m = BTreeMap::new();
        m.insert("slo_service".to_string(), slo.service.clone());
        m.insert("slo_window".to_string(), format!("{}/{}", long_window, short_window));
        m
    },
    annotations: {
        let mut m = BTreeMap::new();
        m.insert("summary".to_string(),
                 format!("SLO burn-rate alert for {} ({}/{})", slo.service, long_window, short_window));
        m.insert("source_slo".to_string(), slo.source_path.clone());
        m
    },
    inhibits: vec![],
    sinks: slo.sinks.clone(),
}
```

The synthesised PromQL is **deterministic**: same SLO inputs
produce byte-identical PromQL strings. The acceptance test snapshots
the synthesised expressions and asserts byte-equality across runs.

## Cross-validation contract

The slice 05 acceptance test exercises:

1. **Positive case**: a synthetic 24-hour trace with 0.5% sustained
   error rate (above 99.9% target). The synthesised rules MUST fire
   the page-level alert (1h/5m and 6h/30m thresholds both exceeded)
   within the first hour, and the ticket-level alert later. Byte-
   equal firing pattern to a hand-authored reference PromQL alert
   computed against the same trace.

2. **Negative case**: a synthetic 24-hour trace with 0.05% sustained
   error rate (below 99.9% target). The synthesised rules MUST NOT
   fire any alert. Zero spurious pages.

The reference rule lives in `crates/beacon/tests/fixtures/reference-rules/`
as plain `.cue` files for review by an SRE who understands the
methodology.

## Why not use Sloth or PromTools

The Apache-2.0 ecosystem includes Sloth (a YAML-to-PromQL
synthesiser, MIT) and PromTools. Both could synthesise MWMBR rules.
They are not used because:

- Sloth is YAML; Beacon's catalogue language is CUE
- Both produce static PromQL files; Beacon needs the rules
  in-memory for the evaluator
- Adding either as a runtime dep imports their YAML schema as
  Beacon's contract, fighting the CUE-first decision

The MWMBR table is small (four rows), the synthesis is a few
hundred lines of pure Rust, and the cross-validation test pins
correctness. Building it in-tree is cheaper than wrapping Sloth.

## Consequences

- One CUE SLO declaration → five synthesised rules, all carried
  through the same evaluator + sink path as hand-authored rules
- Byte-equal firing decisions to a hand-authored reference (KPI 5)
- The workbook citation is in the code; reviewers can audit the
  threshold values without re-reading the workbook
- A future v1 may add 7-day and 90-day budget tables; the
  synthesis function takes the table as input so adding a budget
  period is a constant addition, not a code rewrite

## Knowledge Gap

The MWMBR table values (14.4, 6, 3, 1) assume a 30-day error
budget. For shorter budgets (7-day) or longer (90-day), the
thresholds change. Beacon v0 supports only 30-day budgets. The CUE
schema validates `error_budget_period == "30d"` and rejects others
with a diagnostic naming the supported value.
