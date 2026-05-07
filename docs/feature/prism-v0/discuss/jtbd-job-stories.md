# JTBD Job Stories — `prism-v0`

> **Wave**: DISCUSS — Phase 1 (JTBD analysis).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-07.
> **Companion documents**: `jtbd-four-forces.md`, `jtbd-opportunity-scores.md`, `journey-incident-response-visual.md`, `wave-decisions.md`.

This file documents the operator's incident-time job-to-be-done that Prism v0 hires itself to do. Phase-1 result: the **primary** job is "see the shape of a misbehaving signal, fast enough to make the next mitigation decision". Three secondary jobs are recorded for triage but explicitly deferred to post-v0.

Prior Kaleidoscope features (Codex, Spark, Sieve, Aperture, the harness) all served developers — operators inherited those tools indirectly. Prism is the first feature where the operator IS the primary user. Developer-side framing does not transfer.

---

## Persona — the operator on incident call

**Name**: Priya Raman.
**Role**: senior site reliability engineer at `acme-observability`, a mid-sized SaaS company running a multi-service product.
**Context**: she is on the on-call rota one week in four. PagerDuty has just paged her at 03:14 with "checkout-service p99 latency ≥ 800 ms for 5 min". She has 90 seconds to acknowledge before escalation, and roughly 5–10 minutes to make a triage decision (rollback / scale / hand over to subject-matter expert / declare incident) before the customer-facing impact compounds.

**What is in her hands**: a laptop, a half-finished cup of tea, the Aperture-fed Mimir backend her company already runs, the Kaleidoscope Prism SPA at `https://prism.acme-observability.internal`. No Grafana plug-in, no Datadog tab, no other dashboard tool open at this moment.

**What is in her head**: the service map of `acme-observability`'s 23 services; a rough mental model of which services are "noisy" vs "quiet"; the names of half a dozen PromQL metrics she has seen during prior incidents (`http_server_duration_seconds`, `process_cpu_seconds_total`, `kafka_consumergroup_lag`); zero patience for tools that hide the data behind walls of UI.

**What she does NOT have**: the patience to fight authentication; the willingness to learn a new query DSL at 03:14; the cognitive surplus to debug Prism itself while debugging the incident.

The persona is grounded in the [Google SRE Workbook chapter on alerting](https://sre.google/workbook/alerting-on-slos/) and the operator interviews implicit in the implementation roadmap (line 439 — operators can route Spark → Aperture → external backend, then "see logs / metrics / traces in Prism").

---

## Primary job — "see the shape of the signal"

### Job statement

> When a paging alert fires for a service I am responsible for, I want to see the shape of the misbehaving signal — its current value, its recent trajectory, and whether it correlates with deploys or other recent changes — so that I can decide within five minutes whether to roll back, scale up, hand the incident to a subject-matter expert, or declare a customer-facing incident.

### Job-as-a-progress framing

The progress Priya is trying to make:

- **From** "an alert text says something is wrong but I cannot see WHAT is wrong, so my next action is guessing."
- **To** "I have looked at the misbehaving signal and at least one correlated signal; I know whether it is a known shape (deploy regression / capacity exhaustion / dependency outage / spurious blip) or a novel shape; my next action is informed."

The key insight: Priya does NOT need a dashboard at 03:14. She needs **one chart of one query against a backend her ops team already trusts**. Dashboards are post-incident learning tools. Incident-time tools are query panels.

### Functional aspect

Render a PromQL query result, interactively edited, as a line chart, against the Mimir or Prometheus instance the operator already runs. Auto-refresh while the operator watches. Time range adjustable. Query syntax errors surfaced inline so she can fix typos without leaving the chart.

### Emotional aspect

- Reduce the cognitive load of operating a query panel during cortisol-soaked stress.
- Provide a calm, monochrome, low-chrome surface — no celebratory animations, no surprise dialogs, no "Did you mean...?" overlays. The interface itself must not become another thing she fights.
- Be **predictable**: the same query, executed twice, produces the same chart. The same time range, kept open during a refresh, holds steady. Trust accumulates by being dull.

### Social aspect

- The chart's URL must be shareable. When Priya screenshots-and-pastes into an incident Slack channel, or pastes the URL into a postmortem doc, the recipient must reach the same chart with the same query and the same time range. (Without this, Prism is a private tool, not a teammate.)
- The query that drove the chart must be visible verbatim above the chart, not buried in a "Show query" toggle. An over-the-shoulder colleague reads it instantly.

### Forces — see [`jtbd-four-forces.md`](jtbd-four-forces.md) for the full Push/Pull/Anxiety/Habit analysis.

---

## Secondary jobs (deferred to post-v0)

These are real operator jobs adjacent to the primary, but cut from v0 scope. They are recorded here so DESIGN does not unwittingly close doors against them.

### SJ1 — "trace through a request"

When a metric anomaly correlates with a slow customer request, I want to see the full trace of one such request, so I can identify the offending span. **Deferred**: TraceQL / Tempo support is a post-v0 slice or a Phase-3+ feature (Ray is the first-party trace engine, planned Phase 5). v0's PromQL focus does not preclude it; the SPA's panel architecture must allow a second panel type (TraceQL) to be added without a rewrite.

### SJ2 — "grep through logs around an event"

When something blew up at 03:14:22, I want to see all logs from the affected service in the surrounding two-minute window, so I can find the stack trace or the noisy log line. **Deferred**: LogQL / Loki support is a post-v0 slice. v0's PromQL focus does not preclude it.

### SJ3 — "save the query for next time"

When I have hand-crafted a useful query during an incident, I want to save it (named, tagged, sharable with my team), so the next on-call engineer does not have to re-derive it at 03:14. **Deferred**: a saved-queries surface is a post-v0 slice. The shareable-URL property of v0 (see Primary above) is a partial substitute — operators can paste URLs into a Slack channel, a runbook, or a wiki page, and that URL is the saved query.

These three secondary jobs are **explicitly listed as deferred** in `wave-decisions.md`. v0 focuses on the primary job only.

---

## Why one job, not three?

Slice-01 elephant-carpaccio discipline says: render one query against one backend at one moment in time. Three jobs in v0 means three integration surfaces (PromQL + LogQL + TraceQL), three query languages, three chart types (line + log-tail + waterfall), and three sources of latency-tail risk on the first frontend feature in the project. The Phase-1 roadmap exit criterion (line 439) names "logs / metrics / traces in Prism", but it does not require all three at v0; it requires the architecture to grow into all three. Picking PromQL/metrics first is the **highest-readiness** choice for the OTel ecosystem (Prometheus' HTTP API is the most stable, best-documented, and most-deployed query interface across Mimir / Prometheus / Grafana Cloud / Thanos / VictoriaMetrics).

This decision is **locked** in the wave brief from Andrea (see `wave-decisions.md` § Inherited decisions). It is not a DISCUSS choice; this section records the rationale for posterity.

---

## Job map (8 steps from Ulwick)

Walk Priya's journey through the primary job to surface edge-case test scenarios:

| Step | What Priya does | What Prism owes her |
|---|---|---|
| **1. Define** | She identifies which signal to investigate from the alert text | The alert names a metric or service; Prism must accept that metric name in a PromQL query without ceremony |
| **2. Locate** | She opens Prism and reaches the query panel | Prism must load fast (under 2 s on a stale browser tab) and present the query input as the first focusable element |
| **3. Prepare** | She types or pastes a PromQL query and picks a time range | Prism must offer a sensible default time range (last 15 min) and remember the previous query in the session if she navigated back |
| **4. Confirm** | She verifies the query is syntactically valid before running | Prism must surface PromQL syntax errors inline, with the parser's actual error message, NOT a generic "query failed" |
| **5. Execute** | She runs the query and waits for the result | Prism must show a loading state within 100 ms and the rendered chart within whatever the backend's own response time allows; under 2 s p95 for typical queries against Mimir |
| **6. Monitor** | She watches the chart, possibly with auto-refresh | Prism must not lose her place; auto-refresh must not jump the time range; the chart must redraw without flicker |
| **7. Modify** | She tweaks the query (try `rate(... [5m])` instead of `[1m]`) and re-runs | Prism must keep the time range stable across query edits and let her edit-and-rerun without losing scroll position |
| **8. Conclude** | She copies the chart URL into Slack and moves to the next step of triage | Prism's URL must encode the query, time range, and backend; a fresh-tab paste of that URL must reproduce the exact view |

These 8 steps are the source of UAT scenarios in `journey-incident-response.feature` and `user-stories.md`. Each step's failure mode (what goes wrong here that we have not tested?) drives at least one Gherkin scenario per the `jtbd-bdd-integration` skill.

---

## Connection to Outcome KPIs

The primary job decomposes into measurable outcomes (full table in [`outcome-kpis.md`](outcome-kpis.md)):

| Job aspect | Outcome KPI | Type |
|---|---|---|
| Functional: render a PromQL chart | Time-to-first-chart on a fresh page load (p95) | Leading (primary) |
| Functional: edit-and-rerun loop | Edit-to-rerun cycle latency (p95) | Leading (primary) |
| Emotional: predictability | Auto-refresh causes zero unexpected time-range jumps | Leading (primary, structural) |
| Social: shareable | URL-roundtrip fidelity (paste URL → identical view) | Leading (primary, structural) |

Every Prism v0 user story traces to one of these outcomes. Stories that do not are flagged for review.

---

## Anti-jobs explicitly NOT in scope

Things operators sometimes ask Prism-shaped tools to do, that v0 explicitly does NOT do:

| Anti-job | Why excluded from v0 |
|---|---|
| Author multi-panel dashboards | Loom (Phase 2) owns dashboards-as-code; Prism v0 is a query panel, not a dashboard editor |
| Define alert rules | Beacon (Phase 2) owns alert rules; Prism is read-only |
| Manage tenants / users / RBAC | Aegis (Phase 2) owns identity; Prism v0 inherits whatever auth the operator's reverse proxy provides |
| Modify or annotate metrics | Pulse (Phase 4) owns the metric engine; Prism is a query consumer, not a metric writer |
| Cross-pillar correlation (metric → trace → flame) | Strata + exemplars (Phase 6); Prism v0 does metrics only |

Surface this list to DESIGN so Morgan does not budget for any of it.
