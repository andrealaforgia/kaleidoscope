<!-- markdownlint-disable MD024 -->

# aegis-ingest-auth-v0 — user stories

Five LeanUX user stories wiring the correct-but-unwired `aegis::Validator`
onto the live `aperture` OTLP ingest path, fail-closed. Each story carries
the mandatory Elevator Pitch (Before / After / Decision enabled) per the
nWave DISCUSS template, concrete domain examples with real data, BDD UAT
scenarios derived from the job, embedded acceptance criteria, and outcome
KPIs.

The principal user is **Priya, a platform-security operator** running a
multi-tenant Kaleidoscope deployment for `acme-observability`. Priya owns
the running `aperture` gateway and is accountable to a security audit
that asks "can an unauthenticated caller write telemetry, and can a
caller forge another tenant's id?". Today the honest answer is "yes to
both" — and Priya needs that to become "no to both, fail-closed".

The secondary user is **Diego, an SRE** who operates client fleets that
ship OTLP to the gateway. Diego presents a bearer token on every export
and needs a correctly-authenticated client to keep ingesting exactly as
before, and an incident-readable signal when a token is rejected.

The adversary persona is **Mallory, an unauthenticated caller** on the
same network who today can POST OTLP with any `tenant_id` she likes and
have it accepted and stored under a victim tenant.

## System Constraints

These cross-cutting constraints apply to every story. They are
requirements, not mechanisms; DESIGN owns the how (see
`wave-decisions.md` DD1-DD7).

1. **Reuse the correct validator verbatim.** `aegis::Validator`,
   `TenantContext`, `ValidationError`, and `load_catalogue` are reused
   as-is. This feature WIRES the validator onto the gateway; it does NOT
   change the validator's crypto. HS256 pre-shared key, alg-confusion-
   safe, fail-closed `exp`, exact issuer + audience, catalogue-checked
   tenant — all inherited from aegis v0.

2. **aperture must depend on aegis.** It does not today (verified: zero
   `aegis` references in `crates/aperture/`). DESIGN adds the path
   dependency (non-wildcard, per the workspace `cargo deny` rule).

3. **Fail-closed is the whole point.** Any ambiguity REJECTS, never
   default-accepts. A missing token, an empty token, a malformed token,
   an expired token, a wrong-issuer/audience token, an unknown-tenant
   token, an unknown-role token — all reject, with nothing reaching the
   sink. **No token means no ingest. There is never a silent
   default-tenant accept.**

4. **The HS256 secret is sensitive and must never be logged.** Not in a
   Debug, not in a Display, not in a config-validation error, not in an
   audit event, not in a reject status/body. aegis already renders the
   key opaque in `Validator`'s Debug; aperture must not undo that and
   must not echo the secret from its own config layer.

5. **No regression of the existing ingest happy path.** A correctly-
   authenticated client ingests exactly as a client does today, with the
   same accept response shape, the same backpressure (concurrency cap)
   behaviour, the same graceful-shutdown / serve-loop behaviour. The
   existing aperture slice tests stay green.

6. **Audit alignment.** Denials and accepts emit structured events using
   aegis's locked field contract (`tenant_id`, `role`, `decision`,
   `subject`, `reason`) and aperture's closed event vocabulary; the
   `subject` names the ingest action (e.g. `ingest_logs`). Exactly one
   decision event per request — never zero, never duplicated.

7. **Scope fence.** v0 = the ingest path only. Read-path auth
   (query-api / log-query-api / trace-query-api) is a separate future
   feature. SPIFFE / RS256 / JWKS / OPA are aegis v1, OUT.

8. **Inherited gates.** ADR-0005's five gates; per-feature mutation
   testing at 100% on the modified files; Rust idiomatic; never 1.0.0.

---

## US-AUTH-01 — Walking skeleton: authenticate the bearer token on gRPC logs, fail-closed

### Elevator Pitch

- **Before**: a client opens a gRPC connection to `aperture:4317` and
  calls `ExportLogsServiceRequest` with **no `authorization` metadata**
  (or a forged `tenant_id` baked into the payload). Aperture accepts the
  batch and the records land in the sink under whatever tenant the
  caller claimed.
- **After**: the same gRPC `Export` call **without a valid bearer token**
  returns gRPC status **`UNAUTHENTICATED`** with a reason from the aegis
  taxonomy (e.g. `missing_claim`, `expired`), the operator sees one
  structured `decision=deny` line on aperture's stderr, and **zero
  records reach the sink** (no `event=sink_accepted` line). A call WITH a
  valid `Bearer <jwt>` for a catalogued tenant accepts exactly as today
  and the accepted record is tagged with the tenant from the validated
  token.
- **Decision enabled**: Priya can answer the audit's "can an
  unauthenticated caller write telemetry on gRPC?" with a demonstrated
  **no** — she runs the gateway, sends a tokenless gRPC export, and shows
  the `UNAUTHENTICATED` status plus the absent `sink_accepted` line.

### Problem

Priya is a platform-security operator who must certify that the
multi-tenant gateway enforces tenant identity at the boundary. She finds
it impossible to certify today: `aperture`'s gRPC `LogsServiceImpl::export`
never reads request metadata, so any caller writes logs under any tenant
id, and the only "enforcement" is a typed `TenantId` newtype whose
provenance is an unauthenticated payload.

### Who

- **Priya**, platform-security operator | runs the live `aperture`
  gateway for `acme-observability` | motivated to pass a tenant-isolation
  security audit and to stop trusting caller-claimed tenants.
- **Diego**, SRE running a client fleet | exports OTLP logs over gRPC with
  a bearer token | motivated to keep ingesting without interruption.
- **Mallory**, unauthenticated caller | sends tokenless gRPC exports |
  motivated to write under a victim tenant — must be rejected.

### Solution

Wire `aegis::Validator` into aperture's gRPC logs path. Construct the
validator once at startup from aperture config (HS256 secret + issuer +
audience + catalogue path — DD1). In `LogsServiceImpl::export`, extract
the bearer token from the gRPC `authorization` metadata before the body
is read, call `validate`, and: on failure return `Status::unauthenticated`
with the aegis reason and route nothing to the sink; on success thread
the authenticated `TenantContext.tenant_id` into the accepted logs record
(DD3 ripple).

### Domain Examples

#### 1: Happy Path — Diego's authenticated gRPC logs export

Diego's `payments-api` fleet exports a batch of 3 log records over gRPC
to `aperture:4317`, presenting `authorization: Bearer <jwt>` where the
JWT is HS256-signed by `acme-observability`'s issuer, audience
`kaleidoscope-ingest`, `tenant_id=acme-prod` (a catalogued tenant),
`kaleidoscope_role=operator`, `exp` 30 minutes in the future. Aperture
validates, accepts, and the sink records the batch tagged
`tenant_id=acme-prod`. The accept response is byte-identical to today's.

#### 2: Error/Boundary — Mallory's tokenless gRPC export

Mallory calls `ExportLogsServiceRequest` over gRPC with **no
`authorization` metadata**, with a payload whose resource attributes
claim `tenant_id=acme-prod`. Aperture rejects with gRPC `UNAUTHENTICATED`,
reason `missing_claim` (no token at all), emits one
`decision=deny subject=ingest_logs reason=missing_claim` audit line, and
**no `event=sink_accepted`** line appears — nothing is stored under
`acme-prod`.

#### 3: Error/Boundary — Diego's expired token

Diego's fleet ships a batch with a `Bearer <jwt>` that was valid an hour
ago but whose `exp` is now 5 minutes in the past (catalogued tenant
`acme-prod`, correct issuer/audience/signature). Aperture rejects with
gRPC `UNAUTHENTICATED`, reason `expired`, one deny audit line, nothing
stored. Diego's fleet refreshes its token and the next export (Example 1)
succeeds.

### UAT Scenarios (BDD)

#### Scenario: An authenticated client ingests logs over gRPC and the batch is tagged with its tenant

```gherkin
Given aperture is running with auth configured for issuer "acme-observability",
  audience "kaleidoscope-ingest", and a tenant catalogue containing "acme-prod"
And Diego presents a valid HS256 bearer token for tenant "acme-prod" with role "operator"
When Diego sends a gRPC ExportLogsServiceRequest of 3 log records with that token
Then aperture accepts the batch with the same success response as an unauthenticated v0 accept
And the sink records the batch tagged with tenant "acme-prod"
And exactly one audit event with decision "allow", subject "ingest_logs", tenant_id "acme-prod" is emitted
```

#### Scenario: A request with no bearer token is rejected unauthenticated and nothing is stored

```gherkin
Given aperture is running with auth configured for tenant catalogue containing "acme-prod"
And Mallory presents no authorization metadata on the gRPC call
When Mallory sends a gRPC ExportLogsServiceRequest claiming tenant "acme-prod" in the payload
Then aperture rejects the call with gRPC status UNAUTHENTICATED
And no record reaches the sink (no sink_accepted event is emitted)
And exactly one audit event with decision "deny", subject "ingest_logs", reason "missing_claim" is emitted
```

#### Scenario: An expired token is rejected with the matching reason and nothing is stored

```gherkin
Given aperture is running with auth configured for tenant catalogue containing "acme-prod"
And Diego presents a bearer token for tenant "acme-prod" whose exp is in the past
When Diego sends a gRPC ExportLogsServiceRequest with that token
Then aperture rejects the call with gRPC status UNAUTHENTICATED
And no record reaches the sink
And exactly one audit event with decision "deny", subject "ingest_logs", reason "expired" is emitted
```

#### Scenario: A correctly-authenticated client is not slower or shaped differently than today (non-regression)

```gherkin
Given aperture is running with auth configured and a catalogued tenant "acme-prod"
And Diego presents a valid bearer token for "acme-prod"
When Diego sends a gRPC ExportLogsServiceRequest that the OTLP harness accepts
Then the accept response shape is identical to the pre-auth v0 accept response
And the existing backpressure, shutdown, and serve-loop behaviours are unchanged
```

### Acceptance Criteria

- [ ] **no-token-is-rejected-unauthenticated-nothing-stored**: a gRPC
  logs export with no `authorization` metadata returns `UNAUTHENTICATED`
  and emits no `sink_accepted` event.
- [ ] **a-valid-token-ingests-tagged-with-its-tenant**: a gRPC logs
  export with a valid bearer token for a catalogued tenant accepts and
  the sink record carries that tenant's `tenant_id`.
- [ ] **expired-token-rejected-with-the-matching-reason**: an expired
  token rejects `UNAUTHENTICATED` with reason `expired`.
- [ ] **one-audit-event-per-request**: exactly one decision event
  (`allow` on accept, `deny` on reject) per gRPC logs request — never
  zero, never duplicated.
- [ ] Non-regression: a correctly-authenticated accept is byte-shape
  identical to the current v0 accept; existing aperture slice tests stay
  green.

### Outcome KPIs

- **Who**: clients ingesting logs over gRPC to the live aperture gateway.
- **Does what**: ingest only when carrying a valid bearer token; accepted
  batches are tagged with the token's tenant.
- **By how much**: 100% of accepted gRPC logs batches carry an
  authenticated tenant id; 100% of tokenless/invalid gRPC logs requests
  are rejected with nothing stored.
- **Measured by**: aperture audit events (`decision`, `subject`,
  `tenant_id`) correlated with `sink_accepted` events.
- **Baseline**: 0% authenticated (no auth on the path today); 0% of
  invalid requests rejected.

### Technical Notes

- Reuse `aegis::Validator::validate` verbatim; construct once at startup.
- DD1: aperture config needs new HS256 secret / issuer / audience /
  catalogue-path fields (the reserved hooks are TLS+SPIFFE, not HS256).
- DD3: `ingest_logs` signature + `SinkRecord` tenant ripple — DESIGN maps
  it; constrained by the `single-validator-per-signal` CI invariant and
  the `#[non_exhaustive]` SinkRecord.
- Depends on: aegis v0 (`Validator`, `TenantContext`, `load_catalogue`) —
  available. ADR-0061 fail-closed precedent — available.

---

## US-AUTH-02 — Fail-closed auth configuration: aperture refuses to run an unauthenticated ingest path by accident

### Elevator Pitch

- **Before**: aperture starts and binds its listeners regardless of any
  auth configuration; there is no auth, so there is nothing to forget.
  Once auth exists but is gated by an off-by-default flag, an operator who
  forgets the flag silently ships an open gateway.
- **After**: when Priya runs `aperture --config aperture.toml`, the
  gateway will not serve the ingest path unauthenticated by accident: if
  auth config is absent or incomplete in a way that would leave the
  boundary open, aperture **refuses to start** at config-validation time
  with `event=config_validation_failed`, exit code 2, and **no listener
  binds** — exactly the ADR-0061 fail-closed reflex. A complete auth
  config starts normally.
- **Decision enabled**: Priya can deploy with confidence that a
  misconfiguration fails loud (refuse-to-start) rather than silently
  shipping an unauthenticated gateway — she reads the exit code and the
  stderr line, not a packet capture after every deploy.

### Problem

Priya needs the gateway to be safe-by-default. The ADR-0061 precedent
established that a security property v0 cannot honour must refuse-to-start
rather than downgrade silently. The symmetric trap for auth: a flag that
defaults OFF turns "forgot to enable auth" into "silently running an open
gateway" — the exact silent downgrade ADR-0061 closed for TLS.

### Who

- **Priya**, platform-security operator | writes `aperture.toml` and runs
  the binary | motivated to never ship an open gateway by omission.

### Solution

Make the ingest auth boundary fail-closed by configuration. DESIGN
chooses the mechanism (DD4): either auth is unconditionally on for the
ingest path, or auth-config-present ⇒ enforced and auth-config-
absent/incomplete ⇒ refuse-to-start at `RawConfig::into_config` (the
ADR-0061 seam) with `event=config_validation_failed`, exit 2, no listener
bound. The requirement is the observable fail-closed property; the secret
is never echoed in the refusal.

### Domain Examples

#### 1: Happy Path — Priya's complete auth config starts

Priya's `aperture.toml` carries `[aperture.security.auth.jwt]` with the
issuer `acme-observability`, audience `kaleidoscope-ingest`, an HS256
secret sourced from a file, and a catalogue path. `aperture --config
aperture.toml` starts, binds `:4317` + `:4318`, emits `startup` then
`ready`, and the ingest path is authenticated.

#### 2: Error/Boundary — Priya forgets the auth config and the gateway refuses to start

Priya's `aperture.toml` has the transport sections but no auth config at
all (in a deployment where DESIGN's mechanism requires it). `aperture
--config aperture.toml` exits **2** with a stderr line
`event=config_validation_failed reason: <names the missing auth config>`,
and **no listener binds** (no `listener_bound`, no `ready`). Priya adds
the auth config and Example 1 starts.

#### 3: Error/Boundary — the secret is referenced but unreadable

Priya's config points the HS256 secret at a file path that does not
exist. aperture refuses to start with `event=config_validation_failed`
naming the unreadable secret source (by path, **never echoing any secret
bytes**), exit 2, no listener bound.

### UAT Scenarios (BDD)

#### Scenario: A complete auth configuration starts the gateway with the ingest path authenticated

```gherkin
Given Priya's aperture config contains a complete JWT auth section with issuer, audience, secret source, and catalogue path
When Priya runs aperture with that config
Then aperture binds both listeners and emits the ready event
And the ingest path requires a valid bearer token from that point on
```

#### Scenario: A configuration that would leave the ingest path unauthenticated refuses to start

```gherkin
Given Priya's aperture config omits the auth configuration the ingest path requires
When Priya runs aperture with that config
Then aperture exits with code 2
And a config_validation_failed event names the missing auth configuration on stderr
And no listener binds
```

#### Scenario: The secret is never echoed in a configuration error

```gherkin
Given Priya's aperture config references an HS256 secret source that cannot be read
When aperture validates the configuration at startup
Then aperture refuses to start with a config_validation_failed event
And the error names the secret source by reference (e.g. file path) only
And no secret bytes appear anywhere in the error or logs
```

### Acceptance Criteria

- [ ] A complete auth config starts the gateway with the ingest path
  authenticated (`ready` emitted, both listeners bound).
- [ ] A config that would leave the ingest path unauthenticated by
  omission refuses to start: exit 2, `event=config_validation_failed`
  naming the missing auth config, **no listener binds** (structural,
  per the ADR-0061 seam).
- [ ] **the-secret-is-never-logged**: no secret bytes appear in any
  config-validation error or log line; secret sources are named by
  reference only.

### Outcome KPIs

- **Who**: operators deploying the aperture gateway.
- **Does what**: never ship an unauthenticated ingest path by
  configuration omission.
- **By how much**: 100% of startups that would leave the ingest path
  unauthenticated by omission exit non-zero with zero listeners bound
  (target 1.0; baseline 0.0).
- **Measured by**: exit code + `config_validation_failed` event +
  absence of `listener_bound` on the refusal path.
- **Baseline**: 0.0 — no auth, so no such refusal exists today.

### Technical Notes

- Mirrors ADR-0061 (`tls-config-reject-v0`): refuse at
  `RawConfig::into_config`, `event=config_validation_failed`, exit 2, no
  listener binds — structural guarantee (Config never constructed ⇒ bind
  path never entered).
- DD1 + DD4. The secret-never-logged invariant (System Constraint 4)
  binds here hardest.

---

## US-AUTH-03 — HTTP transport parity: authenticate the bearer token on the HTTP/protobuf logs path

### Elevator Pitch

- **Before**: even after gRPC is authenticated (US-AUTH-01), the HTTP
  front door at `aperture:4318` `POST /v1/logs` accepts OTLP with no
  `Authorization` header — the gateway is still open on HTTP.
- **After**: `POST /v1/logs` **without a valid `Authorization: Bearer
  <jwt>` header** returns **HTTP 401 Unauthorized** with a
  `WWW-Authenticate: Bearer` header and a reason from the aegis taxonomy,
  nothing stored, one deny audit line; a valid header accepts and tags
  the record with the token's tenant.
- **Decision enabled**: Priya can certify BOTH front doors are closed —
  she curls `POST /v1/logs` with no header and shows the 401, then with a
  valid token and shows the tagged accept.

### Problem

Priya cannot certify the gateway is closed while one of its two transports
is open. aperture's `handle_logs` (HTTP) never reads the `Authorization`
header today; a tokenless `POST /v1/logs` is accepted.

### Who

- **Priya**, platform-security operator | certifies both transports |
  motivated to leave no open door.
- **Diego**, SRE | some client fleets export over HTTP/protobuf | needs
  the authenticated HTTP path to keep ingesting.
- **Mallory**, unauthenticated caller | will try the HTTP door once gRPC
  is closed — must be rejected there too.

### Solution

Apply the US-AUTH-01 spine to the HTTP logs handler: extract `Bearer`
from the `Authorization` header, `validate`, reject `401` (RFC 6750
`WWW-Authenticate: Bearer`) with the aegis reason and nothing stored, or
accept and thread the tenant into the record.

### Domain Examples

#### 1: Happy Path — Diego's authenticated HTTP/protobuf logs POST

Diego's edge fleet POSTs `application/x-protobuf` logs to
`aperture:4318/v1/logs` with `Authorization: Bearer <jwt>` for catalogued
tenant `globex-staging`, role `operator`. aperture validates, accepts
with the existing 200 + empty-protobuf-body shape, and tags the record
`tenant_id=globex-staging`.

#### 2: Error/Boundary — Mallory's tokenless HTTP POST

Mallory POSTs valid OTLP/protobuf to `/v1/logs` with **no `Authorization`
header**. aperture returns `401 Unauthorized`, `WWW-Authenticate: Bearer`,
reason `missing_claim`, one deny audit line, nothing stored.

#### 3: Error/Boundary — Mallory's unknown-tenant token over HTTP

Mallory presents a well-formed, correctly-signed token for
`tenant_id=acme-prod-evil`, which is NOT in the catalogue. aperture
returns `401`, reason `unknown_tenant`, one deny audit line, nothing
stored.

### UAT Scenarios (BDD)

#### Scenario: An authenticated client ingests logs over HTTP and the record is tagged with its tenant

```gherkin
Given aperture is running with auth configured and a catalogue containing "globex-staging"
And Diego presents a valid bearer token for "globex-staging" in the Authorization header
When Diego POSTs application/x-protobuf logs to /v1/logs with that header
Then aperture accepts with the existing 200 response shape
And the sink record is tagged with tenant "globex-staging"
And exactly one audit event with decision "allow", subject "ingest_logs" is emitted
```

#### Scenario: A POST with no Authorization header is rejected 401 and nothing is stored

```gherkin
Given aperture is running with auth configured
And Mallory sends no Authorization header
When Mallory POSTs valid OTLP/protobuf logs to /v1/logs
Then aperture responds 401 Unauthorized with a WWW-Authenticate: Bearer header
And no record reaches the sink
And exactly one audit event with decision "deny", subject "ingest_logs", reason "missing_claim" is emitted
```

#### Scenario: An unknown-tenant token is rejected 401 with the matching reason

```gherkin
Given aperture is running with auth configured and a catalogue NOT containing "acme-prod-evil"
And Mallory presents a correctly-signed token claiming tenant "acme-prod-evil"
When Mallory POSTs logs to /v1/logs with that token
Then aperture responds 401 Unauthorized
And no record reaches the sink
And exactly one audit event with decision "deny", reason "unknown_tenant" is emitted
```

### Acceptance Criteria

- [ ] **no-token-is-rejected-unauthenticated-nothing-stored** (HTTP): a
  `POST /v1/logs` with no `Authorization` header returns 401 and stores
  nothing.
- [ ] **a-valid-token-ingests-tagged-with-its-tenant** (HTTP): a valid
  header accepts with the existing 200 shape and tags the record.
- [ ] **unknown-tenant-rejected-with-the-matching-reason**: an
  unknown-tenant token returns 401 reason `unknown_tenant`, nothing
  stored.
- [ ] **one-audit-event-per-request** (HTTP): exactly one decision event
  per request.
- [ ] Non-regression: the 401 path interacts correctly with the existing
  415 (unsupported-media-type) and 503 (backpressure) paths; existing
  HTTP slice tests stay green.

### Outcome KPIs

- **Who**: clients ingesting logs over HTTP/protobuf.
- **Does what**: ingest only with a valid bearer header; accepted records
  tagged with the token's tenant.
- **By how much**: 100% of accepted HTTP logs carry an authenticated
  tenant; 100% of tokenless/invalid HTTP logs rejected 401 with nothing
  stored.
- **Measured by**: HTTP status + audit events + `sink_accepted`
  correlation.
- **Baseline**: 0% (no auth on HTTP today).

### Technical Notes

- DD2 (HTTP leg): RFC 6750 `WWW-Authenticate: Bearer`; reason taxonomy in
  body; no secret/token leak.
- Reuses the US-AUTH-01 extract→validate→reject→tag spine.
- Depends on US-AUTH-01 (the spine + config + sink ripple).

---

## US-AUTH-04 — Three-signal parity: authenticate and tag traces and metrics on both transports

### Elevator Pitch

- **Before**: with logs authenticated on both transports, the traces
  (`/v1/traces`, gRPC `TraceService`) and metrics (`/v1/metrics`, gRPC
  `MetricsService`) doors are still open — a tokenless trace or metric
  export is accepted.
- **After**: every one of the three OTLP signals on both transports
  requires a valid bearer token; a tokenless or invalid traces/metrics
  export is rejected (`UNAUTHENTICATED` / `401`) with nothing stored, and
  an authenticated export tags the record with the token's tenant.
- **Decision enabled**: Priya can certify the ENTIRE ingest surface (3
  signals × 2 transports) is authenticated — she demonstrates a tokenless
  reject and a tagged accept for traces and for metrics.

### Problem

Priya's certification must cover the whole ingest surface. aperture's
`TraceServiceImpl`/`MetricsServiceImpl` (gRPC) and
`handle_traces`/`handle_metrics` (HTTP) never read a token today — an
authenticated logs path with open traces/metrics is still an open gateway.

### Who

- **Priya**, platform-security operator | certifies the full surface.
- **Diego**, SRE | fleets export all three signals | needs each
  authenticated path to keep ingesting.

### Solution

Extend the spine to traces and metrics on both transports, tagging each
accepted `SinkRecord` (Traces/Metrics) with the authenticated tenant and
rejecting invalid/missing tokens identically. The `subject` audit field
names `ingest_traces` / `ingest_metrics`.

### Domain Examples

#### 1: Happy Path — Diego's authenticated traces export over gRPC

Diego's `checkout-api` exports 3 spans over gRPC with a valid token for
tenant `acme-prod`, role `operator`. aperture validates, accepts, tags
the spans `tenant_id=acme-prod`, audit `subject=ingest_traces`.

#### 2: Error/Boundary — Mallory's tokenless metrics POST over HTTP

Mallory POSTs metrics to `/v1/metrics` with no `Authorization` header.
aperture returns 401, reason `missing_claim`, `subject=ingest_metrics`,
nothing stored.

#### 3: Error/Boundary — Diego's wrong-audience token on traces

Diego's misconfigured fleet ships a traces export with a token whose
`aud` is `kaleidoscope-query` (the read-path audience) rather than
`kaleidoscope-ingest`. aperture rejects (`UNAUTHENTICATED` / `401`),
reason `wrong_audience`, nothing stored — the ingest gateway will not
accept a token minted for a different audience.

### UAT Scenarios (BDD)

#### Scenario: An authenticated client ingests traces and the spans are tagged with its tenant

```gherkin
Given aperture is running with auth configured and a catalogue containing "acme-prod"
And Diego presents a valid bearer token for "acme-prod"
When Diego sends a gRPC ExportTraceServiceRequest of 3 spans with that token
Then aperture accepts the batch
And the sink record is tagged with tenant "acme-prod"
And exactly one audit event with decision "allow", subject "ingest_traces" is emitted
```

#### Scenario: A tokenless metrics request is rejected and nothing is stored

```gherkin
Given aperture is running with auth configured
And Mallory sends no Authorization header
When Mallory POSTs metrics to /v1/metrics
Then aperture responds 401 Unauthorized
And no record reaches the sink
And exactly one audit event with decision "deny", subject "ingest_metrics", reason "missing_claim" is emitted
```

#### Scenario: A token minted for a different audience is rejected on the ingest path

```gherkin
Given aperture is running with auth configured for audience "kaleidoscope-ingest"
And Diego presents a token whose audience is "kaleidoscope-query"
When Diego sends a traces export with that token
Then aperture rejects the request as unauthenticated
And no record reaches the sink
And exactly one audit event with decision "deny", reason "wrong_audience" is emitted
```

### Acceptance Criteria

- [ ] **a-valid-token-ingests-tagged-with-its-tenant** for traces and
  metrics on both transports.
- [ ] **no-token-is-rejected-unauthenticated-nothing-stored** for traces
  and metrics on both transports.
- [ ] **wrong-audience-rejected-with-the-matching-reason**: a token for a
  different audience is rejected on the ingest path with reason
  `wrong_audience`.
- [ ] **one-audit-event-per-request**: `subject` correctly names
  `ingest_traces` / `ingest_metrics`.
- [ ] Non-regression: existing traces/metrics slice tests + the
  signal-mismatch reject paths stay green.

### Outcome KPIs

- **Who**: clients ingesting traces and metrics on either transport.
- **Does what**: ingest only with a valid bearer token; accepted records
  tagged with the token's tenant.
- **By how much**: 100% of accepted traces/metrics (both transports)
  carry an authenticated tenant; 100% of invalid/missing rejected with
  nothing stored. Full surface = 3 signals × 2 transports.
- **Measured by**: audit events (`subject`) + status + `sink_accepted`.
- **Baseline**: 0%.

### Technical Notes

- Mirrors logs; aperture's per-signal symmetry (app.rs / transport.rs)
  makes this lower-risk than the WS.
- Depends on US-AUTH-01 + US-AUTH-03.

---

## US-AUTH-05 — Legible denials: every rejection reports the matching aegis reason (and the role question resolved)

### Elevator Pitch

- **Before**: rejections (once they exist) are uniform "unauthenticated"
  with no machine-readable cause — an operator triaging a flood of
  rejects cannot tell an expired token from a forged signature from an
  unknown tenant.
- **After**: every rejected ingest request carries exactly one of the 8
  aegis reasons (`invalid_signature`, `expired`, `wrong_issuer`,
  `wrong_audience`, `missing_claim`, `unknown_tenant`, `unknown_role`,
  `malformed`) in its audit event, so Priya can `grep reason=expired` and
  see "Diego's fleet token expired" distinctly from
  `reason=invalid_signature` ("someone is forging tokens").
- **Decision enabled**: Priya triages an incident by reason — she filters
  the audit stream by `reason` and routes "expired" (tell the client to
  refresh) differently from "invalid_signature" (escalate: possible
  attack).

### Problem

A fail-closed gateway that rejects everything invalid is necessary but not
sufficient for operations: Priya needs to know WHY each request was
rejected to triage. The aegis taxonomy exists (8 `reason()` strings); this
story guarantees the gateway surfaces it per request and resolves the open
DD6 question of whether v0 also role-gates ingest.

### Who

- **Priya**, platform-security operator | triages reject floods and audit
  queries | motivated to distinguish benign expiry from active forgery.
- **Riley**, SRE on the security audit | queries the audit stream by
  reason | motivated to answer "how many forged-signature attempts last
  week?".

### Solution

Guarantee the aegis `reason()` taxonomy flows into aperture's per-request
deny audit event for every variant, on every signal and transport. Resolve
DD6: decide whether v0 also requires the `operator` role to ingest (and if
so, `unknown_role` / a viewer-tries-to-write case rejects), or whether
role-gated authorization is explicitly deferred to a follow-up with the
decision recorded.

### Domain Examples

#### 1: Happy Path (triage) — Priya distinguishes expiry from forgery

Over one hour the audit stream shows 412 `reason=expired` denials (one
client fleet's token lapsed) and 3 `reason=invalid_signature` denials
(from an unfamiliar source IP). Priya routes the 412 to "tell the fleet to
refresh" and escalates the 3 as possible forgery. Both are distinct lines
she can filter.

#### 2: Boundary — malformed token

A client sends `Authorization: Bearer not-a-jwt`. aperture rejects with
reason `malformed`, distinct from `invalid_signature` (a structurally
valid JWT with a bad signature) and `missing_claim` (no token at all).

#### 3: Error/Boundary — unknown role (role question)

A token validates on signature/exp/issuer/audience/tenant but carries
`kaleidoscope_role=auditor` (not `viewer`/`operator`). aegis rejects it
`unknown_role`; aperture surfaces reason `unknown_role`, nothing stored.
(If DESIGN resolves DD6 to require `operator` to ingest, a valid `viewer`
token writing logs is ALSO rejected with a recorded decision — flagged for
DESIGN.)

### UAT Scenarios (BDD)

#### Scenario: A malformed token is rejected with the malformed reason, distinct from a bad signature

```gherkin
Given aperture is running with auth configured
And a client presents an Authorization header whose bearer value is not a JWT
When the client sends an ingest request with that header
Then aperture rejects the request as unauthenticated
And exactly one audit event with decision "deny", reason "malformed" is emitted
And the reason is distinct from "invalid_signature" and "missing_claim"
```

#### Scenario: A token with an unknown role is rejected with the unknown-role reason

```gherkin
Given aperture is running with auth configured and a catalogue containing "acme-prod"
And a client presents an otherwise-valid token whose role claim is "auditor"
When the client sends an ingest request with that token
Then aperture rejects the request as unauthenticated
And exactly one audit event with decision "deny", reason "unknown_role" is emitted
And no record reaches the sink
```

#### Scenario: Each of the eight rejection reasons appears with its own distinct value

```gherkin
Given aperture is running with auth configured
When clients send requests that fail validation for each distinct cause
Then a request with no token is rejected with reason "missing_claim"
And a request with a bad signature is rejected with reason "invalid_signature"
And an expired token is rejected with reason "expired"
And a wrong-issuer token is rejected with reason "wrong_issuer"
And a wrong-audience token is rejected with reason "wrong_audience"
And an unknown-tenant token is rejected with reason "unknown_tenant"
And an unknown-role token is rejected with reason "unknown_role"
And a malformed token is rejected with reason "malformed"
```

### Acceptance Criteria

- [ ] **rejected-with-the-matching-reason**: each of the 8 aegis
  `ValidationError` variants surfaces with its matching `reason` string
  in the per-request deny audit event.
- [ ] **one-audit-event-per-rejected-request**: exactly one deny event
  per rejected request, never zero, never duplicated, across all 8
  reasons.
- [ ] The reasons are mutually distinct (`malformed` ≠ `invalid_signature`
  ≠ `missing_claim`).
- [ ] **DD6 resolved**: a recorded decision on whether v0 role-gates
  ingest — either enforced (a `viewer` writing is rejected with a
  recorded decision) or explicitly deferred to a follow-up feature with
  the rationale captured.

### Outcome KPIs

- **Who**: operators triaging ingest-auth denials.
- **Does what**: distinguish denial causes by reason in the audit stream.
- **By how much**: 100% of denials carry exactly one of the 8 distinct
  reasons; an operator can filter the stream by `reason` and partition
  denials by cause with no "unknown/other" bucket.
- **Measured by**: distribution of the `reason` field across deny events;
  zero deny events lacking a reason.
- **Baseline**: n/a (no denials emitted today).

### Technical Notes

- The taxonomy is `ValidationError::reason()` (aegis validator.rs:96-107)
  — reused verbatim.
- DD5 (one deny event per request) + DD6 (role question).
- Depends on US-AUTH-01..04.
