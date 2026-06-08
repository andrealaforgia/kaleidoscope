# Wave Decisions — read-path-query-api-auth-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Agent**: Scholar (`nw-acceptance-designer`, as Quinn/Scholar)
- **Date**: 2026-06-08
- **Mode**: Autonomous overnight. Strategy C (real local): real axum transport
  on EPHEMERAL ports, real HS256 tokens minted in-suite, the existing
  tenant-scoped durable store seam (real filesystem I/O via the
  FileBacked\*Store the siblings use).
- **Scope**: write acceptance TESTS + RED-ready scaffolds for
  `read-path-query-api-auth-v0`. NO production logic. NO commit.
- **Inputs read (2026-06-08)**: `design/wave-decisions.md` (the three driving
  ports + per-AC asserts; DD1-DD6; the no-bearer-bypass; the audience fence;
  the redaction discipline), `adr-0074-read-path-query-api-auth.md` (the
  additive model; arm-2 no-fall-through; `aud=kaleidoscope-query`;
  `aegis::Validator` reuse; the test seam), all of `discuss/` (US-RAUTH-01..04
  + AC + Elevator Pitches), `devops/{environments.yaml,wave-decisions.md}` (the
  in-process auth-test environment; the NEW instrumentation DELIVER owes:
  pre-validate `missing_claim` event, the `subject` value, the auth startup
  negative probe, the partial-config refusal; C-DEVOPS-1..9), the brief's
  `### For Acceptance Designer — read-path-query-api-auth-v0`, the aperture
  INGEST-AUTH suite to mirror (`crates/aperture/tests/slice_10_ingest_auth.rs`)
  + aegis `slice_03_audit.rs` (the in-process capturing subscriber),
  `query-http-common/src/lib.rs` (the `resolve_tenant_or_refuse`/`ErrorBody`/
  `error_response`/redaction seam onto which the new
  `resolve_request_tenant_or_refuse` is scaffolded), the three read APIs'
  router signatures + existing slice/common harnesses, and the
  `aegis::Validator`/`ValidatorConfig`/`load_catalogue` public surface.

## Headline

Four acceptance test files across the four touched crates, **41 auth scenarios
all proven RED-not-BROKEN** + **6 backward-compat / DD6 guardrails proven
GREEN**, every API-bound scenario driving the **real router on an EPHEMERAL
port** with **real HS256 tokens minted in-suite**. The six load-bearing
security controls each have a dedicated, falsifiable scenario. No production
logic written; minimal scaffolds added so the suites COMPILE while staying RED.

## WS strategy (Dimension 9a — DECLARED)

**Strategy C — real local.** The walking skeleton and every API-bound scenario:

- bind the REAL read-API router (`*_query_api::router_with_auth(...)`) on a
  REAL ephemeral loopback listener (`tokio::net::TcpListener::bind("127.0.0.1:0")`),
- drive a REAL `reqwest` HTTP `GET` over loopback carrying a REAL
  `Authorization: Bearer <jwt>` header,
- seed and read the REAL durable store (`FileBackedMetricStore` /
  `FileBackedLogStore` / `FileBackedTraceStore` — real filesystem I/O), and
- mint REAL HS256 tokens in-suite with `jsonwebtoken::encode` (the same engine
  aegis validates with), built against a REAL `aegis::Validator` constructed
  from a catalogue loaded via the production `load_catalogue` (real TOML I/O).

There is NO `@in-memory` walking skeleton and NO InMemory store double on the
auth path. The litmus test ("if I deleted the real adapter, would the WS still
pass?") is satisfied: delete the real axum transport / the real validator and
the WS cannot pass. The shared-crate matrix suite drives the shared capability
function directly under an in-process capturing subscriber (the analogue of
aperture's reason-matrix) because the 8-reason taxonomy and the
one-audit-event-per-request property are properties of the SHARED capability,
best pinned once at the crate where the capability lands (ADR-0054, Option B
rejected) rather than re-proven three times.

## Ephemeral-port decision (MANDATORY — stated)

The query APIs default to fixed `9090/9091/9092`. A test that binds a fixed
port flakes the instant a leaked binder or a parallel test holds it (cf. the
`aperture-4317-flake` project memory). **Every scenario that binds a real query
API binds an EPHEMERAL port (`127.0.0.1:0`) and reads the actual address back
from the listener (`listener.local_addr()`).** No fixed port is ever bound by
this suite. The bound `SocketAddr` is the only address the `reqwest` client
targets. The shared-crate suite binds NO socket at all (it drives the function
seam directly), so it is immune to port contention by construction.

## Slice → file → scenario map (the 3 DESIGN slices)

| DESIGN slice | Stories | File | Scenarios (auth RED / guardrail GREEN) |
|---|---|---|---|
| **Slice 1 — WS (metrics)** | US-RAUTH-01 (+ US-RAUTH-02 no-bypass + US-RAUTH-04 audience fence on metrics) | `crates/query-api/tests/slice_07_read_auth.rs` | 8 RED / 3 GREEN |
| **Slice 2 — log+trace parity** | US-RAUTH-03 (logs) | `crates/log-query-api/tests/slice_09_read_auth.rs` | 7 RED / 2 GREEN |
| **Slice 2 — log+trace parity** | US-RAUTH-03 (traces + lookup-by-id) | `crates/trace-query-api/tests/slice_05_read_auth.rs` | 10 RED / 2 GREEN |
| **Slices 3+4 — closing controls** | US-RAUTH-04 (8-reason matrix, audience fence, DD6) + DD5 (one-event-per-request) | `crates/query-http-common/tests/slice_07_read_auth_shared.rs` | 13 RED / 1 GREEN (DD6 doc) |

**Totals: 38 ignored auth scenarios proven RED + 8 GREEN guardrail/doc tests**
(plus the 3 inline `query-api` doc tests / `red_reason_is_documented`).
Error/edge ratio (reject + isolation-negative + fail-closed-config + no-bypass +
audience + redaction): **>= 40%** by a wide margin — of the 38 auth scenarios
only 5 are pure happy-path accept/positive-isolation; the remaining 33 (87%)
are reject / negative-control / fail-closed / redaction.

## Security-control → scenario mapping (the six load-bearing controls)

Each load-bearing control (what the Verifier will attack) has a dedicated,
falsifiable scenario. Falsifiability tactic for the reject controls: the
reject scenarios set the **env tenant to `acme-prod` AND seed acme-prod's
data**, so a fall-through impl (or the scaffold's env path) returns 200 with
acme-prod's data — making the `status==401` / `data-absent` assertion FAIL
against any no-validate / env-fall-through implementation (the brief's
"every reject AC must FAIL against an env-tenant fall-through" requirement).

| Control | Where it is pinned | Falsifiability |
|---|---|---|
| **NO-BEARER-BYPASS** (R3, DD3 arm 2) | `query-api::no_bearer_does_not_downgrade_to_env_tenant` — auth ON + env tenant ALSO `acme-prod` + NO bearer → MUST 401 AND acme-prod's `up` series ABSENT, store never read. | Against the scaffold (which downgrades to env) the request returns 200 with acme-prod's data → both assertions FAIL. An `else env_tenant` fall-through CANNOT pass. (RED-proven: returns 200 with the up-series.) |
| **TENANT ISOLATION** (north star, R4) | positive+negative pair on metrics (`isolation_positive/negative_control_*`), logs, traces (window), AND **trace lookup-by-id** (`lookup_by_id_negative_control_*`, ADR-0053). Both halves mandatory; no AC asserts isolation with one half. | A non-isolating impl returns acme-prod's data for a globex-staging token → the negative control's `count==0` FAILS. |
| **AUDIENCE FENCE** (R6, KPI-6) | `ingest_audience_token_is_rejected_*` on metrics, logs, traces + `reason_wrong_audience_ingest_token` on the shared seam. A correctly-signed `aud=kaleidoscope-ingest` token → 401 `wrong_audience`, nothing read. | env tenant set → the scaffold serves acme-prod's data 200 → the 401 assertion FAILS. An impl that never checks the audience CANNOT pass. |
| **BACKWARD COMPAT** (R5, KPI-5) | `auth_off_resolves_env_tenant_and_ignores_the_header_*` (auth off → env tenant, stray header IGNORED) + `auth_off_unset_env_tenant_still_refuses_401_*` on all three APIs. These are GUARDRAILS — **GREEN now and after DELIVER** (they exercise the additive opt-out the scaffold already honours). | A DELIVER change that read the header when auth is off would turn these RED. The existing read-API slice tests stay green (unchanged `router()` signature; the new constructor is additive). |
| **REDACTION** (System Constraint 4, hard guardrail) | wire body: `the_secret_and_token_never_appear_in_the_*_401_body` on metrics, logs, traces. audit line: `the_secret_and_token_never_appear_in_any_audit_line` on the shared seam (substring-absence over every captured field). | A mutation that echoes the token into the reason/body or logs the secret is caught by the substring scan (secret + token are ASCII). |
| **PARTIAL CONFIG / startup negative probe** (DD1, DD4) | **DEFERRED to DELIVER's subprocess suite — see "Honest residual" below.** The config-validation refuse-to-start and the auth-startup negative probe are composition-root / `main`-boundary behaviours (subprocess exit-code + no-listener assertions), which DISTILL cannot author against a `router()` seam. The contract is recorded here + in `devops/wave-decisions.md` C-DEVOPS-2/C-DEVOPS-6 + ADR-0074 DD1/DD4. |

Additional pinned controls: the **8-reason matrix** (each distinct,
`reason_*` + `the_eight_reasons_are_mutually_distinct`), **one-audit-event-per-
request** incl. the pre-validate `missing_claim` case
(`exactly_one_decision_event_for_a_missing_claim_request`,
`exactly_one_allow_event_for_a_valid_token`), the **WWW-Authenticate: Bearer**
challenge on every 401, and the **DD6 role-question-resolved** recorded decision.

## Adapter coverage table (Dimension 9c)

The "driven adapters" exercised on the auth path and their real-I/O coverage:

| Adapter / boundary | Real-I/O coverage scenario | Tag |
|---|---|---|
| axum HTTP transport + `Authorization` header extraction (the NEW boundary the feature adds) | every API-bound scenario binds a real ephemeral listener + real `reqwest` GET | `@real-io @driving_adapter` |
| `aegis::Validator` (HS256 verify, catalogue lookup) | constructed real, fed real minted tokens; the 8-reason matrix exercises every variant | `@real-io` (in-process) |
| `aegis::load_catalogue` (TOML file I/O) | the validator catalogue is written to a temp `.toml` and loaded via the production loader | `@real-io` |
| `FileBackedMetricStore` / `FileBackedLogStore` / `FileBackedTraceStore` (durable read) | seeded + read via real filesystem I/O in every isolation scenario (incl. trace lookup-by-id `get_trace`) | `@real-io` (reused verbatim) |
| `query_http_common::resolve_request_tenant_or_refuse` (the shared capability — NEW) | the shared-crate suite drives it directly under a capturing subscriber | seam test |

Every NEW boundary the feature adds (the header-extraction transport + the
shared resolution capability) has at least one real-I/O scenario. InMemory
doubles are NOT used on the auth path (they cannot catch wiring / path-
resolution / output-format bugs).

## RED-not-BROKEN proof (Mandate 7)

The production wiring does not exist; minimal scaffolds make the suites COMPILE
while every auth scenario stays behaviourally RED. Proof, by RUNNING with
`--ignored` (each scenario fails on an ASSERTION, never on a missing symbol):

```
query-api    slice_07_read_auth          --ignored : 0 passed; 8 failed   (RED)
log-query    slice_09_read_auth          --ignored : 0 passed; 7 failed   (RED)
trace-query  slice_05_read_auth          --ignored : 0 passed; 10 failed  (RED)
qhc(shared)  slice_07_read_auth_shared   --ignored : 0 passed; 13 failed  (RED)
```

Default `cargo test` (the ignores keep trunk green): the 6 backward-compat
guardrails + the DD6 doc test + `red_reason_is_documented` PASS; the 38 auth
scenarios are `ignored`. **Failure shape (representative):**

- `no_bearer_does_not_downgrade_to_env_tenant`: scaffold returns
  `200 {...,"result":[{"metric":{"__name__":"up",...}}]}` (the env tenant's
  data) → `assert_eq!(status, 401)` FAILS `left: 200, right: 401`. This is the
  exact assertion that catches an env fall-through.
- `ws_valid_token_reads_its_own_tenant_metrics`: scaffold returns
  `401 {"error":"no tenant resolvable..."}` → `assert_eq!(status, 200)` FAILS.
- shared `reason_expired`: the scaffold PANICS; `catch_unwind` swallows it →
  zero captured events → `assert_eq!(deny_lines.len(), 1)` FAILS `left: 0`.

None of these is a compile/symbol error — RED, not BROKEN.

## Scaffolds added (minimum to compile RED; DELIVER un-ignores + implements)

1. **`query-http-common/src/lib.rs`** — added
   `pub fn resolve_request_tenant_or_refuse(auth: Option<&Arc<Validator>>,
   headers: &HeaderMap, env_tenant: &Option<TenantId>, service_label:
   &'static str, subject: &'static str, now: SystemTime) -> Result<TenantId,
   Response>` whose body PANICS `__SCAFFOLD__ read-path-query-api-auth-v0 RED`
   (the contract — the 3-arm precedence + the no-fall-through — is documented in
   the rustdoc). Re-exported `aegis::{load_catalogue, TenantContext, TenantId,
   ValidationError, Validator, ValidatorConfig}` so the capability + its callers
   depend on ONE crate. Dev-deps: `jsonwebtoken = "9"`, `tracing` (both already
   in `Cargo.lock`).
2. **The three read APIs (`query-api`/`log-query-api`/`trace-query-api`)** —
   added an `Option<Arc<aegis::Validator>> auth` field to each `ApiState` and an
   ADDITIVE `pub fn router_with_auth(store, tenant, auth[, static_dir])`
   constructor. The existing `pub fn router(...)` now delegates to
   `router_with_auth(.., None, ..)`, so its signature is **byte-for-byte
   unchanged** and every existing slice test stays GREEN (backward compat). The
   scaffold stores `auth` but the handler still resolves via the EXISTING
   `resolve_tenant_or_refuse` env seam — which is exactly why the auth scenarios
   are RED. Dev-deps added to each: `reqwest` (loopback client), `jsonwebtoken`
   (token mint), `serde` derive (the `Claims` struct). All three already in
   `Cargo.lock` — no new external/license/advisory surface (C-DEVOPS-3 honoured).

**What DELIVER does**: replace the scaffold panic with the 3-arm precedence
body; swap each handler's `resolve_tenant_or_refuse` call for
`resolve_request_tenant_or_refuse`; resolve the optional read-auth config + the
auth startup negative probe in each `composition.rs`; read the four auth env
vars in each `main.rs`; emit the one pre-validate `missing_claim` event +
thread the `subject`. Then un-ignore the suites one at a time.

## Token-minting (mirroring aperture slice_10 + aegis slice_03)

Each suite mints HS256 JWTs in-suite with `jsonwebtoken::encode`, signed with
the SAME `SECRET` bytes the test validator is built from, `aud=kaleidoscope-query`
(DD6 — the READ audience, NOT ingest), `iss=acme-observability`, a catalogued
tenant, future `exp`. The `Claims` struct (`iss/aud/exp/tenant_id/
kaleidoscope_role`) is identical to aperture's and aegis's so the SAME token
shape drives both the ingest door and the read door, and the suite ALIGNS with
the Verifier's A19/A20 harness. Negative-control mints (each perturbs one axis):
no token, empty `Bearer `, `not-a-jwt` (malformed), past `exp` (expired),
`WRONG_SECRET` (invalid_signature), `evil-issuer` (wrong_issuer),
`aud=kaleidoscope-ingest` (wrong_audience — the cross-surface fence),
out-of-catalogue tenant (unknown_tenant), role `auditor` (unknown_role).

## Honest residuals / what DISTILL did NOT author (handed to DELIVER)

- **The fail-closed config refuse-to-start + the auth startup negative probe
  (DD1, DD4)** are composition-root / `main`-boundary behaviours (subprocess
  exit-code + no-listener assertions), authored in DELIVER's subprocess suite
  (the analogue of aperture's `slice_10_ingest_auth_config_reject.rs`), NOT
  against the `router()` seam DISTILL drives. The contract is recorded here +
  ADR-0074 DD1/DD4 + `devops/wave-decisions.md` C-DEVOPS-2/C-DEVOPS-6. This is
  the ONE security control without a DISTILL scenario, and the reason is
  structural (DISTILL's seam is the router, not the binary).
- **The `WWW-Authenticate` parameterisation nuance** (bare `Bearer` for the
  no-token case vs `Bearer error="invalid_token"...` for a present-but-invalid
  token, ADR-0074 DD2 / RFC 6750 §3) is asserted at the coarse grain
  (`www.contains("Bearer")`); DELIVER may tighten the per-case parameterisation,
  and the suite will still pass.
- **The exact `subject` literal per API** (`query_range`/`log_query`/
  `trace_query`) is pinned on the shared seam for `query_range`; the per-API
  `subject` value is wired by DELIVER (C-DEVOPS handoff item b) and asserted in
  the shared-seam allow/deny line shape.

## Self-Review (acceptance-designer critique dimensions)

The `nw-acceptance-designer-reviewer` (Sentinel) was not nested-invocable in
this autonomous subagent run; a structured self-review against the
`nw-ad-critique-dimensions` skill follows.

| Dimension | Verdict | Evidence |
|---|---|---|
| **D1 Happy-path bias** | PASS | 33/38 auth scenarios (87%) are reject / negative-control / fail-closed / redaction; only 5 are happy accept/positive isolation. Error ratio >> 40%. |
| **D2 GWT compliance** | PASS | Each scenario = one Given (auth-configured instance + seeded tenant + a minted token), one When (one real GET), one Then (status + data-presence/absence + audit line). Single action per scenario. |
| **D3 Business-language purity** | CONDITIONAL-PASS | Rust integration tests (not Gherkin) — the project convention. The DISCUSS Gherkin in `user-stories.md` is the business-language layer; the test names + module docs map each scenario to its US-/AC- id in domain terms ("a token for one tenant cannot read another tenant's traces"). HTTP status codes appear (unavoidable at the real driving port) but are framed by domain intent. |
| **D4 Coverage completeness** | PASS | Every story (US-RAUTH-01/-02/-03/-04) and every brief AC has >= 1 scenario; trace lookup-by-id isolation explicitly covered (both halves). Story→scenario map above. |
| **D5 WS user-centricity** | PASS | The WS title is a user goal ("a valid token reads its OWN tenant"), the Then is a user observation (the tenant's `up` series present), not a layer-connectivity claim. |
| **D6 Priority** | PASS | The WS lands the riskiest assumption (validator wired into the shared read seam, fail-closed, isolated) on metrics first, per DESIGN's slice-1 recommendation; the no-bypass + audience fence ride the WS slice (DESIGN's permitted collapse). |
| **D7 Observable-behaviour assertions** | PASS | Every Then asserts a return value (`Result`/HTTP status) or an observable outcome (response body data, the audit decision line, the `WWW-Authenticate` header) — never internal state, never a private field, never a method-call count. The store-not-read property is asserted via the OBSERVABLE 401 + absent data, not by spying on the store. |
| **D8 Traceability** | PASS | Each scenario tags its US-/AC- id in the name/doc; environments (`clean`/`with-pre-commit`/`ci` from `environments.yaml`) are the build matrix for an internal multi-crate change — the auth-test environment (in-suite tokens, ephemeral bind) is realised by the suite itself. |
| **D9 WS boundary proof** | PASS | 9a strategy DECLARED (C). 9b match: real axum + real validator + real store, no `@in-memory` on the auth path. 9c every new adapter has a real-I/O scenario. 9d litmus: delete the real transport/validator → the WS cannot pass. 9e: zero `@in-memory` tags on any WS scenario. |
| **No Fixture Theater (Critical Rule 7)** | PASS | The Given steps set up PRECONDITIONS (seeded tenant data, a minted token, an env tenant), never the expected output. The reject scenarios deliberately set the env tenant to `acme-prod` + seed its data so the test FAILS unless the production code actually validates and refuses — proven by the `--ignored` RED run (every reject returns the env tenant's 200, failing the 401 assertion). |

**Self-review verdict: APPROVED.** All dimensions PASS (D3 conditional on the
project's Rust-integration-test convention, consistent with every sibling
DISTILL e.g. `slice_10_ingest_auth.rs`). The six load-bearing controls are each
pinned and falsifiable except the config-refuse-to-start / startup-probe pair,
which is structurally a DELIVER subprocess concern (recorded, not dropped).
Recommendation: proceed to DELIVER; un-ignore the suites one at a time per the
outer-loop convention, WS-metrics first.

## What this DISTILL wave does NOT do

- Does not write production logic (the scaffold `resolve_request_tenant_or_refuse`
  PANICS; the handlers still use the env seam; DELIVER owns `crates/*/src/`).
- Does not change the existing `router(...)` signatures (the new
  `router_with_auth` is additive) — so existing slice tests stay green.
- Does not author the config-refuse-to-start / auth-startup-negative-probe
  subprocess tests (a DELIVER `main`-boundary concern; contract recorded).
- Does not add a CI job or change CI config (C-DEVOPS-1) — the four existing
  `gate-5-mutants-<crate>` jobs cover the modified files via `--in-diff`.
- Does not bump any `Cargo.toml` version (C-DEVOPS-4; all pre-1.0, NEVER 1.0.0).
- Does not commit (the orchestrator commits between waves).
```
