# Story Map: prism-backend-wiring-v0

## User: Priya, on-call SRE who has been paged and needs to see signal shape

## Goal: Open Prism in a browser against a running query-api and see a real metric plotted from the durable Pulse store

## Backbone

| Open Prism in a browser | Load runtime config | Reach query-api from the browser | See the series |
|---|---|---|---|
| Navigate to Prism's URL | Serve a valid config.json at origin root | Browser fetch to query-api succeeds (CORS or same-origin) | Series plotted from Pulse |
| | config.json validates against Prism's own loader | backend.url path join resolves to /api/v1/query_range | Footer confirms round-trip (counts + latency) |

---

### Walking Skeleton

Decision 2 says no separate walking-skeleton slice. The thinnest end-to-end
slice IS Slice 01 itself: a valid config.json + the chosen browser-reachability
mechanism, proven by a test that mounts the QueryPanel and renders one series
end-to-end. Every backbone activity is touched by Slice 01.

### Release 1 (Slice 01): "Operator sees a metric plotted in a browser"

Tasks: serve a valid config.json at Prism's origin root; make a browser-served
Prism reach query-api (cross-origin CORS or same-origin — decided in DESIGN);
prove end-to-end that the QueryPanel mounts and one series renders.

Target outcome: North-Star KPI — QueryPanel mounts and renders a real series
end-to-end. This is the entire honest scope of the feature.

## Priority Rationale

There is one user-visible outcome (a metric on screen in a browser) and it
cannot be split into independently shippable behaviours without one half being
useless: a config.json with no reachable backend still leaves the panel unable
to query; a reachable backend with no valid config.json leaves the panel
unmounted. Both halves must land together to deliver any value, so they are one
slice. Config-serving and browser-reachability are two tasks within Slice 01,
not two releases. Auth, TLS, multi-origin allowlists, and deploy orchestration
are explicitly deferred (see Scope Assessment and wave-decisions.md).

## Scope Assessment: PASS — 2 stories, 2 modules (apps/prism config asset + crates/query-api reachability), estimated 1-2 days

Oversized signals checked: 2 stories (not >10); touches 2 surfaces — the Prism
static config asset and query-api's reachability layer (not >3); reachability
is the single integration point (not >5); estimated 1-2 days (not >2 weeks);
one user outcome (not multiple independently shippable). Right-sized: one slice.
