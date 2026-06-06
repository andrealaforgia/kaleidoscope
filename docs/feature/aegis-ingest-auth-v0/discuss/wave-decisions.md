# DISCUSS Decisions — aegis-ingest-auth-v0

> **Wave**: DISCUSS (nWave). **Analyst**: Luna (`nw-product-owner`).
> **Date**: 2026-06-06. **Feature type**: Backend / Cross-cutting —
> a security boundary on the live OTLP ingest gateway.
> **Origin**: four-quadrants assessment for aegis
> (`~/dev/kaleidoscope-4-quadrants-theory/reports/aegis.md`), the
> UNWIRED dominant finding; the verifier asked for aegis auth next.

## The job (fail-closed framing)

When a client sends telemetry to the ingest gateway, the gateway
authenticates the bearer token **before accepting a single record**.
An unsigned, expired, wrong-issuer, wrong-audience, unknown-tenant,
unknown-role, or **missing** token is REJECTED with an unauthenticated
status and one audit event, and **nothing reaches the sink**. An
accepted batch is tagged with the tenant from the VALIDATED token, so
the platform's multi-tenancy is enforced at the boundary instead of
trusting a tenant id from an unauthenticated caller. **No token means
no ingest — never a silent default-tenant accept.**

## Verified facts (grounded in code, not the brief)

These were confirmed by reading the source on 2026-06-06. They are the
load-bearing premises for every story and slice.

- **F1 — aegis is a correct lock with no door fitted.**
  `aegis::Validator::validate(token, now) -> Result<TenantContext,
  ValidationError>` is real HS256 (jsonwebtoken), alg-confusion-safe
  (`Algorithm::HS256` pinned in `validate_inner`,
  `crates/aegis/src/validator.rs:220`), fail-closed on `exp`
  (validator.rs:234-241), exact issuer + audience equality
  (validator.rs:228-233), unknown-tenant rejected against the TOML
  catalogue (validator.rs:246-248), 8 typed `ValidationError` variants
  each with a stable `reason()` audit string (validator.rs:74-108),
  and **exactly one** `tracing` audit event per call
  (validator.rs:186-209: `info!` on allow, `warn!` on deny, fields
  `tenant_id`/`role`/`decision`/`subject`/`reason`).

- **F2 — aperture has zero auth today.** The gRPC handlers
  (`LogsServiceImpl::export` etc., `transport.rs:638,715,781`) and the
  HTTP handlers (`handle_logs`/`handle_traces`/`handle_metrics`,
  `transport.rs:344,436,523`) never read gRPC request metadata nor the
  HTTP `Authorization` header. Each ingest path
  (`app::ingest_logs`/`ingest_traces`/`ingest_metrics`,
  `app.rs:64,89,115`) runs `(bytes, transport, sink) -> validate
  (OTLP-conformance, NOT auth) -> sink.accept(SinkRecord)`. There is no
  authenticated tenant; the sink receives a `SinkRecord` carrying no
  tenant at all.

- **F3 — aperture does NOT depend on aegis.** A grep for `aegis` across
  the whole `crates/aperture/` tree returns zero matches, and
  `crates/aperture/Cargo.toml` has no aegis dependency. DESIGN must add
  the dependency.

- **F4 — zero production authn/z call sites platform-wide.** A
  workspace grep for `aegis::Validator` / `Validator::new` /
  `ValidatorConfig` outside `crates/aegis/` returns ZERO production
  call sites. Every `.validate(` hit outside aegis is a DIFFERENT
  validator — `codex::catalogue.validate` (semconv attribute linting)
  or `spark`'s catalogue — never `aegis::Validator::validate`. The
  15+ crates depending on aegis import ONLY the `TenantId` newtype for
  production use; none import `Validator`, `Role`, `ValidationError`,
  `TenantContext`, `load_catalogue`, or `TenantCatalogue`. The tenant
  id therefore flows through the platform as a typed newtype but its
  **provenance is unauthenticated at the system boundary**.

- **F5 — the reserved Phase 2 config hooks are TLS + SPIFFE, NOT the
  HS256/JWT fields this feature needs.** `config/mod.rs:160-177`
  reserves `tls_enabled` and `spiffe_enabled` accessors "for Phase 2
  (Aegis)", and the TOML schema has `[aperture.security.tls]`
  (enabled/cert_path/key_path, lines 432-443) and
  `[aperture.security.auth.spiffe]` (enabled/workload_api_socket/
  trust_domain, lines 457-468). **There is NO `[aperture.security.auth]`
  field for an HS256 secret, issuer, audience, or catalogue path.**
  The forward-compat markers reserve the SPIFFE scheme (aegis v1's
  workload identity) — not the HS256 pre-shared-key scheme this v0
  feature wires. So the config fields this feature needs **do not exist
  yet and must be added** (see DESIGN decision DD1). The brief's hint
  that the hooks "already carry the aegis fields" is INACCURATE; the
  honest answer is "the schema reserves the right *table*
  (`[aperture.security.auth]`) but not the HS256 *fields*".

- **F6 — aegis v0's own DISCUSS deferred exactly this.** `aegis-v0`
  wave-decisions D10: "No retrofit at v0. Aperture / Beacon / Prism keep
  auth-free at v0; integrating Aegis into each component is its own
  slice in v1." **This feature IS that deferred integration slice.** The
  unwired-validator gap was a planned deferral, now being closed.

- **F7 — the audit field-name contract is locked by aegis D5.** Every
  authz decision emits stable fields `tenant_id`, `role`, `decision`
  (`allow`|`deny`), `subject` (the action), `reason`. Aperture's deny
  events must align to this contract and to aperture's own closed event
  vocabulary (`observability.rs:30-52`); the `subject` will name the
  ingest action (e.g. `ingest_logs`).

- **F8 — the JWKS doc overstatement is real but separable.**
  `crates/aegis/src/lib.rs:18-23,39-41` says aegis validates "against a
  configured issuer + JWKS"; the validator is HS256 **pre-shared key**
  only (validator.rs:134-136 `hs256_key: Vec<u8>`, `Algorithm::HS256`
  at validator.rs:220 — no JWKS, no network at validation time). This
  is the report's LOW doc finding. **Decision: flag as adjacent, do NOT
  fold into this feature.** Rationale below (DD7).

## Walking-slice decision (this is LARGE and security-critical)

**Walking slice (WS) = aegis validates the bearer token on ONE
transport (gRPC) for ONE signal (logs)**, reject-on-invalid
(UNAUTHENTICATED, nothing stored) PLUS accept-with-authenticated-tenant,
establishing the auth boundary end-to-end. The security boundary
(reject-on-no-token) is IN the first slice — a slice that only adds the
happy path without the reject is NOT shippable (carpaccio taste test:
each slice must be demonstrable AND must not regress the fail-closed
posture). Then carpaccio: the HTTP transport, the other two signals,
and the full reject-reason matrix as thin follow-on slices.

The **read-path auth** (query-api / log-query-api / trace-query-api) is
a SEPARATE future feature — explicitly OUT of this feature's scope and
noted as a follow-up. SPIFFE / RS256 / JWKS / OPA are aegis v1, OUT.

## Six decisions flagged for DESIGN (solution-architect owns the mechanism)

> Requirements stay solution-neutral. These are the seams DESIGN must
> resolve; the requirement says WHAT must be observable, DESIGN says HOW.

### DD1 — aperture config wiring for the HS256 validator

The reserved Phase 2 accessors (`config/mod.rs:160-177`) carry TLS +
SPIFFE knobs, NOT the HS256 fields (F5). DESIGN must decide the config
shape for: the HS256 **secret**, the **issuer**, the **audience**, and
the **tenant catalogue path** — most naturally a new
`[aperture.security.auth.jwt]` (or `.hs256`) sub-table sibling to the
existing `[aperture.security.auth.spiffe]`, with `deny_unknown_fields`
like every other aperture config struct.
**Hard constraint: the HS256 secret is sensitive and MUST NEVER be
logged.** aegis already renders the key opaque in `Validator`'s `Debug`
(validator.rs:149-158, `key = "<opaque>"`); aperture must not leak it —
no Debug/Display of the raw secret, no echo on config-validation error,
and the secret should be sourceable from a file or env var rather than
inline TOML (clig.dev: never accept secrets via flags; prefer files/
stdin/env). Whether the secret is inline, a file path, or an env var is
DESIGN's call; the **never-logged** invariant is the requirement.

### DD2 — token extraction per transport + exact reject mapping

- **gRPC**: the JWT arrives in request metadata under the
  `authorization` key as `Bearer <jwt>`. On failure, reject with
  **gRPC `UNAUTHENTICATED`** (tonic `Status::unauthenticated`). DESIGN
  locks the exact status message (must NOT leak the secret or the raw
  token; should carry the aegis `reason()` taxonomy verbatim, e.g.
  `expired` / `unknown_tenant`).
- **HTTP**: the JWT arrives in the `Authorization` request header as
  `Bearer <jwt>`. On failure, reject with **HTTP 401 Unauthorized**.
  DESIGN locks the exact body + any `WWW-Authenticate: Bearer` header
  shape (RFC 6750). The 401 body must carry the reason taxonomy and
  must NOT leak the secret/token.
- Confirmed (F2): aperture reads neither today.

### DD3 — authenticated tenant flow into the ingest path + sink

On success, the authenticated `TenantContext.tenant_id` must flow into
the accepted records — the sink must be told the REAL tenant. Today
`SinkRecord` (ports.rs:30-34, `#[non_exhaustive]`) carries no tenant
and `ingest_logs(body, transport, sink)` (app.rs:64) has no tenant
parameter. DESIGN must map the ripple: a signature change on
`ingest_*` to thread the `TenantId`, and how the tenant rides the
`SinkRecord` / sink boundary (a new field, a wrapper, or a sibling
parameter on `OtlpSink::accept`). The `single-validator-per-signal` CI
invariant (app.rs:13-19) and the `#[non_exhaustive]` SinkRecord
evolution guarantee constrain the shape. This is the largest
brownfield ripple and the reason the feature is sliced thin.

### DD4 — fail-closed posture: missing token, and on/off gating

- A **missing or empty** Authorization (no metadata key / no header /
  `Bearer ` with empty token) is a **reject**, NOT a default-tenant
  accept. This is the whole point; any ambiguity rejects.
- **Is auth unconditionally on, or gated by a config flag?** DESIGN
  decides, but the requirement constrains the default: **prefer
  on-by-default, OR refuse-to-start-without-auth-config**, echoing the
  ADR-0061 (`tls-config-reject-v0`) fail-closed precedent. ADR-0061
  established: when a security-relevant config is in a state v0 cannot
  safely honour, aperture refuses to start at config-validation time
  (`event=config_validation_failed`, exit 2, no listener binds —
  structural guarantee). The symmetric trap here: if auth is gated by a
  flag that defaults OFF, an operator who forgets the flag silently
  ships an unauthenticated gateway — the exact silent-downgrade
  ADR-0061 closed for TLS. **Requirement: a running aperture on the
  ingest path must not accept un-authenticated telemetry by accident.**
  Whether that is "auth always on" or "auth config present ⇒ enforced,
  absent ⇒ refuse-to-start" is DESIGN's mechanism choice constrained to
  fail-closed.

### DD5 — audit / observability of denials

One structured **deny event per rejected request**, reusing aegis's
`reason()` taxonomy (the 8 variants → 8 stable reason strings, F1/F7),
aligned with aperture's closed event vocabulary
(`observability.rs:30-52`) and stderr JSON convention. aegis already
emits exactly one audit event per `validate` call (F1); DESIGN decides
whether aperture relies on aegis's event, emits its own
gateway-flavoured deny event (naming transport + signal + reason), or
both — without producing duplicate or zero audit lines per rejected
request. The `subject` field names the ingest action (e.g.
`ingest_logs`). No secret, no raw token in any audit field.

### DD6 — scope fence

v0 = the **ingest path only** (gRPC + HTTP, three signals, full reject
matrix, across the slices). The **read-path auth** on query-api /
log-query-api / trace-query-api is a FOLLOW-UP feature, explicitly NOT
this one. SPIFFE / RS256 / JWKS / OPA are aegis v1, OUT. Role-based
authorization beyond authentication (e.g. requiring `operator` role to
write) is noted: the WS authenticates and tags the tenant; whether v0
also enforces `operator`-role-to-ingest is a DESIGN question flagged in
US-AUTH-05 — the minimum fail-closed requirement is authentication +
tenant tagging, not yet role-gated authorization.

### DD7 — the JWKS doc overstatement (adjacent, NOT folded)

`aegis/src/lib.rs:18-23,39-41` says "JWKS" but the validator is HS256
pre-shared-key only (F8). **Decision: flag as adjacent low-priority
doc-fix, do NOT fold into this feature.** Rationale: (a) this feature
is security-critical and large; mixing a doc edit in another crate
dilutes the review focus and the mutation-scope; (b) it touches aegis,
not aperture, so it is outside this feature's modified-file set;
(c) per the "decide rather than ask" posture, the call is made — it is
a one-line correction worth its own trivial wave or a fix-forward note,
not scope-creep on the auth boundary. Logged here so it is not lost.

## Risks

| ID | Risk | Prob | Impact | Mitigation |
|----|------|------|--------|------------|
| R1 | aperture is the LIVE ingest gateway — turning on auth changes WHO can write (max blast radius). | High | High | Slice thin; WS proves the boundary on one transport/signal; negative-control scenario (valid token still ingests) in every slice; fail-closed default per DD4. |
| R2 | The HS256 secret leaks into a log line / error / Debug. | Medium | Critical | DD1 never-logged invariant; AC `the-secret-is-never-logged` in US-AUTH-04; aegis already opaque-Debugs the key — aperture must not undo that. |
| R3 | A gated-OFF default ships an unauthenticated gateway silently (the ADR-0061 trap). | Medium | Critical | DD4 fail-closed default: on-by-default or refuse-to-start-without-config. |
| R4 | The `ingest_*`/`SinkRecord` signature ripple regresses the existing happy path, backpressure, shutdown, or serve-loop behaviour. | Medium | High | DD3 maps the ripple; non-regression AC: a correctly-authenticated client ingests exactly as before; existing slice tests stay green. |
| R5 | Duplicate or missing audit lines per rejected request once aperture adds its own deny event on top of aegis's. | Low | Medium | DD5: DESIGN picks one source of truth for the per-request deny event. |
| R6 | No DIVERGE artifacts exist for this feature (`docs/feature/aegis-ingest-auth-v0/diverge/` absent). | n/a | Low | Job grounded directly in the four-quadrants report + aegis-v0 D10 + verified code; JTBD re-derived in DISCUSS. Noted, not blocking. |

## DIVERGE grounding

No `diverge/recommendation.md` or `diverge/job-analysis.md` exists for
this feature. The job is grounded instead in: the four-quadrants aegis
report (UNWIRED dominant finding), aegis-v0 D10 (the planned deferral),
ADR-0061 (the fail-closed precedent), and the verified code facts F1-F8
above. This is acceptable for a brownfield wiring feature with a
single, well-understood fail-closed job; noted as R6.

## Inherited gates

ADR-0005's five gates apply; per-feature mutation testing at 100% kill
rate on the modified files (CLAUDE.md). Rust idiomatic (data + free
functions + traits where polymorphism is genuinely needed). NEVER bump
any crate to 1.0.0. Kaleidoscope is pure trunk-based (CI is feedback,
not a gate).
