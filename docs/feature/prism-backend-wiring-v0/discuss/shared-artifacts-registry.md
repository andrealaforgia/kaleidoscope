# Shared Artifacts Registry: prism-backend-wiring-v0

Every value that crosses the Prism / query-api boundary, its single source of
truth, and its consumers. Untracked artifacts here are the integration risks.

## config.json

```yaml
shared_artifacts:
  config_json:
    source_of_truth: "served at Prism's own origin root as /config.json (serving mechanism chosen in DESIGN)"
    shape: "{ backend: { url: string, label: string }, prism: { version: string } }"
    consumers:
      - "apps/prism/src/lib/config/loader.ts — isRuntimeConfig guard + three error arms"
      - "apps/prism/src/app/App.tsx — gates QueryPanel mount on result.kind === 'ok'"
      - "apps/prism/src/panels/query/QueryPanel.tsx — renders backend.label and prism.version"
    owner: "prism-backend-wiring-v0 (the config asset is this feature's deliverable)"
    integration_risk: "HIGH — a config.json the loader rejects keeps the panel dark; must validate against the REAL loader, not an eyeballed shape"
    validation: "served config.json passes apps/prism/src/lib/config/loader.ts isRuntimeConfig with the production fetch path"
```

## backend.url and the query_range path join

```yaml
shared_artifacts:
  backend_url:
    source_of_truth: "config.json -> backend.url"
    consumers:
      - "apps/prism/src/lib/promql/queryRange.ts buildUrl -> `${backend.url}/query_range`"
    owner: "prism-backend-wiring-v0"
    integration_risk: >
      HIGH — buildUrl joins `${backend.url}/query_range` (NOT /api/v1/query_range).
      query-api serves /api/v1/query_range (crates/query-api/src/lib.rs
      QUERY_RANGE_ROUTE). Therefore backend.url MUST carry the /api/v1 segment:
      cross-origin "http://host:9090/api/v1", or same-origin "/api/v1" behind a
      proxy (the Vite dev proxy already maps /api/v1 -> :9090). Omitting /api/v1
      yields a 404 -> transport-error http-status.
    validation: "backend.url + '/query_range' === query-api's /api/v1/query_range, asserted by the slice's end-to-end test"

  query_range_route:
    source_of_truth: "crates/query-api/src/lib.rs QUERY_RANGE_ROUTE = \"/api/v1/query_range\""
    consumers:
      - "crates/query-api/src/lib.rs router registration"
      - "apps/prism/src/lib/promql/queryRange.ts buildUrl target"
    owner: "query-api (existing; this feature does not move the route)"
    integration_risk: "HIGH — name/path mismatch makes every query 404"
    validation: "the slice's end-to-end test issues the real path and gets a 200 matrix"
```

## Browser reachability (CORS allow-origin OR same-origin)

```yaml
shared_artifacts:
  browser_reachability:
    source_of_truth: "DESIGN-chosen mechanism (query-api CORS allow-origin config, OR same-origin serving)"
    consumers:
      - "the browser's same-origin policy (enforces it)"
      - "crates/query-api (must answer CORS preflight + allow-origin if cross-origin)"
      - "apps/prism queryRange fetch (the request that succeeds or is blocked)"
    owner: "prism-backend-wiring-v0 (this is the second half of the deliverable)"
    integration_risk: >
      HIGH — query-api has NO CORS today (verified: no cors/CorsLayer in
      crates/query-api/src; tower is a dev-dependency only). A cross-origin
      browser fetch is blocked unless query-api gains a configurable
      Access-Control-Allow-Origin layer, OR both are served same-origin.
    validation: >
      cross-origin posture: query-api responds with the allow-origin header for
      Prism's configured origin (backend-side test asserts the header is
      present). same-origin posture: a single server serves both / and /api/v1.
```

## Prometheus matrix JSON (already pinned, no change here)

```yaml
shared_artifacts:
  prometheus_matrix_json:
    source_of_truth: "crates/query-api/src/lib.rs success_response — { status:'success', data:{ resultType:'matrix', result:[...] } }"
    consumers:
      - "apps/prism/src/lib/promql/queryRange.ts parseSeries"
      - "apps/prism/src/panels/query/QueryPanel.tsx chart + table + footer"
    owner: "query-api + prism (contract pinned by ADR-0027 and ADR-0042)"
    integration_risk: "LOW for this feature — the shape is already live and tested; this feature only makes it reachable from a browser"
    validation: "existing query-api slice_01 test + prism queryRange unit tests"
```

## Integration checkpoints

1. Served config.json passes Prism's own loader (not a hand-checked shape).
2. backend.url + "/query_range" resolves to query-api's /api/v1/query_range.
3. A browser-served Prism reaches query-api: CORS allow-origin present
   (cross-origin) OR single origin (same-origin). This is the central
   DESIGN fork.
