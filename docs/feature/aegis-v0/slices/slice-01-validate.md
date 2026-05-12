# Slice 01 — JWT validate walking skeleton (US-AE-01)

## Goal

`aegis::validate(token, now)` returns `Ok(TenantContext)` for a
signed-and-current JWT carrying `iss`, `aud`, `exp`, `tenant_id`,
`kaleidoscope_role` claims; returns typed `ValidationError` for
every failure mode.

## IN scope

- Public types: `TenantId(String)`, `Role` enum (Viewer | Operator),
  `TenantContext`, `ValidationError`
- JWT validator built around `jsonwebtoken` crate (Apache-2.0/MIT)
- Pre-loaded JWKS at validator construction; no network at
  validation time
- Acceptance test exercising every variant: ok / expired /
  wrong-issuer / wrong-audience / unknown-tenant / unknown-role /
  invalid-signature

## OUT scope

- Catalogue (slice 02)
- Audit (slice 03)
- OIDC federation (v1)
- mTLS / SPIFFE (v1)

## Learning hypothesis

Disproves "jsonwebtoken's Validator API fits Aegis's typed-error
contract cleanly". Risk: jsonwebtoken returns one error type for
all signature/claims failures; we may need to distinguish via
error-text inspection.
