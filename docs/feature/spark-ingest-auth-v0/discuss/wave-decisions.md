# DISCUSS Decisions — spark-ingest-auth-v0

> **Wave**: DISCUSS (nWave). **Analyst**: Luna (`nw-product-owner`).
> **Date**: 2026-06-06. **Feature type**: Backend / SDK (client-side
> auth). **Walking skeleton**: No (brownfield; the Spark SDK exists and
> ships telemetry today). **UX research**: Lightweight (one integrator
> persona). **Origin**: the symmetric consequence of
> `aegis-ingest-auth-v0` (ADR-0068). Bea Verifier (msg 038, N29) found
> that aperture now MANDATES a bearer token on every ingest request,
> but the Spark SDK has no way to send one — so the integrator cannot
> ship authenticated telemetry through aperture at all. E01-E04 (the
> Spark→Aperture round-trip) were GREEN pre-auth and are now BLOCKED.
> "I locked the door; the SDK has no key."

## The job (send-an-authenticated-export framing)

When an integrator instruments their service with the Spark SDK and
points it at an authenticated Kaleidoscope gateway, they can give the
SDK a bearer token (or set the standard `OTEL_EXPORTER_OTLP_HEADERS`
env var) so its telemetry is **accepted at the door** instead of every
span / log / metric being silently denied. The token rides on **all
three** OTLP signals (traces, logs, metrics). Without a token knob the
SDK can only talk to an UNauthenticated gateway — which, at aperture's
v0 fail-closed posture, no longer exists. The token is a **secret**:
the SDK sends it but never logs it.

## Verified facts (grounded in code on 2026-06-06, not the brief)

These were confirmed by reading the source. They are the load-bearing
premises for every story and slice.

- **F1 — `SparkConfig` has no auth knob.** `crates/spark/src/config.rs`
  exposes a value-consuming builder with six methods
  (`for_service`, `require_tenant_id`, `with_tenant_id`,
  `with_feature_flags`, `with_experiment_id`, `with_endpoint`,
  `with_flush_timeout`, `with_strict_schema_lint`). `with_endpoint`
  (config.rs:120) is the ONLY transport knob. There is **no**
  `with_bearer_token`, `with_auth_header`, or any
  authorization/metadata field. The struct is `#[non_exhaustive]`, so a
  new field is a non-breaking addition.

- **F2 — none of the three exporters attaches auth metadata.**
  `crates/spark/src/init.rs > build_pipeline` (init.rs:276-365) builds
  three OTLP/gRPC exporters: `SpanExporter` (282-289), `LogExporter`
  (314-321), `MetricExporter` (345-352). Each is
  `.with_tonic().with_endpoint(endpoint).build()` — **none** calls
  `.with_metadata(...)`, none installs a tonic interceptor, none
  attaches an `authorization` header. The tonic exporter therefore
  sends **no** authorization metadata on any signal.

- **F3 — Spark does NOT honour `OTEL_EXPORTER_OTLP_HEADERS` today.**
  The only env var Spark reads is `OTEL_EXPORTER_OTLP_ENDPOINT`
  (init.rs:70, `ENV_OTLP_ENDPOINT`; `operator_supplied_endpoint`,
  init.rs:611, reads only that one). A workspace grep across
  `crates/spark/` for `OTLP_HEADERS`, `with_metadata`, `MetadataMap`,
  `authorization`, and `bearer` returns **zero** matches. So neither a
  programmatic knob NOR the standard OTLP headers env var lets an SDK
  user attach a bearer today.

- **F4 — the observability surface logs endpoint, not secrets.**
  `crates/spark/src/observability.rs > emit_init_succeeded`
  (observability.rs:53-70) logs `service.name`, `endpoint`, `protocol`,
  `flush_timeout_ms` on the `target="spark"` `spark::init succeeded`
  INFO event. The event vocabulary is **closed** and asserted verbatim
  by the integration tests (observability.rs:13-26). A bearer-token
  field MUST NOT be added to this event or any other (see DD3 / System
  Constraint 1).

- **F5 — the gateway side already mints and demands the token.**
  ADR-0068 (`brief.md` §`aegis-ingest-auth-v0`) wires
  `aegis::Validator` onto aperture's ingest path: each of the 6 handlers
  (3 signals × 2 transports) reads the `authorization` metadata /
  `Authorization` header as `Bearer <jwt>` and rejects
  `UNAUTHENTICATED` / `401` with `reason=missing_claim` when none
  arrives — **nothing stored**. The aegis token is an HS256-signed JWT
  with `iss`/`aud`/`exp`/`tenant_id`/`kaleidoscope_role`. The harness
  already has a token-minting test seam (ADR-0068 "token-minting test
  seam"). So E01-E04 flip back GREEN the instant Spark gains an auth
  knob that puts `authorization: Bearer <jwt>` on the gRPC exporters.

## Walking-slice decision (brownfield; no walking skeleton)

Spark already ships telemetry end-to-end, so there is no walking
skeleton to build — this feature ADDS an auth knob to an existing,
working pipeline. The slices are carpaccio cuts of one capability:

1. **WS' (the driving slice) — programmatic `with_bearer_token`
   attaches `authorization: Bearer <token>` to all three exporters**,
   proven against a real aegis-authenticated aperture (the E01-E04
   shape): an authenticated export is ACCEPTED, and the negative
   control (no token configured → no header → an unauthenticated
   endpoint still accepts) stays green. The token reaching **all three**
   signals is IN this slice — a partial wire that authenticates traces
   but not logs is the verifier's E01-E04 failure (traces AND logs),
   so it is not shippable.
2. **The env-var slice — `OTEL_EXPORTER_OTLP_HEADERS` attaches the
   bearer**, mirroring how Spark already reads
   `OTEL_EXPORTER_OTLP_ENDPOINT`, with precedence resolved (DD2).
3. **The secret-posture slice — the token is never logged**, and the
   no-token-no-header negative control against an unauthenticated
   endpoint still works (Spark adds nothing when none is configured).

Carpaccio taste test applied: every slice is demonstrable against a
running aperture and none regresses the no-auth path. Because the cuts
are thin and share one exporter-build path, they may collapse into
fewer DELIVER slices at DESIGN's discretion — the requirement is the
observable behaviour, not the slice count.

## Five decisions flagged for DESIGN (solution-architect owns the mechanism)

> Requirements stay solution-neutral. These are the seams DESIGN must
> resolve; the requirement says WHAT must be observable, DESIGN says HOW.

### DD1 — the exact `opentelemetry_otlp` tonic API for attaching metadata, applied uniformly to all three exporters

`opentelemetry_otlp`'s tonic exporter exposes a metadata map
(`.with_metadata(MetadataMap)`) and/or a tonic interceptor surface.
DESIGN locks which one carries `authorization: Bearer <jwt>` and — the
load-bearing constraint — applies it **uniformly to `SpanExporter`,
`LogExporter`, and `MetricExporter` without duplication** (a single
helper that builds the metadata once and threads it into all three
exporter builders in `build_pipeline`, so a future signal cannot be
added un-authenticated by omission). A partial wire (some signals
authenticated, some not) is the explicit non-goal (System Constraint
3). Confirmed (F2): none attaches metadata today.

### DD2 — the `SparkConfig` surface + precedence between programmatic and env-var paths

DESIGN locks the programmatic surface — at minimum a
`with_bearer_token(token)` convenience that means `authorization:
Bearer <token>`; optionally a more general `with_auth_header(name,
value)` (kept v0-scoped to authorization — see DD4). And it locks the
**precedence** when both the programmatic knob and
`OTEL_EXPORTER_OTLP_HEADERS` are set. The strong recommendation,
mirroring the established endpoint chain
(`with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` > default,
init.rs:586-620): **programmatic value wins; the env var is the
fallback when the application did not call the knob.** The requirement
constrains the property (a deterministic, documented precedence;
programmatic-wins preferred); DESIGN locks the API names. The struct is
`#[non_exhaustive]` (F1) so the new field is non-breaking.

### DD3 — the SECRET posture: the bearer token must NEVER be logged

The bearer token is sensitive — it is a credential equivalent to the
HS256 secret on the gateway side. It MUST NOT appear in any
`target="spark"` event (not `spark::init succeeded`, not any new
event), not in a `Debug`/`Display` of `SparkConfig`, not in a
config-validation error, not in the resolved-config tracing fields
(F4). This mirrors aegis/aperture's never-log-the-secret discipline
(ADR-0068 DD1, `secret_file` stored as a `PathBuf`, key
opaque-Debugged). The token is **supplied by the integrator** (a
string passed to the knob, or the env var) — never baked into Spark.
DESIGN decides the redaction mechanism (an opaque `Debug` impl on the
config field, a newtype wrapper, etc.); the **never-logged** invariant
is the requirement. `SparkConfig` derives `Debug` today (config.rs:26)
— the new field must not let the token escape through it.

### DD4 — `OTEL_EXPORTER_OTLP_HEADERS` parsing scope: authorization-only vs general headers

The OTel spec format is a comma-separated `key=value` list, e.g.
`authorization=Bearer%20<jwt>` (percent-encoded per the spec). The v0
**need** is the `authorization` Bearer header — that is what aperture
demands (F5). DESIGN decides whether the parser supports the general
header list or just extracts `authorization`. **Recommendation: keep
v0 scoped to `authorization`** (parse the list, take the
`authorization` entry, ignore or warn-and-skip the rest), so the
surface stays minimal and the secret-handling path is the only
sensitive value. General-header support is an optional later
widening. Percent-decoding of the value (the spec mandates it for
`Bearer%20`) is part of the parse; DESIGN locks whether a malformed
header value is an error or a warn-and-skip (prefer: a malformed
`OTLP_HEADERS` that the operator clearly intended as auth should
surface, not silently drop the credential — but DESIGN owns the exact
failure mode, consistent with how an empty endpoint env var falls
through to default at init.rs:615-619).

### DD5 — failure mode when no token is configured against a remote endpoint: warn or stay silent

If no token is configured and the gateway rejects, that rejection is
the gateway's `UNAUTHENTICATED` / `401` surfacing (the aegis side,
ADR-0068) — **Spark's job is only to SEND the token it was given.**
The open question: should Spark **warn** when it is about to export to
a remote (non-loopback) endpoint with no token configured (a gentle
nudge that the operator likely forgot the credential), or stay
**silent** (the integrator may legitimately target an unauthenticated
local collector)? **Recommendation: silent-but-documented** — not
every endpoint requires auth, a warn would be noisy and wrong for the
local-collector case, and a false-positive security warning erodes
trust. The doc comment on the knob explains the no-token-no-header
behaviour. DESIGN owns the final call; if it chooses to warn, the warn
MUST NOT echo the (absent) token and SHOULD be suppressible. **Whatever
DESIGN chooses, the warn — if any — must never log a token value**
(DD3 binds here too).

## Risks

| ID | Risk | Prob | Impact | Mitigation |
|----|------|------|--------|------------|
| R1 | The bearer token leaks into a log line / error / `Debug`. | Medium | Critical | DD3 never-logged invariant; AC `the-token-is-never-logged` (US-SP-AUTH-03); mirror aegis/aperture opaque-Debug discipline; `SparkConfig` derives `Debug` (config.rs:26) so the field needs explicit redaction. |
| R2 | The token wires onto some signals but not all three (a partial auth). | Medium | High | System Constraint 3 + DD1 (one helper, uniform application); AC `the-token-reaches-all-three-signals` (US-SP-AUTH-01); the verifier's E01-E04 cover traces AND logs. |
| R3 | Adding auth metadata regresses the no-auth path (an unauthenticated local collector breaks). | Medium | High | Negative control in every slice: no token configured ⇒ Spark adds NO header; AC `no-token-no-header-against-an-unauthenticated-endpoint-still-works` (US-SP-AUTH-03). |
| R4 | Programmatic-vs-env precedence is ambiguous, so a deployment override behaves surprisingly. | Low | Medium | DD2: a deterministic documented precedence mirroring the established endpoint chain; AC in US-SP-AUTH-02. |
| R5 | `OTEL_EXPORTER_OTLP_HEADERS` percent-decoding / multi-header parsing is mishandled, dropping or corrupting the credential. | Low | Medium | DD4: v0 scoped to `authorization`, spec-conformant percent-decode, locked malformed-value failure mode; AC in US-SP-AUTH-02. |
| R6 | No DIVERGE artifacts exist for this feature (`docs/feature/spark-ingest-auth-v0/diverge/` absent). | n/a | Low | Job grounded directly in ADR-0068 (the gateway sibling), the verifier's N29/E01-E04, and the verified code facts F1-F5. JTBD re-derived in DISCUSS. Noted, not blocking. |

## DIVERGE grounding

No `diverge/recommendation.md` or `diverge/job-analysis.md` exists for
this feature. The job is grounded instead in: ADR-0068
(`aegis-ingest-auth-v0` — the gateway that now mandates the token), the
verifier's msg 038 / N29 (E01-E04 blocked by the missing SDK key), and
the verified Spark code facts F1-F5 above. This is acceptable for a
brownfield SDK-knob feature with a single, well-understood job; noted
as R6.

## Inherited gates

ADR-0005's five gates apply; per-feature mutation testing at 100% kill
rate on the modified spark files (CLAUDE.md); `gate-5-mutants-spark`
exists. Rust idiomatic (data + free functions + traits where
polymorphism is genuinely needed). NEVER bump any crate to 1.0.0.
Kaleidoscope is pure trunk-based (CI is feedback, not a gate).

## Scope Assessment: PASS

3 user stories, 1 bounded context (the `spark` crate only — `config.rs`
+ `init.rs`, with the secret-handling discipline mirrored from aegis),
no new crate, no new dependency beyond what `opentelemetry_otlp`
already provides (the tonic metadata surface). Estimated 1-2 days. Well
within the Elephant Carpaccio right-sizing bound; no split needed.
