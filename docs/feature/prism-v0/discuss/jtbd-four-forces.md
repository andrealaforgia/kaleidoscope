# JTBD Four Forces — `prism-v0`

> **Wave**: DISCUSS — Phase 1 (JTBD analysis).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-07.
> **Companion documents**: `jtbd-job-stories.md`, `jtbd-opportunity-scores.md`, `journey-incident-response-visual.md`.

The Four Forces (Push / Pull / Anxiety / Habit) decompose Priya's situation when she reaches for a query panel during an incident. The forces inform UAT scenario discovery (per `jtbd-bdd-integration` skill: every demand-reducing force gets at least one anxiety or habit scenario in `journey-incident-response.feature`).

The job statement under analysis (from `jtbd-job-stories.md`):

> When a paging alert fires for a service I am responsible for, I want to see the shape of the misbehaving signal — its current value, its recent trajectory, and whether it correlates with deploys or other recent changes — so that I can decide within five minutes whether to roll back, scale up, hand the incident to a subject-matter expert, or declare a customer-facing incident.

---

## Push — what is making Priya leave her current solution?

What is wrong with the world that makes her pick up a tool right now.

| Push | Strength | Translates to v0 requirement |
|---|---|---|
| **Pager noise**: an alert just fired and customer impact is escalating per second she stalls | High | Time-to-first-chart on fresh page load p95 ≤ 2 s; loading state within 100 ms (KPI 1) |
| **Tab fatigue**: she already has 5 browser tabs open from the last three incidents and cannot find the right one | High | Prism is a SPA at one URL; the URL itself encodes the view (KPI 4 — shareable URL roundtrip) |
| **Context-switch cost**: switching from PagerDuty / Slack / runbook into Grafana, then a different Grafana for a different backend, then back, costs minutes | High | One Prism instance speaks to one backend per deployment; URL-encoded view means a Slack-pasted link replaces the navigation chain |
| **Cognitive surplus is zero**: she will not learn a new tool at 03:14; she will use the most familiar shape and walk away if it fights her | High | UI uses standard PromQL editor patterns (Monaco-style code editor with bracket matching, autocomplete deferred to post-v0); no proprietary DSL |
| **Alert-text-to-query gap**: the alert says `checkout-service p99 latency ≥ 800 ms`; she has to translate that to a PromQL expression | Medium | v0 does NOT auto-translate (that is Beacon's job in Phase 2); v0 makes manual translation as low-friction as possible (paste, edit, run) |

**Strongest Push**: pager noise + zero cognitive surplus. Together they say: every second of UI delay is real customer cost. Prism's first-screen latency budget IS the user-facing contract.

---

## Pull — what is attracting Priya to a new solution?

What she imagines a better tool could give her.

| Pull | Strength | Translates to v0 requirement |
|---|---|---|
| **A query panel that is just a query panel**: no clutter, no upsell, no "Try the new dashboard editor!" sidebar | High | Visual minimalism; one panel; no marketing chrome; no nav drawer in v0 |
| **Trust the same data the alert was computed from**: she knows the alert backend (Mimir); the query panel must read from the SAME backend | High | Prism v0 reads from the SAME backend the operator already runs (decision locked: Mimir / Prometheus PromQL HTTP API) |
| **Predictable**: same query, same time range, same chart, every time | High | Determinism: auto-refresh holds the time range stable; URL roundtrip reproduces the view exactly (KPI 4) |
| **Fast feedback on errors**: a typo in a PromQL query gets surfaced as a parse error in 50 ms, not as "Query failed" after a 3 s round-trip | High | Prism surfaces backend-supplied error messages verbatim, inline next to the query input (US-PR-04) |
| **Shareable**: paste the URL into Slack and a colleague reaches the same view | High | URL-encoded query + range + backend; KPI 4 |
| **Calm**: no spinning logos, no animations, no cheerful microcopy | Medium-high | Tone of voice: neutral, present-tense, instructions in 1-2 sentences; no celebratory toasts; loading states are skeletons not spinners |

**Strongest Pull**: predictability + same-backend-as-the-alert. Together they make Prism trustworthy in a high-stakes moment: the data she sees IS the data the alert fired on.

---

## Anxiety — what is making Priya hesitate to switch?

Fears about adopting Prism that could keep her in Grafana / direct-Prometheus-curl / nothing.

| Anxiety | Strength | Translates to v0 requirement (and to UAT scenario) |
|---|---|---|
| **"What if Prism shows me different data than Grafana / curl?"** | High | Prism uses the official PromQL HTTP API (`/api/v1/query`, `/api/v1/query_range`) without any pre-processing; the same query against Mimir from `curl` and from Prism returns the same data — UAT scenario "Prism's PromQL result matches a curl-issued reference query" (US-PR-03) |
| **"What if the auto-refresh silently drops a sample / smooths the line?"** | High | Auto-refresh re-issues the same `query_range` request; no client-side aggregation, no client-side smoothing — UAT scenario "Auto-refresh re-issues the same query without smoothing" (US-PR-05) |
| **"What if the SPA crashes on a malformed query and I lose my session?"** | High | Errors are caught and rendered as inline messages, not as JS exceptions; the page never goes blank — UAT scenario "Malformed query renders error inline, page stays usable" (US-PR-04) |
| **"What if Prism caches a stale result and lies to me?"** | High | No client-side query result cache in v0; every "Run" or auto-refresh tick fetches fresh — UAT scenario "Pressing Run always fetches fresh data" (US-PR-05); also a hard guardrail in `outcome-kpis.md` |
| **"What if I cannot reproduce what I saw five minutes ago for a postmortem?"** | High | URL roundtrip preserves query + time range + backend; the URL IS the bookmark — UAT scenario "URL roundtrip reproduces the exact view" (US-PR-04 / KPI 4) |
| **"What if I have to learn a new query language at 03:14?"** | Medium | Prism uses PromQL exactly as the backend implements it; no Prism-specific extensions, no custom functions — documentation: link directly to Prometheus' PromQL docs |
| **"What if Prism is slower than `curl` would be?"** | Medium | Time-to-first-chart p95 ≤ 2 s for a typical 15-min `rate(metric[5m])` query — KPI 1 |

**Strongest Anxieties**: data-fidelity worries (different / smoothed / stale data) and session-loss worries (crash / lost view). Each maps to at least one UAT scenario per `jtbd-bdd-integration`'s anxiety-path pattern.

---

## Habit — what existing workflow resists adoption?

What Priya is doing today that has the inertia of muscle memory.

| Habit | Strength | Translates to v0 requirement |
|---|---|---|
| **Open Grafana, click "Explore", paste query** | Very high | Prism's first-load state IS the Grafana-Explore equivalent: query input focused, time range visible, "Run" button next to it |
| **Use PromQL** | Very high | v0 IS PromQL; no DSL translation, no custom syntax |
| **Adjust time range with a relative-time picker (last 5 min, 15 min, 1 h, 6 h, 24 h)** | Very high | Time-range picker offers exactly these relative presets, plus a custom absolute range for postmortems (US-PR-02) |
| **Auto-refresh every 5 s during an active incident** | High | Auto-refresh interval picker with 5 s / 10 s / 30 s / 1 min / off (US-PR-05) |
| **Read the legend to identify which line is which series** | High | Chart legend is visible by default, names each series by its labels (e.g. `instance="checkout-1"`, `method="POST"`) (US-PR-03) |
| **Reach for the URL bar to copy the view** | High | URL is permalink-shaped; query + range + backend encoded as URL params (KPI 4, US-PR-04) |
| **Read PromQL parse errors to fix typos** | High | Backend-supplied error message is rendered verbatim, no rewriting (US-PR-04) |

**Strongest Habit**: the Grafana-Explore-style query panel. Prism v0 must feel familiar to anyone who has used Explore — query input on top, time range to the right, chart below, legend below the chart. This is not a rip-off — this is the OTel ecosystem's lingua franca for incident-time query panels, and material honesty (a query panel should feel like a query panel) tracks the existing convention.

---

## Force balance summary

```
       Push (toward Prism)                Anxiety (back to current)
  +-----------------------+         +-----------------------+
  | pager noise [HIGH]    |    vs.  | data fidelity [HIGH]  |
  | tab fatigue [HIGH]    |         | session loss [HIGH]   |
  | context cost [HIGH]   |         | learning curve [MED]  |
  | zero cognitive room   |         | speed regression [MED]|
  +-----------------------+         +-----------------------+
              |                                  |
              v                                  v
       Pull (toward Prism)                 Habit (current)
  +-----------------------+         +-----------------------+
  | predictable [HIGH]    |    vs.  | Grafana Explore [V.HIGH] |
  | same backend [HIGH]   |         | PromQL muscle memory  |
  | shareable URL [HIGH]  |         | URL-bar copy reflex   |
  | fast errors [HIGH]    |         | relative-time presets |
  | calm chrome [MED-HIGH]|         | legend reading        |
  +-----------------------+         +-----------------------+
```

**Net force**: Pull + Push are very strong; Anxiety is high and concentrated on data-fidelity (which v0's design must defeat with deterministic, transparent behaviour); Habit is high and **aligned** with v0's chosen patterns (the Grafana-Explore-shaped panel is what v0 ships, not what v0 fights). The job is gettable for v0 if and only if anxiety is defused by the UAT scenarios this analysis surfaces.

---

## UAT scenarios surfaced from this analysis

Every demand-reducing force (Anxiety + Habit) gets at least one Gherkin scenario, per the `jtbd-bdd-integration` skill's force-to-scenario mapping. Mapping, with story IDs:

| Force | Scenario name | Story |
|---|---|---|
| Anxiety: data fidelity | `Prism's PromQL result matches a reference curl query` | US-PR-03 |
| Anxiety: smoothing | `Auto-refresh re-issues the same query without client-side smoothing` | US-PR-05 |
| Anxiety: SPA crash on bad query | `Malformed query renders error inline, page stays usable` | US-PR-04 |
| Anxiety: stale cache | `Pressing Run always fetches fresh data, never a cached result` | US-PR-05 |
| Anxiety: irreproducibility | `URL roundtrip reproduces the exact view (query + range + backend)` | US-PR-04 |
| Habit: Grafana-Explore shape | `Query input focused on first load; Enter runs the query` | US-PR-02 |
| Habit: PromQL fidelity | `Prism does not rewrite or extend the PromQL the user typed` | US-PR-03 |
| Habit: relative-time presets | `Time-range picker offers 5 min / 15 min / 1 h / 6 h / 24 h` | US-PR-02 |
| Habit: auto-refresh interval | `Auto-refresh interval picker offers 5 s / 10 s / 30 s / 1 min / off` | US-PR-05 |

These nine force-derived scenarios join the happy-path scenarios (one per story) and the job-map-edge-case scenarios (one per cut of the 8-step job map) to form the full UAT set in `journey-incident-response.feature` and the embedded scenarios in `user-stories.md`.

---

## Implications for DESIGN (Morgan)

The forces, especially data-fidelity anxiety, constrain Morgan's technology choices:

1. **No client-side smoothing or aggregation.** The chart library (Apache ECharts, locked) must be configured to render the raw points the backend returns, with no interpolation, no LOESS smoothing, no auto-downsampling. ECharts has a `large` rendering mode for many points; the dataset is whatever Prometheus' `query_range` returned and no more.
2. **No client-side query result cache.** Every "Run" or auto-refresh issues a fresh HTTP request. (Browser HTTP cache is fine — it's deterministic and respects the backend's `Cache-Control`. But no JS-level memoisation.)
3. **PromQL passes through unmodified.** Prism does NOT parse, rewrite, or extend PromQL. The string the user types IS the string sent to `/api/v1/query_range`. Any parse error comes from Prometheus, with Prometheus's error text. Prism does not implement its own PromQL parser at v0.
4. **Backend-supplied error messages are rendered verbatim.** No Prism-side rewriting, no friendly translations. The operator trusts what the backend says.

These are contract-level constraints. DESIGN locks the technology and module structure but cannot change these contracts without amending DISCUSS.
