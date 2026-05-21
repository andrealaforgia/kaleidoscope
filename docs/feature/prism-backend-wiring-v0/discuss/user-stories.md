<!-- markdownlint-disable MD024 -->

# User Stories: prism-backend-wiring-v0

## System Constraints

- British English throughout; no em dashes.
- Solution-neutral: the requirement is "a browser-served Prism reaches a
  running query-api and plots a real series". The cross-origin mechanism
  (CORS allow-origin vs same-origin serving) is the central DESIGN fork and is
  NOT decided in DISCUSS (see `wave-decisions.md`).
- The served config.json MUST validate against Prism's own loader
  (`apps/prism/src/lib/config/loader.ts`), not an eyeballed shape. The loader
  has three error arms (fetch-failed, parse-failed, shape-failed); any one
  keeps the QueryPanel unmounted.
- `backend.url` MUST carry the `/api/v1` segment: `buildUrl` in
  `queryRange.ts` joins `${backend.url}/query_range`, and query-api serves
  `/api/v1/query_range`.
- query-api has NO CORS today (verified). `tower` is a dev-dependency only;
  `tower-http` is present in the workspace Cargo.lock (pulled by aperture) but
  is not a direct dependency of query-api.
- Out of scope at v0: auth, TLS, multi-origin allowlists, full deploy
  orchestration. These are later/v1.
- These two stories form one slice (Slice 01); they ship together. Neither
  delivers operator value alone (config without reach, or reach without config,
  both leave the panel useless).

---

## US-01: The QueryPanel mounts against a valid served config.json

### Elevator Pitch

- **Before**: Priya opens Prism in a browser and sees "Backend:
  (unconfigured)" with a "Configuration is missing" banner. No query input, no
  chart. The read loop is green in CI but invisible.
- **After**: Priya opens Prism's URL in a browser (the operator-facing entry
  point) and sees the chrome read "Backend: Pulse (durable)", "Prism v0.1.0",
  and a focused PromQL query input with a Run button.
- **Decision enabled**: Priya can decide which metric to query, because the
  panel is now live instead of dark.

### Problem

Priya is an on-call SRE who has just been paged. She knows ingest -> store ->
query works because CI is green, but she has never seen a metric in a browser:
Prism deliberately refuses to mount its QueryPanel because it cannot load a
valid `/config.json`. She finds it useless to "trust the loop is fine" when she
cannot see a single series during an incident.

### Who

- On-call SRE (Priya) | incident-time, in a browser | needs to see signal shape
  fast, with a backend she can trust the URL of.

### Solution

Serve a valid `config.json` at Prism's origin root that validates against
Prism's own loader, so the App composition root mounts the QueryPanel and
renders the real backend label and Prism version.

### Domain Examples

#### 1: Happy Path — valid config.json mounts the panel

Priya opens Prism. A served `config.json` declares
`{ backend: { url: "http://obs.internal:9090/api/v1", label: "Pulse (durable)" }, prism: { version: "0.1.0" } }`.
The loader returns `{ kind: 'ok' }`. The chrome reads "Backend: Pulse
(durable)", "Prism v0.1.0"; the query input takes focus.

#### 2: Edge Case — missing field keeps the panel dark

A served `config.json` is `{ backend: { url: "http://obs.internal:9090/api/v1" }, prism: { version: "0.1.0" } }`
(no `label`). The loader's `isRuntimeConfig` guard returns false; the result is
`shape-failed`. The QueryPanel stays unmounted and the "Configuration is
missing" banner names `shape-failed`. This is correct refusal, not a bug.

#### 3: Error/Boundary — no config.json served at all

No `config.json` is served at the origin root. `loadConfig` fetches
`/config.json`, gets HTTP 404, returns `fetch-failed`. The panel shows
"Backend: (unconfigured)" with the banner naming `fetch-failed: HTTP 404 Not
Found`. The panel must NOT silently mount.

### UAT Scenarios (BDD)

#### Scenario: The query panel goes live when configuration is valid

```gherkin
Given Prism is served with a config.json declaring backend label "Pulse (durable)" and prism version "0.1.0"
And the served config.json validates against Prism's own loader
When Priya opens Prism in a browser
Then the backend label reads "Pulse (durable)" instead of "(unconfigured)"
And the Prism version reads "0.1.0"
And the PromQL query input is present and focused
```

#### Scenario: A config missing a required field keeps the panel dark

```gherkin
Given Prism is served with a config.json whose backend object has no "label" field
When Priya opens Prism in a browser
Then the QueryPanel does not mount
And a "Configuration is missing" banner names the shape-failed reason
```

#### Scenario: A missing config.json keeps the panel dark

```gherkin
Given no config.json is served at Prism's origin root
When Priya opens Prism in a browser
Then the QueryPanel does not mount
And a "Configuration is missing" banner names the fetch-failed reason
```

### Acceptance Criteria

- [ ] A served config.json that validates against Prism's own loader causes the
  QueryPanel to mount with the real backend label and version (Scenario 1).
- [ ] A config.json missing a required field keeps the QueryPanel unmounted and
  shows the shape-failed banner (Scenario 2).
- [ ] A missing config.json keeps the QueryPanel unmounted and shows the
  fetch-failed banner (Scenario 3).

### Outcome KPIs

- **Who**: a browser-served Prism instance
- **Does what**: mounts the QueryPanel against a valid served config.json
- **By how much**: 100% of runs mount when config is valid; config validates
  against Prism's own loader
- **Measured by**: Prism Vitest/Playwright suite asserting mount + backend label
- **Baseline**: 0% today (no config.json served; always "(unconfigured)")

### Technical Notes

- Loader contract: `apps/prism/src/lib/config/loader.ts`, types in
  `types.ts`. Three error arms must continue to keep the panel dark.
- The config asset must be served at the origin root path `/config.json`
  (where `loadConfig` fetches). Serving mechanism is a DESIGN concern.
- `public/config.json.example` is referenced in ADR-0026 as a shape-only
  exemplar; the operator overrides it. This feature provides a real one.

---

## US-02: A browser-served Prism reaches query-api and plots a real series

### Elevator Pitch

- **Before**: Even with the panel mounted, Priya types "up", presses Run, and
  sees "Cannot reach Pulse (durable) — transport failure: network": the
  browser blocks the cross-origin fetch (query-api has no CORS), or the path
  does not resolve.
- **After**: Priya enters "up" in the mounted QueryPanel and presses Run; a
  series is plotted in the chart area and the footer reads "1 series • 61
  points • 7 ms", served from query-api over the durable Pulse store.
- **Decision enabled**: Priya can read the shape of the signal and decide
  whether the metric is healthy, because the round-trip to the durable store
  now completes in a browser.

### Problem

Priya has a mounted QueryPanel but a browser-served Prism calling query-api on
another origin is blocked by the same-origin policy (query-api has no CORS
today), and the path join must resolve correctly. She finds it maddening to be
one fetch away from seeing the metric and still see only a transport error.

### Who

- On-call SRE (Priya) | incident-time, in a browser | needs the query to
  actually reach query-api and return a real series from Pulse.

### Solution

Give a browser-served Prism an honest way to reach query-api — either query-api
gains a configurable CORS allow-origin for Prism's origin, or both are served
same-origin (DESIGN fork) — and ensure `backend.url` carries `/api/v1` so the
path resolves to query-api's `/api/v1/query_range`.

### Domain Examples

#### 1: Happy Path — a real series renders end-to-end

Priya's Prism is configured with `backend.url = "http://obs.internal:9090/api/v1"`
and query-api answers CORS for Prism's origin (or both are same-origin). She
types "up" and presses Run. `buildUrl` issues
`http://obs.internal:9090/api/v1/query_range?query=up&...`; query-api reads
Pulse and returns a 200 matrix with the `up` series. The chart plots one
series; the footer reads "1 series • 61 points • 7 ms".

#### 2: Edge Case — empty result is a calm message, not an error

Priya types `nonexistent_metric` over the last 15 minutes. query-api returns
`200 { status:'success', data:{ result:[] } }`. The panel shows "No data for
last 15 minutes. Check the metric name or widen the range." This is the empty
arm, not a transport error.

#### 3: Error/Boundary — cross-origin reach blocked without the mechanism

Priya's Prism is on origin `https://prism.internal` and `backend.url` points at
`https://obs.internal:9090/api/v1`, but query-api answers no CORS allow-origin.
The browser blocks the fetch; `queryRange` returns `transport-error` with cause
`network`. The panel stays usable and shows "Cannot reach Pulse (durable)".
Removing this arm IS the feature.

### UAT Scenarios (BDD)

#### Scenario: A real series renders end-to-end from the durable store

```gherkin
Given the QueryPanel is mounted against a running query-api over Pulse
And the served config.json sets backend.url to include the /api/v1 segment
And the metric "up" has samples in the durable Pulse store
When Priya enters "up" and presses Run
Then a series is plotted in the chart area
And the footer reports the series count, point count, and query latency
```

#### Scenario: The browser fetch to query-api succeeds across origins

```gherkin
Given a browser-served Prism reaches query-api with the reachability mechanism in place
When Priya runs a query
Then the browser's fetch to query-api is not blocked by the same-origin policy
And query-api answers (with the allow-origin header for Prism's origin if cross-origin, or same-origin)
```

#### Scenario: The query path resolves to query-api's route, not a 404

```gherkin
Given backend.url carries the /api/v1 segment
When Prism issues the query_range request
Then the request targets query-api's /api/v1/query_range route
And query-api returns a 200 matrix rather than a 404
```

#### Scenario: An empty result is shown calmly, not as an error

```gherkin
Given the QueryPanel is mounted and reachable
When Priya queries a metric with no samples in the selected range
Then the panel shows a calm "No data" empty state
And no transport-error or parse-error banner is shown
```

### Acceptance Criteria

- [ ] A browser-served Prism, with the reachability mechanism in place and
  backend.url carrying /api/v1, plots a real series end-to-end and the footer
  reports counts and latency (Scenario 1).
- [ ] The browser's cross-origin (or same-origin) fetch to query-api is not
  blocked; the reachability is asserted on the query-api side (allow-origin
  header present, or single origin) (Scenario 2).
- [ ] The query path resolves to /api/v1/query_range and returns a 200 matrix,
  not a 404 (Scenario 3).
- [ ] A metric with no samples renders the calm empty state, not an error
  banner (Scenario 4).

### Outcome KPIs

- **Who**: a browser-served Prism instance
- **Does what**: completes a cross-origin (or same-origin) query_range against
  query-api and renders a real series
- **By how much**: 100% of end-to-end runs render >=1 series; reachability
  header present (cross-origin) or single origin (same-origin)
- **Measured by**: Prism end-to-end test + query-api backend test asserting
  reachability
- **Baseline**: 0% today (browser reach blocked / untested)

### Technical Notes

- Path join: `apps/prism/src/lib/promql/queryRange.ts` `buildUrl` joins
  `${backend.url}/query_range`; query-api serves `/api/v1/query_range`
  (`crates/query-api/src/lib.rs` `QUERY_RANGE_ROUTE`). backend.url MUST carry
  `/api/v1`.
- query-api has NO CORS today. If DESIGN picks cross-origin, query-api needs a
  configurable allow-origin layer (e.g. tower-http CorsLayer; tower-http is in
  the workspace lock but not a direct query-api dependency). If DESIGN picks
  same-origin, one server serves both / and /api/v1.
- query-api is fail-closed on tenancy: an unresolved tenant returns 401
  status:error -> Prism renders a parse-error banner. The test environment must
  set KALEIDOSCOPE_QUERY_TENANT.
- The Vite dev proxy already maps /api/v1 -> :9090 (same-origin in dev). This
  feature is about the browser-served (non-dev) reachability.
- Header redaction invariant (ADR-0027 §6) must not regress.

### Dependencies

- query-api `/api/v1/query_range` over Pulse (live, tested — `crates/query-api`,
  ADR-0042).
- Prism config loader and QueryPanel (live — ADR-0026, ADR-0027, ADR-0030).
- US-02 depends on US-01 (the panel must mount before a query can run). Both
  ship in Slice 01.
- No DIVERGE artifacts exist for this feature (no `diverge/` directory). Risk
  noted in `wave-decisions.md`; persona grounded directly from the brief.
