# Acceptance Test Scenarios — aegis-ingest-auth-v0

Outer-loop acceptance tests for wiring `aegis::Validator` onto the live aperture
OTLP ingest path, fail-closed (ADR-0068). All RED scenarios are
`#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]`; DELIVER removes the
ignores one at a time. Driven end-to-end through the real binary's driving ports.

Test files:
- `crates/aperture/tests/slice_10_ingest_auth.rs` (gRPC + HTTP logs boundary)
- `crates/aperture/tests/slice_10_ingest_auth_config_reject.rs` (fail-closed config)

## Scenario list (Given-When-Then, business language)

### US-AUTH-01 — Walking skeleton: gRPC logs, fail-closed (`@walking_skeleton @driving_port`)

**WS-1 — A request with no bearer token is rejected unauthenticated**
Given aperture is running with auth configured for a catalogue containing "acme-prod"
And Mallory presents no authorization metadata on the gRPC call
When Mallory sends a gRPC logs export claiming tenant "acme-prod" in the payload
Then aperture rejects the call as unauthenticated.

**WS-2 — A tokenless request stores nothing**
Given the same tokenless gRPC export
When Mallory sends it
Then no record reaches the sink.

**WS-3 — A tokenless request emits exactly one deny decision (reason missing_claim)**
Given the same tokenless gRPC export
When Mallory sends it
Then exactly one audit decision with decision "deny", reason "missing_claim", subject "ingest_logs" is emitted.

**WS-4 — A valid token is accepted with one tenant-tagged allow decision**
Given Diego presents a valid bearer token for catalogued tenant "acme-prod"
When Diego sends a gRPC logs export with that token
Then aperture accepts the batch
And exactly one audit decision with decision "allow", subject "ingest_logs" is emitted.

**WS-5 — A valid token tags the accepted record with its tenant**
Given Diego presents a valid bearer token for "acme-prod"
When Diego sends a gRPC logs export with that token
Then the accepted record carries an authenticated tenant "acme-prod" (the allow decision names tenant_id "acme-prod").

**WS-6 — A valid authenticated export reaches the sink** (guardrail)
Given Diego presents a valid bearer token for "acme-prod"
When Diego sends a gRPC logs export with that token
Then exactly one record reaches the sink.

**WS-7 — An expired token is rejected with reason expired, nothing stored**
Given Diego presents a bearer token for "acme-prod" whose expiry is in the past
When Diego sends a gRPC logs export with that token
Then aperture rejects it as unauthenticated
And nothing reaches the sink
And exactly one deny decision with reason "expired", subject "ingest_logs" is emitted.

### US-AUTH-03 — HTTP transport parity (`@driving_port`)

**HTTP-1 — A POST with no Authorization header is rejected 401 + challenge, nothing stored**
Given aperture is running with auth configured
And Mallory sends no Authorization header
When Mallory POSTs OTLP/protobuf logs to /v1/logs
Then aperture responds 401 Unauthorized with a WWW-Authenticate: Bearer challenge
And no record reaches the sink.

**HTTP-2 — A tokenless POST emits exactly one deny decision (reason missing_claim)**
Given the same tokenless HTTP POST
When Mallory sends it
Then exactly one deny decision with reason "missing_claim", subject "ingest_logs" is emitted.

**HTTP-3 — An unknown-tenant token is rejected 401 with reason unknown_tenant, nothing stored**
Given Mallory presents a correctly-signed token claiming tenant "acme-prod-evil" (not in the catalogue)
When Mallory POSTs logs to /v1/logs with that token
Then aperture responds 401
And nothing reaches the sink
And exactly one deny decision with reason "unknown_tenant" is emitted.

**HTTP-4 — A valid token is accepted 200 with a tenant-tagged allow decision**
Given Diego presents a valid bearer token for "acme-prod" in the Authorization header
When Diego POSTs OTLP/protobuf logs to /v1/logs with that header
Then aperture accepts with the existing 200 response shape
And exactly one allow decision naming tenant_id "acme-prod", subject "ingest_logs" is emitted.

### US-AUTH-05 — Reject-reason matrix (`@driving_port @property` reason-coverage)

Each rejected request carries exactly one distinct aegis reason in exactly one
deny decision line; nothing is stored. Driven over gRPC logs.

**M-invalid_signature** — a structurally-valid JWT signed with the wrong key → reason "invalid_signature".
**M-wrong_issuer** — `iss` ≠ configured issuer → reason "wrong_issuer".
**M-wrong_audience** — a token minted for "kaleidoscope-query" → reason "wrong_audience".
**M-unknown_role** — an otherwise-valid token whose role is "auditor" → reason "unknown_role".
**M-malformed** — a bearer value that is not a JWT → reason "malformed".
**M-empty_bearer** — `Bearer ` with no token → reason "missing_claim" (decided at the extraction boundary, distinct from malformed).

### Secret-never-logged guardrail (`@kpi` guardrail — System Constraint 4)

**S-1 — The configured secret never appears in any log line**
Given an auth-configured instance
When a denied request and an accepted request both run
Then the configured HS256 secret bytes appear in NO captured stderr/audit line.

### US-AUTH-02 — Fail-closed config: refuse-to-start (`@driving_port @real-io @adapter-integration`)

**CFG-1 — An absent auth config refuses to start naming the missing auth**
Given an aperture config with transport but no [aperture.security.auth.jwt] block
When the operator runs `aperture --config <file>`
Then aperture exits 2
And a config_validation_failed event names the missing auth (jwt) configuration
And no listener binds on the OTLP ports.

**CFG-2 — An incomplete auth config refuses to start naming the missing field**
Given a jwt block missing the required catalogue_path
When the operator runs aperture with it
Then aperture exits 2, config_validation_failed names "catalogue_path", no listener binds.

**CFG-3 — An unreadable secret_file refuses to start naming the path, not the bytes**
Given a jwt block whose secret_file points at an unreadable path
When the operator runs aperture with it
Then aperture exits 2, config_validation_failed names the secret source by reference, no listener binds, and no secret bytes appear.

**CFG-4 — A complete, readable jwt config starts and binds** (negative control)
Given a complete, readable [aperture.security.auth.jwt] block on ephemeral ports
When the operator runs aperture with it
Then aperture starts and binds the listeners.

## Test-fn → US / AC map

| Test fn | Suite | Story | AC |
|---|---|---|---|
| `grpc_logs_without_token_is_rejected_unauthenticated` | auth | US-AUTH-01 | no-token-rejected-unauthenticated |
| `grpc_logs_without_token_stores_nothing` | auth | US-AUTH-01 | nothing-stored |
| `grpc_logs_without_token_emits_one_deny_audit_line_missing_claim` | auth | US-AUTH-01 | one-audit-event-per-request |
| `grpc_logs_with_valid_token_is_accepted_with_one_allow_line` | auth | US-AUTH-01 | valid-token-ingests + one-allow |
| `grpc_logs_with_valid_token_tags_the_authenticated_tenant` | auth | US-AUTH-01 | tagged-with-its-tenant (KPI-1) |
| `grpc_logs_with_valid_token_reaches_the_sink` | auth | US-AUTH-01 | valid-token-ingests (guardrail) |
| `grpc_logs_with_expired_token_is_rejected_reason_expired` | auth | US-AUTH-01 | expired-rejected-matching-reason |
| `http_logs_without_authorization_header_is_rejected_401` | auth | US-AUTH-03 | no-token-rejected-401-nothing-stored |
| `http_logs_without_authorization_header_emits_one_deny_line` | auth | US-AUTH-03 | one-audit-event-per-request (HTTP) |
| `http_logs_with_unknown_tenant_token_is_rejected_reason_unknown_tenant` | auth | US-AUTH-03 | unknown-tenant-matching-reason |
| `http_logs_with_valid_token_is_accepted_200_with_tenant_tagged_allow_line` | auth | US-AUTH-03 | valid-token-ingests-tagged (HTTP) |
| `grpc_logs_with_bad_signature_token_reason_invalid_signature` | auth | US-AUTH-05 | rejected-with-matching-reason |
| `grpc_logs_with_wrong_issuer_token_reason_wrong_issuer` | auth | US-AUTH-05 | rejected-with-matching-reason |
| `grpc_logs_with_wrong_audience_token_reason_wrong_audience` | auth | US-AUTH-05 | rejected-with-matching-reason |
| `grpc_logs_with_unknown_role_token_reason_unknown_role` | auth | US-AUTH-05 | rejected-with-matching-reason |
| `grpc_logs_with_malformed_token_reason_malformed` | auth | US-AUTH-05 | rejected-with-matching-reason (distinct) |
| `grpc_logs_with_empty_bearer_reason_missing_claim` | auth | US-AUTH-05 | rejected-with-matching-reason (distinct) |
| `the_configured_secret_never_appears_in_any_log_line` | auth | US-AUTH-02/05 | the-secret-is-never-logged |
| `absent_auth_config_refuses_to_start_naming_missing_auth` | config | US-AUTH-02 | refuses-to-start exit-2 no-listener |
| `incomplete_auth_config_refuses_to_start_naming_the_missing_field` | config | US-AUTH-02 | refuses-to-start (missing field) |
| `unreadable_secret_file_refuses_to_start_naming_the_path_not_the_bytes` | config | US-AUTH-02 | the-secret-is-never-logged + refuse |
| `complete_jwt_config_starts_and_binds` | config | US-AUTH-02 | complete-config-starts (control) |
| `red_reason_is_documented` (×2) | both | — | suite documentation (not a behaviour) |

## Adapter coverage table (Dim 9c)

The single NEW driven boundary this feature adds is the **bearer-token
validation** boundary (an IN-PROCESS HS256 validation against a pre-shared key +
a local TOML catalogue — no network at validation time, per DEVOPS). It is
exercised with REAL I/O on every reject-matrix and accept scenario: real minted
tokens over real gRPC metadata / HTTP headers against a real listener, with a
real temp `secret_file` + a real temp catalogue TOML. The config boundary (the
`into_config` refuse-to-start) is exercised by the real `aperture --config`
binary subprocess (`@real-io`). No InMemory double stands in for either boundary.

| Driven boundary | Real-I/O scenario(s) | Tag |
|---|---|---|
| Bearer-token validation (HS256 + catalogue) | the whole reject matrix + accept (real listener, real token, real files) | `@real-io @adapter-integration` |
| Refuse-to-start config validation | the 4 config-reject subprocess tests (real binary, real exit code) | `@real-io @adapter-integration` |

## Error-path ratio (Dim 1)

A security feature should be sad-path heavy. Counting the 22 behavioural
scenarios (excluding the 2 `red_reason_is_documented` documentation tests):

- **Error / reject / refuse scenarios**: WS-1, WS-2, WS-3, WS-7 (gRPC tokenless +
  expired) + HTTP-1, HTTP-2, HTTP-3 (HTTP tokenless + unknown-tenant) + the 6
  reason-matrix + CFG-1, CFG-2, CFG-3 (3 refusals) = **16**.
- **Happy-path / accept / boot scenarios**: WS-4, WS-5, WS-6 (gRPC accept) +
  HTTP-4 (HTTP accept) + CFG-4 (boot) + S-1 (secret-absence guardrail) = **6**.

**Error-path ratio = 16 / 22 = 73%** — well above the 40% mandate. Correct for a
fail-closed security boundary.

## Self-review checklist (critique-dimensions, applied directly)

| Dimension | Result |
|---|---|
| **1. Happy-path bias** | PASS — 73% error/reject/refuse (≥ 40%). |
| **2. GWT compliance** | PASS — each scenario is one precondition + one action + observable outcome(s); reject and accept are separate scenarios (no multi-action). |
| **3. Business-language purity** | PASS — titles use "rejected unauthenticated", "stores nothing", "refuses to start", "tagged with its tenant", "the secret never appears". Wire-level terms (401, UNAUTHENTICATED, WWW-Authenticate) appear only as the OBSERVABLE protocol contract the operator certifies against (RFC 6750), which the stories and ADR name as the user-visible outcome — not as implementation leakage. |
| **4. Coverage completeness** | PASS — US-AUTH-01/02/03/05 each have ≥ 1 scenario per AC; US-AUTH-04 explicitly deferred (logged). |
| **5. Walking-skeleton user-centricity** | PASS — the WS scenarios are framed as Diego/Mallory goals ("a request with no token is rejected and nothing is stored"; "a valid token ingests tagged with its tenant"), demo-able to Priya's audit ("can an unauthenticated caller write telemetry? — no"). Not "the layers connect". |
| **6. Priority validation** | PASS — the WS is the riskiest assumption (aegis wires onto the live path fail-closed without regressing the happy path); scenarios ordered WS → HTTP parity → reason matrix per the story map. |
| **7. Observable-behaviour assertions** | PASS — every Then asserts a driving-port return value (gRPC status / HTTP status + header), an observable outcome (sink emptiness, the allow-line tenant_id, the exit code), or a captured stderr decision line. The tenant tag is asserted via the OBSERVABLE allow-line `tenant_id` field, NOT by reaching into the internal `TenantScoped` payload. No private-field / method-call assertions. |
| **8. Traceability** | PASS — Check A: every US-AUTH-01/02/03/05 id maps to ≥ 1 test (table above). Check B: the `clean` + `with-pre-commit` environments both run `cargo test --workspace` (the suites run identically there); the auth fixture's Given clauses reference the real temp `secret_file` + catalogue (the environment precondition). |
| **9. WS boundary proof** | PASS — 9a: Strategy C declared in `wave-decisions.md` D1. 9b: implementation matches (real listeners + real files + real subprocess; `@in-memory` on NO scenario). 9c: the bearer-validation + config boundaries each have a real-I/O test. 9d: deleting the real listener/binary would fail every WS scenario. 9e: zero `@in-memory` tags. |

## Mandate compliance evidence (for handoff)

- **CM-A (Hexagonal)**: every test imports/drives only the driving ports —
  `LogsServiceClient` (gRPC), `reqwest` (HTTP), `CARGO_BIN_EXE_aperture` (binary),
  `aperture::testing::RecordingSink` + `stderr_capture` (observers). Zero
  internal-auth-component imports (`extract_bearer_*`, `reject_to_*`,
  `TenantScoped` are never named).
- **CM-B (Business language)**: titles + assertions speak outcomes; the only
  technical tokens are the OBSERVABLE protocol contract (HTTP 401, gRPC
  UNAUTHENTICATED, `WWW-Authenticate: Bearer`) that the audit certifies against.
- **CM-C (Walking skeleton)**: 7 WS scenarios (US-AUTH-01) prove the user goal
  (no unauthenticated write; authenticated write tagged); 15 focused scenarios
  (HTTP parity + reason matrix + config refusal) cover breadth.
- **CM-D (Pure-function / adapter isolation)**: the validation logic is reused
  verbatim from aegis (a pure, I/O-free `validate`); the test parametrises only
  the thin token-mint + temp-file adapter layer; no environment-matrix fixture
  parametrisation beyond the real `secret_file`/catalogue temp files.
