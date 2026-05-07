# DISCUSS Decisions — `prism-v0`

> **Wave**: DISCUSS.
> **Author**: Luna (`nw-product-owner`); finalised by Bea (orchestrator) after Luna's overload during the user-stories phase.
> **Date**: 2026-05-07.

---

## Key decisions

### [D1] Operator is the primary user; developer is not

Prism is the project's first feature whose primary user is the operator-on-incident-call rather than the developer building services. The whole DISCUSS wave reframes around that persona. Developer-side framing inherited from prior features (Spark, Codex, Sieve, Aperture, harness) does NOT transfer.

Rationale: roadmap line 439 specifies "a user can instrument a service via Spark, route through Aperture, and **see** logs / metrics / traces in Prism" — the user who SEES is operationally distinct from the user who INSTRUMENTS. Treating them as one persona produces UI scoped for log browsing rather than incident triage.

Source: `jtbd-job-stories.md > Persona — the operator on incident call`.

### [D2] PromQL against Prometheus / Mimir is the v0 query language

The roadmap names PromQL, LogQL, and TraceQL as supported query interfaces post-v0. v0 ships PromQL only.

Rationale: PromQL has the most stable HTTP API across the OTel-compatible ecosystem (Prometheus, Mimir, VictoriaMetrics, Grafana Cloud). Operators are most familiar with it. Logs (LogQL) and traces (TraceQL) ride on storage backends Kaleidoscope has not yet built (Lumen, Ray) and depend on protocol surface choices that Phase 3 / Phase 5 own.

Backend reference target for v0: a real Prometheus or Mimir instance (the operator's choice; the SPA does not own its lifecycle).

Source: `journey-incident-response.yaml > integrations`.

### [D3] Walking skeleton is "real backend, not mocked"

Slice 01 ships a real round-trip from the browser to a real Prometheus or Mimir HTTP API. No fixtures, no mock servers, no in-memory stubs.

Rationale: this is the same Strategy C "real local" posture every prior feature used (harness, Aperture's grpc/http listeners, Spark's real Aperture fixture, Sieve's real Aperture, Codex's xtask real upstream constants). The integration risk Prism faces is `Prometheus-JSON-shape × CORS × ECharts-rendering`; landing it at Slice 01 surfaces the surprise early when remediation cost is bounded.

Source: `slice-01-walking-skeleton.md > What it lights up`.

### [D4] AGPL-3.0-or-later for Prism

Prism is operator-facing platform infrastructure, not a developer-facing SDK. Per the LICENSING.md pattern (server-side and operator-facing platform components are AGPL; SDKs are Apache-2.0), Prism is AGPL.

Rationale: Aperture is AGPL because operators run it as a long-lived process on their infrastructure. Prism is the same shape (a long-lived single-page application served from a long-lived web server, run on operator infrastructure). The SaaS loophole AGPL closes is the same loophole a competitor could use to wrap Prism.

Source: `wave-decisions.md` (this file). DESIGN's first ADR records the licence formally.

### [D5] No saved-queries / no shared-dashboards at v0; the URL is the share artefact

Every state-affecting picker change updates `history.replaceState` synchronously. The URL is a complete description of the viewable state. There is no Prism-side persistence (no database, no cookie, no localStorage), and no shared-dashboards surface (no "save this for the team").

Rationale: at v0 the cost of building a saved-queries / shared-dashboards surface is large, the cost of pasting a URL into Slack is zero, and the URL roundtrip is provably correct (Playwright KPI 4). Loom (Phase 2) will own dashboards-as-code; Prism v0 does not encroach.

Source: `user-stories.md > US-PR-04`, `journey-incident-response.yaml > out_of_scope > saved_queries`.

### [D6] Six slices, no more, no less, in named order

The carpaccio slicing is fixed at six slices: 01 walking skeleton, 02 relative time ranges, 03 error / empty states, 04 auto-refresh, 05 absolute time ranges + permalink, 06 accessibility audit.

Rationale: six is the smallest set that delivers the operator's primary job (KPIs 1–5 all green) plus the WCAG 2.2 AA quality bar. Adding a seventh slice (e.g. saved queries, multi-panel) re-opens the v0 scope; removing a slice (e.g. accessibility) defaults Prism to "sighted-mouse-only" which is incompatible with the operator-population reality at production-grade SRE shops.

Source: `story-map.md > Release slices`.

### [D7] Auto-refresh disabled for absolute ranges

When the operator picks an absolute time range, the auto-refresh picker disables itself. The data does not move; auto-refresh would be a no-op that wastes the operator's attention.

Rationale: this is a UX correctness invariant, not a feature trade-off. Operationally it also means "URL-based postmortem reproduction" (Slice 05) lands without a hidden auto-refresh interaction.

Source: `slice-04-auto-refresh.md`, `slice-05-absolute-time-range-and-permalink.md`.

### [D8] Accessibility is a single audit-and-remediate slice, not a per-slice gate

Slice 06 audits Slices 01–05 for WCAG 2.2 AA conformance and remediates anything that fails.

Rationale: WCAG audits are most efficient as a single sweep across a complete UI surface; per-slice gates risk premature lock-in on patterns that need to change once the surface is complete.

Tension: accessibility is a quality bar, not a feature to "add later". The discipline that protects this is two-fold:
- Slice 06 is a non-negotiable v0 slice (NOT a "may slip to post-v0" item).
- Per-slice briefs include the accessibility-aware patterns (focus-visible, keyboard navigation order, semantic ARIA roles) so that Slice 06's audit-and-remediate finds little to remediate.

Source: `slice-06-accessibility-pass.md`.

---

## Requirements summary

- **Primary job**: operator-on-incident-call needs to see the shape of a misbehaving signal fast enough to make a 5–10 min triage decision.
- **Walking skeleton scope**: real PromQL query from the browser → real Prometheus / Mimir HTTP API → ECharts line chart, with URL roundtrip and backend-aware page chrome.
- **Feature type**: user-facing operator UI.
- **Prism v0 boundaries**: PromQL only; URL-based share only; v0 audits (slice 06) WCAG 2.2 AA on the cumulative surface.

---

## Constraints established

- **Backend coupling**: v0 talks to one Prometheus-compatible HTTP API. The backend identity (URL, label, version) is in `/config.json`, served by the same web server that serves the SPA bundle. The SPA does NOT own backend lifecycle, auth, or TLS — those are the operator's reverse-proxy concern (Aegis Phase 2 graduates this when Kaleidoscope-native auth lands).
- **No client-side state persistence**: no cookies, no localStorage, no IndexedDB. The URL is the only state container.
- **No multi-panel dashboards**: one query → one chart. Loom (Phase 2) owns dashboards.
- **Bundle budget**: 300 KB gzipped at v0 (CI gate). ECharts is the largest dependency; everything else fits in the remaining 100 KB.
- **Browser matrix**: latest two stable versions of Chrome, Firefox, Safari. No legacy support.

---

## Upstream changes

None. This is a greenfield feature; no DISCOVER artefact pre-existed for Prism. The roadmap (Phase 1) and the LICENSING.md pattern were the only upstream inputs; both transferred without contradiction.

---

## Recovery note: Luna stalled mid-wave

This is the fifth occurrence of the agent-stall recovery pattern in this project (Morgan twice on Codex; Scholar twice on Codex / Spark; Luna once here). Luna produced through the slice-mapping phase (jtbd, journey, story-map, prioritization, six slice briefs) and overloaded just before the user-stories.md write. Bea finalised user-stories.md, dor-validation.md, outcome-kpis.md, this file, and the SSOT entries.

The methodology absorbs this. The reviewer (`@nw-product-owner-reviewer`) treats Luna's halves and Bea's halves equivalently for review purposes.

---

## Next-wave handoffs

- **DESIGN** (`@nw-solution-architect` Morgan): receives the full DISCUSS artefact set. Architectural choices (Vite vs Next vs Remix; React Router vs file-based routing; ECharts integration shape; SPA-vs-SSR; package manager) land in DESIGN's ADRs, not here.
- **DEVOPS** (`@nw-platform-architect` Apex): receives `outcome-kpis.md` only. Designs the data-collection pipeline, CI fixtures (Vitest + Playwright), browser-matrix runner, and bundle-size gate.

Per the orchestrator's parallel-handoff design, DESIGN and DEVOPS proceed in parallel after the DISCUSS reviewer approves this wave.
