# JTBD Opportunity Scores — `prism-v0`

> **Wave**: DISCUSS — Phase 1 (JTBD analysis).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-07.
> **Companion documents**: `jtbd-job-stories.md`, `jtbd-four-forces.md`.

Ulwick's opportunity-score formula is `Importance + max(0, Importance − Satisfaction)`. The output is a 0–20 ranking number; jobs scoring ≥ 12 are "underserved" and worth prioritising; scores 10–12 are "right-served"; under 10 are "overserved" (already handled well by existing tools).

For Prism v0 the analysis is **single-job**: one primary job is in scope, three secondary jobs are explicitly deferred (see [`jtbd-job-stories.md`](jtbd-job-stories.md)). No multi-job comparison is needed to drive prioritisation — the prioritisation is already locked by the wave brief. This file records the underlying ratings so that:

1. DESIGN (Morgan) can see the importance-vs-satisfaction frame Luna used.
2. Post-v0 wave planning (Phase 1 Slice 09+ or a Prism v1 feature) has a baseline for the secondary jobs to be re-scored against measured operator data.

---

## Scoring scale

The 1–10 scale used here, applied as Ulwick prescribes:

| Score | Importance interpretation | Satisfaction interpretation |
|---|---|---|
| 9–10 | Job is **critical** to the operator's work; failure to do it well means missed SLOs / customer impact | Existing tools do it **excellently**; would be hard to improve |
| 7–8 | Job is **important**; doing it badly is a measurable productivity loss | Existing tools do it **well**; minor improvements possible |
| 5–6 | Job matters but is not load-bearing | Existing tools do it **adequately**; gaps are visible but not painful |
| 3–4 | Job is occasional / niche | Existing tools do it **poorly**; clear room for improvement |
| 1–2 | Job is rare or low-value | Existing tools **barely** do it / not at all |

Ratings are based on Andrea's domain knowledge of operators-on-incident-call, the implicit operator persona in the implementation roadmap, and the SRE-workbook-shaped framing of incident response. They are **not** survey data; the survey data does not exist yet. Post-v0 the ratings should be re-measured by interviewing pilot operators (cross-reference: `outcome-kpis.md` KPI 6 — the survey-shaped KPI).

---

## Primary job (in v0 scope)

### "See the shape of the misbehaving signal during an incident"

| Aspect | Rating | Reasoning |
|---|---|---|
| **Importance** | 9 | Incident-time triage is the canonical SRE-on-call task. Doing it badly costs MTTR, customer trust, and SLO budget. The whole rationale of the integration plane (Aperture → backend → Prism) hinges on this being a fast, calm experience |
| **Satisfaction with current tools** | 5 | Operators today use Grafana, Datadog, or direct curl. Each has tradeoffs: Grafana is fast but is opinionated about dashboards (incident-time UI is a workaround); Datadog is fast but vendor-locked and expensive; curl is honest but cannot draw a chart. None of the three is perfectly fit for "single-query single-chart at 03:14 against MY backend". |
| **Opportunity score** | 9 + max(0, 9 − 5) = **13** | Underserved → prioritise. Confirms the wave brief's prioritisation. |

A score of 13 puts this job firmly in the "underserved" bucket per Ulwick. It is the right place to invest a Phase-1 deliverable.

---

## Secondary jobs (explicitly deferred from v0)

### SJ1 — "Trace through a request"

| Aspect | Rating | Reasoning |
|---|---|---|
| **Importance** | 7 | Important during specific incident shapes (latency anomalies, error attribution); not every incident needs traces |
| **Satisfaction with current tools** | 6 | Tempo + Grafana traces are decent; Jaeger UI works; the gap is mainly cross-pillar correlation (metric → trace → flame), not the trace view itself |
| **Opportunity score** | 7 + max(0, 7 − 6) = **8** | Not underserved enough to crowd v0; revisit in Phase 5 (Ray) |

### SJ2 — "Grep through logs around an event"

| Aspect | Rating | Reasoning |
|---|---|---|
| **Importance** | 8 | High during error-shaped incidents (stack traces, panics, exception text live in logs); medium during latency-shaped incidents |
| **Satisfaction with current tools** | 7 | Loki + Grafana log search is very good; Datadog logs is excellent; Elasticsearch + Kibana is the legacy excellence. The tooling here is mature |
| **Opportunity score** | 8 + max(0, 8 − 7) = **9** | Right-served; not the place to spend a Phase-1 budget |

### SJ3 — "Save the query for next time"

| Aspect | Rating | Reasoning |
|---|---|---|
| **Importance** | 6 | Quality-of-life across incidents; not load-bearing during a single incident |
| **Satisfaction with current tools** | 4 | Grafana saved queries are fine but per-instance; Datadog saved views work but lock you in; the cross-team cross-tool sharing is the gap |
| **Opportunity score** | 6 + max(0, 6 − 4) = **8** | Right-served by URL-paste-into-Slack at v0; revisit in Phase 1 Slice 09+ if pilot operators ask for it |

---

## Score table summary

| Job | Importance | Satisfaction | Opportunity | Bucket | v0 status |
|---|---|---|---|---|---|
| Primary: see signal shape | 9 | 5 | **13** | Underserved | **In scope** |
| SJ1: trace a request | 7 | 6 | 8 | Right-served | Deferred (Phase 5 — Ray) |
| SJ2: grep logs around event | 8 | 7 | 9 | Right-served | Deferred (post-v0 Prism slice) |
| SJ3: save query for next time | 6 | 4 | 8 | Right-served | Deferred — URL paste partial substitute |

The 13-vs-9 gap between the primary job and the highest secondary is enough to justify v0 focusing exclusively on the primary. If the gap were ≤ 2 the case for fanning out at v0 would be stronger; at 4 points apart, focus is the right call.

---

## Re-measurement plan

After Phase 1 ships, the scores above should be re-derived from operator interviews. Specifically:

- **30 days post-launch**: interview the pilot operators (≥ 3 per `outcome-kpis.md` KPI 6). Ask them to rate, on the 1–10 scale, both Importance and Satisfaction for each of the four jobs above.
- **Recompute the opportunity scores** using their numbers, not Luna's estimates.
- **Re-prioritise post-v0 Prism slices** based on the measured scores. If SJ2 (logs) jumps to a 12+ in measured operator data, it becomes the next slice. If SJ1 (traces) jumps, it becomes the next slice. If neither jumps, the secondary jobs stay deferred and v0's bets stand validated.

This re-measurement is itself listed as a post-launch DEVOPS deliverable in `outcome-kpis.md`.
