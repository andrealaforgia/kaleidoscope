# Beacon v0 — Slice ↔ Architecture Mapping

| Slice | User story | Architectural elements introduced | ADRs touched |
|---|---|---|---|
| 01 | US-BE-01 | CUE loader (single-rule shape), evaluator pure function, per-rule state machine, `WebhookSink`, `RealScheduler`, `beacon-server` binary skeleton, OTLP telemetry no-op default | 0033 (crate layout), 0034 (CUE schema — single-rule), 0035 (sink trait — webhook only), 0037 (evaluator + scheduler) |
| 02 | US-BE-02 | CUE loader generalised to a directory, multi-file diagnostics, `nearest_blessed_match` helper, `SIGHUP` handler | 0034 (CUE schema — full, with diagnostics) |
| 03 | US-BE-03 | Inhibition + grouping pure function, `Incident.inhibitor` / `inhibited` fields, property test for storm collapse | 0037 (state machine extended) |
| 04 | US-BE-04 | `SmtpSink`, `MattermostSink`, `ZulipSink`, `OnCallSink`, per-sink retry, header-redaction property test | 0035 (sink trait — all five impls + redaction) |
| 05 | US-BE-05 | SLO CUE schema, MWMBR synthesiser, cross-validation acceptance test | 0034 (CUE schema — SLO addendum), 0036 (MWMBR synthesis) |

## Reading order for the crafter

1. Slice 01 brief (`slices/slice-01-walking-skeleton.md`)
2. ADR-0033 (crate layout)
3. ADR-0037 (evaluator + scheduler)
4. ADR-0034 (CUE schema — minimum slice 01 shape)
5. ADR-0035 (sink trait — webhook only at slice 01)
6. The acceptance test design from DISTILL (when authored)

The crafter implements slice 01 against this stack; subsequent
slices extend the same modules per the slice briefs and the ADRs.
