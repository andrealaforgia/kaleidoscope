<!-- markdownlint-disable MD024 -->

# read-path-query-api-auth-v0 — user stories

Four LeanUX user stories wiring the correct `aegis::Validator` onto the
three live READ query APIs (`query-api`, `log-query-api`,
`trace-query-api`) as an OPTIONAL per-request bearer path, fail-closed,
mirroring the ingest auth (ADR-0068) on every axis except audience. Each
story carries the mandatory Elevator Pitch (Before / After / Decision
enabled), concrete domain examples with real data, BDD UAT scenarios
derived from the job, embedded acceptance criteria, and outcome KPIs.

> **MODEL FORK — ANDREA MAY VETO** (recorded in full in
> `wave-decisions.md`): these stories proceed on the **ADDITIVE** model —
> the per-instance env tenant default is PRESERVED, an OPTIONAL
> per-request bearer path is ADDED on top. The alternative is
> per-request-only (mandatory bearer, no env fallback). Every story's
> fail-closed and isolation AC hold identically under both models; only
> US-RAUTH-02 (backward compatibility) reshapes on a veto. Andrea: to
> veto, see the flag in `wave-decisions.md`.

The principal user is **Priya, a platform-security operator** running a
multi-tenant Kaleidoscope deployment for `acme-observability`. Priya owns
the running read APIs and is accountable to a security audit that asks
"can a caller read another tenant's telemetry?". Today the honest answer
is "the read side is authenticated per-DEPLOYMENT (one tenant per
process), not per-CALLER" — and Priya needs per-request tenant isolation
that mirrors the ingest door she just closed.

The query-side users are **Nadia, an SRE** who queries metrics, logs, and
traces through Prism (or curl) for her own tenant and needs an
authenticated query to return exactly her tenant's data, and an
incident-readable signal when a token is rejected.

The adversary persona is **Mallory, a cross-tenant caller** who holds (or
forges) a token and tries to read a victim tenant's data — and **Trent**,
who presents a token minted for the INGEST audience and tries to use it to
read.

## System Constraints

These cross-cutting constraints apply to every story. They are
requirements, not mechanisms; DESIGN owns the how (see `wave-decisions.md`
DD1-DD6).

1. **Reuse the correct validator verbatim, mirror the ingest auth.**
   `aegis::Validator`, `TenantContext`, `ValidationError`,
   `validate_with_subject`, and `load_catalogue` are reused as-is — the
   SAME validator the ingest door uses (ADR-0068). HS256 pre-shared key,
   alg-confusion-safe, fail-closed `exp`, exact issuer + audience,
   catalogue-checked tenant. This feature WIRES the validator onto the
   read request path; it does NOT change the crypto.

2. **The shared capability lands ONCE in `query-http-common`.** The
   per-request bearer-validation + tenant-resolution capability is the
   per-request analogue of the existing
   `query_http_common::resolve_tenant_or_refuse` fail-closed seam and
   belongs in the same shared crate (ADR-0054 rationale), wired through
   all three read APIs — not duplicated per crate.

3. **Fail-closed is the whole point.** A request whose tenant cannot be
   resolved is refused with a 401 BEFORE the backing store is touched
   (the existing `query-http-common` seam). A missing token (when auth is
   configured), an empty token, a malformed token, an expired token, a
   wrong-issuer/wrong-audience token, an unknown-tenant token, an
   unknown-role token — all reject, with nothing read. **When auth is
   configured, no valid token means no read of any tenant's data — never
   a silent downgrade to the env tenant.**

4. **The HS256 secret is sensitive and must never be logged.** Not in a
   Debug, not in a Display, not in a config-validation error, not in an
   audit event, not in a reject status/body. aegis already renders the
   key opaque in `Validator`'s Debug; the read APIs must not undo that and
   must not echo the secret from their own config layer. The raw token
   must never appear in any error, log, or audit field either.

5. **Tenant isolation is provable (positive + negative control).** A
   token scopes the query to ITS tenant only. The project's standard
   negative control is mandatory: prove the data IS returned for the
   right tenant AND ABSENT (empty / forbidden) for the wrong one. A
   tenant-A token can never read tenant-B data.

6. **Backward compatibility (additive model).** When no auth config is
   present, the read APIs behave byte-for-byte as today: the per-instance
   env tenant (`KALEIDOSCOPE_*_QUERY_TENANT`) scopes every request, and an
   unset/empty env tenant still refuses 401 via the existing seam. The
   existing read-API slice tests stay green. (On an Andrea veto to
   per-request-only, this constraint inverts to "auth always required" —
   see US-RAUTH-02.)

7. **Cross-surface audience fence.** Read tokens carry
   `aud=kaleidoscope-query`; ingest tokens carry `aud=kaleidoscope-ingest`.
   A token minted for the ingest audience MUST reject `wrong_audience` on
   the read path. This is the boundary that stops an ingest token from
   reading data.

8. **Audit alignment.** Denials and accepts emit structured events using
   aegis's locked field contract (`tenant_id`, `role`, `decision`,
   `subject`, `reason`); the `subject` names the read action (e.g.
   `query_range`, `log_query`, `trace_query`). Exactly one decision event
   per request — never zero, never duplicated. Aligns with the read tier's
   `query_http_common::init_tracing` JSON-stderr subscriber.

9. **Inherited gates.** ADR-0005's five gates; per-feature mutation
   testing at 100% on the modified files; Rust idiomatic; never 1.0.0.

---

## US-RAUTH-01 — Walking slice: per-request bearer auth + tenant isolation on the metrics query API, fail-closed

### Elevator Pitch

- **Before**: a client sends `GET /api/v1/query_range?...` to the metrics
  read API (`query-api`, `:9090`) with **no `Authorization` header**. The
  process resolves the tenant from its single env var
  `KALEIDOSCOPE_QUERY_TENANT`, so EVERY caller of that process reads the
  SAME tenant's metrics — there is no per-caller scoping. A second tenant
  cannot be served by the same process, and a caller's identity is never
  checked.
- **After**: with read auth configured, the same `GET /api/v1/query_range`
  call carrying `Authorization: Bearer <jwt>` for catalogued tenant
  `acme-prod` (audience `kaleidoscope-query`) returns **`acme-prod`'s
  metrics and only those**; the same query carrying a valid token for
  `globex-staging` returns `globex-staging`'s data and an empty result
  where `acme-prod` had data (isolation, the negative control); a call
  with **no/invalid token returns HTTP 401** with a `WWW-Authenticate:
  Bearer` challenge and the aegis reason, **before the Pulse store is
  touched**, one deny audit line, nothing read.
- **Decision enabled**: Priya can answer the audit's "can a caller read
  another tenant's metrics?" with a demonstrated **no** — she runs the
  API with auth, queries with an `acme-prod` token and shows `acme-prod`
  data, queries the same series with a `globex-staging` token and shows it
  is ABSENT, and queries with no token and shows the 401 with no store
  read.

### Problem

Priya is a platform-security operator who must certify that the read APIs
enforce per-tenant isolation at the request boundary. She finds it
impossible today: `query-api` resolves one tenant per process from
`KALEIDOSCOPE_QUERY_TENANT` and applies it to every request, so the only
"isolation" is running one process per tenant, and no caller's identity is
ever checked. She has just closed the ingest door per-request (ADR-0068)
and needs the symmetric per-request control on the read side.

### Who

- **Priya**, platform-security operator | runs the live `query-api` for
  `acme-observability` | motivated to certify per-tenant read isolation
  and to stop relying on one-process-per-tenant.
- **Nadia**, SRE | queries `query-api` for her tenant's metrics through
  Prism / curl with a bearer token | motivated to get exactly her tenant's
  data back.
- **Mallory**, cross-tenant caller | holds a valid token for her own
  tenant and tries to read a victim tenant's metrics | must be isolated.

### Solution

Wire `aegis::Validator` into the metrics read path via a new per-request
resolution capability in `query-http-common` (the shared seam, reused by
all three APIs). On each `GET /api/v1/query_range`, when auth is
configured: extract `Bearer` from the `Authorization` header, call
`validate_with_subject(token, now, "query_range")`, and resolve the tenant
to the token's `TenantContext.tenant_id`; on any failure return 401 with
the aegis reason and read nothing; on success scope the Pulse query to
THAT tenant. When auth is NOT configured, fall back to the env tenant
(backward compatibility, US-RAUTH-02). The fail-closed refusal reuses the
existing `query-http-common` seam (refused before the store).

### Domain Examples

#### 1: Happy Path — Nadia reads her own tenant's metrics

Nadia queries `GET /api/v1/query_range?query=up&start=1717000000&end=1717003600&step=60`
against `query-api` with `Authorization: Bearer <jwt>` where the JWT is
HS256-signed by `acme-observability`'s issuer, audience
`kaleidoscope-query`, `tenant_id=acme-prod` (catalogued), role `viewer`,
`exp` 30 minutes ahead. `query-api` validates, scopes the Pulse query to
`acme-prod`, and returns the Prometheus `query_range` matrix of
`acme-prod`'s `up` series — byte-shape identical to today's response. One
`decision=allow subject=query_range tenant_id=acme-prod` audit line.

#### 2: Error/Boundary (isolation — the negative control) — Mallory's cross-tenant read returns ABSENT

`acme-prod` has an `up` series at value 1. Mallory holds a VALID token for
her own catalogued tenant `globex-staging` and queries the same
`query=up` range hoping to see `acme-prod`'s data. `query-api` validates
her token, scopes the query to `globex-staging`, and returns
`globex-staging`'s result — which is EMPTY for that series. `acme-prod`'s
data is ABSENT from Mallory's response. The positive control (Example 1:
`acme-prod` token sees the data) and this negative control (Mallory's
`globex-staging` token does not) together prove isolation.

#### 3: Error/Boundary (fail-closed) — Nadia's missing token is refused before the store

Nadia's script drops the `Authorization` header (a bug in her client).
With auth configured, `query-api` returns **HTTP 401**, `WWW-Authenticate:
Bearer`, reason `missing_claim`, emits one
`decision=deny subject=query_range reason=missing_claim` audit line, and
**the Pulse store is never queried** (no store read on the refusal path).
The raw (absent) token and the secret appear nowhere. Nadia fixes her
client and Example 1 succeeds.

### UAT Scenarios (BDD)

#### Scenario: An authenticated client reads its own tenant's metrics

```gherkin
Given query-api is running with read auth configured for issuer "acme-observability",
  audience "kaleidoscope-query", and a catalogue containing "acme-prod"
And tenant "acme-prod" has an "up" metric series in the Pulse store
And Nadia presents a valid bearer token for tenant "acme-prod"
When Nadia sends GET /api/v1/query_range for "up" over a valid window with that token
Then query-api returns the query_range matrix for tenant "acme-prod" with the same response shape as today
And exactly one audit event with decision "allow", subject "query_range", tenant_id "acme-prod" is emitted
```

#### Scenario: A token for one tenant cannot read another tenant's metrics (isolation)

```gherkin
Given query-api is running with read auth configured and a catalogue containing "acme-prod" and "globex-staging"
And tenant "acme-prod" has an "up" series and tenant "globex-staging" has none
And Mallory presents a valid bearer token for tenant "globex-staging"
When Mallory sends GET /api/v1/query_range for "up" with that token
Then query-api returns a result scoped to "globex-staging" with the "up" series absent
And no data belonging to "acme-prod" appears in the response
And exactly one audit event with decision "allow", subject "query_range", tenant_id "globex-staging" is emitted
```

#### Scenario: A request with no bearer token is refused 401 before the store is touched

```gherkin
Given query-api is running with read auth configured
And Nadia sends no Authorization header
When Nadia sends GET /api/v1/query_range for "up"
Then query-api responds 401 Unauthorized with a WWW-Authenticate: Bearer header
And the Pulse store is never queried
And exactly one audit event with decision "deny", subject "query_range", reason "missing_claim" is emitted
```

#### Scenario: An expired token is refused with the matching reason and nothing is read

```gherkin
Given query-api is running with read auth configured and a catalogue containing "acme-prod"
And Nadia presents a bearer token for "acme-prod" whose exp is in the past
When Nadia sends GET /api/v1/query_range for "up" with that token
Then query-api responds 401 Unauthorized
And the Pulse store is never queried
And exactly one audit event with decision "deny", subject "query_range", reason "expired" is emitted
```

#### Scenario: The bearer token and the secret never appear in any error, body, or log (redaction)

```gherkin
Given query-api is running with read auth configured
And Nadia presents a malformed bearer token value
When query-api rejects the request
Then the 401 response body carries the aegis reason but never the token value
And no log line, audit event, or error contains the secret bytes or the raw token
```

### Acceptance Criteria

- [ ] **a-valid-token-reads-its-own-tenant**: a `GET /api/v1/query_range`
  with a valid `kaleidoscope-query`-audience token for a catalogued tenant
  returns that tenant's metrics with the existing response shape.
- [ ] **tenant-isolation-positive-and-negative-control**: an `acme-prod`
  token sees `acme-prod`'s `up` series; a `globex-staging` token sees it
  ABSENT — no cross-tenant read.
- [ ] **no-token-is-refused-401-before-the-store**: with auth configured,
  a request with no `Authorization` header returns 401 + `WWW-Authenticate:
  Bearer` and the Pulse store is never queried.
- [ ] **expired-token-refused-with-the-matching-reason**: an expired token
  returns 401 reason `expired`, nothing read.
- [ ] **the-secret-and-token-are-never-logged**: no secret bytes and no
  raw token appear in any 401 body, error, log line, or audit event.
- [ ] **one-audit-event-per-request**: exactly one decision event (`allow`
  on accept, `deny` on reject) per `query_range` request — never zero,
  never duplicated.

### Outcome KPIs

- **Who**: clients querying metrics on the live `query-api`.
- **Does what**: read only their own tenant's metrics, gated by a valid
  bearer token, when auth is configured.
- **By how much**: 100% of authenticated metric reads are scoped to the
  token's tenant (positive + negative control); 100% of invalid/missing
  (auth-on) metric requests refused 401 with the store never touched.
- **Measured by**: audit `allow`/`deny` events (`subject=query_range`,
  `tenant_id`) correlated with store-read events; isolation tests.
- **Baseline**: 0% per-request authenticated (one tenant per process via
  env today); 0% of invalid requests refused (no auth on the read path).

### Technical Notes

- Reuse `aegis::Validator::validate_with_subject` verbatim; mirror
  ADR-0068's extract→validate→reject→scope spine on HTTP (DD2).
- The per-request resolution capability lands in `query-http-common`
  (System Constraint 2; ADR-0054 rationale), reused by all three APIs.
- The fail-closed refusal reuses
  `query_http_common::resolve_tenant_or_refuse` / `error_response` (F2) —
  refused before the store, returning the existing
  `{"status":"error","error":...}` envelope or a 401 sibling DESIGN pins
  (DD2).
- DD1: read-API auth config (HS256 secret-by-reference, issuer, audience
  `kaleidoscope-query`, catalogue path) — the fields do not exist yet.
- DD3: the additive precedence (auth-on → token tenant; auth-off → env
  tenant); on a per-request-only veto this collapses (see US-RAUTH-02).
- Depends on: aegis v0 (`Validator`, `TenantContext`, `load_catalogue`)
  available; `query-http-common` shared seam available (F2); ADR-0068
  ingest-auth shape to mirror available.

---

## US-RAUTH-02 — Backward compatibility: an unconfigured deployment keeps today's env-tenant behaviour (the additive default)

### Elevator Pitch

- **Before**: every read-API deployment resolves one tenant per process
  from `KALEIDOSCOPE_*_QUERY_TENANT`. Single-tenant operators rely on this
  exact behaviour; an unset/empty env tenant already refuses 401 via the
  shared seam.
- **After**: a deployment that configures **no read auth** behaves
  byte-for-byte as today — the env tenant scopes every request, the unset
  env tenant still refuses 401, and the `Authorization` header (if any) is
  ignored. The per-request bearer path is OPT-IN: it only activates when
  an auth config is present. Existing single-tenant deployments and their
  tests are untouched.
- **Decision enabled**: Priya (and existing operators) can adopt this
  release with ZERO change to single-tenant deployments — she upgrades the
  binary, sets no auth config, and her env-tenant deployment runs
  identically, then opts into per-request auth tenant-by-tenant when ready.

> **ANDREA-VETO PIVOT**: this story is the ONE that reshapes on a veto to
> per-request-only. On a veto, replace this story with "auth is always
> required; a read API with no auth config (or no resolvable per-request
> tenant) refuses to start / refuses every request" — mirroring the ingest
> path's mandatory posture (ADR-0068 US-AUTH-02). The fail-closed and
> isolation stories (US-RAUTH-01, -03, -04) are UNCHANGED by the veto.

### Problem

Priya cannot risk breaking the fleet of existing single-tenant read-API
deployments while adding per-request auth. The additive model promises
backward compatibility; that promise must be a tested, observable
property, not an assumption — otherwise an upgrade silently changes tenant
resolution for every existing operator.

### Who

- **Priya**, platform-security operator | runs both new (auth-configured)
  and legacy (env-tenant) deployments during rollout | motivated to
  upgrade without breaking legacy deployments.
- **Omar**, an operator of a legacy single-tenant deployment | sets only
  `KALEIDOSCOPE_LOG_QUERY_TENANT=acme-prod` and never touches auth |
  motivated for his deployment to keep working exactly as before.

### Solution

Make the per-request bearer path strictly OPT-IN: auth-config-absent ⇒
today's env-tenant resolution and fail-closed `probe` are unchanged (the
three `composition.rs` `resolve_tenant`/`probe` stay); auth-config-present
⇒ the per-request path (US-RAUTH-01) is in force. DESIGN locks the
precedence (DD3): an unconfigured deployment never reads the
`Authorization` header.

### Domain Examples

#### 1: Happy Path — Omar's legacy env-tenant deployment is unchanged

Omar runs `log-query-api` with `KALEIDOSCOPE_LOG_QUERY_TENANT=acme-prod`
and NO auth config. A `GET /api/v1/logs?...` request (with or without an
`Authorization` header) returns `acme-prod`'s logs exactly as it did
before this feature — the header is ignored, the env tenant scopes the
query, the response is byte-for-byte today's.

#### 2: Boundary — the unset env tenant still refuses 401 (unchanged)

Omar's deployment has NO auth config and an UNSET
`KALEIDOSCOPE_LOG_QUERY_TENANT`. A request is refused with the existing
401 `"no tenant resolvable: the log query service refuses unscoped
requests"` — exactly today's fail-closed behaviour via the shared seam.
Nothing about this changes.

#### 3: Boundary — auth-configured deployment does NOT fall back to the env tenant

Priya's NEW deployment configures read auth AND also has
`KALEIDOSCOPE_QUERY_TENANT=acme-prod` set (a leftover). A request with NO
bearer token is **refused 401** (auth is on; missing token rejects) —
it does NOT silently downgrade to the env tenant `acme-prod`. Once auth is
configured, the bearer is the authority; the env tenant is not a bypass.

### UAT Scenarios (BDD)

#### Scenario: A deployment with no auth config resolves the env tenant exactly as today

```gherkin
Given log-query-api is running with no read auth configured
And the environment sets the log query tenant to "acme-prod"
When a client sends GET /api/v1/logs over a valid window
Then log-query-api returns "acme-prod" logs with the same response shape as before this feature
And any Authorization header on the request is ignored
```

#### Scenario: A deployment with no auth config and no env tenant still refuses unscoped requests

```gherkin
Given log-query-api is running with no read auth configured
And the environment sets no log query tenant
When a client sends GET /api/v1/logs
Then log-query-api responds 401 with the existing "no tenant resolvable" reason
And the behaviour is byte-for-byte the same as before this feature
```

#### Scenario: An auth-configured deployment never downgrades a missing token to the env tenant

```gherkin
Given query-api is running with read auth configured
And the environment also sets the query tenant to "acme-prod"
When a client sends GET /api/v1/query_range with no Authorization header
Then query-api responds 401 Unauthorized
And the request is not silently scoped to the env tenant "acme-prod"
And the Pulse store is never queried
```

### Acceptance Criteria

- [ ] **env-tenant-unchanged-when-auth-absent**: a deployment with no auth
  config resolves the per-instance env tenant and returns the same
  response shape as before this feature; the `Authorization` header is
  ignored.
- [ ] **unset-env-tenant-still-refuses-401**: with no auth config and an
  unset env tenant, the existing 401 "no tenant resolvable" refusal is
  byte-for-byte unchanged.
- [ ] **auth-on-missing-token-does-not-downgrade-to-env-tenant**: with
  auth configured, a missing bearer rejects 401 and is NOT scoped to the
  env tenant (no bearer-bypass).
- [ ] **existing-read-api-slice-tests-stay-green**: the existing
  `query-api` / `log-query-api` / `trace-query-api` slice tests (which use
  the env tenant) pass unchanged.

### Outcome KPIs

- **Who**: operators of existing single-tenant (env-tenant) read-API
  deployments.
- **Does what**: upgrade to the auth-capable binary with zero behaviour
  change when they configure no auth.
- **By how much**: 100% of env-tenant-only deployments behave byte-for-byte
  as before (no regression); 100% of auth-configured deployments refuse a
  missing token rather than downgrading to the env tenant.
- **Measured by**: existing read-API slice-test suite (unchanged, green) +
  a missing-token-with-auth-on refusal assertion.
- **Baseline**: 100% env-tenant today (the only mode); target: that mode
  preserved AND the new opt-in mode never bypassable.

### Technical Notes

- DD3 precedence: auth-config-absent ⇒ env tenant (`composition.rs`
  `resolve_tenant`/`probe` unchanged); auth-config-present ⇒ per-request
  path, no env fallback on a missing token.
- **VETO PIVOT** (DD3): on per-request-only, this story becomes "auth
  always required; no-auth-config refuses to start" (mirror ADR-0068
  US-AUTH-02); US-RAUTH-01/-03/-04 unchanged.
- Depends on US-RAUTH-01 (the per-request capability whose presence this
  story gates on).

---

## US-RAUTH-03 — Parity: per-request bearer auth + isolation on the log and trace query APIs

### Elevator Pitch

- **Before**: with the metrics read API authenticated per-request
  (US-RAUTH-01), the log query API (`log-query-api`, `:9091`,
  `GET /api/v1/logs`) and the trace query API (`trace-query-api`, `:9092`,
  `GET /api/v1/traces` + trace lookup-by-id) are still per-instance: one
  env tenant per process, no per-caller scoping. Two of the three read
  doors are still un-authenticated per-request.
- **After**: every one of the three read APIs requires a valid
  `kaleidoscope-query`-audience bearer token (when auth is configured) and
  scopes the query to the token's tenant; a tenant-A token reading
  tenant-B's logs or traces gets that tenant's (empty) data, a
  missing/invalid token gets 401 before the store, one decision audit
  line per request. The shared capability from `query-http-common` is
  REUSED, not re-implemented.
- **Decision enabled**: Priya can certify the ENTIRE read surface (metrics
  + logs + traces) enforces per-tenant isolation — she demonstrates a
  tagged read and a cross-tenant ABSENT read for logs and for traces, and
  a tokenless 401 with no store read on each.

### Problem

Priya's read-isolation certification must cover all three read APIs. With
only metrics authenticated, an isolated metrics path beside per-instance
log and trace paths is still a partially-open read surface — a caller can
still read whatever tenant the log/trace process is pinned to.

### Who

- **Priya**, platform-security operator | certifies the full read surface
  (metrics + logs + traces).
- **Nadia**, SRE | queries logs and traces for her tenant through Prism /
  curl with a bearer token | needs each authenticated read scoped to her
  tenant.
- **Mallory**, cross-tenant caller | tries her own-tenant token against a
  victim tenant's logs and traces | must be isolated on both.

### Solution

Apply the US-RAUTH-01 spine to `log-query-api` and `trace-query-api` by
REUSING the `query-http-common` per-request resolution capability: extract
`Bearer` from `Authorization`, validate (audience `kaleidoscope-query`),
reject 401 with the aegis reason and nothing read, or scope the Lumen /
Ray query to the token's tenant. The audit `subject` names `log_query` /
`trace_query`. Because all three handlers already route tenant resolution
through the shared seam (F2), this is a thin wiring slice over the shared
capability, not new auth logic. (DESIGN may collapse this into one slice
with US-RAUTH-01 if the shared capability makes wiring all three a single
small change — see `wave-decisions.md` placement note.)

### Domain Examples

#### 1: Happy Path — Nadia reads her own tenant's logs

Nadia queries `GET /api/v1/logs?start=1717000000&end=1717003600` against
`log-query-api` with a valid token for `acme-prod`, audience
`kaleidoscope-query`. `log-query-api` validates, scopes the Lumen query to
`acme-prod`, and returns `acme-prod`'s log records — same shape as today.
One `decision=allow subject=log_query tenant_id=acme-prod` audit line.

#### 2: Error/Boundary (isolation) — Mallory's cross-tenant trace lookup returns ABSENT

`acme-prod` has a trace `7f3a...` in Ray. Mallory holds a valid token for
`globex-staging` and does a trace lookup-by-id for `7f3a...` against
`trace-query-api`. The query is scoped to `globex-staging`; the trace is
NOT in `globex-staging`'s data, so the response is empty/not-found.
`acme-prod`'s trace is ABSENT from Mallory's response — isolation holds on
traces, including the lookup-by-id path (ADR-0053).

#### 3: Error/Boundary (fail-closed) — Nadia's invalid token on logs is refused before the store

Nadia presents a token with a bad signature to `GET /api/v1/logs`.
`log-query-api` returns 401, `WWW-Authenticate: Bearer`, reason
`invalid_signature`, one deny audit line, and the Lumen store is never
queried. The secret and token appear nowhere.

### UAT Scenarios (BDD)

#### Scenario: An authenticated client reads its own tenant's logs

```gherkin
Given log-query-api is running with read auth configured and a catalogue containing "acme-prod"
And tenant "acme-prod" has log records in the Lumen store
And Nadia presents a valid bearer token for "acme-prod"
When Nadia sends GET /api/v1/logs over a valid window with that token
Then log-query-api returns "acme-prod" logs with the same response shape as today
And exactly one audit event with decision "allow", subject "log_query", tenant_id "acme-prod" is emitted
```

#### Scenario: A token for one tenant cannot read another tenant's traces (isolation)

```gherkin
Given trace-query-api is running with read auth configured and a catalogue containing "acme-prod" and "globex-staging"
And tenant "acme-prod" has a trace stored and "globex-staging" has none
And Mallory presents a valid bearer token for "globex-staging"
When Mallory looks up that trace id with her token
Then trace-query-api returns a result scoped to "globex-staging" with the trace absent
And no data belonging to "acme-prod" appears in the response
And exactly one audit event with decision "allow", subject "trace_query", tenant_id "globex-staging" is emitted
```

#### Scenario: A request with no bearer token is refused 401 before the store, on both log and trace APIs

```gherkin
Given log-query-api and trace-query-api are running with read auth configured
And a client sends no Authorization header
When the client sends GET /api/v1/logs and GET /api/v1/traces respectively
Then each responds 401 Unauthorized with a WWW-Authenticate: Bearer header
And neither the Lumen store nor the Ray store is queried
And exactly one deny audit event with reason "missing_claim" is emitted per request
```

#### Scenario: An invalid-signature token is refused with the matching reason and nothing is read

```gherkin
Given log-query-api is running with read auth configured
And Nadia presents a token whose signature does not verify
When Nadia sends GET /api/v1/logs with that token
Then log-query-api responds 401 Unauthorized
And the Lumen store is never queried
And exactly one audit event with decision "deny", subject "log_query", reason "invalid_signature" is emitted
```

### Acceptance Criteria

- [ ] **a-valid-token-reads-its-own-tenant** for logs (`/api/v1/logs`) and
  traces (`/api/v1/traces` + trace lookup-by-id), with the existing
  response shape.
- [ ] **tenant-isolation-positive-and-negative-control** for logs and
  traces: an `acme-prod` token sees `acme-prod`'s data; a `globex-staging`
  token sees it ABSENT (including the trace lookup-by-id path).
- [ ] **no-token-is-refused-401-before-the-store** on both APIs: a
  tokenless request returns 401 + `WWW-Authenticate: Bearer` and the
  Lumen / Ray store is never queried.
- [ ] **invalid-token-refused-with-the-matching-reason**: a bad-signature
  token returns 401 reason `invalid_signature`, nothing read.
- [ ] **one-audit-event-per-request**: `subject` correctly names
  `log_query` / `trace_query`; exactly one decision event per request.
- [ ] **shared-capability-reused**: the per-request resolution lands in
  `query-http-common` and is reused by all three APIs (no per-crate
  duplication); existing log/trace slice tests stay green.

### Outcome KPIs

- **Who**: clients querying logs and traces on the live read APIs.
- **Does what**: read only their own tenant's logs/traces, gated by a
  valid bearer token, when auth is configured.
- **By how much**: 100% of authenticated log/trace reads scoped to the
  token's tenant (positive + negative control); 100% of invalid/missing
  (auth-on) log/trace requests refused 401 with the store never touched.
  Full read surface = 3 APIs.
- **Measured by**: audit events (`subject=log_query`/`trace_query`,
  `tenant_id`) + isolation tests + store-read correlation.
- **Baseline**: 0% per-request authenticated on logs/traces (one tenant
  per process today).

### Technical Notes

- Reuses the US-RAUTH-01 `query-http-common` capability verbatim; mirrors
  the logs spine across logs + traces (the trace API has the extra
  lookup-by-id path, ADR-0053, which must ALSO be isolated).
- aperture's per-signal symmetry has a read-side analogue: the three
  handlers already share the tenant seam, so this is lower-risk than the
  WS.
- Depends on US-RAUTH-01 (the shared capability).

---

## US-RAUTH-04 — Legible denials + the cross-surface audience fence

### Elevator Pitch

- **Before**: read-path rejections (once they exist) are uniform
  "unauthorized" with no machine-readable cause, and there is no guard
  stopping an INGEST-audience token from being replayed against the read
  APIs — a token minted to write could be used to read.
- **After**: every rejected read request carries exactly one of the 8
  aegis reasons (`invalid_signature`, `expired`, `wrong_issuer`,
  `wrong_audience`, `missing_claim`, `unknown_tenant`, `unknown_role`,
  `malformed`) in its audit event, so Priya can `grep reason=` and triage;
  AND a token minted for the INGEST audience (`kaleidoscope-ingest`) is
  rejected `wrong_audience` on the read path — the cross-surface fence
  holds.
- **Decision enabled**: Priya triages read-auth incidents by reason
  (route `expired` to "tell the client to refresh", escalate
  `invalid_signature` as possible forgery) AND certifies that an ingest
  token cannot read (and the role question is resolved/deferred with a
  recorded decision).

### Problem

A fail-closed read boundary that rejects everything invalid is necessary
but not sufficient for operations and not sufficient for the cross-surface
security story: Priya must (a) know WHY each read request was rejected to
triage, and (b) be certain an ingest-minted token cannot be replayed to
read another surface's data. The aegis taxonomy exists (8 `reason()`
strings) and the audience distinction is the fence; this story guarantees
the read APIs surface the reason per request and enforce the
read-audience.

### Who

- **Priya**, platform-security operator | triages read-auth reject floods
  and audit queries | motivated to distinguish benign expiry from active
  forgery and to prove ingest tokens cannot read.
- **Riley**, SRE on the security audit | queries the audit stream by
  reason | motivated to answer "how many wrong-audience read attempts last
  week?".
- **Trent**, a caller holding an INGEST-audience token | tries to use it
  to read | must be rejected `wrong_audience`.

### Solution

Guarantee the aegis `reason()` taxonomy flows into each read API's
per-request deny audit event for every variant, and enforce the read
audience `kaleidoscope-query` so an `kaleidoscope-ingest` token rejects
`wrong_audience`. Resolve DD6's role question: v0 is authentication +
tenant-scoping only (any catalogued `viewer`/`operator` token may read),
`unknown_role` rejected free; role-gated read authorization deferred with
the decision recorded.

### Domain Examples

#### 1: Happy Path (triage) — Priya distinguishes expiry from forgery on the read path

Over one hour the read-API audit stream shows 318 `reason=expired`
denials (one team's query token lapsed) and 2 `reason=invalid_signature`
denials (from an unfamiliar source IP). Priya routes the 318 to "tell the
team to refresh" and escalates the 2 as possible forgery. Both are
distinct filterable lines.

#### 2: Error/Boundary (the audience fence) — Trent's ingest token cannot read

Trent presents a well-formed, correctly-signed token whose `aud` is
`kaleidoscope-ingest` (minted to WRITE telemetry) to `GET
/api/v1/query_range`. `query-api` rejects with 401, reason
`wrong_audience`, one deny audit line, nothing read — an ingest token
cannot be replayed to read.

#### 3: Error/Boundary — unknown role on a read

A token validates on signature/exp/issuer/audience/tenant but carries
`kaleidoscope_role=auditor` (not `viewer`/`operator`). aegis rejects it
`unknown_role`; the read API surfaces reason `unknown_role`, nothing read.
(v0 does NOT additionally require a specific role to read — a valid
`viewer` token reads; role-gated read authorization is deferred, DD6.)

### UAT Scenarios (BDD)

#### Scenario: A token minted for the ingest audience is rejected on the read path

```gherkin
Given query-api is running with read auth configured for audience "kaleidoscope-query"
And Trent presents a correctly-signed token whose audience is "kaleidoscope-ingest"
When Trent sends GET /api/v1/query_range with that token
Then query-api responds 401 Unauthorized
And the store is never queried
And exactly one audit event with decision "deny", reason "wrong_audience" is emitted
```

#### Scenario: A token with an unknown role is rejected with the unknown-role reason

```gherkin
Given a read API is running with read auth configured and a catalogue containing "acme-prod"
And a client presents an otherwise-valid token whose role claim is "auditor"
When the client sends a read request with that token
Then the read API responds 401 Unauthorized
And exactly one audit event with decision "deny", reason "unknown_role" is emitted
And nothing is read
```

#### Scenario: Each of the eight rejection reasons appears with its own distinct value on the read path

```gherkin
Given a read API is running with read auth configured
When clients send read requests that fail validation for each distinct cause
Then a request with no token is rejected with reason "missing_claim"
And a request with a bad signature is rejected with reason "invalid_signature"
And an expired token is rejected with reason "expired"
And a wrong-issuer token is rejected with reason "wrong_issuer"
And an ingest-audience token is rejected with reason "wrong_audience"
And an unknown-tenant token is rejected with reason "unknown_tenant"
And an unknown-role token is rejected with reason "unknown_role"
And a malformed token is rejected with reason "malformed"
```

### Acceptance Criteria

- [ ] **rejected-with-the-matching-reason**: each of the 8 aegis
  `ValidationError` variants surfaces with its matching `reason` string in
  the per-request deny audit event on the read path.
- [ ] **ingest-audience-token-rejected-wrong-audience**: a token minted
  for `kaleidoscope-ingest` is rejected `wrong_audience` on the read APIs
  (the cross-surface fence), nothing read.
- [ ] **one-audit-event-per-rejected-request**: exactly one deny event per
  rejected read request, never zero, never duplicated, across all 8
  reasons.
- [ ] The reasons are mutually distinct (`malformed` != `invalid_signature`
  != `missing_claim`).
- [ ] **DD6-role-question-resolved**: a recorded decision that v0 read
  auth is authentication + tenant-scoping only (any catalogued
  `viewer`/`operator` reads; `unknown_role` rejected), with role-gated
  read authorization explicitly deferred and the rationale captured.

### Outcome KPIs

- **Who**: operators triaging read-auth denials and certifying the
  cross-surface fence.
- **Does what**: distinguish denial causes by reason and prove ingest
  tokens cannot read.
- **By how much**: 100% of read-path denials carry exactly one of the 8
  distinct reasons (no "unknown/other" bucket); 100% of ingest-audience
  tokens rejected `wrong_audience` on the read path.
- **Measured by**: distribution of the `reason` field across read deny
  events; the wrong-audience reject rate for ingest-audience tokens.
- **Baseline**: n/a (no read-path denials emitted today).

### Technical Notes

- The taxonomy is `aegis::ValidationError::reason()`
  (`crates/aegis/src/validator.rs:96-107`) — reused verbatim.
- The audience fence is the SAME `aegis::Validator` exact-audience check
  (`validator.rs:228-233`) configured with `kaleidoscope-query`; ADR-0068
  US-AUTH-04 already names `kaleidoscope-query` as the read-path audience.
- DD5 (one deny event per request) + DD6 (audience fence + role question).
- Depends on US-RAUTH-01 + US-RAUTH-03.
</content>
