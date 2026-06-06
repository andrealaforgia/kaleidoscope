<!-- markdownlint-disable MD024 -->

# spark-ingest-auth-v0 — user stories

Three LeanUX user stories giving the Spark SDK a way to attach a bearer
token to its OTLP exports, so an integrator can ship authenticated
telemetry through the now-fail-closed aperture gateway. This is the
**client-side sibling** of `aegis-ingest-auth-v0` (ADR-0068): the
gateway locked the door (it rejects every tokenless ingest with
`reason=missing_claim`); this feature gives the SDK the key. It
unblocks the verifier's E01-E04 (N29) — the Spark→Aperture round-trip
that was GREEN pre-auth and is now BLOCKED.

Each story carries the mandatory Elevator Pitch (Before / After /
Decision enabled), concrete domain examples with real data, BDD UAT
scenarios derived from the job, embedded acceptance criteria, and
outcome KPIs. The cross-cutting constraints live in System Constraints
below; the five DESIGN seams live in `wave-decisions.md` (DD1-DD5).

The principal user is **Marco Bianchi, a backend integrator** at
`acme-observability`. Marco owns the `payments-api` Rust service. He
instruments it with the Spark SDK and points it at his team's
Kaleidoscope gateway. Until last week his telemetry flowed; since the
gateway turned on auth, **every span, log, and metric Marco's service
emits is silently denied at the door** (`decision=deny ...
reason=missing_claim`), and Marco has no way to give the SDK a token.
He needs `payments-api` to send authenticated telemetry again — by
calling a builder method or by setting the standard OTLP headers env
var in his deployment manifest.

The secondary user is **Priya, the platform-security operator** (the
principal of the gateway sibling), who minted Marco a bearer token and
needs Marco's SDK to actually present it so her authenticated gateway
accepts his telemetry — proving the lock has a matching key.

## System Constraints

These cross-cutting constraints apply to every story. They are
requirements, not mechanisms; DESIGN owns the how (see
`wave-decisions.md` DD1-DD5).

1. **The bearer token is a SECRET and must never be logged.** Not in
   the `target="spark"` `spark::init succeeded` event, not in any other
   event, not in a `Debug`/`Display` of `SparkConfig`, not in a
   config-validation error, not in the resolved-config tracing fields.
   `SparkConfig` derives `Debug` today (`config.rs:26`) and
   `emit_init_succeeded` logs `service.name`/`endpoint`/`protocol`/
   `flush_timeout_ms` (`observability.rs:53-70`) — the token must NOT
   join that surface. This mirrors aegis/aperture's never-log-the-secret
   discipline (ADR-0068 DD1). This is the load-bearing security
   constraint.

2. **The token is supplied by the integrator, never baked.** It comes
   from a string passed to the builder knob OR from the
   `OTEL_EXPORTER_OTLP_HEADERS` env var. Spark stores and forwards it;
   Spark never hard-codes, generates, or persists it.

3. **The token must reach ALL THREE signals uniformly.** When a token
   is configured, the `authorization: Bearer <token>` metadata rides on
   the `SpanExporter`, the `LogExporter`, AND the `MetricExporter`
   (`init.rs:282-352`). A partial wire that authenticates traces but not
   logs is the verifier's E01-E04 failure (E01-E04 cover traces AND
   logs) and is the explicit non-goal. DESIGN applies one helper
   uniformly (DD1).

4. **The no-auth path must not break.** When no token is configured (no
   builder call, no env var), Spark adds **no** authorization header —
   exactly as it does today. An integrator targeting an unauthenticated
   local collector keeps working unchanged. Spark's existing slice tests
   (slice_01..slice_07) stay green.

5. **Add auth metadata; do not restructure the pipeline.** This feature
   adds an `authorization` header to the existing exporter-build path in
   `build_pipeline`. It does not change the batch processors, the
   provider construction, the resource composition, the resolution
   chain, the flush mechanism, or the single-init invariant. The
   `#[non_exhaustive]` `SparkConfig` (`config.rs:25`) makes the new
   field a non-breaking addition.

6. **Inherited gates.** ADR-0005's five gates; per-feature mutation
   testing at 100% on the modified spark files (`gate-5-mutants-spark`
   exists); Rust idiomatic; never 1.0.0.

---

## US-SP-AUTH-01 — Driving slice: a programmatic bearer token authenticates all three signals at the gateway

### Elevator Pitch

- **Before**: Marco's `payments-api` calls
  `SparkConfig::for_service("payments-api").with_endpoint("https://aperture.acme.internal:4317")`
  and `spark::init`s. The SDK opens gRPC connections and exports spans,
  logs, and metrics with **no `authorization` metadata**. The
  now-authenticated aperture rejects every batch
  (`decision=deny subject=ingest_traces reason=missing_claim`, and the
  same for `ingest_logs`/`ingest_metrics`) — Marco's telemetry never
  reaches the sink. His dashboards are empty and he does not know why.
- **After**: Marco adds one builder call —
  `.with_bearer_token(token)` — where `token` is the HS256 JWT Priya
  minted for tenant `acme-prod`. Now the SDK attaches `authorization:
  Bearer <jwt>` to the span, log, AND metric exporters, aperture
  **accepts** all three signals (`decision=allow`, the records land in
  the sink tagged `tenant_id=acme-prod`), and Marco's dashboards fill in
  again. The negative control still holds: with **no**
  `with_bearer_token` call, Spark adds no header and an unauthenticated
  local collector still accepts (System Constraint 4).
- **Decision enabled**: Marco can answer "is my service's telemetry
  getting through the secured gateway?" with a demonstrated **yes** — he
  adds the one knob, runs `payments-api`, and sees the spans/logs/metrics
  arrive at the gateway (the `sink_accepted` lines, the `decision=allow`
  audit) instead of the silent `missing_claim` denials. This is exactly
  the verifier's E01-E04 flipping back GREEN.

### Problem

Marco is a backend integrator who instruments his Rust service with the
Spark SDK. Since aperture turned on fail-closed ingest auth (ADR-0068),
his telemetry is silently denied at the door — and `SparkConfig` has no
auth knob: `with_endpoint` (`config.rs:120`) is the only transport
knob, and none of the three OTLP exporters (`init.rs:282-352`) attaches
an `authorization` header. Marco literally cannot send the token his
gateway demands. The SDK can only talk to an unauthenticated gateway,
which at v0's fail-closed posture no longer exists.

### Who

- **Marco Bianchi**, backend integrator at `acme-observability` |
  instruments the `payments-api` Rust service with the Spark SDK and
  points it at the team's authenticated gateway | motivated to get his
  service's telemetry accepted again instead of silently denied.
- **Priya**, platform-security operator | minted Marco's bearer token
  and runs the authenticated gateway | motivated to see the matching key
  present a valid token so her lock accepts legitimate clients.

### Solution

Add a programmatic auth knob to `SparkConfig` (DD2 — at minimum
`with_bearer_token(token)`, meaning `authorization: Bearer <token>`).
In `build_pipeline`, when a token is configured, attach the
`authorization` metadata uniformly to the `SpanExporter`, `LogExporter`,
and `MetricExporter` via one shared helper (DD1), reusing the existing
exporter-build path. The token field is redacted on every loggable
surface (System Constraint 1, DD3).

### Domain Examples

#### 1: Happy Path — Marco's authenticated three-signal export is accepted

Marco's `payments-api` calls
`SparkConfig::for_service("payments-api").with_tenant_id("acme-prod").with_endpoint("https://aperture.acme.internal:4317").with_bearer_token(jwt)`,
where `jwt` is the HS256 token Priya minted: issuer
`acme-observability`, audience `kaleidoscope-ingest`, `tenant_id=acme-prod`
(a catalogued tenant), role `operator`, `exp` 30 minutes out. The
service emits 3 spans, 5 logs, and a metric. aperture validates the
bearer on each signal, accepts all three, and the sink records them
tagged `tenant_id=acme-prod`. Marco's dashboard shows the data.

#### 2: Edge Case — Marco's service emits only metrics, and they are authenticated too

Marco's batch job emits no spans and no logs in a given window — only a
periodic metric export. With `with_bearer_token(jwt)` set, the
`MetricExporter` still carries `authorization: Bearer <jwt>` and the
metric is accepted (`decision=allow subject=ingest_metrics`). The token
reaching the metric path is not an afterthought — a partial wire that
authenticated only traces+logs would silently drop this integrator's
only signal.

#### 3: Error/Boundary — Marco's token has expired; the gateway rejects, Spark sent it honestly

Marco's deployment has a stale `jwt` whose `exp` is now in the past.
Spark attaches `authorization: Bearer <jwt>` exactly as configured (its
job is to SEND the token it was given, DD5); aperture rejects with
`UNAUTHENTICATED reason=expired`, nothing stored. Marco refreshes the
token in his config and Example 1 succeeds. Spark logged nothing about
the token value in either case (System Constraint 1).

### UAT Scenarios (BDD)

#### Scenario: A bearer-configured export is accepted by the authenticated gateway

```gherkin
Given an aperture gateway is running with ingest auth configured for a catalogued tenant "acme-prod"
And Priya has minted Marco a valid HS256 bearer token for "acme-prod" with role "operator"
And Marco configures Spark with that endpoint and SparkConfig::with_bearer_token(token)
When Marco's payments-api service exports spans through Spark
Then the gateway accepts the export
And the sink records the spans tagged with tenant "acme-prod"
And the gateway emits decision "allow" for subject "ingest_traces"
```

#### Scenario: The token reaches all three signals

```gherkin
Given an aperture gateway is running with ingest auth configured for "acme-prod"
And Marco configures Spark with a valid bearer token for "acme-prod"
When Marco's service exports traces and logs and metrics through Spark
Then the gateway accepts the traces export
And the gateway accepts the logs export
And the gateway accepts the metrics export
And each accepted record is tagged with tenant "acme-prod"
```

#### Scenario: An export configured with an expired token is sent honestly and rejected by the gateway

```gherkin
Given an aperture gateway is running with ingest auth configured for "acme-prod"
And Marco configures Spark with a bearer token for "acme-prod" whose exp is in the past
When Marco's service exports telemetry through Spark
Then Spark attaches the configured bearer token to the export
And the gateway rejects the export as unauthenticated with reason "expired"
And no record reaches the sink
```

### Acceptance Criteria

- [ ] **a-bearer-configured-export-is-accepted-by-the-authenticated-gateway**:
  with `SparkConfig::with_bearer_token(<valid jwt>)` set, an export to
  the authenticated aperture is ACCEPTED and the record is tagged with
  the token's tenant — the same export with no token is DENIED
  (`missing_claim`). This MUST fail against today's no-knob code (no
  way to set a token) and pass only once the knob attaches the header.
- [ ] **the-token-reaches-all-three-signals**: when a token is
  configured, `authorization: Bearer <token>` rides on the
  `SpanExporter`, `LogExporter`, AND `MetricExporter` — all three are
  accepted; no signal is left un-authenticated by omission.
- [ ] Spark attaches the configured token verbatim and SENDS it; a
  gateway rejection (expired / invalid) is the gateway's surfacing, not
  Spark's concern (DD5) — Spark's job is correct transmission.
- [ ] Non-regression: with no token configured, Spark's existing
  exporter behaviour is byte-unchanged (System Constraint 4); slice_01..
  slice_07 stay green.

### Outcome KPIs

- **Who**: integrators whose Spark-instrumented services export to an
  authenticated Kaleidoscope gateway.
- **Does what**: ship authenticated telemetry that is ACCEPTED, by
  configuring a bearer token on the SDK — across all three OTLP signals.
- **By how much**: 100% of the three signals carry the configured bearer
  when a token is set (target 1.0); the verifier's E01-E04
  (Spark→Aperture round-trip) return GREEN (currently BLOCKED).
- **Measured by**: the E01-E04 round-trip suite + the gateway's
  `decision=allow`/`sink_accepted` audit for each of `ingest_traces` /
  `ingest_logs` / `ingest_metrics` correlated with a Spark export.
- **Baseline**: 0% — there is no auth knob today; every signal is denied
  `missing_claim` against the authenticated gateway; E01-E04 BLOCKED.

### Technical Notes

- F1: `SparkConfig` (`config.rs:27`) has no auth field;
  `#[non_exhaustive]` makes the addition non-breaking.
- F2: all three exporters (`init.rs:282-352`)
  `.with_tonic().with_endpoint(...).build()` with no `.with_metadata`;
  DD1 adds the metadata uniformly via one helper.
- DD2: the `with_bearer_token` surface (and optional
  `with_auth_header`); DD3: the token is redacted on every loggable
  surface (System Constraint 1).
- F5: the token shape aperture demands is an HS256 JWT presented as
  `authorization: Bearer <jwt>` (ADR-0068); the harness has a
  token-minting seam to reuse.
- Depends on: `aegis-ingest-auth-v0` / ADR-0068 (the gateway side that
  mandates and validates the token) — DONE, available. The
  `opentelemetry_otlp` tonic metadata surface — available.

---

## US-SP-AUTH-02 — The conventional path: OTEL_EXPORTER_OTLP_HEADERS attaches the bearer

### Elevator Pitch

- **Before**: Marco's platform team deploys `payments-api` via a
  manifest that already sets `OTEL_EXPORTER_OTLP_ENDPOINT` (which Spark
  honours, `init.rs:70`). They want to set the bearer the same
  conventional way — `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<jwt>`,
  the standard OTLP env var every other OTel SDK reads — without
  touching application code. But Spark **ignores** `OTLP_HEADERS` today
  (it reads only the endpoint env var), so the manifest-set credential
  is silently dropped and the gateway denies every export.
- **After**: when the manifest sets
  `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<jwt>`, Spark parses
  the `authorization` entry, percent-decodes the value, and attaches
  `authorization: Bearer <jwt>` to all three exporters — exactly as the
  programmatic knob does. The gateway accepts. If BOTH the programmatic
  `with_bearer_token` and the env var are set, the documented precedence
  applies (DD2 — programmatic wins, env is the fallback), so a
  deployment override behaves predictably.
- **Decision enabled**: Marco's platform team can authenticate the SDK
  the same conventional, code-free way they already set the endpoint —
  they read the gateway's `decision=allow` and the filled dashboard, not
  a code diff, to confirm the credential took effect.

### Problem

Marco's team manages credentials through deployment manifests, not
application rebuilds — that is how they already set the OTLP endpoint.
The OTel convention for headers is `OTEL_EXPORTER_OTLP_HEADERS`, and
every conformant SDK reads it. Spark reads only
`OTEL_EXPORTER_OTLP_ENDPOINT` (F3: a grep for `OTLP_HEADERS` in
`crates/spark/` returns zero matches), so the conventional credential
path does not work — the manifest-set token is silently ignored and the
gateway denies the telemetry.

### Who

- **Marco Bianchi**, backend integrator | wants the bearer set in the
  deployment manifest, not the application binary | motivated to manage
  the credential the same conventional, code-free way as the endpoint.
- **Priya**, platform-security operator | mints and rotates the token |
  motivated for the credential to be deployment-managed so rotation
  does not require an application rebuild.

### Solution

Honour `OTEL_EXPORTER_OTLP_HEADERS` (DD4 — v0 scoped to the
`authorization` entry): parse the comma-separated `key=value` list,
extract `authorization`, percent-decode the value, and attach it as the
bearer metadata via the same uniform helper as US-SP-AUTH-01 (DD1).
Resolve precedence against the programmatic knob (DD2), mirroring the
established `with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` > default
chain (`init.rs:586-620`). The parsed token is redacted on every
loggable surface (System Constraint 1).

### Domain Examples

#### 1: Happy Path — the manifest sets the headers env var and the export is accepted

Marco's deployment manifest sets
`OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20eyJhbGci...` (the
percent-encoded HS256 JWT for `acme-prod`) and
`OTEL_EXPORTER_OTLP_ENDPOINT=https://aperture.acme.internal:4317`. The
application code calls `SparkConfig::for_service("payments-api")` with
no auth knob. Spark parses the env var, decodes `Bearer eyJhbGci...`,
attaches it to all three exporters, and the gateway accepts the
telemetry tagged `tenant_id=acme-prod`.

#### 2: Edge Case — both the programmatic knob and the env var are set; programmatic wins

Marco's code calls `.with_bearer_token(jwt_a)` AND the manifest sets
`OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<jwt_b>`. Per the
documented precedence (DD2, mirroring the endpoint chain), the
programmatic `jwt_a` wins and `jwt_b` is the unused fallback. Marco can
predict which credential is on the wire from the precedence rule alone,
without a packet capture.

#### 3: Error/Boundary — the env var is set but empty; Spark adds no header

Marco's manifest sets `OTEL_EXPORTER_OTLP_HEADERS=` (empty), mirroring
how an empty `OTEL_EXPORTER_OTLP_ENDPOINT=""` falls through to the
default rather than producing an invalid endpoint (`init.rs:615-619`).
Spark treats an empty headers env var as absent — no `authorization`
header is attached. (Against an authenticated gateway this then denies
`missing_claim`; against an unauthenticated collector it still works —
System Constraint 4.)

### UAT Scenarios (BDD)

#### Scenario: OTEL_EXPORTER_OTLP_HEADERS attaches the bearer

```gherkin
Given an aperture gateway is running with ingest auth configured for "acme-prod"
And the deployment sets OTEL_EXPORTER_OTLP_HEADERS to "authorization=Bearer%20<valid-jwt-for-acme-prod>"
And Marco's application configures Spark without any programmatic auth knob
When Marco's service exports telemetry through Spark
Then Spark attaches the decoded "authorization: Bearer <jwt>" to all three exporters
And the gateway accepts the export
And the records are tagged with tenant "acme-prod"
```

#### Scenario: The programmatic bearer token wins over the headers env var

```gherkin
Given the deployment sets OTEL_EXPORTER_OTLP_HEADERS to "authorization=Bearer%20<env-token>"
And Marco's application calls SparkConfig::with_bearer_token(<programmatic-token>)
When Marco's service exports telemetry through Spark
Then Spark attaches the programmatic token, not the env-var token
And the precedence matches the documented endpoint resolution order
```

#### Scenario: An empty headers env var is treated as no credential

```gherkin
Given the deployment sets OTEL_EXPORTER_OTLP_HEADERS to an empty string
And Marco's application configures Spark without a programmatic auth knob
When Marco's service exports telemetry through Spark
Then Spark attaches no authorization header
And an export to an unauthenticated local collector still succeeds
```

### Acceptance Criteria

- [ ] **OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer**: when the env
  var sets `authorization=Bearer%20<jwt>`, Spark percent-decodes it and
  attaches `authorization: Bearer <jwt>` to all three exporters; the
  authenticated gateway accepts. This MUST fail against today's code
  (Spark ignores `OTLP_HEADERS`, F3) and pass only once the env path is
  honoured.
- [ ] Precedence between the programmatic knob and the env var is
  deterministic and documented (DD2 — programmatic wins; env is the
  fallback), mirroring the established endpoint chain.
- [ ] An empty / absent `OTEL_EXPORTER_OTLP_HEADERS` is treated as no
  credential — no header attached (mirrors the empty-endpoint
  fall-through, `init.rs:615-619`).
- [ ] v0 parsing is scoped to the `authorization` entry (DD4); a
  conformant `Bearer%20` value is percent-decoded per the OTel spec.

### Outcome KPIs

- **Who**: integrators who manage the OTLP credential through the
  deployment manifest rather than application code.
- **Does what**: authenticate the SDK by setting the standard
  `OTEL_EXPORTER_OTLP_HEADERS` env var, with no application rebuild.
- **By how much**: 100% of exports where the env var sets a valid
  `authorization=Bearer%20<jwt>` are accepted by the authenticated
  gateway (target 1.0); credential rotation requires zero application
  rebuilds.
- **Measured by**: the gateway's `decision=allow` for a Spark export
  whose token was supplied only via the env var; a precedence test when
  both paths are set.
- **Baseline**: 0% — Spark ignores `OTEL_EXPORTER_OTLP_HEADERS` today
  (F3); the env-set credential is silently dropped.

### Technical Notes

- F3: the only env var Spark reads is `OTEL_EXPORTER_OTLP_ENDPOINT`
  (`init.rs:70`, `operator_supplied_endpoint` `init.rs:611`); no
  `OTLP_HEADERS` support exists.
- DD4: v0 parsing scoped to `authorization`; spec-conformant
  percent-decode; locked malformed-value failure mode. DD2: precedence
  vs the programmatic knob.
- Mirror the env-var test pattern in
  `crates/spark/tests/slice_04_env_var_precedence.rs` (`serial_test`,
  clean-env helper, recording-sink aperture spawn).
- Depends on US-SP-AUTH-01 (the uniform exporter-metadata helper + the
  redaction discipline).

---

## US-SP-AUTH-03 — Safe by construction: the token is never logged, and the no-auth path still works

### Elevator Pitch

- **Before**: even once Spark can attach a bearer (US-SP-AUTH-01/02),
  two hazards remain unproven. (a) The token is a credential — if it
  leaks into the `spark::init succeeded` event, a `Debug` of the config,
  or an error line, Marco has copied a live credential into his log
  aggregator, a leak as bad as the gateway leaking its HS256 secret. (b)
  If attaching the header is unconditional, Marco's CI integration test
  pointing Spark at an **unauthenticated** local collector breaks.
- **After**: the bearer token NEVER appears on any Spark log surface —
  not the `target="spark"` events, not a config `Debug`, not an error;
  a developer who dumps `SparkConfig` or greps the init logs sees a
  redacted placeholder (e.g. `<redacted>`), never the JWT. AND when no
  token is configured (no knob, no env var), Spark attaches **no**
  authorization header — Marco's unauthenticated local collector accepts
  exactly as it does today.
- **Decision enabled**: Marco can ship the SDK into a service whose logs
  flow to a shared aggregator **without** fear of leaking the gateway
  credential, and can keep his existing no-auth local-collector
  workflow — he greps the init logs for the token and finds it absent,
  and runs his unauthenticated-collector test and sees it pass.

### Problem

The bearer token is sensitive — it is a credential equivalent to the
HS256 secret on the gateway side, which aegis/aperture go to lengths
never to log (ADR-0068 DD1). Spark already logs the resolved config
(`emit_init_succeeded` logs `service.name`/`endpoint`/`protocol`/
`flush_timeout_ms`, `observability.rs:53-70`) and `SparkConfig` derives
`Debug` (`config.rs:26`) — a naively-added token field would leak
straight onto those surfaces. Separately, not every endpoint requires
auth: an integrator may target an unauthenticated local collector, so
attaching a header unconditionally (or when none is configured) would
break that path.

### Who

- **Marco Bianchi**, backend integrator | ships the SDK into a service
  whose logs flow to a shared aggregator, and runs local tests against
  an unauthenticated collector | motivated to never leak the credential
  and to keep his no-auth workflow.
- **Priya**, platform-security operator | accountable for credential
  hygiene across the platform | motivated that an SDK presenting her
  token does not echo it into log storage.

### Solution

Redact the token on every loggable surface (DD3): the new config field
is opaque in `Debug`/`Display`, absent from the `spark::init succeeded`
event and any other `target="spark"` event, and never echoed in a
config-validation error. Make the header attachment conditional: when
no token is resolved (no knob, no env var), `build_pipeline` attaches no
`authorization` metadata — the exporters are built exactly as today
(System Constraint 4). DD5: when no token is configured against a
remote endpoint, prefer silent-but-documented (no noisy security warn);
any warn DESIGN chooses must never echo a token value.

### Domain Examples

#### 1: Happy Path — Marco greps the init logs and the token is absent

Marco configures `.with_bearer_token("eyJhbGciOiJIUzI1NiIs...")` and
runs `payments-api`. The `spark::init succeeded` line shows
`service.name=payments-api endpoint=https://aperture.acme.internal:4317
protocol=grpc flush_timeout_ms=5000` — and **no token**. Marco runs
`grep eyJhbGci app.log` and gets zero hits. If anything appears for the
auth field at all, it is a redacted placeholder, never the JWT.

#### 2: Edge Case — a panic dumps the SparkConfig and the token does not leak

During development Marco's code hits a `dbg!(&config)` /
`panic!("{config:?}")`. The `Debug` output renders the auth field as
`<redacted>` (mirroring aegis's `key = "<opaque>"` at validator.rs),
so even a debug dump in a crash report does not carry the live
credential into the bug tracker.

#### 3: Error/Boundary — no token configured, unauthenticated local collector still accepts

Marco runs his integration test against a local OTLP collector with no
auth, calling `SparkConfig::for_service("payments-api").with_endpoint("http://localhost:4317")`
and **no** `with_bearer_token`, **no** `OTEL_EXPORTER_OTLP_HEADERS`.
Spark attaches no `authorization` header; the collector accepts the
telemetry exactly as it did before this feature existed. The no-auth
path is preserved.

### UAT Scenarios (BDD)

#### Scenario: The token is never logged

```gherkin
Given Marco configures Spark with SparkConfig::with_bearer_token("<a-recognisable-jwt>")
When Spark initialises and emits its init-succeeded event
Then the bearer token value does not appear in any target="spark" log event
And the bearer token value does not appear in a Debug or Display of the config
And the bearer token value does not appear in any configuration error
```

#### Scenario: No token, no header — an unauthenticated endpoint still works

```gherkin
Given Marco configures Spark with an endpoint but no bearer token and no OTEL_EXPORTER_OTLP_HEADERS
And the target endpoint is an unauthenticated local collector
When Marco's service exports telemetry through Spark
Then Spark attaches no authorization header to any exporter
And the unauthenticated collector accepts the telemetry exactly as before
```

#### Scenario: No token against a remote endpoint is silent, not a noisy security warning

```gherkin
Given Marco configures Spark with a remote endpoint but no bearer token and no OTEL_EXPORTER_OTLP_HEADERS
When Spark initialises
Then Spark does not emit a log line containing any token value
And Spark's no-token behaviour is documented on the bearer-token knob
```

### Acceptance Criteria

- [ ] **the-token-is-never-logged**: the bearer token value never
  appears in any `target="spark"` event, in a `Debug`/`Display` of
  `SparkConfig`, or in any configuration error — only a redacted
  placeholder, if anything. This mirrors aegis/aperture's
  never-log-the-secret discipline (System Constraint 1).
- [ ] **no-token-no-header-against-an-unauthenticated-endpoint-still-works**:
  with no token configured (no knob, no env var), Spark attaches no
  authorization header and an unauthenticated endpoint accepts exactly
  as before (System Constraint 4); slice_01..slice_07 stay green.
- [ ] The header attachment is conditional on a token being resolved;
  the no-token exporter-build path is byte-unchanged from today.
- [ ] DD5 resolved: no-token-against-remote is silent-but-documented
  (or, if DESIGN chooses to warn, the warn never echoes a token and is
  suppressible).

### Outcome KPIs

- **Who**: integrators shipping the Spark SDK into services whose logs
  flow to shared storage, and integrators targeting unauthenticated
  collectors.
- **Does what**: present a gateway credential without ever leaking it
  into logs, and keep the no-auth path working unchanged.
- **By how much**: 0 occurrences of a bearer token value on any Spark
  log/Debug/error surface (target 0; a single occurrence is a defect);
  100% of no-token exports to an unauthenticated endpoint still accepted
  (target 1.0).
- **Measured by**: a redaction test grepping every Spark log/Debug
  surface for the configured token; the no-token negative-control export
  test against an unauthenticated collector.
- **Baseline**: n/a — no token field exists today, so neither the leak
  surface nor the conditional-header path exists yet.

### Technical Notes

- F4 / System Constraint 1: `emit_init_succeeded`
  (`observability.rs:53-70`) and `SparkConfig`'s derived `Debug`
  (`config.rs:26`) are the leak surfaces to defend; DD3 owns the
  redaction mechanism (opaque Debug, newtype, etc.).
- DD5: silent-but-documented no-token-against-remote.
- Mirror aegis's opaque-key Debug (`key = "<opaque>"`,
  validator.rs:149-158) for the redaction shape.
- Depends on US-SP-AUTH-01 (the field exists) and US-SP-AUTH-02 (the env
  path resolves the same field).
