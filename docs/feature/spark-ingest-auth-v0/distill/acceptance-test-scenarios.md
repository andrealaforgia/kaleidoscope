# Acceptance Test Scenarios — spark-ingest-auth-v0

- **Wave**: DISTILL. **Agent**: Quinn (`nw-acceptance-designer`). **Date**: 2026-06-06.
- **RECONCILED 2026-06-06** to `adr-0069 § Amendment (DISTILL
  back-propagation)` + `design/wave-decisions.md § Changed Assumptions`.
  Three reconciliations (detail below): (1) env-happy-path ADDED and
  classified GREEN-today by RUNNING (upstream honours
  `OTEL_EXPORTER_OTLP_HEADERS` code-free — the amendment's env-before-init
  disambiguation probe, reconciling Bea Verifier msg-038); (2) precedence
  test #5 INVERTED to env-as-override (upstream `HeaderMap::extend`
  overwrites on collision — env wins, not programmatic); (3) the
  malformed-fail-fast test #7 REMOVED (env malformed is upstream's
  silent-drop, and the programmatic token is a plain String with no
  malformed case). This reconciles the verifier's msg-038 contradiction.
- **Test file**: `crates/spark/tests/slice_08_ingest_auth.rs`
  (`[[test]]` block + dev-deps in `crates/spark/Cargo.toml`; shared fixture
  in `crates/spark/tests/common/mod.rs`).
- **Driving port**: `spark::init(SparkConfig)` (builder +
  `OTEL_EXPORTER_OTLP_HEADERS`) -> telemetry via the OTel global API ->
  guard-drop. Observable outcome: the `RecordingSink` behind a REAL
  aegis-authenticated aperture (accepted => non-empty; denied => empty),
  plus the `target="spark"` event/`Debug` surfaces for never-log.

## Scenario list -> test-fn -> US/AC map -> tag

| # | Scenario (test fn) | US / AC | Category | Tags | State |
|---|---|---|---|---|---|
| 1 | `marco_with_a_valid_bearer_token_has_his_export_accepted_by_the_authenticated_gateway` | US-SP-AUTH-01 / a-bearer-configured-export-is-accepted | happy / WS | `@walking_skeleton @driving_port @real-io @adapter-integration` | RED (ignored) |
| 2 | `marco_without_a_token_is_denied_by_the_authenticated_gateway_nothing_stored` | US-SP-AUTH-01 / accept-vs-deny control | error (negative control) | `@walking_skeleton @driving_port @real-io` | GREEN control |
| 3 | `a_metric_only_export_is_authenticated_proving_the_token_reaches_the_metric_signal` | US-SP-AUTH-01 / the-token-reaches-all-three-signals | edge (all-three witness) | `@driving_port @real-io @property` | RED (ignored) |
| 4 | `marco_with_an_expired_token_still_initialises_spark_sends_it_honestly` | US-SP-AUTH-01 / Spark-sends-the-token-honestly (DD5) | error / boundary | `@driving_port @real-io` | GREEN control |
| 5 | `an_env_authorization_header_set_before_init_is_accepted_by_the_authenticated_gateway` | US-SP-AUTH-02 / OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer (env happy-path / disambiguation probe) | happy / env | `@driving_port @real-io @adapter-integration` | **GREEN guard (classified by RUNNING)** |
| 6 | `the_env_authorization_overrides_the_programmatic_bearer_token_on_collision` | US-SP-AUTH-02 / precedence — REVISED to env-as-override | edge | `@driving_port @real-io` | ignored (DELIVER-completion; trivially-green vs scaffold) |
| 7 | `an_empty_headers_env_var_is_treated_as_no_credential_and_unauth_collector_accepts` | US-SP-AUTH-02 / empty-env-var-is-no-credential | error / boundary | `@driving_port @real-io` | GREEN control |
| 8 | `the_configured_token_never_appears_in_any_spark_log_event_or_config_debug` | US-SP-AUTH-03 / the-token-is-never-logged | error / security GUARDRAIL | `@driving_port @property` | GREEN guardrail |
| 9 | `no_token_no_header_against_an_unauthenticated_collector_still_exports` | US-SP-AUTH-03 / no-token-no-header-still-works | non-regression | `@driving_port @real-io` | GREEN control |

REMOVED by the amendment: the former #7
`a_malformed_headers_authorization_value_fails_init_fast_without_echoing_bytes`
(see "Removed test" below).

Plus `red_reason_is_documented` (a meta self-test keeping the RED constant
referenced) and the lib `catalogue_returns_the_same_instance_across_calls`
(pre-existing, unrelated).

**Story coverage (Dim 4 / Dim 8 Check A)**: US-SP-AUTH-01 -> #1,2,3,4;
US-SP-AUTH-02 -> #5 (env happy-path, NOW PRESENT — upstream honours it
code-free), #6 (env-as-override precedence), #7 (empty-as-absent);
US-SP-AUTH-03 -> #8,9. Every story has >=2 scenarios; every live AC has a
mapped scenario. The malformed-env AC is withdrawn from spark scope by the
amendment (upstream silent-drop; no spark-owned malformed case).

## Error/edge path ratio (>=40%)

Of the 9 black-box scenarios, the error/edge/boundary/security/non-regression
set is #2, #3, #4, #6, #7, #8, #9 = 7/9 (78%). Happy path: #1 (programmatic
accept WS) and #5 (env happy-path). The ratio comfortably exceeds 40% — the
feature is security-shaped, so the deny/reject/never-log/non-regression paths
dominate by design.

## Adapter coverage table (Dim 9c)

The feature adds NO new driven adapter — it adds an `authorization` metadata
header to the three EXISTING OTLP/gRPC exporters Spark already drives. The
"adapter" exercised with REAL I/O is the OTLP/gRPC export path to a real
aperture + real `aegis::Validator`:

| Driven path | Real-I/O scenario | Verdict |
|---|---|---|
| OTLP/gRPC SpanExporter -> aperture | #1 (span emitted in the all-three flow) accept; #2 deny | covered `@real-io` |
| OTLP/gRPC LogExporter -> aperture | #1 (log emitted in the all-three flow) accept; #2 deny | covered `@real-io` |
| OTLP/gRPC MetricExporter -> aperture | #3 metric-only accept (the falsifiable all-three witness); #1 metric in the flow | covered `@real-io` |
| Env path (`OTEL_EXPORTER_OTLP_HEADERS`, honoured by UPSTREAM) | #5 env happy-path accept (real aperture), #6 env-as-override precedence, #7 empty-as-absent | covered (real aperture round-trips; the parse/decode is upstream's, exercised through it) |
| Never-log surface (`target="spark"` event + config `Debug`) | #8 grep zero-occurrence + redacted placeholder | covered |

No `@in-memory` is used anywhere (no InMemory exporter exists in the spark
suite — Strategy C throughout). No adapter lacks a real-I/O scenario.

## Walking skeleton litmus (Dim 5)

WS = #1 `marco_with_a_valid_bearer_token_has_his_export_accepted_by_the_
authenticated_gateway`. (1) Title = a user goal (Marco gets his telemetry
through the secured gateway), not a technical flow. (2) Given/When = user
context (Marco configures a token, his service exports). (3) Then = an
observable user outcome (the export is ACCEPTED — the record reaches the
sink — vs the deny control #2 where nothing reaches it). (4) A non-technical
stakeholder (Priya) can confirm "yes, that proves the lock has a matching
key". The deny half (#2) is paired so the skeleton carries its own security
boundary — a happy-path-only skeleton would not be shippable.

## Observable-behaviour assertions (Dim 7)

Every Then asserts an observable outcome at the driving-port boundary:
`fixture.sink.is_empty()` (the record reached/did not reach the real sink —
an OBSERVABLE outcome, the user-visible "did my telemetry get through"),
`result.expect_err(...)` + `matches!(err, ExporterInitFailed)` (a return value
from the driving port `init`), substring-absence over captured `target="spark"`
events + the config `Debug` string (observable log surfaces). NO scenario
asserts a private field, a method-call count, or an internal type. The
all-three property is observed via the metric-only ACCEPT (a wire outcome),
not by reaching into `build_auth_metadata`.

## @property-tagged criteria

- #3 (`@property`): "the token reaches ALL three signals" is a universal
  invariant ("every exporter carries the same `authorization`"); the
  metric-only witness is its falsifiable edge. Signals DELIVER to also pin it
  with a `build_auth_metadata` unit assertion exercised across span/log/metric
  builder types (the Gate-5 anchor for the partial-wire mutant).
- #8 (`@property`): "the token NEVER appears on any loggable surface"
  (universal "never"). Signals DELIVER that the redaction is structural (the
  newtype `Debug`), provable by a property over arbitrary token values.

## Environment-to-scenario mapping (Dim 8 Check B)

DEVOPS `environments.yaml` target environments: `clean`, `with-pre-commit`,
`ci`, and `auth_test_environment` (the in-process authenticated aperture +
in-suite HS256 mint). The auth_test_environment is the one with behavioural
preconditions, and every WS/accept scenario's `Given` references it (a real
aperture with `jwt_auth(issuer, audience, secret_file, catalogue)` for the
catalogued tenant `acme-prod`). `clean`/`with-pre-commit`/`ci` are
build/test-matrix environments (not deploy targets) — they are satisfied by
the deterministic, no-wall-clock-threshold design (C-DEVOPS-8); the default
suite runs identically in the pre-commit hook and CI Gate 1.

## Upstream-overlap finding — RESOLVED by the ADR-0069 amendment

The DISTILL upstream-overlap finding (the env happy-path is honoured by
upstream, not spark) was escalated to DESIGN and is now RESOLVED by
`adr-0069 § Amendment (DISTILL back-propagation)`. The amendment confirmed
from a locked-source read that `opentelemetry-otlp =0.27` honours
`OTEL_EXPORTER_OTLP_HEADERS` UNCONDITIONALLY on spark's `.with_tonic()...
build()` path (`parse_headers_from_env`, tonic/mod.rs:156; `url_decode`
percent-decode, mod.rs:233), and DROPPED the spark-owned env parser. The
test inventory is reconciled accordingly:

- **Env happy-path is NO LONGER omitted — it is ADDED as #5** and serves as
  the amendment's "env-before-init disambiguation probe". Classified by
  RUNNING: it is **GREEN today with no spark change** (`cargo test -p spark
  --test slice_08_ingest_auth
  an_env_authorization_header_set_before_init_is_accepted_by_the_authenticated_gateway`
  passes). This is exactly the amendment's expected outcome — the
  env-honouring half works code-free. It is left UN-ignored as a
  non-regression GUARD that documents the env path needs no spark code, and
  it empirically reconciles Bea Verifier msg-038 (the black-box "no bearer
  arrived via env" observation was environmental, not a code gap).
- **Precedence (#6) is INVERTED to env-as-override**, not "programmatic
  wins". Upstream `merge_metadata_with_headers_from_env` does
  `HeaderMap::extend` (tonic/mod.rs:320-321), which OVERWRITES on key
  collision — so a concurrently-set env `authorization` is the FINAL writer.
  #6 sets the env token to an UNKNOWN tenant and the programmatic knob to a
  VALID tenant and asserts the gateway DENIES (env wins). Classified by
  RUNNING: against today's scaffold the deny is satisfied TRIVIALLY (the
  programmatic knob is a no-op, so only the env token attaches anyway) — it
  is NOT a falsifiable scaffold-RED. It is kept `#[ignore]`d (out of the
  default suite) so its trivial green is not counted as a real control
  (Critical Rule 7); it becomes the meaningful env-override assertion once
  DELIVER lands the programmatic `.with_metadata` attach, and DELIVER
  un-ignores it together with that landing.
- **The double-attach reconciliation DELIVER once owed is now a DESIGN
  decision** (amendment DD2-revised, option 1): the programmatic knob is the
  supported in-code API; a concurrently-set env header is honoured by
  upstream and final on collision (documented on `with_bearer_token`). No
  spark env-handling code; no double-attach to engineer.

## Removed test (amendment reconciliation)

The former #7
`a_malformed_headers_authorization_value_fails_init_fast_without_echoing_bytes`
(asserting spark surfaces a corrupt env `authorization` value as
`SparkError::ExporterInitFailed` fail-fast, per the original DD4) is
**REMOVED**. Per the amendment (DD4-revised + "Malformed-header AC —
RESOLVED: DROP it from spark scope"):

- Env parsing is **upstream's** concern now; upstream's actual behaviour for
  a malformed env header value is **SILENT-DROP**
  (`HeaderValue::from_str(&value).ok()?`, tonic/mod.rs:335), NOT fail-fast.
  Spark cannot impose fail-fast without re-adding the env parser the
  amendment just removed.
- The **programmatic** token is a plain `String` passed to
  `MetadataValue::try_from` — no percent-decode, no list-parse — so there is
  no "malformed header" case for the knob, only the existing "bytes are not
  a valid HTTP header value" guard, which stays in DELIVER as
  `SparkError::ExporterInitFailed` (DD1, unchanged; reason names the kind,
  never the bytes — DD3 still binds). That guard is a DELIVER inner-loop
  unit concern, not a black-box acceptance scenario.

Consequence: there is no spark-owned malformed-env-header acceptance
scenario. The `SparkError` import was removed from the slice file (no
remaining use). `@escalate:solution-architect` — RESOLVED (amendment
accepted).

## Mandate-7 / driving-port / falsifiable / never-log self-review checklist

- [x] **Mandate 7 (RED-not-BROKEN)**: the 2 genuinely-RED ignored tests
  (#1 WS programmatic accept, #3 metric-only programmatic accept) COMPILE
  (minimal scaffold) and fail on OUTCOME assertions under `--ignored`
  (`wait_for predicate did not become true within 3s` — sink empty, no
  metadata attached), not on missing symbols. The precedence test (#6) is
  ignored as a DELIVER-completion assertion (trivially green vs scaffold,
  not a falsifiable scaffold-RED — see Upstream-overlap RESOLVED). Classified
  by RUNNING (evidence in wave-decisions.md).
- [x] **Driving port (Mandate 1 / CM-A)**: every test imports only
  `spark::{init, SparkConfig}` + the public OTel global API + the aperture
  test fixture; zero spark-internal imports. (`SparkError` import removed
  with the malformed test.)
- [x] **Business/observable (Mandate 2,3 / CM-B,C / Dim 7)**: titles name
  Marco's goal; Then steps assert the sink outcome / the init return / the
  log-surface absence — observable, not internal state.
- [x] **Falsifiable**: the programmatic-attach tests (#1,#3) fail against
  today's no-knob spark. The env happy-path (#5) is intentionally GREEN-today
  (upstream honours it code-free — the amendment's disambiguation probe, a
  non-regression guard, NOT a feature-under-test); the precedence test (#6)
  is ignored because it is only trivially-green vs scaffold and becomes
  meaningful at DELIVER.
- [x] **Never-log (System Constraint 1 / KPI-3, C-DEVOPS-5)**: #8 asserts 0
  occurrences across `target="spark"` events AND the config `Debug`, AND the
  redacted placeholder presence — already GREEN via the scaffold's redacting
  newtype, a permanent guardrail.
- [x] **All-three via the helper (KPI-1 / C-DEVOPS-6)**: observed via the
  metric-only accept (#3) + the full-flow accept (#1); the `build_auth_metadata`
  unit assertion is handed to DELIVER as the inner-loop Gate-5 anchor.
- [x] **Ephemeral-port hygiene**: all apertures bind `127.0.0.1:0`; children +
  temp files reaped on Drop; `pgrep target/debug/aperture` empty post-run.
- [x] **Non-regression (System Constraint 4 / KPI-4, C-DEVOPS-7)**: #9 + the
  full prior spark suite (slice_01..07 + invariants) stay GREEN with the
  scaffold present.
- [x] **KPI contracts**: MISSING (soft gate); no `@kpi` scenarios authored
  (no metric-event contract exists; KPI-1/2 ride the aperture audit, KPI-3/4
  are CI gates). Warning logged.

## Handoff to DELIVER (one-at-a-time sequence) — RECONCILED to the amendment

Build ONLY the genuinely spark-owned core (amendment "DELIVER scope"):
NO env parser, NO percent-decode, NO spark-owned malformed-env fail-fast.

1. Land `BearerToken::expose` consumption + `build_auth_metadata(&SparkConfig)
   -> Option<MetadataMap>` (knob-only resolution, DD1) + the per-signal
   apply-shim + the three `.with_metadata(map.clone())` + the
   `WithTonicConfig` import; un-ignore #1 (WS programmatic accept) and #3
   (metric-only). Add the inner-loop unit assertion on `build_auth_metadata`
   (all three builder types) in `src/init.rs`, plus the construction-time
   invalid-header-value guard (`SparkError::ExporterInitFailed`, reason names
   the kind not the bytes — the only remaining "malformed" case, for the
   programmatic token).
2. Confirm env-as-override precedence (DD2-revised, no double-attach, no env
   code): the knob attaches via `.with_metadata`; document on
   `with_bearer_token` that a concurrent `OTEL_EXPORTER_OTLP_HEADERS=
   authorization=...` is honoured by upstream and final on collision.
   Un-ignore #6 alongside the programmatic-attach landing (it then asserts
   the env-override deny is real, not trivial).
3. Confirm all controls/guards (#2,4,5,7,8,9) + slice_01..07 stay GREEN
   (note #5 the env happy-path is already GREEN — keep it so; a double-attach
   regression would break it). Bump `crates/spark/Cargo.toml`
   `0.1.0 -> 0.2.0` and accept the public-api baseline in the same commit
   (C-DEVOPS-3); reach Gate-5 100% kill on the new branches; remove the
   scaffold `#[allow(dead_code)]`.
4. Run the env-before-init disambiguation probe (#5) as the empirical
   confirmation that msg-038 was environmental (it passes today; it must keep
   passing).
