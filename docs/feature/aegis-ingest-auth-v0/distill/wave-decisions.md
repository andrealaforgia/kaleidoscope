# DISTILL Decisions — aegis-ingest-auth-v0

- **Wave**: DISTILL (nWave). **Acceptance Designer**: Quinn
  (`nw-acceptance-designer`).
- **Date**: 2026-06-06. **Mode**: PROPOSE (autonomous overnight run).
- **Feature**: wire the correct-but-unwired `aegis::Validator` onto the live
  `aperture` OTLP ingest path, fail-closed (ADR-0068, DD1-DD7).
- **Decision records consumed**: ADR-0068, DESIGN `wave-decisions.md` (DD1-DD7),
  DEVOPS `environments.yaml` + `wave-decisions.md` (C-DEVOPS-1..9), DISCUSS
  `user-stories.md` (US-AUTH-01..05), `story-map.md`, `outcome-kpis.md`,
  `brief.md` §"Application Architecture — aegis-ingest-auth-v0" (the
  For-Acceptance-Designer note: driving ports + token-minting seam).

This wave produces the **outer-loop acceptance tests** that drive the DELIVER
wave Outside-In: behaviourally-RED, `#[ignore]`d-until-DELIVER tests against the
INTENDED post-change auth behaviour, driven end-to-end through the real aperture
binary's driving ports.

## Prior-wave reconciliation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| DISCUSS `user-stories.md` | US-AUTH-01 (WS, gRPC logs, fail-closed) + US-AUTH-03 (HTTP parity) + US-AUTH-05 (reject-reason matrix) → scenarios; the elevator pitches + domain examples (Diego/Mallory/Priya) → business-language scenario titles; the embedded BDD → starting Gherkin; the AC ids → the test-fn→AC map | − US-AUTH-04 (traces/metrics parity) is in the story set but OUT of THIS slice's test scope per the task delta (the logs spine is the falsifiable boundary; traces/metrics reuse the same spine and are a follow-on slice). Logged, not authored. |
| DISCUSS `story-map.md` | the WS definition (the security boundary reject-on-no-token, nothing-stored is IN the skeleton) → the `@walking_skeleton` scenarios; priority rationale → scenario ordering | − none |
| DISCUSS `outcome-kpis.md` | KPI-1 (authenticated-tenant coverage) → the tenant-tagged allow-line assertions; KPI-2 (reject-coverage) → the reject + sink-empty assertions; KPI-3 (reason distribution) → the 8-reason matrix; KPI-4 (refuse-to-start) → the config-reject suite; the secret-bytes-in-logs CRITICAL guardrail → the secret-never-logged test | − KPI measurability is PO-reviewer scope (DELIVER post-merge); not evaluated here |
| DESIGN `wave-decisions.md` / ADR-0068 | DD1 (jwt config + never-logged `secret_file`) → the `jwt_auth` builder scaffold + the secret-never-logged test; DD2 (per-transport extract + exact reject mapping: gRPC UNAUTHENTICATED, HTTP 401 + `WWW-Authenticate: Bearer`) → the reject assertions; DD3 (TenantScoped ripple) → the tenant-tag assertion via the OBSERVABLE allow audit line (not the internal payload type); DD4 (refuse-to-start at `into_config`, exit 2, no listener) → the config-reject subprocess suite; DD5 (aegis owns the per-validated-request audit; aperture owns the pre-validate no-token line; exactly one per request) → the `expect_one_decision` helper; DD6 (auth-only, role-gating deferred) → no role-gate enforcement test (only `unknown_role` reject, which aegis gives free); the test-seam note → the token-minting seam | − DD7 (aegis "JWKS" doc-fix) is ADJACENT (a `docs:` fix-forward), not in this feature — no test authored, correctly out of scope |
| DESIGN test-seam note | "drive the real binary; mirror slice_02 (HTTP), slice_07/09 (config), tests/common; mint HS256 tokens in-suite" → Strategy C real-local-IO + the in-suite minting | − none; the seam is grounded verbatim |
| DEVOPS `environments.yaml` | the refuse-to-start matrix (absent/incomplete/unreadable → exit 2 + `config_validation_failed` + no listener) → the four config-reject tests; the `clean` + `with-pre-commit` environments + the determinism mandate (boolean reject/accept + reason-string + exit-code, NO wall-clock) → the assertion style; C-DEVOPS-3 (jsonwebtoken already in lockfile via aegis) → the dev-dep addition | − none |
| DEVOPS `wave-decisions.md` | C-DEVOPS-5 (deterministic, runs in pre-commit AND CI) → no timing thresholds; C-DEVOPS-6 (falsifiability + non-regression mandatory) → the RED proof + the negative controls; C-DEVOPS-7 (guardrails green) → existing `slice_0*` + `invariant_single_validator` stay green; C-DEVOPS-1 (no new CI job; the `gate-5-mutants-aperture --in-diff` job covers the new files) → no CI change | − none |

**Contradictions found: NONE.** DISCUSS, DESIGN, and DEVOPS agree on the
fail-closed posture, the exact reject mappings, the secret-never-logged
invariant, the one-event-per-request audit contract, the refuse-to-start exit-2
seam, and the in-process token-minting test seam. The only scope reconciliation
is the explicit deferral of US-AUTH-04 traces/metrics parity to a follow-on
slice (the task delta scopes THIS slice to the logs spine as the falsifiable
boundary), recorded above and below.

## D1 — Walking-skeleton strategy: Strategy C (real-local-IO)

**Decision: Strategy C — real listeners + real temp files + in-suite-minted
HS256 tokens + a real binary subprocess for the exit-2 refusal.** No InMemory
doubles on the boundary. Concretely:

- **Real listeners**: the in-process `slice_10_ingest_auth.rs` suite starts a
  real aperture instance (`aperture::spawn`) on ephemeral loopback ports and
  drives it with a real `tonic` gRPC client over real TCP and a real `reqwest`
  HTTP client — the exact `slice_01`/`slice_02` driving-port pattern.
- **Real temp secret_file + real temp catalogue TOML**: the auth fixture writes
  a real HS256 secret file and a real `[[tenants]]` catalogue TOML to
  `std::env::temp_dir()` (the aegis catalogue on-disk shape), reaped by a
  Drop-guard. This mirrors the production `[aperture.security.auth.jwt]`
  `secret_file` + `catalogue_path` references (DD1).
- **In-suite-minted HS256 tokens**: `jsonwebtoken::encode` signs each token with
  the SAME secret bytes the `secret_file` holds, for the catalogued test tenant,
  with `iss`/`aud` matching the test config and a future `exp`. Each negative
  control perturbs exactly one axis.
- **Real binary subprocess for the exit-2 refusal**: the
  `slice_10_ingest_auth_config_reject.rs` suite runs the real
  `aperture --config <file>` binary (`CARGO_BIN_EXE_aperture`) and asserts the
  operator-visible surface: exit code 2, a `config_validation_failed` stderr
  line naming the offending config by reference, and a connect-refused probe on
  the default OTLP ports (the black-box "no listener bound" observable). This is
  the `slice_09_tls_config_reject` refuse-to-start pattern, applied to the
  auth-config invariant.

The litmus test (Dim 9d): "if I deleted the real adapter, would the WS still
pass?" — NO. The reject tests drive a real listener over real TCP; the config
tests drive a real OS process and read a real exit code. There is no InMemory
shortcut on the boundary. `@in-memory` appears on NO scenario.

## D2 — The token-minting seam (grounded)

The seam is the in-suite `sign(claims, secret)` helper over `jsonwebtoken::encode`
(the SAME engine `aegis::Validator` validates with), mirroring aegis's own
`crates/aegis/tests/slice_01_validate.rs` `make_jwt` helper. aegis exposes no
public token-minting helper (its `Validator` only validates), so the test mints
with `jsonwebtoken` directly using the test config's secret — exactly the path
the DESIGN test-seam note and the brief's For-Acceptance-Designer note prescribe.
The minted-token matrix:

| Mint | Perturbed axis | Expected aegis `reason` |
|---|---|---|
| `valid_token` | none (catalogued tenant, future exp, correct iss/aud/sig/role) | `allow` |
| no metadata / no header | token absent | `missing_claim` (pre-validate, aperture-owned) |
| empty `Bearer ` | empty token | `missing_claim` (pre-validate) |
| `not-a-jwt` | not a JWT | `malformed` |
| `expired_token` | `exp` in the past | `expired` |
| `invalid_signature_token` | signed with WRONG_SECRET | `invalid_signature` |
| `wrong_issuer_token` | `iss` ≠ configured | `wrong_issuer` |
| `wrong_audience_token` | `aud` = `kaleidoscope-query` | `wrong_audience` |
| `unknown_tenant_token` | tenant not in catalogue | `unknown_tenant` |
| `unknown_role_token` | role `auditor` | `unknown_role` |

## D3 — `#[ignore]`-until-DELIVER with proven-RED evidence (Mandate 7)

Every behaviourally-RED scenario is `#[ignore = "RED until DELIVER:
aegis-ingest-auth-v0"]`, because the project pre-commit hook runs
`cargo test --workspace` on EVERY commit and NEVER uses `--no-verify` — so trunk
must stay green at the DISTILL commit. The negative-control
`red_reason_is_documented` (one per suite) is NOT ignored (it passes today,
documents the ignore reason). RED was PROVEN by running with `--ignored`:

- **Default `cargo test -p aperture`: GREEN** — 19 test binaries, all
  `test result: ok`, 22 auth scenarios ignored, the existing `slice_0*` +
  `invariant_single_validator` unchanged.
- **`slice_10_ingest_auth -- --ignored`: 16 FAIL on assertion panics** (the
  request was accepted 200/OK instead of rejected; the sink was non-empty; zero
  decision lines), 2 PASS (the two genuine GUARDRAILS: `..._reaches_the_sink` —
  a valid token must store — and `the_configured_secret_never_appears...` — a
  hard invariant that must always hold). Every failure is a behavioural
  assertion, NOT a missing-symbol compile error → RED, not BROKEN.
- **`slice_10_ingest_auth_config_reject -- --ignored`: all 4 FAIL on assertion
  panics** — the absent-config test sees `exit_code: None` (today's binary
  STARTS and BINDS, never refusing) vs the required `Some(2)`; the
  incomplete/unreadable tests see `unknown field: found jwt` (today the jwt table
  is rejected as an unknown field) instead of a named `catalogue_path`/
  `secret_file` reference; the complete-jwt-starts control sees the binary exit
  early (today the jwt table is rejected) instead of binding. RED, not BROKEN.
  The suite finished in 5.18s (bounded waits) and leaked ZERO aperture
  processes.

## D4 — Falsifiability note (each reject AC fails against today's no-auth code)

The DEVOPS C-DEVOPS-6 mandate: each reject AC MUST FAIL against a build with no
auth wiring and pass ONLY when the token is validated and the request rejected
with nothing stored; each fail-closed-config AC MUST FAIL against a build that
boots without auth config and pass ONLY when the binary refuses to start. Proven
above (D3): against today's no-auth aperture every reject scenario is accepted
(200/OK, sink non-empty, no deny line) and every refusal scenario boots-and-binds
(or refuses for the wrong unknown-field reason). The happy-path accept
assertions were STRENGTHENED to remain falsifiable: rather than asserting only
"200/OK" (which passes against today's no-auth code), they assert "200/OK AND
exactly one tenant-tagged `allow` decision line" — and the allow-line half FAILS
RED today (no decision is taken). The only un-falsifiable-today tests are the two
guardrails noted in D3, which is correct: a guardrail must hold at every commit.

## D5 — The minimal RED scaffold (the only production-source touch)

Mandate 7 prefers behavioural-RED over scaffolds, but where a genuinely-absent
symbol would make a test BROKEN (a compile error) rather than RED, a minimal
scaffold is added ONLY there. ONE scaffold was required: the in-process accept/
reject suite drives an auth-CONFIGURED instance, and `Config::builder()` has no
way to express jwt auth today. Added to `crates/aperture/src/config/mod.rs`
(mirroring the existing `tls_enabled`/`spiffe_enabled` forward-compat scaffold
precedent that the slice_07/09 tests rely on):

- `ConfigBuilder::jwt_auth(issuer, audience, secret_file, catalogue_path)` — a
  setter that STORES the params,
- `pub struct JwtAuthConfig { issuer, audience, secret_file: PathBuf,
  catalogue_path: PathBuf }` — the secret is a PATH, never bytes (DD1's
  never-logged invariant honoured even in the scaffold),
- `Config { jwt_auth: Option<JwtAuthConfig> }` + a `pub(crate) jwt_auth()`
  accessor.

The scaffold does NOT wire a validator — DELIVER does. An instance built with it
behaves like today's no-auth aperture, which is exactly WHY the accept/reject
scenarios driven against it are behaviourally RED. No `transport.rs`/`app.rs`/
`ports.rs` production logic was touched — the feature is unimplemented, as nWave
order requires (DISTILL precedes DELIVER). The config-reject subprocess suite
needs NO scaffold: it drives the real binary against the real TOML schema (where
the jwt table is genuinely absent today), so it is purely behaviourally RED.

## D6 — Driving-port discipline (Mandate 1 / Hexagonal)

Every scenario enters through a DRIVING PORT only — the gRPC `authorization`
metadata, the HTTP `Authorization` header, or the `aperture --config` binary —
and asserts OBSERVABLE outcomes only: the gRPC status / HTTP status + header, the
recording sink's emptiness/tenant-tag (via the OBSERVABLE allow-line `tenant_id`
field, NOT by reaching into the internal `TenantScoped` payload type), the
captured stderr decision lines, and the process exit code. No internal auth type
(`extract_bearer_*`, `reject_to_*`, the `Validator` wiring, `TenantScoped`) is
named or constructed by any test. This keeps the tests refactor-proof and avoids
Testing Theater.

## D7 — Audit-line assertion (DD5 one-event-per-request)

The `expect_one_decision(events, decision, reason, subject)` helper asserts
EXACTLY ONE decision line per request (never zero, never duplicated), matching on
the structured `decision`/`reason`/`subject` fields (the aegis audit-event field
contract), not on the event message name. This encodes DD5's "exactly one
decision event per request" invariant as a first-class assertion the
reject-matrix and accept tests all share.

## Test scope (this slice)

| Suite | File | Stories | Scenarios | Driving port |
|---|---|---|---|---|
| Ingest-auth boundary | `crates/aperture/tests/slice_10_ingest_auth.rs` | US-AUTH-01 (WS), US-AUTH-03 (HTTP), US-AUTH-05 (reasons) | 18 (`#[ignore]`d) + 1 control | gRPC metadata + HTTP header + sink + stderr |
| Fail-closed config | `crates/aperture/tests/slice_10_ingest_auth_config_reject.rs` | US-AUTH-02 | 4 (`#[ignore]`d) + 1 control | `aperture --config` binary (exit code + stderr + connect-refused) |

US-AUTH-04 (traces/metrics parity) reuses this spine and is a follow-on slice —
out of THIS slice's falsifiable-boundary scope (recorded per the task delta).

## Constraints carried into DELIVER

- Remove the `#[ignore]`s one at a time, walking-skeleton first
  (`grpc_logs_without_token_*`), implementing the auth wiring until each goes
  GREEN. Do NOT batch-unignore.
- DELIVER replaces the `jwt_auth` scaffold with the real validator construction
  (`load_catalogue` + `Validator::new`) at composition, the `from_toml_str` jwt
  schema (DD1), the `into_config` refuse-to-start invariant (DD4), the
  per-transport bearer extraction + reject mapping (DD2), and the `TenantScoped`
  tenant ripple (DD3).
- The two guardrails (`..._reaches_the_sink`, `secret_never_appears`) must STAY
  green throughout (they pass today and must keep passing).
- `invariant_single_validator` must stay green (the aegis `validate` call is a
  different symbol in the transport layer, not a harness `validate_*` call site).
- The existing `slice_0*` tests will need a valid token + auth config once auth
  is on (DELIVER updates `tests/common` once — per ADR-0068 Negative
  Consequences); that is a DELIVER task, not a DISTILL one.
- Determinism: NO wall-clock thresholds (C-DEVOPS-5); all assertions are boolean
  reject/accept + reason-string + sink-empty/tenant-tag + audit-line-count +
  exit-code.
- Every spawned binary is reaped (kill + wait via a Drop-guard on every path);
  verified ZERO leaked aperture processes after the DISTILL runs.

## Hygiene (proven at DISTILL)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p aperture --tests` and `--lib`: clean (zero warnings).
- Default `cargo test -p aperture`: GREEN (exit 0, all 19 binaries `ok`).
- No leaked aperture processes after `--ignored` runs.
