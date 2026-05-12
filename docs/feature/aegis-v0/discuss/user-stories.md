# Aegis v0 — user stories

Three LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`.

The principal user is **Sasha, a platform engineer** wiring tenant
identity into Aperture, Prism, and Beacon. Sasha needs a single
library that takes a request-bound JWT and returns a typed
`TenantContext` carrying tenant id + role; everything downstream
keys off that context.

The secondary user is **Riley, an SRE** answering a security audit
asking "who can read what, and when did they last do so?". Riley
needs every authz decision recorded as a structured event the
audit team can query.

System constraints (apply to every story):

1. Library at v0. Aegis ships as a Rust crate (`aegis`) consumed by
   Aperture, Prism (via WebAssembly stub at v1; v0 keeps Prism's
   auth client-side), and Beacon when each enables auth. No
   service / daemon at v0; the eventual SPIFFE/SPIRE control plane
   arrives at v1.
2. AGPL-3.0-or-later. Same licensing posture as every platform
   component.
3. **JWT-only at v0.** The roadmap names Dex + Keycloak as the OIDC
   federation layer; at v0 Aegis takes the JWT as input and
   validates it against a configured issuer + JWKS. The OIDC
   client / federation wiring is v1.
4. **Tenant catalogue is a TOML file at v0.** FoundationDB is the
   long-term tenant catalogue per the architecture doc; v0 reads
   `tenants.toml` from disk at startup with the same shape Beacon's
   rules use. Migration to FoundationDB is an adapter swap.
5. **Two roles at v0.** `viewer` (read-only) and `operator`
   (read + write). The full OPA RBAC matrix arrives at v1. The
   role lives in a `kaleidoscope_role` JWT claim.
6. **Audit log via `tracing`.** Every authz decision emits a
   structured `tracing::info!` event with `tenant_id`, `role`,
   `decision` (`allow` | `deny`), `subject` (the action: e.g.
   `query_range`), and `reason`. The operator's tracing-subscriber
   captures these into the audit pipeline of their choice (Lumen
   when it exists; stdout meanwhile).
7. **PII scrubbing is v1.** The architecture doc names PII scrub
   as Aegis-owned policy authoring; v0 ships the validation +
   catalogue + audit primitives and defers the policy authoring +
   Sieve integration to v1.
8. **No telemetry-on-telemetry.** Aegis itself emits OTLP
   telemetry to the operator's own Aperture deployment per the
   architecture doc §A.2.
9. **Pure validation.** `validate(token, now) -> Result<TenantContext, ValidationError>`
   is total: every input yields either a typed context or a
   typed error. No panics, no external I/O during validation
   (the JWKS is loaded at startup).

---

## US-AE-01 — Walking skeleton: validate a JWT into a TenantContext

### Elevator Pitch

- **Before**: Aperture accepts OTLP traffic from any caller. There
  is no notion of tenant; logs land in a shared sink. The team
  cannot run a multi-customer deployment because there is no
  identity-bearing contract.
- **After**: a call to `aegis::validate(token, now)` returns
  `Ok(TenantContext { tenant_id: "acme-prod", role: Operator })`
  for a signed-and-current JWT carrying the canonical claims, and
  `Err(ValidationError::Expired)` / `ValidationError::WrongIssuer`
  / `ValidationError::UnknownTenant` / `ValidationError::InvalidSignature`
  for every failure mode.
- **Decision enabled**: Sasha wires the validator into Aperture's
  request path; from now on Aperture refuses OTLP traffic without a
  tenant context.

### Acceptance criteria

- AC-1.1 — `aegis::validate(token, now)` returns `Ok(TenantContext)`
  for a JWT signed by the configured issuer key, with `iss`,
  `aud`, `exp`, `tenant_id`, `kaleidoscope_role` claims.
- AC-1.2 — Returns `Err(ValidationError::InvalidSignature)` when
  the token signature does not verify.
- AC-1.3 — Returns `Err(ValidationError::Expired)` when `now > exp`.
- AC-1.4 — Returns `Err(ValidationError::WrongIssuer)` when `iss`
  does not match the configured issuer.
- AC-1.5 — Returns `Err(ValidationError::WrongAudience)` when `aud`
  does not match.
- AC-1.6 — Returns `Err(ValidationError::UnknownTenant)` when the
  `tenant_id` claim is not in the tenant catalogue.
- AC-1.7 — Returns `Err(ValidationError::UnknownRole)` when the
  `kaleidoscope_role` claim is neither `viewer` nor `operator`.

### KPI anchor

- KPI 1 (Validation latency): p95 ≤ 1 ms on a pre-loaded JWKS +
  catalogue, on a single token. Aperture's per-request budget is
  tight; Aegis must not be the bottleneck.

---

## US-AE-02 — Tenant catalogue loader

### Elevator Pitch

- **Before**: tenants are an implicit concept — anyone with a
  signed JWT is implicitly accepted. The team cannot reject a
  shut-off customer's traffic without rotating signing keys.
- **After**: an operator-authored `tenants.toml` declares every
  active tenant with id, display name, and optional notes. Loading
  produces a `TenantCatalogue`; the validator rejects tokens whose
  `tenant_id` claim is not in the catalogue with
  `ValidationError::UnknownTenant`.
- **Decision enabled**: Sasha removes a tenant by editing
  `tenants.toml` and restarting Aperture; no key rotation needed.

### Acceptance criteria

- AC-2.1 — TOML schema: `[[tenants]]` tables with `id`, optional
  `display_name`, optional `notes`.
- AC-2.2 — Unknown field in `[[tenants]]` → load error naming the
  field + suggesting the nearest blessed field (same shape as
  Beacon's loader).
- AC-2.3 — Duplicate `id` across tables → load error.
- AC-2.4 — The loader returns a typed `TenantCatalogue` with O(1)
  `contains(&TenantId) -> bool`.

### KPI anchor

- KPI 2 (Catalogue load latency): ≤ 10 ms for a 1000-tenant
  TOML file. Aegis is meant to scale to large multi-tenant
  deployments; the catalogue must not bottleneck Aperture's start-up.

---

## US-AE-03 — Audit log: every decision is observable

### Elevator Pitch

- **Before**: there is no record of who did what. A compliance
  audit ("show every action the `read_only_customer` tenant took
  last week") cannot be answered.
- **After**: every `aegis::validate` call emits a structured
  `tracing::info!` event with `tenant_id`, `role`, `decision`,
  `subject`, and `reason`. The operator's tracing-subscriber
  captures these into the audit pipeline of their choice (Lumen
  when it ships; stdout for v0).
- **Decision enabled**: Riley can grep the audit log for `tenant_id`
  and see every decision; compliance audits become a Unix tool
  exercise.

### Acceptance criteria

- AC-3.1 — Every `Ok(TenantContext)` returns AND emits one
  `tracing::info!` event with `decision = "allow"`.
- AC-3.2 — Every `Err(ValidationError)` returns AND emits one
  `tracing::warn!` event with `decision = "deny"`, `reason =
  "expired" | "invalid_signature" | "wrong_issuer" | ...`.
- AC-3.3 — The event fields are stable across versions:
  `tenant_id`, `role`, `decision`, `subject`, `reason`. The
  operator's audit pipeline keys off these names.
- AC-3.4 — The `subject` field defaults to `"validate"` when no
  context is supplied; callers can pass a `subject: &str`
  parameter to record the specific action being authorised
  (`"query_range"`, `"emit_incident"`, etc.).

### KPI anchor

- KPI 3 (Audit completeness): 100% of validations (both allow and
  deny) produce exactly one audit event. The acceptance test
  intercepts the tracing layer and counts events.
