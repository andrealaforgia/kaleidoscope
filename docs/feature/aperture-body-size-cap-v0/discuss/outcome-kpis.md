# Outcome KPIs: aperture-body-size-cap-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-07.
British English. No em dashes in body.

Companion to `user-stories.md` (US-01/US-02/US-03 each carry a per-story
Outcome KPIs block) and `wave-decisions.md` (D1-D5). This file
consolidates the feature-level KPIs with numeric targets, baselines, and
measurement methods, so DEVOPS (`platform-architect`) can design tracking
and DISTILL/DELIVER can assert them. Every KPI is falsifiable: each names
a test that MUST FAIL against today's parsed-but-ignored
`max_recv_msg_size` and PASS only once the behaviour is correct.

All baselines are verified on this branch (see `wave-decisions.md` >
Verified Code Findings).

## Objective

Make the aperture gateway honestly enforce a configurable receive-body-size
cap so a single oversized OTLP payload is rejected loudly before it is
decoded into memory, protecting the shared collector from memory
exhaustion, while leaving unset gateways and legitimate traffic untouched.

## Feature-level KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| KPI-1 | aperture's logs + traces ingest paths | reject an over-limit body before the harness decodes it | enforced size-cap sites: 0 -> the configured ingest paths (logs + traces, both transports) | 0 enforced sites today (`max_recv_msg_size` parsed and ignored, `config/mod.rs:474-485`) | oversized-body acceptance test per signal+transport asserting the reject + no sink hand-off | Leading |
| KPI-2 | Priya's log/alert pipeline scraping aperture stderr | emit one `body_too_large` event naming the signal, limit, and actual size on a rejection | exactly 1 event per rejection per signal; 0 on the accept path | 0 events today; the constant exists with no emitter (`observability.rs:46`) | the oversized-body test capturing stderr via `testing::stderr_capture`, asserting one `body_too_large` line with the correct `limit`/`size` | Leading |
| KPI-3 | Priya tuning the cap against measured payload sizes | get an exact inclusive boundary | boundary ambiguity: undefined -> exactly 1 byte (at-limit accepted, at-limit-plus-one rejected) | no cap today, boundary undefined | the at-limit-accepted + at-limit-plus-one-rejected acceptance tests | Leading |
| KPI-4 | every aperture gateway that does NOT set the cap | experience zero behaviour change | behaviour change for unset gateways: 0 | every gateway is uncapped today; there is no set/unset distinction | the unset-no-cap negative-control test + the existing slice-01..05 suites staying green | Guardrail |
| KPI-5 | Priya setting one cap on a gateway that ingests logs and traces | get both signals guarded by the single cap | signal coverage: 0 enforced -> 2 (logs + traces); metrics named for DESIGN (D4) | 0 signals enforced today | two oversized-body tests (logs + traces) asserting the per-signal reject + event | Leading |
| KPI-6 | the project's correctness guarantee (ADR-0005 Gate 5) | kill every mutation on the changed lines | 100% kill on the modified files; 0 survivors on the boundary comparison and the unset-no-cap branch | the size check does not exist, so a mutant on it would have no test to kill it | `cargo mutants` scoped to the modified files per the per-feature strategy (CLAUDE.md) | Guardrail |

## KPI detail and falsifiability

### KPI-1 -- Enforced size-cap sites: 0 -> the configured ingest paths (US-01)

- **Baseline locus**: `config/mod.rs:474-485` (`max_recv_msg_size`
  parsed, `#[allow(dead_code)]`, "unused at v0"); no `Config` field or
  accessor (`:46-58,193-194`).
- **Falsifiability**: with a cap set and an over-limit body, today the body
  is ACCEPTED (200 / gRPC Ok) and handed to the harness. The test MUST
  FAIL on that accept and PASS only when the body is rejected before the
  sink is touched. A test that passes against today's accept-and-ignore is
  rejected (`wave-decisions.md` risk "A too-large test that passes on the
  unwired knob").

### KPI-2 -- One `body_too_large` event per rejection, naming limit + size (US-01)

- **Baseline locus**: `observability.rs:46` (constant exists, no emitter);
  the actual size is already computed as `body.len()` / `bytes.len()` at
  the `request_received` sites (`transport.rs:524-529,871-876`).
- **By how much**: exactly 1 event per injected oversized body per signal
  (not 0, not 2); event count on the accept path stays 0 (KPI-4).
- **Falsifiability**: MUST FAIL today (no event ever fires). The event
  shape (warn level, JSON, fields `transport`/`signal`/`limit`/`size`)
  matches the `concurrency_cap_hit` precedent
  (`backpressure.rs:116-122`); the exact reported-size field shape is
  reconciled with the enforcement site (D3).

### KPI-3 -- Exact inclusive boundary (US-02)

- **By how much**: at-limit accepted, at-limit-plus-one rejected -- a
  one-byte boundary. The reject is driven by the CONFIGURED limit (the
  tiny-cap test proves it is not a hardcoded constant).
- **Falsifiability**: the at-limit body MUST be accepted and the
  at-limit-plus-one body MUST be rejected; a `>` vs `>=` mutation on the
  comparison must be killed (feeds KPI-6).

### KPI-4 -- Zero behaviour change for unset gateways (US-03, guardrail)

- **Baseline**: every gateway is uncapped today; an unset config is the
  only behaviour. This KPI guarantees the new code does not impose a
  default cap, add a reject, or change the unset path.
- **By how much**: 0 behaviour change. An unset gateway accepts any body
  exactly as before, with no size check and no `body_too_large` event.
- **Falsifiability**: the unset-no-cap negative control asserts no event
  and no reject on a body that WOULD be rejected under a set cap; the
  existing slice-01..05 acceptance suites must stay green. A mutation that
  drops the `None` early-return (imposing a cap when unset) must be killed.

### KPI-5 -- Both signals covered, metrics named (US-03)

- **By how much**: coverage 0 -> 2 (logs + traces). Metrics (D4) is a
  DESIGN coverage decision; if deferred it must be disclosed, not left
  silent (the exact pattern this feature closes).
- **Falsifiability**: two distinct oversized-body tests (logs and traces),
  each asserting the per-signal reject and `body_too_large` event with the
  correct `signal` field.

### KPI-6 -- 100% mutation kill on the modified lines (C9, Gate 5 guardrail)

- **Baseline**: the size check does not exist, so today there is nothing to
  mutate; once added, the boundary comparison, the unset-no-cap branch, and
  the emit must each be pinned.
- **By how much**: 100% kill, 0 survivors on the changed lines, including
  the boundary comparison (`>`/`>=`), the `None` no-cap branch, and the
  `body_too_large` emit.
- **Measured by**: `cargo mutants` scoped to the modified files; the run
  is the Gate 5 closing check in DELIVER.

## Metric hierarchy

- **North Star**: an oversized OTLP body is rejected before it is decoded
  into memory, with the rejection named in one structured event (KPI-1 +
  KPI-2). This is the DoS-guard the feature exists to deliver.
- **Leading indicators**: per-signal enforcement (KPI-5), exact boundary
  (KPI-3).
- **Guardrail metrics**: zero behaviour change for unset gateways (KPI-4),
  100% mutation kill (KPI-6). These must NOT degrade: a guard that breaks
  unset deployments or leaves the boundary unpinned is a regression even if
  the over-limit reject works.

## KPI-to-story trace

| KPI | Primary story | Decision dependency | Baseline locus |
|---|---|---|---|
| KPI-1 enforced sites 0 -> configured | US-01 | D1, D2 | `config/mod.rs:474-485` |
| KPI-2 one event per rejection | US-01 | D2, D3 | `observability.rs:46`; no emitter |
| KPI-3 exact inclusive boundary | US-02 | D2 | no cap exists; boundary undefined |
| KPI-4 zero unset behaviour change | US-03 | C2 | every gateway uncapped today |
| KPI-5 both signals covered | US-03 | D4 | 0 enforced signals |
| KPI-6 100% mutation kill | all (guardrail) | C9 | the size check does not exist yet |

## Measurement timing

- **DISTILL** (`acceptance-designer`): KPI-1, KPI-2, KPI-3, KPI-5 become
  executable acceptance assertions; each must fail on today's
  parsed-but-ignored field before DELIVER makes it pass (the EDD
  failing-test-first discipline). KPI-4 is the negative control.
- **DELIVER** (`software-crafter`): KPI-6 (`cargo mutants` 100% on the
  modified files) is the Gate 5 closing check; KPI-4's "existing suites
  green" is the regression gate.
- **DEVOPS** (`platform-architect`): KPI-2 is the operator-facing signal
  (the `body_too_large` event stream, carrying the offending size) worth
  wiring into fleet observability and possibly an alert when a tenant
  repeatedly trips the cap; this file is the tracking-design input. KPI-1
  (rejections before OOM) is the resource-protection outcome to correlate
  against gateway memory pressure.

## Handoff to DEVOPS (instrumentation requirements)

1. **Data collection**: the `body_too_large` event (warn level, fields
   `transport`, `signal`, `limit`, `size`) on aperture stderr -- already
   the structured JSON shape the fleet scrapes.
2. **Dashboards / alerts**: a `body_too_large` rate per tenant/signal is a
   useful signal (a misconfigured exporter repeatedly tripping the cap);
   correlate the rejection rate against gateway memory to confirm the
   guard is preventing the OOM exposure.
3. **Baseline collection**: none required pre-release; the baseline is
   "0 enforced, 0 events" and is verified in code.
