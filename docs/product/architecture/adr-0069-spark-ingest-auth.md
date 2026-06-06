# ADR-0069 — Spark ingest authentication: attach a never-logged bearer token to all three OTLP exporters, uniformly

- **Status**: Accepted
- **Date**: 2026-06-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `spark-ingest-auth-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0068 (`aegis-ingest-auth-v0` — the gateway sibling that mandates and validates this token; this feature is its client-side key), ADR-0011 (spark public-API + crate layout; the `#[non_exhaustive]` `SparkConfig` evolution guarantee), ADR-0013 (spark dependency pinning; `opentelemetry_otlp =0.27`, gRPC/tonic default transport), ADR-0014 (flush-timeout mechanism; the `build_pipeline` exporter-build path this feature extends), ADR-0017 (spark logs via tracing appender; the `target="spark"` event surface a token must not join), ADR-0025 (codex-spark schema lint; the lint/resolve order in `init`), ADR-0005 (CI five gates — spark IS in the Gate 2/3 public-API set).

## Context

**Aegis locked the ingest door (ADR-0068); the Spark SDK has no key.** Aperture now wires `aegis::Validator` onto every ingest request (3 signals × 2 transports), fail-closed: a request that arrives without `authorization: Bearer <jwt>` is rejected `UNAUTHENTICATED` / `401` with `reason=missing_claim`, **nothing stored**. The token aperture demands is an HS256-signed JWT (`iss`/`aud`/`exp`/`tenant_id`/`kaleidoscope_role`), presented as `authorization: Bearer <jwt>`.

But the Spark SDK cannot send one. Confirmed by reading the source on 2026-06-06:

- **F1 — `SparkConfig` has no auth knob.** `crates/spark/src/config.rs:27` defines a value-consuming builder; `with_endpoint` (`config.rs:120`) is the *only* transport knob. There is no `with_bearer_token`, no `with_auth_header`, no authorization/metadata field. The struct is `#[non_exhaustive]` (`config.rs:25`) and derives `Debug, Clone` (`config.rs:26`).
- **F2 — none of the three exporters attaches auth metadata.** `build_pipeline` (`init.rs:276-408`) builds `SpanExporter` (282-289), `LogExporter` (314-321), `MetricExporter` (345-352), each `.with_tonic().with_endpoint(endpoint).build()`. **None** calls `.with_metadata(...)`, none installs an interceptor. The `use` at `init.rs:45` imports `WithExportConfig` but **not** `WithTonicConfig` (the trait carrying `with_metadata`).
- **F3 — Spark ignores `OTEL_EXPORTER_OTLP_HEADERS`.** The only env var Spark reads is `OTEL_EXPORTER_OTLP_ENDPOINT` (`init.rs:70`; `operator_supplied_endpoint`, `init.rs:611`). Zero `OTLP_HEADERS` / `with_metadata` / `MetadataMap` / `authorization` / `bearer` matches across `crates/spark/`.
- **F4 — the observability surface logs endpoint, not secrets.** `emit_init_succeeded` (`observability.rs:53-70`) logs `service.name`/`endpoint`/`protocol`/`flush_timeout_ms` on the `target="spark"` `spark::init succeeded` INFO event; the vocabulary is closed and asserted verbatim by the integration tests. A token must NOT join this surface, nor escape via `SparkConfig`'s derived `Debug`.
- **F5 — the gateway side already mints and demands the token (ADR-0068).** The harness has a token-minting test seam (HS256 `encode` via the aegis test fixture). E01-E04 (the Spark→Aperture round-trip, covering traces AND logs) were GREEN pre-auth and are now BLOCKED.

This feature is the symmetric consequence of ADR-0068: give the SDK the key the gateway demands, so E01-E04 flip back GREEN — without ever logging the credential and without breaking the no-auth path.

### What DESIGN must lock (this ADR)

1. **DD1** — the exact `opentelemetry_otlp` tonic API for attaching `authorization: Bearer <jwt>`, applied uniformly to all three exporters via one shared helper.
2. **DD2** — the `SparkConfig` programmatic surface + precedence between the programmatic knob and the env-var path.
3. **DD3** — the SECRET posture: the token is never logged (the load-bearing constraint).
4. **DD4** — `OTEL_EXPORTER_OTLP_HEADERS` parsing scope + percent-decode + composition with the programmatic knob.
5. **DD5** — the no-token failure mode (warn vs silent).

## Decision

Add an additive, redacted bearer-token field to the `#[non_exhaustive]` `SparkConfig`, resolved along a precedence chain mirroring the established endpoint chain, and attach it as `authorization: Bearer <token>` metadata to the `SpanExporter`, `LogExporter`, and `MetricExporter` **through a single shared helper** so no signal can be left un-authenticated by omission. The token never reaches any loggable surface (structural redaction). When no token is resolved, no metadata is attached and the no-auth path is byte-unchanged.

### DD1 — the metadata mechanism: `.with_metadata(MetadataMap)`, one helper, all three exporters

**Chosen API** (verified against the locked `opentelemetry-otlp =0.27.0` source):

- The builder returned by `.with_tonic()` (`SpanExporterBuilder<TonicExporterBuilderSet>`, and the `Log`/`Metric` siblings) implements `HasTonicConfig` (`span.rs:94`, `logs.rs`, `metric.rs`). The blanket `impl<B: HasTonicConfig> WithTonicConfig for B` (`exporter/tonic/mod.rs:397`) therefore exposes **`fn with_metadata(self, metadata: tonic::metadata::MetadataMap) -> Self`** (`tonic/mod.rs:376,405`) on each of the three builders.
- `with_metadata` **merges** into any existing metadata (`tonic/mod.rs:406-416` extends the header map), so it composes cleanly and is order-independent in the chain.
- The canonical pattern (crate docs `lib.rs:144`; tests `tonic/mod.rs:447-485`): build a `MetadataMap`, insert `authorization` as a `MetadataValue`, pass to `.with_metadata(map)`.

**The chosen call**, applied to each of the three exporter builders:

```text
SpanExporter::builder().with_tonic().with_metadata(auth_metadata).with_endpoint(endpoint).build()
LogExporter::builder().with_tonic().with_metadata(auth_metadata).with_endpoint(endpoint).build()
MetricExporter::builder().with_tonic().with_metadata(auth_metadata).with_endpoint(endpoint).build()
```

`init.rs:45`'s `use` must add `WithTonicConfig` to bring `with_metadata` into scope (alongside the existing `WithExportConfig`).

**One shared helper, uniform application (the load-bearing anti-omission property).** DESIGN locks a single `pub(crate)` free function — call it `build_auth_metadata(&SparkConfig) -> Option<MetadataMap>` — that resolves the token along the DD2 precedence chain, and, **only when a token is resolved**, returns `Some(MetadataMap)` carrying exactly one entry: `authorization = "Bearer <token>"`. `build_pipeline` calls it **once**, then threads the resulting `Option<MetadataMap>` into all three exporter builders via a tiny apply-shim (e.g. `fn apply_auth(builder, &Option<MetadataMap>)` per builder type, or a `match` that calls `.with_metadata(map.clone())` on `Some` and leaves the builder untouched on `None`). The metadata is built **once** and cloned into each builder so the three signals are provably identical. A future fourth signal added to `build_pipeline` that forgets the shim is the failure mode; the enforcement (below) and the all-three integration assertion guard it.

Rust-idiomatic shape (CLAUDE.md): data (`SparkConfig` field) + free functions (`build_auth_metadata`, the per-signal apply-shim) + the existing trait-based builder API. No new trait object, no `dyn`, no inheritance. The `Option<MetadataMap>` is the natural "token or no token" carrier.

**Construction-time failure**: a token whose bytes are not a valid HTTP header value (`MetadataValue::try_from` / `parse` fails) is surfaced as `SparkError::ExporterInitFailed { reason }` (the existing variant the three exporter builds already use, `init.rs:286/318/349`), naming the failure mode **without echoing the token bytes** (DD3). This keeps the error taxonomy unchanged and fail-fast at `init`.

A partial wire (traces authenticated, logs not) is the explicit non-goal (System Constraint 3 / R2); the single helper + single call site makes it structurally hard and the all-three assertion makes it observable.

### DD2 — `SparkConfig` surface + precedence

**Surface (v0):** add **one** builder method, additive on the `#[non_exhaustive]` struct:

```text
#[must_use]
pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self
```

It records the token (redacted field — DD3) and means exactly `authorization: Bearer <token>`. The new private field: `pub(crate) bearer_token: Option<BearerToken>` (the redacting newtype — DD3), defaulted to `None` in `for_service` (`config.rs:50-61`).

**A general `with_auth_header(name, value)` is DEFERRED** (not shipped at v0). Rationale: the v0 need is exactly the `authorization` Bearer header aperture demands (F5); a general header map multiplies the secret-handling surface (every value is potentially sensitive, so every value would need redaction reasoning) for no current story. It is a clean, non-breaking additive widening later (the field stays `Option<BearerToken>`; a future `headers: Vec<(String, RedactedValue)>` is an independent addition on the same `#[non_exhaustive]` struct). `impl Into<String>` matches the established builder ergonomics (`with_endpoint`, `with_tenant_id`).

**Precedence (programmatic wins; env is fallback):** mirror the established endpoint chain (`with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` > default, `init.rs:586-620`). The resolution, centralised in `build_auth_metadata` (or a `resolve_bearer_token(&SparkConfig) -> Option<String>` sibling to `operator_supplied_endpoint`):

1. if `with_bearer_token` was called (the field is `Some`) → use that token;
2. else if `OTEL_EXPORTER_OTLP_HEADERS` carries a non-empty `authorization` entry → use the decoded value (DD4);
3. else → `None` (no auth metadata, DD5).

Deterministic and documented (R4); the programmatic value is the highest-precedence "the application explicitly said so" signal, exactly as `with_endpoint` outranks the endpoint env var. The new field on `#[non_exhaustive]` `SparkConfig` is a non-breaking addition (F1).

### DD3 — the SECRET posture: the token is NEVER logged (load-bearing)

The bearer token is a credential equivalent to the gateway's HS256 secret. It MUST NOT appear in any `target="spark"` event, in a `Debug`/`Display` of `SparkConfig`, in a config-validation/exporter-init error, or in any tracing field (F4 / System Constraint 1 / R1). **Enforced structurally, not by discipline**, mirroring aegis's opaque-key precedent (`validator.rs:149-158`, `key = "<opaque>"`).

**Chosen mechanism — a redacting newtype, NOT a hand-rolled `Debug` on the whole config.** Define a tiny wrapper:

```text
#[derive(Clone)]
pub(crate) struct BearerToken(String);     // the secret value, never exposed in Debug/Display

impl fmt::Debug for BearerToken {
    fn fmt(&self, f) -> fmt::Result { f.write_str("BearerToken(<redacted>)") }
}
// no Display impl, or a Display that also renders "<redacted>"
```

`SparkConfig.bearer_token: Option<BearerToken>` then keeps `#[derive(Debug)]` on `SparkConfig` (no need to hand-write `Debug` for the whole struct — the derived `Debug` recurses into `BearerToken`'s redacting `Debug`, so `dbg!(&config)` / `panic!("{config:?}")` renders `Some(BearerToken(<redacted>))`, never the JWT). The raw token is reached **only** inside `build_auth_metadata` via a `pub(crate)` accessor (e.g. `fn expose(&self) -> &str`) whose single call site is the metadata builder — the value flows config → `MetadataMap` → the wire, and touches no `tracing` macro.

**Why a newtype over a hand-written `SparkConfig::Debug`:** (a) it localises the redaction to one 6-line type, so a future field added to `SparkConfig` cannot accidentally un-redact the token (a hand-written whole-struct `Debug` must be re-audited on every field addition — fragile against `#[non_exhaustive]` growth); (b) the secret-ness travels with the value wherever it is moved/cloned, not just inside `SparkConfig`; (c) it mirrors the aegis opaque-key shape the platform already uses. The derived-`Debug`-recurses-into-redacting-newtype composition is the minimal, audit-stable choice.

`emit_init_succeeded` (`observability.rs:53-70`) is **unchanged** — no new field, no token; the closed vocabulary holds (F4). Exporter-init / config errors name failures by kind, never by token bytes (DD1). The never-log invariant is proven by a test that configures a recognisable token and greps every Spark log/`Debug`/error surface for it (KPI: 0 occurrences; a single occurrence is a defect).

### DD4 — `OTEL_EXPORTER_OTLP_HEADERS` scope: authorization-only, spec-conformant percent-decode

**v0 parses the standard OTLP headers env var but extracts only `authorization`** (general header support deferred — keeps the surface minimal and the only sensitive value the auth one). The parse rule:

- Format: comma-separated `key=value` list per the OTLP spec, e.g. `authorization=Bearer%20<jwt>,x-other=ignored`.
- Split on `,` into entries; for each, split on the first `=` into `(key, value)`; trim surrounding whitespace on the key (the OTel spec permits OWS around list members).
- Match the key **case-insensitively** against `authorization` (HTTP header names are case-insensitive). Take that entry's value; ignore all other entries at v0 (no warn-and-fail on unknown keys — they are simply not v0's concern).
- **Percent-decode** the value per the OTLP spec (`Bearer%20<jwt>` → `Bearer <jwt>`). Use a small dependency-free percent-decode (or the `percent-encoding` crate if already in the lock; verify at DELIVER — prefer reusing an existing workspace dep over adding one).
- **Empty / absent** `OTEL_EXPORTER_OTLP_HEADERS`, or present-but-no-`authorization`-entry, or an `authorization` entry with an empty value → treated as **no credential** (`None`), mirroring the empty-endpoint fall-through (`init.rs:615-619`). This is the documented, predictable behaviour for Example 3 (empty env var).
- **Malformed value failure mode — LOCKED: fail-fast at `init`.** A present `authorization` entry whose percent-decoding fails (an invalid `%` escape) surfaces as `SparkError::ExporterInitFailed { reason }` at metadata-build time, with the reason naming "malformed OTEL_EXPORTER_OTLP_HEADERS authorization value" and **never echoing the (corrupt) bytes** (DD3). It does NOT reuse `InvalidEndpoint` (wrong variant) and it is NOT decoded leniently / silently dropped. Rationale: an operator who set `authorization=` clearly intends a credential, so a corrupt one must surface rather than ship a silent no-auth export (consistent with the DD4 recommendation "a malformed header the operator intended as auth should surface, not silently drop"). A *byte-valid but semantically-wrong* token (e.g. not a JWT) is NOT Spark's concern — Spark sends it verbatim; the gateway rejects (DD5 / US-SP-AUTH-01 Example 3).

Composition with the programmatic knob: the env path is consulted **only** when `with_bearer_token` was not called (DD2 step 2). The same `BearerToken` newtype wraps the env-derived value, so the env path inherits the redaction discipline for free.

### DD5 — no-token failure mode: silent-but-documented

**When no token is resolved (no knob, no env var), Spark attaches NO authorization metadata** — `build_auth_metadata` returns `None`, the apply-shim leaves all three builders untouched, and the exporters are built byte-identically to today (System Constraint 4 / R3). The no-auth path against an unauthenticated collector keeps working unchanged; `slice_01..slice_07` stay green.

**Decision: silent (no warn), documented on the knob.** Rationale: not every endpoint requires auth (the local-collector workflow is legitimate and explicitly supported); a warn-on-remote-without-token would be a false alarm for that case and, worse, a warn tempts echoing context that could leak the (absent or future) token. A false-positive security warning erodes trust. The `with_bearer_token` doc comment explains the no-token-no-header behaviour and that exporting to an authenticated gateway without a token yields gateway-side `missing_claim` denials. **If a future feature chooses to warn, the warn MUST never echo a token value and MUST be suppressible** (DD3 binds here too). Spark's job is to SEND the token it was given; a gateway rejection (expired/invalid/missing) is the gateway's surfacing (ADR-0068), not Spark's.

## Reuse Analysis (MANDATORY)

| Capability | Verdict | Where / How |
|---|---|---|
| Tonic exporter metadata attachment | **REUSE verbatim** | `WithTonicConfig::with_metadata(MetadataMap)` on each `.with_tonic()` builder (`opentelemetry-otlp =0.27` `tonic/mod.rs:376,405`; merges existing headers). No new dependency — `opentelemetry_otlp` + transitive `tonic` already provide `MetadataMap`/`MetadataValue` (F5, ADR-0013). |
| Endpoint-style precedence chain | **REUSE pattern** | The `with_endpoint` > env > default chain (`resolve_endpoint`/`operator_supplied_endpoint`, `init.rs:586-620`); `resolve_bearer_token` mirrors `operator_supplied_endpoint` exactly (programmatic field > env > None). |
| Opaque-Debug redaction of a secret | **REUSE pattern** | aegis `Validator`'s hand-written `Debug` prints `key = "<opaque>"` (`validator.rs:149-158`); `BearerToken`'s `Debug` prints `<redacted>` on the same principle. |
| Empty-env-var-as-absent fall-through | **REUSE pattern** | empty `OTEL_EXPORTER_OTLP_ENDPOINT` → `None` (`init.rs:615-619`); empty/absent `OTEL_EXPORTER_OTLP_HEADERS` → `None`. |
| Exporter-init error variant | **REUSE verbatim** | `SparkError::ExporterInitFailed { reason, source }` (`init.rs:286,318,349`) for a malformed token/header value, reason names the failure not the bytes. |
| `#[non_exhaustive]` additive evolution | **REUSE guarantee** | ADR-0011 / `config.rs:25` — the new `bearer_token` field + `with_bearer_token` method are non-breaking additions. |
| `SparkConfig` builder | **EXTEND** | add the `bearer_token: Option<BearerToken>` field (defaulted `None` in `for_service`) + the `with_bearer_token` builder method (`config.rs`). |
| `build_pipeline` exporter-build path | **EXTEND** | three exporter builds each gain `.with_metadata(map.clone())` via the apply-shim, gated on `build_auth_metadata(&config)` (`init.rs:282-352`); pipeline structure, batch processors, providers, flush, single-init invariant ALL unchanged (System Constraint 5). |
| `init.rs` imports | **EXTEND** | add `WithTonicConfig` to the `opentelemetry_otlp` `use` (`init.rs:45`) to bring `with_metadata` into scope. |
| `BearerToken` redacting newtype | **CREATE** | a ~10-line wrapper with a redacting `Debug`, no `Display`-of-value, one `pub(crate)` accessor. Justified: no existing spark type carries a redacted secret (F3: zero `bearer`/`authorization` matches); aegis's opaque key lives in another crate and wraps a `DecodingKey`, not a `String` token. |
| `build_auth_metadata` + apply-shim | **CREATE** | the single helper that resolves the token (DD2) and builds the one-entry `MetadataMap` (DD1), + the per-signal apply-shim. Justified: no existing code attaches metadata (F2: zero `with_metadata`/`MetadataMap` matches); this is the genuinely new auth-attachment boundary, deliberately single-sited to defend the all-three property. |
| `OTEL_EXPORTER_OTLP_HEADERS` parser | **CREATE** | a small free fn parsing the comma-separated list, extracting case-insensitive `authorization`, percent-decoding the value (DD4). Justified: Spark reads only `OTEL_EXPORTER_OTLP_ENDPOINT` today (F3); no header-parsing code exists. |

**Net**: REUSE the tonic `with_metadata` surface, the endpoint-precedence pattern, the aegis opaque-Debug principle, and the `#[non_exhaustive]` guarantee. EXTEND `SparkConfig` (one field + one method) and `build_pipeline` (three `.with_metadata` calls via one helper). CREATE only the thin redacting newtype, the single metadata helper + apply-shim, and the env-headers parser. No new crate, no new dependency, no pipeline restructuring.

## Security posture (load-bearing)

- **Secret never logged (Info-Disclosure / R1)** — enforced structurally: the token lives inside `BearerToken` whose `Debug` renders `<redacted>` and which has no value-`Display`; `SparkConfig`'s derived `Debug` recurses into it; the raw value is reached only via a single `pub(crate)` accessor whose only caller is `build_auth_metadata`, which writes it into a `MetadataMap` (the wire), never into a `tracing` macro. `emit_init_succeeded`'s closed vocabulary is untouched. Errors name failures by kind, not bytes. There is no loggable surface that holds the token.
- **Fail-closed-at-the-gateway, send-honestly-at-the-SDK** — Spark's contract is correct transmission; the fail-closed enforcement lives on the gateway (ADR-0068). A no-token export to an authenticated gateway is denied `missing_claim` (the gateway's job); a no-token export to an unauthenticated collector still works (System Constraint 4). Spark never silently invents or omits a configured token.
- **Uniform mediation across signals (R2)** — the single helper + single resolution + `MetadataMap` cloned into all three builders makes "traces, logs, AND metrics all carry the same `authorization`" structural; a partial wire is the explicit non-goal.
- **No new attack surface** — Spark gains no network behaviour beyond an extra gRPC metadata entry on exports it already makes; no new listener, no new task, no new dependency.
- **Performance — negligible** — the per-export cost is one additional gRPC `authorization` metadata header (built once at `init`, cloned into three builders), not a per-request computation. No perf KPI is load-bearing for this DESIGN (consistent with ADR-0068's stance on the gateway side).
- **STRIDE residual** — token replay within `exp` and HS256 secret rotation are gateway/aegis-level accepted risks (ADR-0068); Spark stores the token only in memory for the process lifetime (never persisted — System Constraint 2). The integrator supplies the token; Spark never bakes, generates, or persists it.

## Test seam (for DISTILL)

The auth attachment is driven **end-to-end through the real authenticated aperture** (the E01-E04 shape), plus a unit assertion on the helper and a redaction grep:

- **Mint test tokens in-suite** (reuse the ADR-0068 / aegis token-minting seam, F5): an HS256 JWT signed with the secret the test aperture's `secret_file` points at, for a tenant in the test catalogue, with matching `iss`/`aud` and a future `exp`. Negative-control variant: no token configured.
- **E2E accept (US-SP-AUTH-01 AC `a-bearer-configured-export-is-accepted-by-the-authenticated-gateway`)**: spawn a real aegis-authenticated aperture (recording sink); configure Spark with `with_bearer_token(<valid jwt>)` + that endpoint; export → assert the gateway ACCEPTS (`decision=allow`) and the sink record is tagged with the token's `tenant_id`. The **same export with no token** → DENIED `missing_claim`, sink empty. This MUST fail against today's no-knob code (no way to set a token).
- **All-three-signals (US-SP-AUTH-01 AC `the-token-reaches-all-three-signals`)**: because a full three-signal round-trip is integration-heavy, the all-three property is pinned by **two complementary checks**: (a) a **unit assertion on `build_auth_metadata`** — given a configured token, the returned `MetadataMap` carries exactly `authorization: Bearer <token>`, and the apply-shim is exercised for each of the three builder types (a unit test that the shim attaches to span/log/metric builders identically); plus (b) **at least one signal proven end-to-end** through the authenticated aperture (the accept test above), with the integration suite extended to assert accept on traces AND logs (the verifier's E01-E04 cover both) and, where the metric path is exercisable, metrics.
- **Env-var path (US-SP-AUTH-02 AC `OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer`)**: mirror `slice_04_env_var_precedence.rs` (`serial_test`, clean-env helper, recording-sink aperture). Set `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<jwt>`, no programmatic knob → accepted, percent-decoded. Precedence test: both set → programmatic wins (assert which token is on the wire / accepted). Empty env var → no header, unauthenticated collector still accepts.
- **Never-log (US-SP-AUTH-03 AC `the-token-is-never-logged`)**: configure a recognisable token (e.g. `eyJTESTtoken...`); capture every Spark log surface (the `target="spark"` capture seam used by the existing slice tests), `Debug`/`{:?}` of the `SparkConfig`, and any error path; assert the token substring appears **zero** times; assert the redacted placeholder appears where the field renders.
- **Non-regression (US-SP-AUTH-03 AC `no-token-no-header-...`)**: no token → no metadata attached → `slice_01..slice_07` stay green; the no-token exporter-build path is byte-unchanged.

## Public-API / semver posture (REAL DIFFERENCE — flag for DELIVER)

**`spark` IS in the Gate 2/3 public-API set** (verified `ci.yml:334,347` Gate 2 `cargo public-api -p spark`; `ci.yml:426` Gate 3 `cargo semver-checks --package spark`). This is **unlike** the recent cinder/aperture features (which are not public-API-tracked). This feature adds a **public method** (`SparkConfig::with_bearer_token`) to `spark`'s public surface — `BearerToken` and the field stay `pub(crate)`/private, so they do not enter the public API; the new builder method does.

Consequence, **MANDATORY in DELIVER**:
- **Gate 2 (`cargo public-api`)** WILL diff (one new public method). This is the intended additive change; the **public-api baseline must be regenerated/accepted** as part of the DELIVER commit.
- **Gate 3 (`cargo semver-checks`)** classifies a new public method on a `#[non_exhaustive]` struct as a **minor** (additive, non-breaking) change. DELIVER must **bump `spark`'s minor version** (e.g. `0.x.0` → `0.(x+1).0`), pre-1.0. **NEVER 1.0.0** (Andrea's call; CLAUDE.md / MEMORY).

No other crate's public surface changes. No new dependency, so `cargo deny` (Gate 4) is unaffected.

## Alternatives Considered

### Option A — tonic interceptor (`with_interceptor`) instead of `with_metadata`

Attach auth via `WithTonicConfig::with_interceptor` (`tonic/mod.rs:392`, documented "to inject auth tokens"), a per-request closure that mutates outbound metadata.

**Pros**: re-evaluates the token per request (would matter for rotating in-process credentials); the upstream docs explicitly suggest it for auth.
**Cons**: heavier than the need — the v0 token is set once at `init` (rotation is deployment-managed via env/restart, DD5); an interceptor is a `Clone + Send + Sync + 'static` closure that is harder to unit-assert ("does the metadata carry `authorization`?") than a `MetadataMap` the helper returns directly; it captures the secret in a closure (a second redaction-reasoning surface) rather than the single newtype-guarded value; and the three signals would each need the interceptor wired, with no single returnable artefact to assert on. **Rejected**: `with_metadata` gives a directly-assertable `MetadataMap`, one redaction surface, and matches the "set once at init" lifetime. The interceptor is the right tool for per-request dynamic auth, which is not a v0 need; it remains a clean future migration if rotating in-process tokens ever land.

### Option B — a general `with_auth_header(name, value)` (and/or full header map) at v0

Ship a general header-setting surface instead of (or alongside) the bearer convenience.

**Pros**: future-proof; one method covers any header.
**Cons**: every value becomes potentially sensitive, so the redaction reasoning must cover an open-ended map rather than one known-secret field; no v0 story needs a non-`authorization` header (F5); it enlarges the public API (a bigger Gate 2 diff) for unused capability. **Rejected**: ship the minimal `with_bearer_token` (the exact need); a general header map is a clean non-breaking widening later on the same `#[non_exhaustive]` struct. YAGNI + minimal-secret-surface.

### Option C — warn when exporting to a remote endpoint with no token (DD5 sub-alternative)

Emit a `tracing::warn!` when Spark is about to export to a non-loopback endpoint with no token configured.

**Pros**: nudges an operator who forgot the credential.
**Cons**: false alarm for the legitimate unauthenticated-collector case; a security warn tempts echoing context that risks the never-log invariant; a noisy false-positive security warning erodes trust. **Rejected**: silent-but-documented (DD5). The gateway already surfaces `missing_claim` legibly (ADR-0068); that is the right place for the "you forgot the token" signal, not the SDK.

### Option D — attach the header unconditionally (always build a `MetadataMap`)

Always call `.with_metadata`, with an empty `authorization` when no token.

**Cons**: an empty/garbage `authorization` against an unauthenticated collector could change its behaviour (some collectors reject malformed auth); it breaks the byte-unchanged no-auth path (System Constraint 4 / R3); it serves no purpose. **Rejected**: conditional attachment (`Option<MetadataMap>`, `None` ⇒ no `.with_metadata` call) preserves the no-auth path exactly.

## Consequences

### Positive
- The Spark SDK can ship authenticated telemetry through the fail-closed aperture; E01-E04 (the Spark→Aperture round-trip, traces AND logs) flip back GREEN.
- The token reaches all three signals by construction (one helper, one resolution, cloned into all three builders) — no partial-auth omission.
- The credential never touches a loggable surface — structural redaction via the `BearerToken` newtype, audit-stable against future `#[non_exhaustive]` field growth.
- No new crate, no new dependency, no pipeline restructuring; the change is one field + one method + three `.with_metadata` calls via one helper.
- The conventional `OTEL_EXPORTER_OTLP_HEADERS` path works code-free, with deterministic precedence over the programmatic knob.
- The no-auth path is byte-unchanged; the local-collector workflow is preserved.

### Negative
- **`spark`'s public API grows** — Gate 2 baseline must be regenerated and `spark`'s minor version bumped in DELIVER (the real difference from cinder/aperture; flagged above). This is the intended additive evolution, not a regression.
- A malformed env-var header value fails `init` fast (chosen over silent-drop, DD4) — an operator with a corrupt `OTEL_EXPORTER_OTLP_HEADERS` sees an init error (naming the failure, not the bytes) rather than a silent no-auth export; this is the safer surfacing for an operator who clearly intended a credential.
- The integration suite gains an authenticated-aperture spawn dependency for the accept test (reusing the ADR-0068 token-minting seam); the unit helper assertion bounds the integration weight.

### Trade-off ATAM
- **Sensitivity point — Security/Authenticity**: the SDK gains the ability to present a verifiable credential; tenant telemetry transitions from "denied at the door" to "accepted under an authenticated tenant".
- **Sensitivity point — Security/Confidentiality (the token)**: the redacting newtype + single-accessor + closed-vocabulary keep the credential off every loggable surface — the same posture aegis/aperture hold for the HS256 secret.
- **Trade-off point — Maintainability vs minimal surface**: shipping only `with_bearer_token` (not a general header map) keeps the secret-handling surface minimal and the public-API diff small, at the cost of a later additive widening if general headers are ever needed — a deliberate, non-breaking-deferrable trade.

## Enforcement
- The behaviour is covered by integration tests (the authenticated-aperture accept/deny round-trip on traces+logs+metrics), a unit assertion on `build_auth_metadata` (the `MetadataMap` carries `authorization: Bearer <token>` and the apply-shim hits all three builder types), the never-log grep test, and the no-token non-regression — supplying the per-feature 100% mutation kill coverage (Gate 5, `gate-5-mutants-spark`, CLAUDE.md / ADR-0005) on the new resolution/attachment/redaction branches (modified spark files only).
- **Gate 2/3 (the structural enforcement of the public-API/semver contract)**: `cargo public-api -p spark` pins the new method into the regenerated baseline; `cargo semver-checks --package spark` confirms the change is minor (additive) — the additive `#[non_exhaustive]` field guarantee is machine-checked, not asserted by review.
- The single-helper-single-call-site discipline for the all-three property is reinforced by the all-three unit assertion (a fourth signal added without the shim would not satisfy the "metadata on every exporter builder" assertion). No new architectural-style rule is introduced; the redaction is structural (the newtype), not a lint.

## Amendment (DISTILL back-propagation) — 2026-06-06

> **Status of this section**: APPENDED, not a rewrite. The original Decision above is **immutable** and stays on the record as the as-accepted text. This amendment **revises DD4 and DD2** and **drops the spark-owned malformed-header AC** in light of a DISTILL finding that contradicts an assumption the original made. Where this amendment and the original disagree, **this amendment governs DELIVER**. Author: `nw-solution-architect` (Morgan), autonomous DISTILL back-propagation. Trigger: `spark-ingest-auth-v0` DISTILL (Scholar) finding + Bea Verifier msg 038.

### The contradicted assumption

The original DD4 (and the Reuse table's "`OTEL_EXPORTER_OTLP_HEADERS` parser — CREATE") assumed **Spark must parse `OTEL_EXPORTER_OTLP_HEADERS` itself** (spark-owned list parse, case-insensitive `authorization` extraction, percent-decode, fail-fast on malformed). DISTILL (Scholar) found that `opentelemetry-otlp =0.27` **already** honours `OTEL_EXPORTER_OTLP_HEADERS` natively, with percent-decode, on the exact construction path Spark uses — making a spark-owned parser redundant and risking a double-attach.

### Ground truth (read from the locked source on 2026-06-06, not inferred)

Spark's construction path **does honour `OTEL_EXPORTER_OTLP_HEADERS` today, unconditionally**. Traced end-to-end through `opentelemetry-otlp-0.27.0`:

- `SpanExporter::builder().with_tonic().with_endpoint(endpoint).build()` (`crates/spark/src/init.rs:282-289`) → `span.rs:66 build_span_exporter()` → `tonic/mod.rs:300 self.build_channel(...)`.
- `build_channel` calls **`parse_headers_from_env(signal_headers_var)` unconditionally** at `tonic/mod.rs:156`, *before and independent of* whether `.with_metadata`/`.with_interceptor`/`.with_channel` was ever called. There is no `from_env`-gate, no "only if metadata absent" guard.
- `parse_headers_from_env` (`tonic/mod.rs:327-341`) reads `OTEL_EXPORTER_OTLP_TRACES_HEADERS` else falls back to `OTEL_EXPORTER_OTLP_HEADERS`, then `parse_header_string` (`exporter/mod.rs:225`) → `parse_header_key_value_string` (`mod.rs:263`) → **`url_decode`** (`mod.rs:233-260`) percent-decodes the value (`Bearer%20<jwt>` → `Bearer <jwt>`).
- **Identical for logs** (`logs.rs:66 build_log_exporter` → `build_channel`, `OTEL_EXPORTER_OTLP_LOGS_HEADERS`) **and metrics** (`metric.rs:80 build_metrics_exporter` → `build_channel`, `OTEL_EXPORTER_OTLP_METRICS_HEADERS`). All three of Spark's exporters trigger the env read.

**Conclusion: a spark-owned `OTEL_EXPORTER_OTLP_HEADERS` parser is redundant. The env-honouring half of this feature already works code-free.** Scholar is correct.

### Double-attach finding (decides DD2 precedence)

If Spark were to call `.with_metadata(authorization=PROG)` AND the env carried `OTEL_EXPORTER_OTLP_HEADERS=authorization=ENV`, **both reach `build_channel` and the env value WINS** — the opposite of the original DD2's "programmatic wins":

- `build_channel` merges via `merge_metadata_with_headers_from_env(self.tonic_config.metadata, headers_from_env)` (`tonic/mod.rs:157-160`), whose body is `existing_headers.extend(headers_from_env)` (`tonic/mod.rs:320-321`).
- `http::HeaderMap::extend(other_HeaderMap)` **overwrites** (replaces) a key already present — it is *not* `append`. So the env `authorization` **clobbers** the programmatically-attached `authorization`.

Therefore the precedence must NOT be implemented by attaching both and hoping for an order — attaching both gives env-wins, silently, which is wrong and a footgun.

### Reconciling Bea Verifier msg 038 (black-box: no bearer arrived even via env)

The env read is real and on Spark's path, so the observation is most plausibly **environmental, not a code gap**:

1. **Process-env / timing**: the env var was set in the harness/parent but not inherited by the process that actually ran `spark::init`, or was set *after* `.build()` (the read is at exporter-**build** time, not per-export). This is the leading hypothesis.
2. **Silent upstream drop of a malformed value**: a present-but-invalid header value is **silently dropped** upstream — `HeaderValue::from_str(&value).ok()?` (`tonic/mod.rs:335`) returns `None` and the entry vanishes with no error. So a mis-percent-encoded or non-ASCII bearer would produce exactly "no bearer arrived" with no signal. (Note: this is the *opposite* of the original DD4's spark-owned "fail-fast on malformed" — upstream is lenient/silent.)

**Disambiguating test handed to DELIVER** (so this is resolved empirically, not assumed): in a single process, set `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<valid-jwt>` **before** `spark::init`, export to the real aegis-authenticated aperture, assert ACCEPT. If it accepts, msg 038 was environmental (hypothesis 1); the code needs no env work at all. This is the empirical probe (principle 12) for the env dependency.

### DD4 — REVISED: DROP the spark-owned env parser entirely

The original DD4's spark-owned parse / case-insensitive extraction / percent-decode is **dropped**. Upstream does all of it, on Spark's exact path, percent-decode included. The feature **shrinks** to:

- the **programmatic** `with_bearer_token` knob (DD1/DD2/DD3 — genuinely needed, see below), and
- **relying on upstream** for `OTEL_EXPORTER_OTLP_HEADERS` honouring.

Reuse-table row "`OTEL_EXPORTER_OTLP_HEADERS` parser — CREATE" is **withdrawn**; there is nothing to create there. The percent-decode dependency open-question (the original's DELIVER hand-off) is **moot** — no spark decode is written.

### DD2 — REVISED: precedence WITHOUT double-attach

`with_bearer_token` (programmatic) must take precedence over the env path **without** the env value clobbering it. Because upstream overwrites on key collision (above), the mechanism is **mutual exclusion at the source, not post-hoc merge**:

- **`resolve_bearer_token(&SparkConfig)` chooses ONE source**: if `with_bearer_token` was called → attach `.with_metadata(authorization=<knob>)` AND, to defend against the env clobber, the design's precedence claim now requires that **the env `authorization` not also be read into the same builder**. Since Spark cannot stop upstream from reading the env, **"programmatic wins" is honoured by documentation + a precedence integration test, with the explicit caveat that the upstream env read is the final writer**. Two clean DELIVER options, decided here in priority order:
  1. **PREFERRED — programmatic-only attach + documented env behaviour**: when the knob is set, Spark attaches it via `.with_metadata`; the doc comment on `with_bearer_token` states plainly that **a concurrently-set `OTEL_EXPORTER_OTLP_HEADERS=authorization=...` will be honoured by the upstream exporter and, on key collision, takes final effect** (env-as-override). This re-frames precedence honestly to match the locked upstream behaviour rather than asserting a "programmatic wins" that the library would silently violate. It writes **zero** env-handling code. The precedence integration test asserts *this* documented behaviour (both set ⇒ env value on the wire) — i.e. the test is revised to match reality, see "Impact on DISTILL".
  2. **FALLBACK (only if a product decision insists programmatic must out-rank env)**: Spark must *prevent* the upstream env read from colliding — the only in-process lever is to **read and clear / not propagate** the env for the child exporter construction, which is invasive, racy under `serial_test`, and reintroduces env parsing Spark just removed. **Rejected for v0** as contradicting the shrink; recorded only for completeness.

**Decision (autonomous): adopt option 1.** It is the minimal, honest, code-free precedence: the programmatic knob is the *primary* way to set the bearer (the verifier's core gap), and a concurrently-set env header is an operator override that upstream applies last. The original DD2 step-ordering ("programmatic > env > none") is **superseded** by "programmatic knob is the supported API; env is the conventional OTLP override honoured by upstream, final on collision; none ⇒ no auth (DD5 unchanged)."

### Malformed-header AC — RESOLVED: DROP it from spark scope

The original DD4 locked a spark-owned "malformed `OTEL_EXPORTER_OTLP_HEADERS` authorization value ⇒ `SparkError::ExporterInitFailed` fail-fast" behaviour. **Dropped**, because:

- Env parsing is **upstream's** concern now; upstream's actual behaviour for a malformed env header is **silent drop** (`.ok()?`, `tonic/mod.rs:335`), not fail-fast. Spark cannot impose fail-fast without re-adding the parser it just removed.
- The **programmatic** token is a plain `String` passed to `MetadataValue::try_from`; there is **no percent-decode and no list-parse** on that path, so there is no "malformed header" case for the knob — only the existing "bytes are not a valid HTTP header value" case, which **stays** as `SparkError::ExporterInitFailed { reason }` at metadata-build time (DD1, unchanged; reason names the kind, never the bytes — DD3 still binds).

So: **no spark-owned malformed-env-header AC**. The construction-time invalid-header-value guard for the *programmatic* token is retained verbatim from DD1.

### What stands UNCHANGED (the genuinely spark-owned core)

The programmatic knob is **the load-bearing reason this feature exists**: `opentelemetry-otlp =0.27` has **no programmatic bearer/auth method** (it has `with_metadata`, `with_interceptor`, and the env path — but no "set a bearer token" API), so an integrator who wants to set the credential *in code* (Marco's `payments-api`, not via process env) has no way today. That gap is real and this feature closes it. Unchanged by this amendment:

- **DD1** — `.with_metadata(MetadataMap)` via `WithTonicConfig`, one `build_auth_metadata` helper, cloned into all three exporter builders, the all-three anti-omission property, the construction-time invalid-header-value guard. **Stands.**
- **DD2 surface** — one additive `with_bearer_token(impl Into<String>)` on `#[non_exhaustive]` `SparkConfig`; general `with_auth_header` still deferred. **Stands** (only the *precedence framing* is revised above).
- **DD3** — `BearerToken` redacting newtype, never-logged, single accessor. **Stands.**
- **DD5** — no-token ⇒ no metadata ⇒ byte-unchanged no-auth path, silent-but-documented. **Stands.**
- **Public-API / semver** — `with_bearer_token` is still a new public method; Gate 2 baseline regen + Gate 3 minor bump (`0.1.0 → 0.2.0`, pre-1.0, **never 1.0.0**). **Stands** (the env-parser drop does not remove the public method).

### Net effect on the Reuse / CREATE ledger

- **WITHDRAWN — `OTEL_EXPORTER_OTLP_HEADERS` parser (CREATE)**: redundant; upstream handles it.
- **REVISED — `resolve_bearer_token`**: collapses from a 3-step precedence resolver to a simple "is the knob set? → its token; else None" (the env branch is gone — upstream owns env). It no longer mirrors the full endpoint chain; it is a one-source read.
- **UNCHANGED** — `BearerToken` newtype (CREATE), `build_auth_metadata` + apply-shim (CREATE), `SparkConfig` field+method (EXTEND), the three `.with_metadata` calls (EXTEND), the `WithTonicConfig` import (EXTEND).

### DELIVER scope after this amendment (build ONLY this)

1. `BearerToken` redacting newtype (DD3).
2. `SparkConfig::with_bearer_token(impl Into<String>)` + private `bearer_token: Option<BearerToken>` field, defaulted `None` (DD2 surface).
3. `build_auth_metadata(&SparkConfig) -> Option<MetadataMap>` (knob-only resolution) + per-signal apply-shim; three `.with_metadata(map.clone())` calls in `build_pipeline`; `WithTonicConfig` added to the `init.rs:45` `use` (DD1).
4. No-double-attach precedence **as documented** (option 1): knob attaches; doc states env is honoured by upstream and final on collision (DD2 revised).
5. Never-log redaction grep + no-token byte-unchanged non-regression (DD3/DD5).
6. **Do NOT build**: any `OTEL_EXPORTER_OTLP_HEADERS` parser, any percent-decode, any spark-owned malformed-env-header fail-fast.
7. Gate 2 baseline regen + Gate 3 minor bump (`0.1.0 → 0.2.0`).
8. **DELIVER disambiguation probe** (resolves msg 038 empirically): single-process env-before-init test asserting the upstream env path accepts at the real aperture.

### Earned-Trust note (principle 12)

The original DD4 took it on faith that Spark must parse the env. Reading the locked dependency falsified that. The env dependency now has an explicit **probe** handed to DELIVER (the env-before-init acceptance test) so the "upstream honours `OTEL_EXPORTER_OTLP_HEADERS` on our path" claim is demonstrated against the real aperture, not assumed — and the double-attach/silent-drop substrate lies (`extend`-overwrites, `.ok()?`-drops) are now documented as known behaviours the design accounts for rather than is surprised by.
