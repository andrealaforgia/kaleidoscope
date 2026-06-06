# DESIGN Decisions — spark-ingest-auth-v0

> **Wave**: DESIGN (nWave). **Architect**: Morgan (`nw-solution-architect`).
> **Date**: 2026-06-06. **Mode**: PROPOSE (autonomous). **Paradigm**:
> Rust idiomatic (data + free functions + traits where polymorphism is
> genuinely needed; no inheritance, no `dyn` where monomorphisation
> suffices). **ADR**: `docs/product/architecture/adr-0069-spark-ingest-auth.md`.
> **Sibling**: ADR-0068 (`aegis-ingest-auth-v0`) — the gateway that
> mandates the bearer; this is its client-side key.

Grounded by reading the source on 2026-06-06 (confirms DISCUSS F1-F5)
and the locked `opentelemetry-otlp =0.27.0` tonic exporter API.

## The five decisions — RESOLVED

### DD1 — metadata mechanism: `.with_metadata(MetadataMap)`, one helper, all three exporters

**Chosen API** (verified, not guessed): the `.with_tonic()` builder for
each signal implements `HasTonicConfig`, so the blanket
`impl<B: HasTonicConfig> WithTonicConfig for B`
(`opentelemetry-otlp-0.27.0/src/exporter/tonic/mod.rs:397`) gives
**`with_metadata(self, MetadataMap) -> Self`** (`tonic/mod.rs:376,405`)
on all three. It **merges** existing headers (`tonic/mod.rs:406-416`).
The chain: `SpanExporter::builder().with_tonic().with_metadata(map)
.with_endpoint(endpoint).build()` (and `Log`/`Metric` siblings).
`init.rs:45` must add `WithTonicConfig` to the `use` (currently only
`WithExportConfig`). Types: `tonic::metadata::{MetadataMap,
MetadataValue}` (already in the lock via `opentelemetry_otlp` →
`tonic`; no new dependency).

**Uniform application — the anti-omission property.** One `pub(crate)`
free fn `build_auth_metadata(&SparkConfig) -> Option<MetadataMap>`
resolves the token (DD2 precedence) and, only when present, returns
`Some` carrying exactly `authorization = "Bearer <token>"`. Built
**once** in `build_pipeline`; cloned into all three exporter builders
via a per-signal apply-shim (`Some` ⇒ `.with_metadata(map.clone())`;
`None` ⇒ builder untouched). A partial wire is the explicit non-goal
(R2); single helper + single call site makes it structural, the
all-three assertion makes it observable. A token whose bytes are not a
valid header value → `SparkError::ExporterInitFailed { reason }` (the
existing exporter-build error variant), reason names the failure, never
the bytes.

### DD2 — surface + precedence

**Surface (v0):** ONE additive builder method on the `#[non_exhaustive]`
struct:
`#[must_use] pub fn with_bearer_token(self, token: impl Into<String>) -> Self`,
backed by a private `bearer_token: Option<BearerToken>` field
(defaulted `None` in `for_service`). **A general
`with_auth_header(name, value)` is DEFERRED** — v0 need is exactly the
`authorization` Bearer header (F5); a general map multiplies the
secret-handling surface for no current story; clean non-breaking
widening later on the same `#[non_exhaustive]` struct.

**Precedence (programmatic wins; env is fallback)** — mirrors the
endpoint chain (`with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` >
default, `init.rs:586-620`). Resolution (a `resolve_bearer_token`
sibling to `operator_supplied_endpoint`): (1) `with_bearer_token` value
if set; else (2) the `authorization` entry from
`OTEL_EXPORTER_OTLP_HEADERS` if non-empty (DD4); else (3) `None`.
Deterministic, documented (R4). The new field is non-breaking (F1).

### DD3 — SECRET posture: never logged (load-bearing) — a redacting newtype

**Chosen mechanism — a `BearerToken(String)` newtype** with a
hand-written `Debug` rendering `BearerToken(<redacted>)` and no
value-`Display`; the raw value reached only via one `pub(crate)`
accessor whose single caller is `build_auth_metadata`.
`SparkConfig.bearer_token: Option<BearerToken>` keeps `#[derive(Debug)]`
on `SparkConfig` — the derived `Debug` recurses into the newtype's
redacting `Debug`, so `dbg!`/`panic!("{config:?}")` shows
`Some(BearerToken(<redacted>))`, never the JWT.

**Why a newtype over a hand-rolled whole-struct `Debug`:** localises
redaction to one ~10-line type (a future `SparkConfig` field cannot
accidentally un-redact the token — a hand-written struct `Debug` must
be re-audited on every `#[non_exhaustive]` field addition); the
secret-ness travels with the value through every move/clone; mirrors
aegis's opaque-key shape (`validator.rs:149-158`, `key = "<opaque>"`).
`emit_init_succeeded` (`observability.rs:53-70`) is UNCHANGED — closed
vocabulary holds (F4). Errors name failures by kind, never by bytes.

### DD4 — `OTEL_EXPORTER_OTLP_HEADERS`: authorization-only, percent-decoded

v0 parses the standard comma-separated `key=value` list and extracts
ONLY `authorization` (general headers deferred). Rules: split on `,`,
split each on first `=`, trim OWS, match key **case-insensitively**
against `authorization`, take that value, ignore other entries.
**Percent-decode** the value (`Bearer%20<jwt>` → `Bearer <jwt>`; reuse
an existing workspace percent-decode dep if present, else a tiny
dependency-free decode — verify at DELIVER). Empty/absent env var, or
no `authorization` entry, or an empty value → **no credential**
(`None`), mirroring the empty-endpoint fall-through (`init.rs:615-619`).
**Malformed-value failure mode: fail-fast** — a present `authorization`
entry whose percent-decoding fails surfaces as
`SparkError::ExporterInitFailed { reason }` (reason: "malformed
OTEL_EXPORTER_OTLP_HEADERS authorization value", never the bytes),
because an operator who set `authorization=` clearly intended a
credential. A byte-valid-but-semantically-wrong token is NOT Spark's
concern (it sends it; the gateway rejects — DD5). The env value is
wrapped in the same `BearerToken` newtype (inherits redaction).

### DD5 — no-token failure mode: silent-but-documented

No token resolved ⇒ `build_auth_metadata` returns `None` ⇒ no
`.with_metadata` call ⇒ exporters byte-identical to today (System
Constraint 4 / R3); `slice_01..slice_07` stay green. **Chosen:
silent**, documented on the `with_bearer_token` knob (no warn).
Rationale: the unauthenticated-collector workflow is legitimate; a
remote-without-token warn is a false alarm for it and tempts echoing
context that risks the never-log invariant; a false-positive security
warn erodes trust. The gateway already surfaces `missing_claim` legibly
(ADR-0068) — the right place for the "you forgot the token" signal. If
a future feature warns, the warn MUST never echo a token and MUST be
suppressible.

## Reuse Analysis

| Capability | Verdict | Where / How |
|---|---|---|
| Tonic `with_metadata(MetadataMap)` | **REUSE verbatim** | `opentelemetry-otlp =0.27` `tonic/mod.rs:376,405`; no new dep (`tonic` already transitive). |
| Endpoint-style precedence chain | **REUSE pattern** | `operator_supplied_endpoint` (`init.rs:611`) → `resolve_bearer_token`. |
| Opaque-Debug secret redaction | **REUSE pattern** | aegis `Validator` `Debug` `key="<opaque>"` (`validator.rs:149-158`) → `BearerToken` `<redacted>`. |
| Empty-env-as-absent | **REUSE pattern** | empty endpoint env → `None` (`init.rs:615-619`). |
| `ExporterInitFailed` variant | **REUSE verbatim** | `init.rs:286,318,349` for malformed token/header. |
| `#[non_exhaustive]` additive | **REUSE guarantee** | ADR-0011 / `config.rs:25`. |
| `SparkConfig` builder | **EXTEND** | + `bearer_token` field + `with_bearer_token` method (`config.rs`). |
| `build_pipeline` exporter builds | **EXTEND** | three `.with_metadata(map.clone())` via apply-shim, gated on `build_auth_metadata` (`init.rs:282-352`); structure unchanged (Constraint 5). |
| `init.rs` imports | **EXTEND** | add `WithTonicConfig` (`init.rs:45`). |
| `BearerToken` newtype | **CREATE** | ~10-line redacting wrapper; justified (no spark redacted-secret type; aegis's is a `DecodingKey` in another crate). |
| `build_auth_metadata` + apply-shim | **CREATE** | single attach helper; justified (F2: zero `with_metadata` matches). |
| `OTEL_EXPORTER_OTLP_HEADERS` parser | **CREATE** | list parse + case-insensitive `authorization` + percent-decode; justified (F3: zero `OTLP_HEADERS` matches). |

**Net**: REUSE the tonic metadata surface + endpoint-precedence pattern
+ aegis opaque-Debug principle + `#[non_exhaustive]` guarantee; EXTEND
`SparkConfig` and `build_pipeline`; CREATE only the redacting newtype,
the single metadata helper + apply-shim, and the env-headers parser. No
new crate, no new dependency, no pipeline restructure.

## Public-API / SemVer consequence (REAL DIFFERENCE — DELIVER must act)

**`spark` IS in the Gate 2/3 public-API set** — verified in CI:
`cargo public-api -p spark` (`.github/workflows/ci.yml:334,347`) and
`cargo semver-checks --package spark` (`ci.yml:426`). UNLIKE the recent
cinder/aperture features (not public-API-tracked).

`with_bearer_token` is a **new public method** on `spark`'s surface
(`BearerToken` and the field stay private). Therefore **in DELIVER**:
1. **Gate 2** WILL diff (one new public method) → **regenerate /
   accept the `cargo public-api` baseline** in the DELIVER commit.
2. **Gate 3** classifies a new method on a `#[non_exhaustive]` struct
   as **minor (additive)** → **bump `spark`'s minor version** (pre-1.0;
   **NEVER 1.0.0**, Andrea's call / CLAUDE.md / MEMORY).
No other crate's surface changes; no new dep (Gate 4 unaffected).

## Test seam (for DISTILL)

- **Token mint**: reuse the ADR-0068 / aegis HS256 mint seam (F5) — valid
  token for a catalogued tenant, matching `iss`/`aud`, future `exp`;
  no-token negative control.
- **E2E accept** (AC `a-bearer-configured-export-is-accepted-by-the-authenticated-gateway`):
  real authenticated aperture + recording sink; `with_bearer_token(jwt)`
  → ACCEPT (`decision=allow`), sink record tagged with the token tenant;
  same export, no token → DENY `missing_claim`, sink empty. Fails on
  today's no-knob code.
- **All-three** (AC `the-token-reaches-all-three-signals`): (a) UNIT
  assertion on `build_auth_metadata` — the `MetadataMap` carries
  `authorization: Bearer <token>`, apply-shim exercised for span/log/
  metric builder types; plus (b) at least one signal E2E through the
  authenticated aperture, integration extended to traces AND logs
  (E01-E04 cover both), metrics where exercisable.
- **Env path** (AC `OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer`):
  mirror `slice_04_env_var_precedence.rs` (`serial_test`, clean-env,
  recording-sink aperture). `authorization=Bearer%20<jwt>` → accepted,
  decoded. Precedence: both set → programmatic wins. Empty → no header,
  unauth collector still accepts.
- **Never-log** (AC `the-token-is-never-logged`): recognisable token;
  grep every `target="spark"` event, `{:?}` of `SparkConfig`, and error
  surfaces → **0** occurrences; redacted placeholder present where the
  field renders.
- **Non-regression** (AC `no-token-no-header-...`): no token → no
  metadata → `slice_01..slice_07` green; no-token exporter build
  byte-unchanged.

## Constraints (carried from DISCUSS, honoured by this design)

1. Token is a SECRET, never logged — DD3 (structural redaction).
2. Token supplied by the integrator, never baked — `with_bearer_token`
   arg or env var; in-memory only, never persisted.
3. Token reaches ALL THREE signals uniformly — DD1 (one helper).
4. No-auth path unchanged — DD5 (conditional attachment; `None` ⇒
   byte-identical build).
5. Add metadata, do not restructure the pipeline — DD1 (three
   `.with_metadata` calls only; processors/providers/flush/single-init
   untouched).
6. Inherited gates — ADR-0005 five gates; per-feature mutation 100% on
   modified spark files (`gate-5-mutants-spark`); Rust idiomatic; never
   1.0.0. **PLUS** the spark-specific Gate 2/3 public-api consequence
   above (the real difference).

## Upstream Changes

None. No new crate, no new dependency, no new env var beyond honouring
the standard `OTEL_EXPORTER_OTLP_HEADERS`. No change to aegis/aperture
(reuses ADR-0068 verbatim as the gateway counterpart). No infra change.
The DD7 aegis doc overstatement noted in ADR-0068 is out of scope here.

## Open question / hand-off

None blocking. DELIVER picks the percent-decode dependency (reuse an
existing workspace dep if `percent-encoding` is already locked, else a
tiny dependency-free decode) — a DELIVER implementation choice, not a
DESIGN seam. The malformed-header failure mode is locked (fail-fast,
DD4).
