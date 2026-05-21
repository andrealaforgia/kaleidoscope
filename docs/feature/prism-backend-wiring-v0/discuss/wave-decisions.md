# Wave Decisions: prism-backend-wiring-v0 (DISCUSS)

## Configuration (decided, not asked)

| # | Decision | Value |
|---|----------|-------|
| 1 | Feature type | Cross-cutting (React SPA reaching a Rust HTTP backend) |
| 2 | Walking skeleton | No separate skeleton slice; Slice 01 IS the thinnest end-to-end slice |
| 3 | UX research depth | Lightweight |
| 4 | JTBD | No |

## Central design fork (FLAGGED for DESIGN, NOT decided here)

The requirement is solution-neutral: a browser-served Prism must reach a
running query-api and render a real series. Two honest topologies:

### Option 1: Cross-origin + CORS

- Prism served from origin A (a static server); query-api on origin B.
- query-api gains a configurable CORS layer (e.g. tower-http CorsLayer)
  allowing Prism's origin.
- Tradeoffs: keeps frontend and backend decoupled and independently
  deployable; needs CORS configuration; each preflight is an extra round-trip
  on the incident-time path (ADR-0027 Option C rejected cross-origin as the
  default for exactly this reason); an auth proxy in front of Prometheus may
  not propagate preflights.

### Option 2: Same-origin

- One server serves both Prism's static bundle and the /api/v1 routes (query-api
  also serves the static files, or a reverse proxy fronts both).
- Tradeoffs: no CORS needed; one TLS cert, one origin, one log stream (the
  posture ADR-0027 §5 already documents as production default); couples the
  deployment (frontend and backend ship together or behind one proxy).

### Recommendation surface (for DESIGN, not a decision)

ADR-0027 §5 already pins same-origin (reverse proxy) as the production posture
and §C rejects cross-origin as the default. That makes Option 2 the
ADR-aligned default. Option 1 (a configurable CORS allow-origin on query-api)
is named in the brief as the thinnest code change. DESIGN owns the call,
including whether query-api gains a CORS layer regardless (for an operator who
deliberately runs cross-origin).

## Verified contract facts

- config.json shape: `{ backend: { url: string, label: string }, prism: { version: string } }`
  (`apps/prism/src/lib/config/loader.ts` + `types.ts`). Three error arms:
  fetch-failed, parse-failed, shape-failed.
- Path join nuance: `queryRange.ts` `buildUrl` joins `${backend.url}/query_range`,
  NOT `/api/v1/query_range`. query-api serves `/api/v1/query_range`. Therefore
  backend.url MUST include `/api/v1`. (Note: ADR-0027 §5 prose says the request
  URL is `${backend.url}/api/v1/query_range`; the shipped code puts `/api/v1`
  inside backend.url. DESIGN should reconcile the wording.)
- query-api has NO CORS today (verified: no cors/CorsLayer in
  `crates/query-api/src`; `tower` 0.5 is a dev-dependency only).
- tower-http availability: `tower-http` 0.6.8 IS in the workspace `Cargo.lock`
  (pulled in via aperture; ADR-0006), but it is NOT a direct dependency of
  query-api. Adding it to query-api is a one-line Cargo.toml change if DESIGN
  picks the CORS option.
- The Vite dev server already proxies `/api/v1/*` -> `localhost:9090`
  (same-origin in dev). This feature concerns the browser-served (non-dev)
  reachability.
- query-api is fail-closed on tenancy: unresolved tenant -> 401 status:error.
  Test/runtime environments must set `KALEIDOSCOPE_QUERY_TENANT`.

## Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| No DIVERGE artifacts (no `diverge/` directory) | High (certain) | Low | Persona and job grounded directly from the brief; JTBD skipped per Decision 4. Note for traceability only. |
| backend.url misconfigured without /api/v1 -> 404 | Medium | High | Shared-artifacts registry pins the join; Slice 01 end-to-end test asserts a 200 matrix, not a 404 |
| Cross-origin chosen but CORS preflight not handled | Medium | High | If Option 1, query-api must answer OPTIONS preflight + allow-origin; backend test asserts the header |
| Served config.json eyeballed, not loader-validated | Medium | High | DoR + registry require validation against the real loader |

## Honest scope

In: valid config.json at origin root + chosen reachability mechanism + proof the
panel mounts and one series renders end-to-end. Out: auth, TLS, multi-origin
allowlists, deploy orchestration (later/v1).

## Artifacts produced

- `discuss/journey-see-a-metric-visual.md`
- `discuss/journey-see-a-metric.yaml`
- `discuss/story-map.md`
- `discuss/shared-artifacts-registry.md`
- `discuss/outcome-kpis.md`
- `discuss/user-stories.md`
- `discuss/dor-validation.md`
- `discuss/wave-decisions.md`
- `slices/slice-01-see-a-metric-in-a-browser.md`
