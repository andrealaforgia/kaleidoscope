<!-- markdownlint-disable MD013 -->

# Upstream issues — prism-echarts-paint-e2e-v0 (DISTILL)

Two grounded observations surfaced while implementing the specs against the
real app. Neither contradicts the ADR-0075 production scope nor blocks
DELIVER; both are recorded for honesty and for a future slice to weigh.

## 1. Backend label: pseudocode says "dev-local-prom", the app renders "Pulse (durable)"

The original slice-01 pseudocode asserted the chrome shows
`backend: dev-local-prom`. The real `apps/prism/public/config.json` (served
by `pnpm dev`, the e2e webServer) carries
`backend.label = "Pulse (durable)"`, and `QueryPanel` renders
`Backend: {label}` via `[data-testid="backend-label"]`.

**Resolution (no upstream change needed).** I asserted the REAL label
(`/Backend:\s*Pulse \(durable\)/`) so AC-6.1/AC-6.3 match reality and will go
green after DELIVER's scoped change. The AC intent — "the chrome shows the
backend label from `/config.json`" — is honoured; only the placeholder string
in the pseudocode was stale. No requirement changed.

## 2. Prism does not auto-execute the URL query on mount (affects AC-4.2 framing)

The slice-01 URL-roundtrip (AC-4.2) and the fixme'd KPI-1 pseudocode assume
that navigating to `/?q=up&from=-15m&to=now` AUTO-paints a chart on load.
Verified against source, it does not:

- `QueryPanel` has no mount-time `executeQuery`; the query runs only on form
  submit, range change, or an auto-refresh tick.
- The reducer bootstrap dispatches `refresh-changed` with the initial
  interval. With the default `refresh: 'off'`, `reduce` returns `idle` with
  NO `fetch` effect (`reducer.ts:127-134`).

So a fresh tab on that URL pre-fills the query input from the URL but shows
no chart until the operator presses Run/Enter.

**Resolution (no upstream change in this feature).** The URL-roundtrip spec
drives Run explicitly in each tab, then asserts the same series count across
tabs. This keeps the test correct-by-construction: it reds against HEAD on
the paint signal and goes green after DELIVER's paint-signal wiring, WITHOUT
requiring auto-run.

**Observation for a future slice (NOT this feature, NOT ADR-0075 scope).** A
literal "paste the link into Slack, teammate opens it, sees the chart" story
(US-PR-04's spirit) would need auto-run-on-mount when the URL carries a
non-empty `q`. That is a product/behaviour change beyond the paint-signal +
swallow-narrowing scope of ADR-0075 and is deliberately NOT added here.
Recorded so DISCUSS/DESIGN can decide whether a later slice should close the
gap.

## Side note — test substrate hygiene (not a product issue)

During the local RED proof a stale e2e fixture container
(`kaleidoscope-e2e-query-api-1`, up 13 days) was holding host port 9090 and
blocked the prism Prometheus fixture from binding. I removed it to run the
proof. This is an ephemeral test-fixture leak on the dev machine, not a
product or CI concern (CI runners are fresh). Noted only so the cause of the
first run's "port already allocated" is on record.
</content>
