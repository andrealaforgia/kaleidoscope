# Story Map: aegis-ingest-auth-v0

## User: Priya, a platform-security operator running a multi-tenant Kaleidoscope deployment

## Goal: Enforce tenant authentication at the OTLP ingest boundary, so no record is accepted without a valid bearer token and every accepted record is tagged with the tenant from the validated token — fail-closed.

The driving surface is the running `aperture` binary (gRPC `:4317`,
HTTP/protobuf `:4318`). Before this feature: a client sends OTLP with no
token (or a forged tenant) and it is accepted. After: a client without a
valid bearer token gets `UNAUTHENTICATED` / `401` with nothing stored
and one audit line; a client with a valid token has its data accepted
and tagged with the token's tenant.

## Backbone (user activities, chronological)

| A. Present a token | B. Authenticate at the boundary | C. Reject the invalid | D. Tag the accepted | E. Audit the decision |
|--------------------|---------------------------------|------------------------|----------------------|------------------------|
| Client sends OTLP with `Bearer <jwt>` on the transport | Gateway extracts + validates the token before any record is read | Invalid/missing token → reject, nothing stored | Valid token → tenant from the token rides the accepted record into the sink | One structured decision event per request |
| A.1 gRPC `authorization` metadata | B.1 Extract bearer on gRPC | C.1 Reject gRPC `UNAUTHENTICATED` | D.1 Authenticated `tenant_id` into the logs sink record | E.1 One deny event per rejected request (reason taxonomy) |
| A.2 HTTP `Authorization` header | B.2 Extract bearer on HTTP | C.2 Reject HTTP `401` | D.2 Tenant tagging for traces + metrics | E.2 Secret never logged in any event |
| A.3 Missing/empty token | B.3 Validate via aegis (sig/exp/iss/aud/tenant/role) | C.3 Full reject-reason matrix (8 aegis variants) | D.3 Tenant rides HTTP transport too | E.3 One allow event per accepted request |
|  | B.4 Fail-closed config (on-by-default / refuse-to-start) |  |  |  |

---

## Walking Skeleton

The thinnest end-to-end slice that connects ALL activities, on ONE
transport (gRPC) for ONE signal (logs):

- **A.1** client presents `Bearer <jwt>` in gRPC `authorization` metadata
- **B.1 + B.3** aperture extracts the bearer token and validates it via `aegis::Validator`
- **C.1** an invalid OR missing token → gRPC `UNAUTHENTICATED`, **nothing reaches the sink**
- **D.1** a valid token → the authenticated `tenant_id` rides the accepted logs record into the sink
- **E.1 + E.3** exactly one decision event per request (allow on accept, deny on reject), reason from the aegis taxonomy

The security boundary (reject-on-no-token, nothing-stored) is IN the
walking skeleton. A slice that adds only the happy path is not
shippable.

### Priority Rationale

Priority is by outcome impact and dependency, not feature grouping.

1. **Walking Skeleton (P1)** — establishes the auth boundary
   end-to-end. Validates the riskiest assumption: that aegis can be
   wired onto the live gateway's request path, fail-closed, without
   regressing the existing ingest happy path. Until this works, nothing
   else matters. Highest value (moves the north-star "% of accepted
   ingest requests carrying an authenticated tenant" from 0 toward 1)
   and de-risks the fatal assumption (the `SinkRecord` ripple, DD3).
2. **Release 1 (P2): HTTP transport parity** — the gateway has two
   front doors; an authenticated gRPC door with an open HTTP door is
   still an open gateway. This closes the second door. Depends on WS
   (reuses the extract→validate→reject→tag spine).
3. **Release 2 (P3): traces + metrics parity** — extends tenant
   tagging + reject to the other two signals. Lower per-slice risk
   (mirrors the logs path, app.rs/transport.rs already symmetric across
   signals). Depends on WS + R1.
4. **Release 3 (P4): full reject-reason matrix + role authorization
   question** — the 8 aegis `ValidationError` variants each surface
   with their matching reason; and the DD6 question of whether v0 also
   role-gates ingest (require `operator` to write). Lowest urgency: the
   fail-closed boundary already rejects every invalid token in WS; this
   slice makes each rejection *legible* by reason and resolves the role
   question. Depends on all prior.

### Release 1 — HTTP transport parity (outcome: both front doors authenticated)

- A.2 HTTP `Authorization` header extraction
- B.2 extract bearer on HTTP
- C.2 reject HTTP `401` (RFC 6750 `WWW-Authenticate: Bearer`), nothing stored
- D.3 authenticated tenant rides the HTTP logs record
- Target KPI: KPI-1 (authenticated-tenant coverage) now covers HTTP, not just gRPC.

### Release 2 — traces + metrics parity (outcome: all three signals authenticated)

- D.1 extended: authenticated `tenant_id` into traces + metrics sink records (both transports)
- C reject + E audit symmetric across traces + metrics
- Target KPI: KPI-1 covers 3 signals × 2 transports = the full ingest surface.

### Release 3 — full reject-reason matrix + role authorization (outcome: every denial is legible; role question resolved)

- C.3 each of the 8 aegis `ValidationError` variants (invalid_signature, expired, wrong_issuer, wrong_audience, missing_claim, unknown_tenant, unknown_role, malformed) surfaces with its matching reason in the audit event and (where safe) the reject status message
- E.1 deny-event reason taxonomy complete and asserted per variant
- DD6 role question: decide + (if in scope) enforce `operator`-role-to-ingest, else explicitly defer to a follow-up with the decision recorded
- Target KPI: KPI-3 (reason-coverage of denials) reaches 100% of variants.

## Scope Assessment: PASS (with mandatory split already applied)

Assessed against the Elephant Carpaccio oversized signals:

- **User stories**: 5 (US-AUTH-01..05) — within the 1-feature band once
  sliced. PASS.
- **Bounded contexts / modules touched**: 2 crates (aperture wires,
  aegis is reused verbatim) — aperture's `config`, `transport`, `app`,
  `ports`. Within 3. PASS.
- **Walking-skeleton integration points**: aperture↔aegis (validate),
  aperture↔config (HS256 fields), aperture↔sink (tenant ripple) = 3.
  At the boundary, not over it. PASS.
- **Independent user outcomes that could ship separately**: YES — this
  is WHY the feature is sliced into WS + 3 releases by outcome, and why
  the **read-path auth is carved out as a separate future feature**.
  The split is applied, not deferred.

Verdict: **right-sized AS SLICED**. The walking skeleton is one
demonstrable slice (auth boundary on gRPC logs); each release is an
independent thin end-to-end slice delivering a verifiable behavior. The
read-path auth is explicitly a separate deliverable (DD6). No further
split required; autonomous run proceeds without a confirmation prompt
(decision made per the "decide rather than ask" posture).
