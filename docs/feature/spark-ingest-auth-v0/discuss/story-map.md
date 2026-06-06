# Story Map: spark-ingest-auth-v0

## User: Marco Bianchi, backend integrator at `acme-observability`

## Goal: ship authenticated telemetry from a Spark-instrumented service through the now-fail-closed aperture gateway, so it is ACCEPTED instead of silently denied

This is the client-side sibling of `aegis-ingest-auth-v0` (ADR-0068).
The gateway locked the door (rejects every tokenless ingest
`missing_claim`); this map covers giving the Spark SDK the key. It is a
**brownfield** feature — Spark already ships telemetry end-to-end; there
is **no walking skeleton** to build. The map's backbone is the
integrator's authenticate-and-send journey; the slices are carpaccio
cuts of one auth capability.

## Backbone

| A1 — Configure the credential | A2 — Attach it to the wire | A3 — Send & be accepted | A4 — Stay safe |
|-------------------------------|----------------------------|-------------------------|----------------|
| Call `with_bearer_token(token)` (US-SP-AUTH-01) | Put `authorization: Bearer <jwt>` on the span exporter (US-SP-AUTH-01) | Authenticated traces accepted at gateway (US-SP-AUTH-01) | Token never logged (US-SP-AUTH-03) |
| Set `OTEL_EXPORTER_OTLP_HEADERS` in the manifest (US-SP-AUTH-02) | Put it on the log exporter (US-SP-AUTH-01) | Authenticated logs accepted (US-SP-AUTH-01) | No token ⇒ no header; unauth endpoint still works (US-SP-AUTH-03) |
| Precedence when both are set (US-SP-AUTH-02) | Put it on the metric exporter (US-SP-AUTH-01) | Authenticated metrics accepted (US-SP-AUTH-01) | No-token-against-remote silent-but-documented (US-SP-AUTH-03) |
| | Percent-decode the env-var value (US-SP-AUTH-02) | Empty env var ⇒ no credential (US-SP-AUTH-02) | |

---

### Walking Skeleton

Not applicable — brownfield. Spark already exports telemetry
end-to-end (slice_01..slice_07 are GREEN). This feature ADDS an auth
knob to the existing, working `build_pipeline` path; it does not build a
new end-to-end flow. The nearest analogue to a skeleton is the
**driving slice** below (the programmatic token authenticating all three
signals), which is the thinnest demonstrable, shippable cut.

### Slice 1 (driving) — US-SP-AUTH-01: a programmatic bearer authenticates all three signals

- **Tasks**: `with_bearer_token` knob; one uniform helper attaching
  `authorization: Bearer <token>` to the span, log, AND metric exporters
  in `build_pipeline`; proven against a real aegis-authenticated aperture
  (the E01-E04 shape) — accepted-with-tenant, plus the no-token negative
  control.
- **Target outcome**: integrators ship authenticated telemetry that is
  ACCEPTED across all three OTLP signals.
- **KPI**: KPI-1 (authenticated-export-accepted, all-three-signals);
  E01-E04 return GREEN.
- **Rationale**: this is the verifier's blocked round-trip. The token
  reaching ALL three signals is in this slice (a partial wire is the
  E01-E04 failure). Reject-honesty (Spark sends the token; the gateway
  judges it) is here too. Highest value, derisks the whole feature.

### Slice 2 — US-SP-AUTH-02: the conventional `OTEL_EXPORTER_OTLP_HEADERS` path

- **Tasks**: honour `OTEL_EXPORTER_OTLP_HEADERS` (v0 scoped to
  `authorization`); percent-decode `Bearer%20<jwt>`; precedence vs the
  programmatic knob (programmatic wins, env is fallback — mirror the
  endpoint chain); empty-env fall-through.
- **Target outcome**: integrators authenticate the SDK the same
  conventional, code-free way they already set the endpoint;
  credential rotation needs no application rebuild.
- **KPI**: KPI-2 (env-var-credential-accepted, zero-rebuild-rotation).
- **Rationale**: the conventional OTel path; reuses Slice 1's uniform
  helper and redaction. Lower risk, second value.

### Slice 3 — US-SP-AUTH-03: safe by construction (never-log + no-auth-path)

- **Tasks**: redact the token on every loggable surface (events,
  `Debug`, errors); conditional header attachment so no-token ⇒
  no-header; the unauthenticated-collector negative control;
  silent-but-documented no-token-against-remote (DD5).
- **Target outcome**: integrators present the credential without
  leaking it and keep the no-auth workflow.
- **KPI**: KPI-3 (zero-token-leak), KPI-4 (no-auth-path-preserved).
- **Rationale**: the secret-posture guardrail. The redaction and the
  conditional-header are properties that ride on Slices 1-2 but are
  called out as their own story so the never-log invariant (the
  load-bearing security constraint) gets a dedicated AC and review focus.

## Priority Rationale

Outcome impact and dependencies drive the order:

1. **Slice 1 (US-SP-AUTH-01)** — P1. Highest outcome impact: it is the
   exact capability the verifier's E01-E04 need to flip from BLOCKED to
   GREEN, and it derisks the whole feature (the uniform-three-signal
   helper is the riskiest assumption — a partial wire is the dominant
   failure mode, R2). Value 5 × Urgency 5 / Effort 2.
2. **Slice 2 (US-SP-AUTH-02)** — P2. The conventional credential path;
   high value (deployment-managed rotation) but depends on Slice 1's
   helper. Value 4 × Urgency 3 / Effort 2.
3. **Slice 3 (US-SP-AUTH-03)** — P3 by sequence, but its
   never-log invariant is a **non-negotiable guardrail** woven through
   Slices 1-2 (the token must never leak the moment the field exists).
   Treated as a dedicated story for AC/review focus rather than deferred
   value: the redaction AC must hold from the first slice that adds the
   field. Value 4 (guardrail) × Urgency 4 / Effort 2.

Because the three cuts share one exporter-build path and one config
field, DESIGN may collapse them into fewer DELIVER slices; the
requirement is the observable behaviour per story, not the slice count.

## Scope Assessment: PASS — 3 stories, 1 context, estimated 1-2 days

- **User stories**: 3 (< 10). 
- **Bounded contexts / modules**: 1 — the `spark` crate
  (`config.rs` + `init.rs`, with secret-handling mirrored from aegis).
  No new crate, no aperture change (the gateway side is done in
  ADR-0068), no new dependency beyond `opentelemetry_otlp`'s existing
  tonic metadata surface.
- **Integration points for the driving slice**: 1 — the
  authenticated-aperture round-trip (the E01-E04 harness, which already
  mints tokens). Well under the >5 oversized signal.
- **Estimated effort**: 1-2 days (a config field + a uniform exporter
  helper + an env parser + redaction). Under the 2-week bound.
- **Independent outcomes**: one job (send-an-authenticated-export);
  the three stories are facets, not separable products.

Zero of the five oversized signals tripped. No split needed; proceed.
