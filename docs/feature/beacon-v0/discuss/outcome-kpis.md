# Beacon v0 — outcome KPIs

Five outcome KPIs grounded in the user stories. Each has a numeric
target, a measurement plan, and a slice anchor. Convention follows
the Aperture / Sieve / Codex / Prism feature pattern: KPIs are
observed via automated tests, not via vendor analytics.

---

## KPI 1 — Time-to-first-alert

**What it measures**: the end-to-end latency from "underlying
condition becomes true" to "webhook POST received by sink", on the
walking-skeleton golden path.

**Target**: ≤ `rule.interval + rule.for_duration + 5 s` at p95 on a
60-row test harness.

**Why 5 s slack**: Beacon's evaluator schedules at `interval`
granularity; an alert that becomes `Firing` at the end of a
`for_duration` window may have to wait up to one more `interval`
tick before evaluation observes it. The 5 s slack accommodates the
evaluator's tick alignment plus network RTT to the sink.

**How measured**: Acceptance test
`tests/slice_01_walking_skeleton.rs` records the wall-clock at
which the test's `prometheus_mock` first returns `up == 0`, and the
wall-clock at which a fake `WebhookSink` records the POST. The
delta is asserted ≤ 35 s (30 s interval + 1 m for_duration cannot
both be at minimum in the walking skeleton's choice of defaults).

**Slice anchor**: US-BE-01 (walking skeleton).

---

## KPI 2 — Catalogue diagnostic recall

**What it measures**: the proportion of broken CUE rules in a
representative corpus that produce a diagnostic with file, line,
and field name. Plus the false-positive rate on valid rules.

**Target**: 100% recall on broken rules; 0% false-positive on valid
rules.

**Why a hard 100%**: a silent skipped rule is the worst outcome —
the operator believes the alert is in place when it is not. Beacon
v0's load-time discipline is the contract that prevents this; the
KPI just measures that the contract holds.

**How measured**: Acceptance test `tests/slice_02_cue_catalogue.rs`
loads a corpus of 50 hand-crafted CUE files: 5 broken (one with a
typo'd field, one with a missing required field, one with a wrong
type, one with a duplicate name, one with a malformed PromQL), and
45 valid. The test asserts every broken rule produces a diagnostic
naming the exact problem, and that all 45 valid rules load
successfully with no diagnostic noise.

**Slice anchor**: US-BE-02 (CUE catalogue).

---

## KPI 3 — Storm reduction ratio

**What it measures**: the number of sink emissions Beacon produces
on a 20-rule simultaneous-failure scenario where one upstream rule
is declared as the inhibitor.

**Target**: `1 + (resolutions)` emissions, NOT 20. Concretely on a
single-cycle 20-rule trip with one resolution: 2 emissions total.

**Why this ratio**: pager fatigue is the named operational anti-
pattern of incident response. Beacon's inhibition primitive must
collapse a backend-outage storm into a single page that names the
upstream, not 20 pages that bury the signal.

**How measured**: Acceptance test
`tests/slice_03_grouping_and_inhibition.rs` constructs a 20-rule
catalogue where 19 declare `inhibited_by: ["upstream_outage"]`,
fakes a Prometheus response where all 20 conditions trip
simultaneously, runs the evaluator one cycle, and counts the
emissions to a fake sink. The assertion is `emissions.len() == 1`.
The resolution case is exercised in a second test cycle.

**Slice anchor**: US-BE-03 (grouping and inhibition).

---

## KPI 4 — Sink delivery rate

**What it measures**: the proportion of incidents that successfully
reach each configured sink, on a 60-incident burst test against a
fake-sink harness. Retries on transient failure are in scope; the
KPI measures *eventual delivery*, not first-attempt success.

**Target**: 100% eventual delivery on transient failures (retry
recovers); ≤ 0.1% loss on permanent failures (the sink's
configured retry budget is exhausted — Beacon records the failure
and moves on).

**How measured**: Acceptance test `tests/slice_04_sink_routing.rs`
configures all five sink kinds (webhook, SMTP, Mattermost, Zulip,
OnCall) with fake adapters that fail the first attempt with 503
and succeed on retry. A 60-incident burst exercises every sink and
asserts every incident eventually arrives.

**Slice anchor**: US-BE-04 (multi-sink routing).

---

## KPI 5 — Burn-rate fidelity

**What it measures**: the byte-equal alignment between Beacon's
synthesised SLO MWMBR rule firing decisions and a hand-authored
reference PromQL on a 24-hour synthetic trace with controlled
error rates.

**Target**: byte-equal firing pattern across a 24-hour trace with
0.5% sustained error rate (above 99.9% target). Zero spurious
pages on a control trace with 0.05% sustained error rate (below
target).

**Why byte-equal**: the Google SRE workbook's multi-window-multi-
burn-rate methodology is mathematically precise. Beacon's
synthesis from a CUE SLO declaration to PromQL is a code-generation
problem; the correctness criterion is "the generated rules fire
exactly when the reference fires".

**How measured**: Acceptance test `tests/slice_05_slo_burn_rate.rs`
provides a synthetic time series via fake Prometheus, declares an
SLO in CUE, and asserts the firing decisions match the reference.
The reference is a hand-authored PromQL alert computed against the
same trace. The test exercises both the positive case (burn-rate
exceeds threshold → page) and the negative case (burn-rate stays
under threshold → silence).

**Slice anchor**: US-BE-05 (SLO burn-rate).

---

## Cross-KPI guardrails

| Guardrail | Threshold | Rationale |
|---|---|---|
| Memory footprint (RSS) | ≤ 256 MB for 100 rules | Beacon is a sidecar / small-VM workload; cannot compete with the storage engines for memory. |
| Evaluator cycle time p95 | ≤ 500 ms per 100 rules | Slower than 500 ms suggests an N² lookup or a non-cached PromQL parse — both are evolution-blockers. |
| Webhook payload size | ≤ 8 KB | Slack, Mattermost, and most webhook receivers truncate above this. |
| No telemetry-on-telemetry | 0 third-party endpoints | Per architecture doc §A.2; absence of `phone-home` in the codebase is the test. |
| AGPL licence-header coverage | 100% of `.rs` and `.cue` files | Same posture as every prior feature. |
