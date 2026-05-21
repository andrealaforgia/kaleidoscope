# Wave Decisions: prism-backend-wiring-v0 (DESIGN)

- **Wave**: DESIGN (application scope)
- **Interaction mode**: propose
- **Architect**: `nw-solution-architect` (Morgan)
- **Date**: 2026-05-21
- **DISCUSS basis**: commit 581f97d (`discuss/` artefacts)
- **Scope**: Slice 01 only — the minimal panel-mounts-and-plots wiring. No
  auth, no TLS, no multi-origin allowlists.

## Context

DISCUSS flagged one central fork and left it for DESIGN (`discuss/wave-decisions.md`
§ "Central design fork"): how does a browser-served Prism reach query-api?
Everything else (config.json shape, the `/api/v1` path join, the matrix JSON
contract, fail-closed tenancy) is already pinned and shipped. This wave resolves
the fork, pins the config.json contract, and reconciles ADR-0027's prose with
the shipped `buildUrl`.

The shipped code is authoritative. The docs are made to match it.

---

## DD1 — Topology: same-origin via `ServeDir` (NOT CORS) for slice 01

**Decision.** For slice 01, query-api optionally serves Prism's static bundle
AND `/config.json` from the same origin as the `/api/v1` routes, using a
`tower-http` `ServeDir` fallback layer behind config. No `CorsLayer` is added.
A browser-served Prism and query-api share one origin, so the browser's
same-origin policy is satisfied with zero CORS, zero preflight, zero
`Access-Control-Allow-Origin` negotiation.

**The two honest topologies (the fork):**

| | Option 1: cross-origin + CORS | Option 2 (chosen): same-origin via ServeDir |
|---|---|---|
| Mechanism | query-api gains a configurable `tower-http` `CorsLayer` allowing Prism's origin; browser issues a preflight `OPTIONS` then the real request | query-api gains a `tower-http` `ServeDir` fallback that serves Prism's `dist/` (incl. `/config.json` and `index.html`) at the same origin as `/api/v1/*` |
| CORS needed | yes (allow-origin + preflight) | no |
| Round-trips on incident-time path | preflight + request | request only |
| Deploy coupling | decoupled (two origins, two deploys) | coupled (one origin serves both) |
| ADR alignment | ADR-0027 §C rejected cross-origin as the default | ADR-0027 §5 already pins same-origin (reverse-proxy) as the production posture |
| Auth-proxy hazard | a proxy fronting query-api may not propagate preflights | none |
| Code added to query-api | a `CorsLayer` + an allow-origin config knob | a `ServeDir` fallback + a static-dir config knob |

**Rationale (why Option 2 for slice 01):**

1. **Simplest honest option.** Same-origin removes an entire failure class
   (CORS preflight, allow-origin mismatch, credentialed-request rules) rather
   than configuring it correctly. The slice's job is "panel mounts and one
   series plots", not "operate a cross-origin auth surface".
2. **ADR-aligned.** ADR-0027 §5 already documents same-origin (one cert, one
   origin, one log stream) as the production posture and §C explicitly rejected
   cross-origin as the default, on the incident-time round-trip argument. Option
   2 is the ADR-coherent choice; Option 1 would contradict an Accepted ADR.
3. **No CORS preflight on the incident-time path.** KPI 1's latency budget does
   not pay for a preflight round-trip (the same argument ADR-0027 §C makes).
4. **Self-contained demo/dev path.** A single `query-api` binary, pointed at a
   built `dist/` and a pillar root, serves the whole loop. No second static
   server, no reverse proxy, no Vite needed for the non-dev browser path. The
   Vite dev proxy (already mapping `/api/v1 -> :9090`) is untouched and remains
   the developer's same-origin path.

**Do NOT over-build.** We ship a `ServeDir` OR a `CorsLayer`, never both, for
slice 01. A `CorsLayer` is a documented *later* option for an operator who
deliberately runs a decoupled cross-origin deploy (recorded in ADR-0043
"Alternatives"); it is out of scope here and adds no code at v0.

**Read-only behaviour is unchanged when not configured.** The `ServeDir`
fallback is wired only when the static-dir config knob is set. With the knob
unset, query-api's router is exactly today's `/api/v1/query_range`-only router;
the existing `query-range-api-v0` behaviour does not regress.

---

## DD2 — The config.json contract and where it is served

**Shape (already pinned by Prism's loader — verified, not invented):**

```json
{
  "backend": { "url": "<origin>/api/v1", "label": "<human label>" },
  "prism":   { "version": "<semver>" }
}
```

Validated by `apps/prism/src/lib/config/loader.ts` `isRuntimeConfig`: `backend`
and `prism` must be objects; `backend.url`, `backend.label`, `prism.version`
must be strings. Three error arms (`fetch-failed`, `parse-failed`,
`shape-failed`) keep the panel dark; that refusal behaviour is preserved.

**`backend.url` MUST carry the `/api/v1` prefix.** `queryRange.ts` `buildUrl`
joins `${backend.url}/query_range`; query-api serves `/api/v1/query_range`
(`QUERY_RANGE_ROUTE`). So:

- Same-origin (chosen): `backend.url = "/api/v1"` (origin-relative — the browser
  resolves it against the page's own origin; `buildUrl` yields
  `/api/v1/query_range`). This is the slice-01 production-shape value.
- Cross-origin (the dev convenience and the later CORS option): an absolute
  `backend.url = "http://host:9090/api/v1"` also works; `buildUrl` yields the
  absolute `/api/v1/query_range`.

Omitting `/api/v1` yields a 404 -> `transport-error: http-status`. The slice's
end-to-end test asserts a 200 matrix, not a 404.

**Where config.json lives (single source of truth):**

A committed static file at **`apps/prism/public/config.json`**. Vite's default
`publicDir` is `public/` and Vite copies its contents verbatim into `dist/` at
the bundle root during `vite build` (no config change needed — see DD5). So
`dist/config.json` exists at the bundle root, fetched by the loader at
`/config.json` once the bundle is served.

**Who serves it at runtime:** query-api's `ServeDir` (DD1), pointed at the
built `dist/`, serves `/config.json` and `index.html` from the same origin as
`/api/v1/*`. There is no separate query-api `/config.json` route — `ServeDir`
serves the file Vite already placed in `dist/`. One source of truth (the
committed `public/config.json`), one serving mechanism (`ServeDir` over
`dist/`).

The slice-01 committed value uses the origin-relative `backend.url = "/api/v1"`
so the same bundle works under same-origin serving without per-host editing.
`apps/prism/public/config.json.example` (referenced by ADR-0026) remains the
shape-only exemplar an operator overrides for a cross-origin deploy.

---

## DD3 — query-api change: a `ServeDir` fallback behind config

**What changes** (`crates/query-api/src/lib.rs` + `composition.rs` + `main.rs`):

- `lib.rs` `router(...)` gains an optional static-dir parameter. When `Some(dir)`,
  the router attaches a `tower-http` `ServeDir` as a **fallback** under the
  existing `/api/v1/query_range` route (so the API route always wins; any
  unmatched path falls through to the static files, with `index.html` as the
  SPA fallback). When `None`, the router is byte-for-byte today's router.
- `composition.rs` gains a pure `resolve_static_dir(env_value: Option<String>)
  -> Option<PathBuf>` resolver reading `KALEIDOSCOPE_QUERY_STATIC_DIR`
  (unset/empty -> `None` -> no static serving), mirroring the existing
  `resolve_*` precedence helpers.
- `main.rs` reads the env var, resolves the dir, and passes it to `router(...)`.
  The Earned-Trust probe (DD9 of ADR-0042 / `composition::probe`) is unchanged
  and still runs wire-then-probe-then-use before the listener binds.

**Dependency.** Add `tower-http = { version = "0.6", default-features = false,
features = ["fs"] }` to `crates/query-api/Cargo.toml`. `tower-http` 0.6.8 is
already in the workspace `Cargo.lock` (pulled transitively via aperture's tree;
ADR-0006), so this is a feature-enabling addition, not a new dependency
resolution. `ServeDir` lives behind the `fs` feature. License: MIT (tower-http),
compatible with query-api's AGPL-3.0-or-later.

**Minimality.** No `CorsLayer`. No new auth, no TLS. The static-dir knob is
off by default; with it off, `query-range-api-v0` behaviour is identical.

---

## DD4 — Prism change: a committed config.json (code is already complete)

**What changes** (`apps/prism/`):

- Add **`apps/prism/public/config.json`** with the slice-01 value:
  `{ "backend": { "url": "/api/v1", "label": "Pulse (durable)" }, "prism": { "version": "0.1.0" } }`.
  Origin-relative `backend.url` so the same bundle is portable under same-origin
  serving.
- No Prism source change. `loadConfig`, `queryRange`, `App.tsx`, `QueryPanel`
  are already complete and shipped. The feature provides the missing asset, not
  new behaviour.
- Optional: a README note documenting that `public/config.json` is the committed
  default and `config.json.example` is the operator-override exemplar.

---

## DD5 — Vite copies `public/` into `dist/` (verified)

Vite's `publicDir` defaults to `<root>/public` and its contents are copied
**as-is to the root of `dist/`** during `vite build`. `apps/prism/vite.config.ts`
does not override `publicDir` or `build.outDir`, so the default applies:
`apps/prism/public/config.json` -> `apps/prism/dist/config.json`, fetched by the
loader at `/config.json`. No vite.config.ts change is required. (Confirmed:
`vite.config.ts` sets only `plugins`, `server.proxy`, and `build.target` /
`build.sourcemap` / `build.rollupOptions`.)

---

## DD6 — Testability seam for slice 01 (so DISTILL can write RED tests)

**Backend (query-api), the chosen mechanism works:**

The `ServeDir` fallback is the new behaviour to pin. A Rust test drives the
`router(store, tenant, Some(static_dir))` via `tower::ServiceExt::oneshot` (the
existing dev-dependency pattern — no network port bound) against a temp dir
containing a `config.json` and an `index.html`, asserting:

1. `GET /config.json` returns 200 with the file body (the static fallback
   serves it);
2. `GET /` (or an unknown SPA path) returns 200 `index.html` (SPA fallback);
3. `GET /api/v1/query_range?query=up&start=..&end=..` still routes to the API
   handler (the API route wins over the static fallback), returning the matrix /
   `status:error` shape — NOT an `index.html` body.
4. With `router(store, tenant, None)`, `GET /config.json` returns 404 (static
   serving is off by default; existing behaviour unchanged).

The `composition::resolve_static_dir` precedence (unset/empty -> `None`) is a
pure unit test mirroring the existing `resolve_tenant` / `resolve_addr` tests,
so it is mutation-killed without spawning a server.

**Frontend (Prism), the panel mounts and plots given a valid config:**

The existing Vitest/Playwright seams already cover this; slice 01 exercises them
against the served config:

- `App.tsx` takes a `fetchFn` test seam; a Vitest test injects a fetch returning
  the slice-01 `config.json` and asserts the QueryPanel mounts with backend
  label "Pulse (durable)" and version "0.1.0" (US-01 Scenario 1); a missing
  `label` asserts `shape-failed` keeps it dark (Scenario 2); an HTTP 404 asserts
  `fetch-failed` (Scenario 3).
- The end-to-end series-renders path (US-02) is the existing Gate 11 Prometheus
  contract fixture / Playwright E2E, now pointed at a same-origin query-api +
  served `dist/`: assert >=1 series and the footer counts (Scenario 1); empty
  metric -> calm empty state (Scenario 4); the 200-matrix-not-404 path resolves
  (Scenario 3). `KALEIDOSCOPE_QUERY_TENANT` MUST be set in the test environment
  (fail-closed tenancy).

This gives DISTILL one new RED test on the query-api side (the `ServeDir`
fallback + route precedence) and a clear reuse of the existing Prism seams.

---

## DD7 — ADR verdict: NEW ADR-0043 (do not silently modify ADR-0027)

**Decision.** Write a new **ADR-0043** ("Prism backend-wiring topology and the
`backend.url` `/api/v1` reconciliation"). ADR-0027 is Accepted and immutable
(project ADR rule: supersede, never modify in place). ADR-0043:

- Records the same-origin-via-`ServeDir` topology for slice 01 (DD1), with the
  CORS option recorded as a rejected-for-now alternative (DD1 table).
- **Reconciles the prose drift**: ADR-0027 §5 says the request URL is
  `${backend.url}/api/v1/query_range`, but the shipped `buildUrl` joins only
  `${backend.url}/query_range`, so `backend.url` MUST itself include `/api/v1`.
  ADR-0043 corrects the wording: *the shipped code is authoritative; `backend.url`
  carries the `/api/v1` segment and `buildUrl` appends only `/query_range`.*
  ADR-0043 is marked **Related: refines ADR-0027 §5** (a clarification, not a
  full supersession — ADR-0027's client surface, `QueryOutcome` union, and
  redaction invariant all stand unchanged).

Highest existing ADR is 0042; 0043 is the next free number (verified by glob of
`docs/product/architecture/adr-*.md`).

---

## Reuse Analysis

| Asset | Reused / extended | Source |
|---|---|---|
| query-api axum `Router` + `/api/v1/query_range` | reused unchanged; `ServeDir` is an additive fallback | `crates/query-api/src/lib.rs` |
| `composition::resolve_*` precedence pattern | extended with `resolve_static_dir` | `crates/query-api/src/composition.rs` |
| Earned-Trust `probe()` (wire-then-probe-then-use) | reused unchanged | `crates/query-api/src/composition.rs`, ADR-0042 §8 |
| `tower-http` 0.6.8 | already in `Cargo.lock` (via aperture); enable `fs` feature on query-api | workspace `Cargo.lock`, ADR-0006 |
| axum 0.7 + tokio + hyper | reused; no new web framework | `crates/query-api/Cargo.toml`, ADR-0042 §2 |
| `tower::ServiceExt::oneshot` test driver | reused for the `ServeDir` RED test (no port bound) | query-api dev-dependency |
| Prism `loadConfig` + three error arms | reused unchanged | `apps/prism/src/lib/config/loader.ts`, ADR-0030 |
| Prism `queryRange` + `buildUrl` + redaction | reused unchanged | `apps/prism/src/lib/promql/queryRange.ts`, ADR-0027 |
| Vite default `publicDir` -> `dist/` copy | reused (no config change) | `apps/prism/vite.config.ts` |
| Prism `fetchFn` test seam | reused for US-01 mount tests | `apps/prism/src/app/App.tsx` |

**No new component is justified by "no existing alternative" beyond the
`ServeDir` fallback and the `resolve_static_dir` helper — both additive, both
behind config, both off by default.**

---

## DEVOPS handoff

- **query-api Rust change is already gate-covered.** `gate-5-mutants-query-api`
  runs `cargo mutants` scoped to `crates/query-api/**` via path-filtered
  `--in-diff` (CI lines 1036-1123). The `ServeDir` wiring + the
  `resolve_static_dir` helper are mutation-tested by that gate when the diff
  touches the crate. The `composition::resolve_static_dir` unit test and the
  `oneshot` `ServeDir` route-precedence test must give the gate a 100% kill
  surface (CLAUDE.md / ADR-0005 Gate 5). Note: `main.rs` stays `#[mutants::skip]`
  (thin reader of env) — keep the new logic in the `composition`/`router` seam,
  not the binary, so the kill rate stays honest.
- **Prism frontend gates already exist** (verified, CI lines 1549-1732), all
  gated on `apps/prism/` changes:
  - Gate 6 — Prism Vitest (typecheck + unit/integration): covers the US-01 mount
    tests.
  - Gate 7 — Prism Playwright E2E: covers the US-02 end-to-end series render.
  - Gate 8 — bundle size (<=300 KB gzipped): adding a tiny `config.json` to
    `public/` does not affect the JS bundle budget.
  - Gate 9 — lint + format + AGPL header.
  - Gate 10 — Prism mutation (StrykerJS, in-diff): no new Prism source, so no
    new mutation surface.
  - Gate 11 — Prism Prometheus contract (container fixture): the natural home
    for the same-origin end-to-end assertion (query-api serving `dist/` +
    `/api/v1`), with `KALEIDOSCOPE_QUERY_TENANT` set.
- **New env knob to document for operators**: `KALEIDOSCOPE_QUERY_STATIC_DIR`
  (path to Prism's built `dist/`; unset -> no static serving, API-only). Sits
  alongside the existing `KALEIDOSCOPE_QUERY_TENANT` / `_ADDR` / `_PILLAR_ROOT`.
- **No new CI gate is required.** The change is covered by the existing
  per-crate mutation gate plus the existing six Prism gates. The only DEVOPS
  task is ensuring the Gate 11 fixture (or a new same-origin E2E job) sets
  `KALEIDOSCOPE_QUERY_STATIC_DIR` and `KALEIDOSCOPE_QUERY_TENANT`.

### External integrations requiring contract tests

- **Prism query client <-> query-api** (`/api/v1/query_range`): the consumer/
  provider contract is already pinned (ADR-0027 §External-integration handoff,
  ADR-0042 §External-integration handoff) and covered by Gate 11. This feature
  does not change the wire shape — it only makes it reachable from a same-origin
  browser. No new contract test is needed; the existing four-shape contract
  assertion stands.

---

## Honest scope (unchanged from DISCUSS)

In: a committed `public/config.json` with `backend.url = "/api/v1"`; query-api's
`ServeDir` fallback behind `KALEIDOSCOPE_QUERY_STATIC_DIR`; ADR-0043; proof the
panel mounts and one series renders same-origin end-to-end. Out: CORS/CorsLayer
(documented later option), auth, TLS, multi-origin allowlists, deploy
orchestration.

## Artifacts produced (DESIGN)

- `design/wave-decisions.md` (this file)
- `design/application-architecture.md` (C4 + Mermaid)
- `docs/product/architecture/adr-0043-prism-backend-wiring-topology-and-api-v1-reconciliation.md`
- `design/peer-review.md` (review verdict)
