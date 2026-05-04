# Monitoring and Alerting — `aperture` v0 (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-04.
> **Author**: Apex.
> **Companion documents**: `wave-decisions.md`, `observability-design.md`,
> `kpi-instrumentation.md`.

---

## The honest position

Aperture itself emits no metrics. Aperture itself runs no alerting
agent. Aperture itself integrates with no on-call platform.

What Aperture DOES emit is structured JSON Lines to stderr (per
ADR-0009; 20-event closed vocabulary; one event per line; documented
fields). Operators capture that stderr through their own log
aggregator (Loki, Splunk, ELK, journald, Vector→ClickHouse, etc. —
see `observability-design.md > Operator log-aggregation patterns`)
and translate the four DISCUSS-handoff alerting rules into their
preferred alerting system (Alertmanager, Splunk Enterprise Security,
ELK Watcher, Datadog, PagerDuty, Opsgenie, etc.).

This is **monitoring-and-alerting at v0** in its entirety. There is
no Kaleidoscope-side dashboard JSON, no Grafana template, no
Alertmanager rules file, no PagerDuty service definition. Pulse
(roadmap Phase 4) is when Kaleidoscope-side telemetry-on-telemetry
becomes a project concern.

This document exists to:

1. State the honest position above explicitly so future maintainers
   do not introduce monitoring infrastructure under the impression
   that v0 needs it.
2. Document the four guardrail alerting rules as queries operators
   translate to their preferred system.
3. Catalogue the reasonable-default thresholds the DISCUSS handoff
   specified.
4. Name the conditions that would trigger a future iteration to add
   Kaleidoscope-side dashboard or alerting infrastructure.

---

## The four guardrail alerting rules (operator-side)

DISCUSS's `outcome-kpis.md > DEVOPS handoff` named four alerting
thresholds. Operators wire these into their alerting system; Aperture
emits the underlying structured events; the queries below show the
pattern.

### Rule 1 — Acceptance ratio drop (page)

> **`count(sink_accepted) / count(request_received)` per transport
> drops below 95% sustained for 5 minutes → page.**

**What it catches**: either a downstream incident (ForwardingSink
failing) or an Aperture-internal regression. Either way, the
operator's customers are seeing an elevated rate of OTLP export
rejections; intervention is needed.

**Sample LogQL query** (Loki / Grafana Alerting flavour):

```logql
(
  sum by (transport) (
    rate({app="aperture"} | json | event="sink_accepted" [5m])
  )
)
/
(
  sum by (transport) (
    rate({app="aperture"} | json | event="request_received" [5m])
  )
)
< 0.95
```

**Sample Splunk SPL** (Enterprise Security flavour):

```spl
| tstats count where index=k8s sourcetype=kube:container:aperture event=sink_accepted by transport, _time span=5m
| join transport, _time [
    | tstats count where index=k8s sourcetype=kube:container:aperture event=request_received by transport, _time span=5m
  ]
| eval ratio = accepted / received
| where ratio < 0.95
```

**Severity**: **page** (per DISCUSS handoff). Customer-impacting,
on-call response.

**Tier classification** (per `production-readiness` skill's tiered
alerting): "Urgent — error rate >2x baseline, latency SLA breach,
response within 15 min". Aperture's error rate going from ~0% baseline
to 5%+ qualifies.

### Rule 2 — `/healthz` non-200 (page)

> **Any `/healthz` non-200 response → page (fatal invariant).**

**What it catches**: the Aperture process is alive but wedged. Per
`observability-design.md`, this is fatal; `/healthz` returns 200
unconditionally if the process is up.

**Sample query** (operator's HTTP probe is the data source, not
Aperture's stderr):

```yaml
# Sample Prometheus alerting rule against the operator's own probe
# infrastructure (the operator's k8s liveness probe metrics, exposed
# by the kubelet / kube-state-metrics / blackbox-exporter as
# appropriate):
- alert: ApertureHealthzNon200
  expr: probe_http_status_code{job="aperture-healthz"} != 200
  for: 0m  # immediate
  labels:
    severity: page
  annotations:
    runbook_url: https://docs/runbook/aperture-healthz
```

**Severity**: **page**.

**Tier classification**: "Page — service down, immediate response".

### Rule 3 — Concurrency cap hit (ticket)

> **`count(concurrency_cap_hit) > 0` over a 5-min window → ticket
> (not a page; saturation is informational, but it should drive a
> horizontal-scale decision).**

**What it catches**: Aperture is saturated on one or both transports
under offered load. Per the refusal-not-drop contract (DISCUSS Q4 +
KPI 6), every cap hit is a deterministic refusal the SDK retries
naturally; the operator has no production-incident here, but they
SHOULD scale up replicas (or raise the cap if pod memory permits).

**Sample LogQL**:

```logql
sum by (transport) (
  count_over_time({app="aperture"} | json | event="concurrency_cap_hit" [5m])
) > 0
```

**Severity**: **ticket** — review at next-business-day timing, not
on-call. Aligns with "Warning — capacity >80%, response within 1
hour" tier.

### Rule 4 — Unexpected outbound network traffic (page)

> **Any new outbound network connection from Aperture beyond
> ForwardingSink → page (CI invariant `no_telemetry_on_telemetry`
> should have caught this; if it reaches production, that is a
> CI-gate failure, not just a production incident).**

**What it catches**: a regression bypassed the
`gate-7-aperture-no-telemetry` CI test (e.g. a new dependency
silently added an outbound dial; a maintainer disabled the gate;
the gate's runner skipped the Linux-only test on a misconfigured
non-Linux runner).

**Sample query** (operator's network monitoring is the data source,
not Aperture's stderr):

```yaml
# Sample Prometheus alerting rule against the operator's own
# network observability infrastructure (e.g. Cilium, Calico, AWS
# VPC Flow Logs, Tetragon, etc.):
- alert: ApertureUnexpectedOutbound
  expr: |
    rate(network_outbound_connections_total{
      pod=~"aperture-.*",
      destination!~"<configured-forwarding-endpoint-host>:.*"
    }[5m]) > 0
  for: 1m
  labels:
    severity: page
  annotations:
    runbook_url: https://docs/runbook/aperture-unexpected-outbound
```

**Severity**: **page** — security-class incident, immediate
escalation. The CI invariant `no_telemetry_on_telemetry` is the
load-bearing defence; a production hit means the defence failed.

The configured `forwarding.endpoint` host should be allow-listed in
the operator's network policy (k8s NetworkPolicy, Cilium policy,
AWS Security Group, etc.) so unexpected destinations are blocked at
the network layer too — defence in depth.

---

## Alerting tier summary

Per `production-readiness > Alerting Tiers`:

| Tier | Aperture-side rule | Response time |
|---|---|---|
| **Page** | Rule 1 (acceptance ratio drop) | Immediate (within 5 minutes) |
| **Page** | Rule 2 (`/healthz` non-200) | Immediate |
| **Page** | Rule 4 (unexpected outbound) | Immediate (security-class) |
| **Urgent** | n/a at v0 | n/a |
| **Warning** | Rule 3 (concurrency cap hit) | Within 1 hour / next business day |
| **Info** | Drain-deadline-exceeded events (KPI 8 surface; not in DISCUSS handoff but worth surfacing as info) | Review at next-day stand-up |

Three of the four DISCUSS-handoff rules are page-level. This is
appropriate for Aperture's posture: it is the OTLP edge of a
telemetry pipeline, and a misbehaving edge is high-leverage in the
data-loss sense.

---

## Recommended dashboard layout (operator-side)

The operator's existing log-dashboard system (Grafana, Kibana,
Splunk dashboards, etc.) hosts these panels. Aperture has no opinion
on the visualisation tool; the panel queries below are query shapes
operators translate.

### Panel 1: Request rate per transport (RED method, "rate")

```logql
sum by (transport) (
  rate({app="aperture"} | json | event="request_received" [1m])
)
```

Shows the offered-load curve. Useful for capacity planning and for
correlating cap-hit alerts with traffic spikes.

### Panel 2: Error rate per transport (RED method, "errors")

```logql
sum by (transport) (
  rate({app="aperture"} | json | event="sink_failed" [1m])
)
+
sum by (transport) (
  rate({app="aperture"} | json | event="concurrency_cap_hit" [1m])
)
```

Shows refusal volume. The two terms split into "downstream-side
problem" (`sink_failed`) and "Aperture-side capacity"
(`concurrency_cap_hit`); the operator distinguishes them when
diagnosing.

### Panel 3: Acceptance latency p99 per transport (RED method, "duration")

The `latency_ms` field on `sink_accepted` events (per ADR-0009) is
the data source. The query depends on the log aggregator's
quantile-aggregation primitives.

```logql
quantile_over_time(0.99,
  {app="aperture"} | json | event="sink_accepted" | unwrap latency_ms [5m]
)
```

This panel directly tracks the north-star metric (KPI's "≤ 50 ms p99
under non-overload").

### Panel 4: Per-signal acknowledgement ratio (KPI 4)

```logql
sum by (signal) (
  rate({app="aperture"} | json | event="sink_accepted" [5m])
)
/
sum by (signal) (
  rate({app="aperture"} | json | event="request_received" [5m])
)
```

### Panel 5: Concurrency cap-hit count (KPI 5 surface)

```logql
sum by (transport) (
  count_over_time({app="aperture"} | json | event="concurrency_cap_hit" [5m])
)
```

### Panel 6: Lifecycle events (rolling-restart visibility)

```logql
{app="aperture"} | json | event=~"shutdown_initiated|in_flight_drained|drain_deadline_exceeded|shutdown_complete|startup|ready"
```

A streaming log panel (not a graph) showing lifecycle events as they
happen. Useful during rolling deployments.

### Panel 7: Probe / startup refusal events (Earned Trust surface)

```logql
{app="aperture"} | json | event=~"health.startup.refused|listener_bind_failed|config_validation_failed"
```

These are startup-time fatal events; they should be empty during
healthy operation. A streaming log panel or a count-per-hour bar
chart.

The seven-panel layout is a starting point; operators who run multi-
tenant fleets or who care more about specific signals customise.

---

## What Aperture explicitly does NOT provide

| Concern | Aperture v0 | Where it really lives |
|---|---|---|
| Prometheus exporter | None (DISCUSS Q6) | Pulse Phase 4 |
| OTLP-out from Aperture itself | None (CI invariant `no_telemetry_on_telemetry`) | Pulse Phase 4 |
| StatsD output | None | n/a |
| OpenMetrics endpoint | None | Pulse Phase 4 |
| Reference Grafana dashboard JSON | None at v0; the seven panels above are query shapes operators translate | Future Pulse phase may produce a reference dashboard |
| Reference Alertmanager rules YAML | None at v0; the four rules above are query shapes operators translate | Future Pulse phase may produce a reference rules file |
| PagerDuty / Opsgenie / Splunk Connect integration | None | Operator-owned |
| On-call rotation tooling | None | Operator-owned |
| SLO definition file (e.g. Pyrra YAML, Sloth YAML) | None at v0 | Future iteration; the SLO targets are documented in `outcome-kpis.md` and could be encoded as Pyrra/Sloth when Kaleidoscope-side SLO tracking emerges |
| Synthetic-monitoring probe (e.g. Pingdom, Uptime Robot) | None | Operator-owned |

The list is long. The discipline is: **Aperture v0 emits structured
data, period.** Every one of the items above is something a Pulse
phase or an operator-side ecosystem MAY layer on top of Aperture's
output; v0 deliberately stops at the data-emission boundary.

---

## SLO posture

Per `infrastructure-and-observability > SLO Design`:

| SLO | Target | Measurement |
|---|---|---|
| **Availability** (acceptance ratio under non-overload) | 99% (KPI 2 + KPI 4) | `successful_requests / total_requests` per transport, per signal; operator-side from stderr |
| **Latency** (acceptance latency p99) | ≤ 50 ms (north star) | `latency_ms` field on `sink_accepted`; operator-side from stderr |
| **Refusal-not-drop** (no silent drops under overload or restart) | 100% (KPI 6 + KPI 8) | Build-time property tests (slice 05 + slice 08); release-cadence load tests in future |

Error budgets:
- Availability 99% → 1% error budget (~7 hours/month per single
  Aperture replica). Per-replica calculation; operators with N
  replicas amortise.
- Latency 99% under 50 ms → 1% can exceed 50 ms; long-tail accepted.

These targets are recorded in `outcome-kpis.md` as the v0 contract.
Encoding them as machine-checkable SLO definitions (Pyrra, Sloth,
Datadog SLO, etc.) is an operator-side choice; Aperture provides the
underlying data feed.

---

## Conditions that would trigger Kaleidoscope-side monitoring infrastructure

This wave deliberately ships no Kaleidoscope-side monitoring or
alerting. The triggers that would change that:

1. **Pulse Phase 4 lands.** Pulse is the explicit project home for
   telemetry-on-telemetry. When Pulse ships, Kaleidoscope-side
   `/metrics` endpoints and reference dashboards become appropriate.
2. **Three or more pilot operators report the same need.** If
   pilot-operator surveys (KPI 3) consistently say "we want a
   reference Grafana dashboard JSON to copy", that is signal that
   Kaleidoscope-side reference dashboards are worth shipping
   pre-Pulse.
3. **A class of operator-side incident reveals a missing event.**
   If operators consistently report "we had X incident and the
   stderr vocabulary did not include the field/event we needed to
   diagnose it", that is signal to extend the closed vocabulary
   (DISCUSS D1 says "additions are non-breaking").

None of the three are met at v0. Each is a future-iteration
re-evaluation point.

---

## Summary

**Aperture itself emits no metrics, runs no alerting agent, and
exposes no Aperture-specific monitoring opinion.**

What Aperture DOES:

- Writes 20-event closed-vocabulary structured JSON Lines to stderr.
- Serves `/healthz` and `/readyz` on the OTLP HTTP listener.
- Refuses to start if the configured downstream (when
  `sink=forwarding`) does not honour the probe contract.

What operators DO:

- Capture stderr via their existing log aggregator.
- Translate the four DISCUSS-handoff alerting rules into their
  preferred alerting system using the queries above.
- Build (or skip) dashboards using the seven-panel starter layout
  as inspiration.
- Honour `/healthz` and `/readyz` for liveness and readiness probing.

The result is a v0 that is honest about its scope: rich enough to be
useful, sparse enough to not pretend it owns observability
infrastructure it does not own.
</content>
</invoke>
