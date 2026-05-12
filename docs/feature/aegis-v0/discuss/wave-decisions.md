# DISCUSS Decisions — aegis-v0

## Key decisions

- **[D1] Library only at v0.** No service / daemon; the SPIFFE/SPIRE
  control plane is v1. Aperture / Beacon / Prism consume the
  library directly. (See: user-stories.md system constraint 1.)
- **[D2] JWT validation against a configured issuer + JWKS.** OIDC
  federation via Dex is v1; v0 accepts the JWT as input and
  validates it. (See: system constraint 3.)
- **[D3] Tenant catalogue is a TOML file.** FoundationDB is the
  long-term tenant catalogue; v0 reads `tenants.toml` from disk.
  Migration is an adapter swap. (See: system constraint 4.)
- **[D4] Two roles at v0: `viewer` + `operator`.** Full OPA RBAC
  matrix is v1. The role lives in a `kaleidoscope_role` JWT
  claim. (See: system constraint 5.)
- **[D5] Audit via `tracing::info!` / `tracing::warn!`.** Stable
  event field names: `tenant_id`, `role`, `decision`, `subject`,
  `reason`. The operator's tracing-subscriber captures these
  into Lumen when it ships; stdout meanwhile. (See: system
  constraint 6.)
- **[D6] PII scrubbing is v1.** v0 ships validation + catalogue +
  audit. (See: system constraint 7.)
- **[D7] No telemetry-on-telemetry.** (See: system constraint 8.)
- **[D8] AGPL-3.0-or-later.** Same licensing as every platform
  component.
- **[D9] Pure validation.** `validate(token, now) -> Result` is
  total; no I/O during validation; JWKS pre-loaded at startup.
  (See: system constraint 9.)
- **[D10] No retrofit at v0.** Aperture / Beacon / Prism keep
  auth-free at v0; integrating Aegis into each component is its
  own slice in v1.

## Requirements summary

- Primary user need: a typed `TenantContext` derived from a JWT,
  with operator-readable failure modes and a complete audit trail.
- Walking skeleton scope: validate one JWT against a fixed key +
  catalogue, return `TenantContext` or typed error.
- Feature type: backend (library, no UI).

## Constraints established

- Aegis v0 has zero external network deps at validation time
  (JWKS pre-loaded).
- TOML schema mirrors Beacon's shape (deny_unknown_fields +
  per-field diagnostics).
- Audit events use stable field names; operator's pipeline
  depends on the contract.

## Upstream changes

None. The architecture doc §C.14 names Aegis's role. ADR-0034's
TOML schema decision carries forward (already established by
Beacon).
