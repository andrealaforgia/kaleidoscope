# ADR-0043 — Prism backend-wiring topology and the `backend.url` `/api/v1` reconciliation

- **Status**: Accepted
- **Date**: 2026-05-21
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `prism-backend-wiring-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: refines ADR-0027 §5 (Prism backend HTTP client + CORS posture —
  the `backend.url` prose drift corrected here); ADR-0042 (query-api contract +
  `/api/v1/query_range` route); ADR-0026 / ADR-0030 (Prism config loader);
  ADR-0006 (axum + hyper transport stack; tower-http in the workspace lock)

## Context

The ingest -> store -> query loop is green in CI, but the read loop is invisible
in a browser. Two facts block it:

1. Prism refuses to mount its QueryPanel because no `/config.json` is served
   (`apps/prism/src/lib/config/loader.ts` returns `fetch-failed` on HTTP 404,
   and the App composition root keeps the panel dark).
2. A browser-served Prism calling query-api on another origin is blocked by the
   same-origin policy: query-api has NO CORS today (verified — no
   `cors`/`CorsLayer` in `crates/query-api/src`).

A browser-served Prism reaching query-api admits exactly two honest topologies
(flagged for DESIGN in the feature's DISCUSS `wave-decisions.md`):

- **Cross-origin + CORS**: Prism on origin A, query-api on origin B with a
  configurable `tower-http` `CorsLayer` allowing Prism's origin; the browser
  issues a preflight `OPTIONS` then the real request.
- **Same-origin**: one server serves both Prism's static bundle and the
  `/api/v1` routes from one origin; no CORS.

Two contract facts are pinned and authoritative (the shipped code, not the
prose):

- config.json shape `{ backend: { url, label }, prism: { version } }`, validated
  by Prism's own `isRuntimeConfig`.
- `apps/prism/src/lib/promql/queryRange.ts` `buildUrl` joins
  `${backend.url}/query_range`, NOT `${backend.url}/api/v1/query_range`.
  query-api serves `/api/v1/query_range` (`QUERY_RANGE_ROUTE`). Therefore
  `backend.url` MUST itself carry the `/api/v1` segment. **ADR-0027 §5's prose
  reads `${backend.url}/api/v1/query_range`, which contradicts the shipped
  `buildUrl`.** This ADR reconciles that drift in favour of the shipped code.

This ADR locks: the slice-01 topology, where `/config.json` lives and is served,
the `backend.url` `/api/v1` reconciliation, and the minimal query-api change.

## Decision

### 1. Topology for slice 01: same-origin via `tower-http` `ServeDir`

query-api optionally serves Prism's built `dist/` (including `/config.json` and
`index.html`) from the same origin as `/api/v1/*`, using a `tower-http`
`ServeDir` fallback wired behind a config knob. A browser-served Prism and
query-api share one origin, so the same-origin policy is satisfied with **zero
CORS, zero preflight, zero `Access-Control-Allow-Origin`**.

This is the ADR-0027 §5 posture (one cert, one origin, one log stream) realised
in a single binary for the demo/dev path. The Vite dev proxy (`/api/v1 -> :9090`)
is untouched and remains the developer's same-origin path.

### 2. The `/api/v1` reconciliation (refines ADR-0027 §5)

The shipped code is authoritative. `buildUrl` appends only `/query_range`, so
**`backend.url` carries the `/api/v1` segment**. ADR-0027 §5's prose
(`${backend.url}/api/v1/query_range`) is corrected to read: the request URL is
`${backend.url}/query_range` where `backend.url` already includes `/api/v1`.
ADR-0027's client surface, `QueryOutcome` union, header-redaction invariant (§6),
and dev/prod CORS posture (§5 / §C) all stand unchanged; this is a wording
clarification, not a supersession.

### 3. config.json: a committed static file Vite copies into `dist/`

`/config.json` is a committed file at `apps/prism/public/config.json`. Vite's
default `publicDir` copies `public/` verbatim into `dist/` at the bundle root
during `vite build` (verified: `apps/prism/vite.config.ts` does not override
`publicDir` or `build.outDir`). At runtime query-api's `ServeDir` serves the
file Vite placed in `dist/` at the origin-root `/config.json` the loader fetches.
One source of truth, one serving mechanism.

The committed slice-01 value uses an **origin-relative** `backend.url`:

```json
{
  "backend": { "url": "/api/v1", "label": "Pulse (durable)" },
  "prism":   { "version": "0.1.0" }
}
```

Origin-relative so the same bundle is portable under same-origin serving without
per-host editing. An absolute `backend.url` (e.g. `http://host:9090/api/v1`)
remains valid for a cross-origin or dev deploy.

### 4. query-api change: a `ServeDir` fallback behind config

`router(...)` gains an optional static-dir parameter; `Some(dir)` attaches a
`ServeDir` fallback under the existing `/api/v1/query_range` route (the API route
wins; unmatched paths fall through to static, with `index.html` as the SPA
fallback). `None` is byte-for-byte today's router. A pure
`resolve_static_dir(env: Option<String>) -> Option<PathBuf>` reads
`KALEIDOSCOPE_QUERY_STATIC_DIR` (unset/empty -> `None`), mirroring the existing
`resolve_*` helpers. `tower-http = { version = "0.6", default-features = false,
features = ["fs"] }` is added to query-api; tower-http 0.6.8 is already in the
workspace `Cargo.lock` (via aperture; ADR-0006), so this enables a feature on an
already-resolved crate. The Earned-Trust probe (ADR-0042 §8) is unchanged.

## Alternatives considered

### Topology A (rejected for slice 01): cross-origin + `CorsLayer`

query-api gains a configurable `tower-http` `CorsLayer` allowing Prism's origin.
**For**: keeps frontend and backend decoupled and independently deployable.
**Against**: every preflight is an extra round-trip on the incident-time path
(the exact argument ADR-0027 §C used to reject cross-origin as the default); an
auth proxy fronting query-api may not propagate preflights; it adds an
allow-origin config surface and a failure class (allow-origin mismatch,
credentialed-request rules) that the slice does not need. ADR-0027 §5 already
pins same-origin as the production posture, so this option would contradict an
Accepted ADR. **Recorded as the documented later option** for an operator who
deliberately runs a decoupled cross-origin deploy; out of scope at v0, adds no
code now.

### Topology B (rejected): ship BOTH a `ServeDir` and a `CorsLayer`

**For**: covers same-origin and cross-origin in one binary. **Against**:
over-building. Slice 01 needs exactly one reachability mechanism; same-origin
suffices and removes CORS entirely. Two mechanisms double the config surface and
the test surface for no slice-01 value. Rejected: ship one, document the other.

### config.json placement A (rejected): a query-api `/config.json` route

Add a dedicated axum route that synthesises `/config.json` from env vars. **For**:
config is fully server-driven, no committed file. **Against**: it splits the
source of truth (the file vs the route handler) and duplicates the shape the
loader already validates; the operator must edit env vars rather than a readable
file. The committed `public/config.json` + `ServeDir` keeps one source of truth
and reuses Vite's existing copy step. Rejected.

### config.json placement B (rejected): inject config at build time into the JS

Bake `backend.url`/`label`/`version` into the bundle via a Vite `define`.
**For**: no runtime fetch. **Against**: ADR-0030 already pins a runtime
`/config.json` loader with three error arms; baking config into the bundle would
discard that wire-then-probe posture and force a rebuild per host. Rejected;
honour the shipped loader.

## Consequences

### Positive

- **No CORS, no preflight.** The simplest honest mechanism removes a failure
  class rather than configuring it. One round-trip per query on the incident-time
  path.
- **ADR-coherent.** Realises ADR-0027 §5's same-origin posture; does not
  contradict §C.
- **Additive and default-off.** With `KALEIDOSCOPE_QUERY_STATIC_DIR` unset,
  query-api is exactly today's API-only server; `query-range-api-v0` does not
  regress.
- **One source of truth for config.** The committed `public/config.json`, copied
  by Vite, served by `ServeDir`.
- **Reuse over reinvention.** axum/tokio/hyper, the existing router and probe,
  tower-http already in the lock, the `oneshot` test driver, and the Prism loader
  are all existing assets.
- **The prose drift is fixed at source.** `backend.url` carries `/api/v1`;
  ADR-0027 §5 now matches the shipped `buildUrl`.

### Negative

- **Deploy is coupled at v0.** Frontend and backend ship behind one origin (one
  query-api process serving `dist/`). Mitigated: the `CorsLayer` cross-origin
  path is documented (Topology A) for an operator who needs a decoupled deploy;
  it lands behind a config knob without changing the query path.
- **query-api now reads the filesystem for static assets.** A new (small)
  responsibility on a read service. Mitigated: behind config, default-off,
  exercised by an explicit RED test (the `ServeDir` adapter demonstrates it can
  serve `/config.json` and `index.html` against a real temp filesystem, and that
  the API route wins).

### Trade-off summary

Slice 01 takes "the simplest honest reachability that removes CORS" over "a
decoupled cross-origin deploy". Same-origin couples the deploy but deletes a
whole failure class; the cross-origin `CorsLayer` is recorded as a non-breaking
later swap behind the same config seam.

## Verification

- A query-api Rust test drives `router(store, tenant, Some(temp_dir))` via
  `tower::ServiceExt::oneshot` (no port bound): `GET /config.json` -> 200 file
  body; `GET /` -> 200 `index.html` (SPA fallback); `GET /api/v1/query_range...`
  -> the matrix/`status:error` handler (API route wins, NOT an `index.html`
  body); `router(.., None)` -> `GET /config.json` 404 (default-off).
- A pure unit test pins `resolve_static_dir` precedence (unset/empty -> `None`),
  mirroring the existing `resolve_tenant`/`resolve_addr` tests, mutation-killed
  without a server.
- Prism Vitest (Gate 6) injects the slice-01 `config.json` via the `App` `fetchFn`
  seam: QueryPanel mounts with label "Pulse (durable)" / version "0.1.0";
  missing `label` -> `shape-failed` keeps it dark; HTTP 404 -> `fetch-failed`.
- Prism Playwright / Gate 11 contract fixture exercises the same-origin
  end-to-end series render (>=1 series + footer counts; empty metric -> calm
  empty state; 200 matrix not 404), with `KALEIDOSCOPE_QUERY_TENANT` set
  (fail-closed tenancy).
- Mutation testing: `gate-5-mutants-query-api` (`cargo mutants` scoped to
  `crates/query-api/**` via `--in-diff`) covers the `ServeDir` wiring and the
  `resolve_static_dir` helper at the project 100% kill-rate gate (ADR-0005 Gate
  5). New logic lives in the `composition`/`router` seam, not the
  `#[mutants::skip]` binary.
