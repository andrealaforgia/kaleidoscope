# DISTILL Wave Decisions — spark-ingest-auth-v0

- **Wave**: DISTILL (nWave). **Agent**: Quinn (`nw-acceptance-designer`).
- **Date**: 2026-06-06. **Mode**: Autonomous overnight run.
- **RECONCILED 2026-06-06** to `adr-0069 § Amendment (DISTILL
  back-propagation)` (which DISTILL's own upstream-overlap finding
  triggered) + `design/wave-decisions.md § Changed Assumptions`. See the
  new "Amendment reconciliation (2026-06-06)" section at the end — it
  governs where it differs from the original-run prose below. Summary:
  env-happy-path ADDED + classified GREEN-today by RUNNING; precedence
  INVERTED to env-as-override; malformed-fail-fast test REMOVED.
- **Feature**: `spark-ingest-auth-v0` (ADR-0069; sibling ADR-0068).
- **Inputs read**: `discuss/user-stories.md` (US-SP-AUTH-01/02/03 + AC +
  System Constraints 1-6), `design/wave-decisions.md` (DD1-DD5, the test
  seam, the public-api/semver consequence), `adr-0069-spark-ingest-auth.md`,
  `devops/wave-decisions.md` + `devops/environments.yaml` (the auth test
  environment; the bump is a DELIVER act, not a test concern; the auth seam
  is in-process), the spark source (`config.rs`, `init.rs`,
  `observability.rs`, `error.rs`, `lib.rs`), the spark test harness
  (`tests/common/mod.rs`, `slice_04_env_var_precedence.rs`, the other
  slices), and the reuse seams (`aperture/tests/slice_10_ingest_auth.rs`
  mint + `Config::builder().jwt_auth(...)`, `aegis` `make_jwt`).

## Prior-wave consultation (+/- checklist)

| Source | + (used) | − (gap / note) |
|---|---|---|
| `discuss/user-stories.md` | US-SP-AUTH-01/02/03, AC, the 9 embedded Gherkin scenarios, Marco/Priya personas, System Constraints 1-6, concrete data (acme-prod, HS256, exp) | − none; scope bounded to these 3 stories |
| `design/wave-decisions.md` | DD1 (one `build_auth_metadata` helper, all three exporters), DD2 (`with_bearer_token`, precedence), DD3 (`BearerToken` redacting newtype), DD4 (OTLP_HEADERS authorization-only, percent-decode, fail-fast), DD5 (silent no-token); the test seam | − the DD4 parser overlaps an upstream behaviour DISTILL discovered (see Upstream-overlap finding) |
| `adr-0069` | driving port = the `SparkConfig` builder + `spark::init`/`build_pipeline`; security posture; the test seam (E2E accept + unit helper + never-log grep + non-regression); public-api/semver posture | − the unit assertion on the `pub(crate)` `build_auth_metadata` cannot live in the black-box `tests/` crate (see All-three decision) |
| `devops/wave-decisions.md` + `environments.yaml` | auth_test_environment = real in-process aegis-authenticated aperture + RecordingSink + in-suite HS256 mint (reuses ADR-0068 F5); C-DEVOPS-5/6/7/8 (never-log hard gate, all-three structural, non-regression, deterministic no-wall-clock-threshold) | − the `0.1.0 -> 0.2.0` bump + public-api accept are DELIVER acts, NOT DISTILL — but DISTILL's scaffold adds the public method early (see Public-API note) |
| `docs/product/kpi-contracts.yaml` | — | − MISSING (soft gate). Per DEVOPS, KPI-1/2 ride the existing aperture audit, KPI-3/4 are CI test gates; no `@kpi` observability scenarios are authored (no metric-event contract to verify). Warning logged; proceeding. |

## WS strategy = Strategy C (real-local-IO)

**Declared: Strategy C.** Every walking-skeleton and accept scenario drives
a REAL aegis-authenticated aperture spawned in-process on EPHEMERAL loopback
ports (`127.0.0.1:0`), fronted by aperture's `RecordingSink`, with the HS256
secret + tenant catalogue written to REAL temp files and the bearer token
minted IN-SUITE with `jsonwebtoken::encode` (the ADR-0068 F5 seam, reused
verbatim from `aperture/tests/slice_10_ingest_auth.rs`). No `InMemoryExporter`,
no synthetic transport — the `authorization` metadata must travel a real gRPC
channel to a real `aegis::Validator` for the accept/deny to mean anything.

This matches the existing spark slice posture (`tests/common/mod.rs` header:
"Strategy C real local") and the DEVOPS `environments.yaml > auth_test_environment`.

**The litmus test (Dim 9d):** "if I deleted the real adapter, would this WS
still pass?" — No. Delete the real aperture/validator and the accept WS has
no sink to land in; the deny control has no validator to reject. The wiring
is genuinely exercised.

**The driving port (Mandate 1):** the tests enter ONLY through Spark's public
surface — `spark::init(SparkConfig)` configured via the builder
(`with_bearer_token`, `with_endpoint`) and/or `OTEL_EXPORTER_OTLP_HEADERS`,
then telemetry through the standard OTel global API, then guard-drop to flush.
No spark-internal symbol (`BearerToken`, `build_auth_metadata`, the env
parser) is reached from a test. The observable outcome is the `RecordingSink`
state behind the real aperture (accepted => non-empty; denied => empty) — the
same observable the slice_04 round-trip witness uses.

## The all-three-signals decision (reconciliation with the ADR test seam)

ADR-0069's seam proposed "(a) a UNIT assertion on `build_auth_metadata`
(the `MetadataMap` carries `authorization: Bearer <token>`, apply-shim
exercised for span/log/metric builder types) + (b) at least one signal E2E".

`build_auth_metadata` is `pub(crate)` (DD1/DD3 — the secret accessor must not
be public). A unit assertion on it cannot live in the black-box `tests/`
integration crate (Mandate 1: tests enter through driving ports only; the
helper is an internal component). Two honest options:

1. The unit assertion lives in a `#[cfg(test)] mod tests` in `src/init.rs` —
   but that is DELIVER's inner-loop, authored alongside the helper it asserts.
2. DISTILL pins the all-three property through the **observable** driving-port
   outcome instead, which is strictly STRONGER than the unit assertion: it
   proves the metadata actually rides the wire to a real validator, not merely
   that a map was built.

**Chosen: option 2 for DISTILL, option 1 delegated to DELIVER's inner loop.**
The all-three property is observable here via TWO complementary driving-port
scenarios: the WS accept (a span + a log + a metric all flow under one token
and are accepted) AND a **metric-only** accept (`a_metric_only_export_is_
authenticated...`) — the falsifiable witness that the METRIC exporter is not
left un-authenticated by omission (a DELIVER partial-wire that authenticated
only traces+logs would deny the metric-only export and fail this test). The
handoff records the `build_auth_metadata` unit assertion as a DELIVER
inner-loop task (it kills the Gate-5 mutant that drops `.with_metadata` on one
builder, complementing the metric-only E2E).

## Upstream-overlap finding (ESCALATE to DESIGN/DELIVER — load-bearing)

> **STATUS: RESOLVED by the ADR-0069 amendment (see "Amendment
> reconciliation (2026-06-06)" at the end of this file).** The finding below
> is the original escalation that TRIGGERED the amendment; it is kept on the
> record as the as-run text. Where it disagrees with the reconciliation
> section (notably: the env happy-path is now ADDED not OMITTED; the
> malformed-fail-fast and DD4-parser expectations are DROPPED; precedence is
> env-as-override not programmatic-wins), the reconciliation governs.

**Discovered at DISTILL by running the env happy-path test:** the locked
`opentelemetry-otlp =0.27` tonic exporter ALREADY honours
`OTEL_EXPORTER_OTLP_HEADERS` NATIVELY at `.with_tonic()` build time
(`exporter/tonic/mod.rs:156-159` — `parse_headers_from_env` +
`merge_metadata_with_headers_from_env`, with the spec percent-decode applied
upstream). Consequence:

- A behavioural test that "an env `authorization=Bearer%20<jwt>` is attached
  and the authenticated gateway accepts" PASSES against today's no-knob
  scaffold — the UPSTREAM crate does the attach, not spark. It is **NOT
  falsifiable** and would be Upstream/Fixture Theater (Critical Rule 7).
  It is therefore deliberately **OMITTED** (a documented non-test in the
  slice file replaces it).
- F3 in DISCUSS/ADR-0069 ("Spark ignores `OTEL_EXPORTER_OTLP_HEADERS`") is
  true of spark's OWN code, but the upstream exporter does not ignore it. The
  net observable for the happy path is already delivered.

**What remains genuinely spark-owned and falsifiable (and IS tested):**

- **Precedence** — programmatic `with_bearer_token` must WIN over the env var.
  Upstream has no notion of spark's programmatic knob, so today the env token
  is what rides the wire. The precedence test sets the env var to a token for
  an UNKNOWN tenant (which the gateway denies) and the programmatic knob to a
  VALID token; it asserts ACCEPT — falsifiable, because today the env
  (unknown-tenant) token wins via upstream and the export is denied.
- **Malformed fail-fast (DD4)** — spark surfaces a corrupt env value as
  `ExporterInitFailed`; upstream silently drops it. Falsifiable: today init
  succeeds (no spark parser), so the `expect_err` fails.
- **Empty == absent** — a guardrail control (upstream-aligned).

**DELIVER reconciliation required (flag):** DELIVER's DD4 parser must
reconcile with the upstream env attach to avoid a double-attach and to make
the programmatic knob actually override the env path (likely: when spark
resolves a token, build the `MetadataMap` explicitly and/or neutralise the
upstream env read; when spark resolves none, decide whether to let upstream's
env attach stand or to own the parse for the fail-fast semantics). This is a
DESIGN/DELIVER decision, not a DISTILL acceptance test. Recommend an ADR-0069
amendment noting the upstream overlap. `@escalate:solution-architect`.

## Falsifiability note (each auth test fails against today's no-knob spark)

> **SUPERSEDED in part by the amendment reconciliation (end of file).** The
> table below is the ORIGINAL 4-ignored-test run. After reconciliation the
> ignored set is 3 (the env-happy-path is now an un-ignored GREEN guard; the
> malformed-fail-fast test is deleted; the precedence test is re-framed as
> env-as-override and is trivially-green-not-RED vs scaffold). See the
> reconciliation section's run-evidence for the current state.

Proven by RUNNING `cargo test -p spark --test slice_08_ingest_auth -- --ignored`:
all 4 ignored tests fail on an OUTCOME assertion, NOT a missing symbol —

| Ignored test | Failure mode against scaffold (RED, not BROKEN) |
|---|---|
| `marco_with_a_valid_bearer_token_...accepted...` | `wait_for predicate did not become true within 3s` — token stored, no metadata attached, authenticated aperture DENIES, sink empty |
| `a_metric_only_export_is_authenticated_...metric_signal` | `wait_for ...` — metric export denied (no metadata) |
| `the_programmatic_bearer_token_wins_over_the_headers_env_var` | `must WIN ... must be ACCEPTED` — env (unknown-tenant) token wins via upstream, denied |
| `a_malformed_headers_authorization_value_fails_init_fast...` | `must fail init fast: SparkGuard { .. }` — init succeeds (no spark parser) |

The UN-ignored controls (already GREEN, and must stay GREEN through DELIVER):
the no-token DENY negative control, the expired-honest-send, the empty-env
control, the no-auth non-regression, and the never-log GUARDRAIL (the
redacting `Debug` is in the DISTILL scaffold).

## The #[ignore]-until-DELIVER decision + proven-RED evidence

Pre-commit runs `cargo test --workspace` and the project NEVER uses
`--no-verify`, so every behaviourally-RED test MUST be
`#[ignore = "RED until DELIVER: spark-ingest-auth-v0"]`. Proven:

- **Default GREEN**: `cargo test -p spark --test slice_08_ingest_auth` =>
  `6 passed; 0 failed; 4 ignored`. Full `cargo test -p spark` => every slice
  GREEN (58 prior-slice tests + 6 invariants/unit + slice_08 6 controls),
  0 failed.
- **`--ignored` FAILING**: `... -- --ignored --test-threads=1` =>
  `0 passed; 4 failed` — each on an OUTCOME panic (table above).

DELIVER removes the `#[ignore]` attributes one at a time (outer-loop drive),
turning each RED test GREEN as it lands `build_auth_metadata`, the apply-shim,
the precedence resolver, and the DD4 fail-fast parser.

## Minimal compile scaffold (Mandate 7) + Public-API note

To make the tests COMPILE against the intended API without implementing the
feature (mirroring the aegis/aperture DISTILL scaffold precedent), DISTILL
added to `crates/spark/src/config.rs`:

- `pub fn with_bearer_token(impl Into<String>) -> Self` — STORES the token,
  does NOT attach it (DELIVER attaches in `init.rs`). NO-OP w.r.t. exporters,
  so the accept tests fail RED.
- `pub(crate) bearer_token: Option<BearerToken>` field (defaulted `None`).
- `pub(crate) struct BearerToken(String)` redacting newtype: hand-written
  `Debug` rendering `BearerToken(<redacted>)`, no value-`Display`, a
  `pub(crate) expose()` accessor (`#[allow(dead_code)]` for the scaffold
  window — DELIVER consumes it in `build_auth_metadata`). The redacting
  `Debug` is implemented NOW because the never-log GUARDRAIL must already hold
  (System Constraint 1 is the load-bearing security property).

**Public-API note (NOT a DISTILL defect):** `with_bearer_token` is a new
public method on `spark`'s Gate-2/3-tracked surface. DISTILL adding it early
means the additive public-API delta appears at the DISTILL commit rather than
the DELIVER commit. On pure trunk-based main (no required status checks; CI is
feedback — MEMORY), this is fine: a branch-side `cargo public-api --deny=added`
report is the EXPECTED intended-add signal and clears on merge. DELIVER still
owns the `0.1.0 -> 0.2.0` MINOR bump (C-DEVOPS-3) and the public-api baseline
accept; the bump need not (and does not) happen at DISTILL — the method's
presence is what the scaffold needs, the version is a DELIVER act. If a
reviewer prefers the public method to first appear at the DELIVER commit, the
alternative is to gate the scaffold method behind `#[cfg(test)]` — rejected
here because that diverges the test-vs-production API shape and the aegis
precedent put the scaffold on the real surface.

## Ephemeral-port hygiene

Every aperture spawned by `slice_08` (via `common::spawn_aperture_with_jwt_auth`
and `common::spawn_aperture_with_recording_sink`) binds `grpc_bind_addr` /
`http_bind_addr` to `127.0.0.1:0` and Spark connects to the OS-assigned bound
address (`fixture.grpc_endpoint()`). The fixed defaults 4317/4318 are NEVER
used (they collide with aperture's slice_09/slice_10 refusal-probe tests under
parallel runs — the known flake). The aperture instance is reaped on
`ApertureFixture::Drop`; the temp secret/catalogue files on `TempAuthFiles::Drop`.
After every run in this wave, `pgrep -fl 'target/debug/aperture'` was empty.

## Determinism (C-DEVOPS-8)

All slice_08 assertions are boolean accept/deny (sink non-empty / empty),
tenant-precedence accept, `is_err` + substring-absence — NO wall-clock
threshold. The p95-flake class (lumen/pulse) does NOT apply. The accept tests
use a bounded `wait_for`/poll (3 s) for the async batch flush, which is a
liveness wait, not a latency assertion.

## Test organisation

Per the spark convention (ADR-0015 §2: one `[[test]]` binary per slice for
pristine OTel global state), the new file is
`crates/spark/tests/slice_08_ingest_auth.rs` with its `[[test]]` block in
`crates/spark/Cargo.toml`. Shared fixture lives in the existing
`tests/common/mod.rs` (the `spawn_aperture_with_jwt_auth` helper added).
Dev-deps added: `jsonwebtoken = "9"`, `serde = { features = ["derive"] }`
(both already in `Cargo.lock`; dev-deps only — Gate 4 licence containment
intact).

## What this wave does NOT do

- Does NOT implement the feature (DELIVER lands `build_auth_metadata`, the
  apply-shim, the redaction wiring, the `0.1.0 -> 0.2.0` bump, the public-api
  accept). NOTE: DELIVER does NOT build an env parser / percent-decode /
  malformed-env fail-fast — see the amendment reconciliation below.
- Does NOT modify the architecture (the upstream-overlap finding was
  escalated and is now RESOLVED by the ADR-0069 amendment).
- Does NOT touch `observability.rs` (DD3 — closed vocabulary unchanged).
- Does NOT proceed into DELIVER.

## Amendment reconciliation (2026-06-06)

The original-run upstream-overlap finding (above) was escalated to DESIGN and
RESOLVED by `adr-0069 § Amendment (DISTILL back-propagation)` +
`design/wave-decisions.md § Changed Assumptions`. The amendment is the
authoritative revised contract; this section reconciles the DISTILL tests to
it. Three substantive changes, each CLASSIFIED BY RUNNING.

### What the amendment changed (authoritative)

1. **No spark env parser.** `opentelemetry-otlp =0.27` already honours
   `OTEL_EXPORTER_OTLP_HEADERS` UNCONDITIONALLY on spark's
   `.with_tonic()...build()` construction path (`parse_headers_from_env`,
   tonic/mod.rs:156; `url_decode` percent-decode, mod.rs:233), for all three
   signals. The original DD4 spark-owned parser is DROPPED as redundant.
2. **Env wins on collision (precedence INVERTED).** Upstream merges via
   `HeaderMap::extend` (tonic/mod.rs:320-321), which OVERWRITES on key
   collision — so a concurrently-set env `authorization` is the FINAL writer,
   the inverse of the original "programmatic wins". Adopted (amendment DD2
   option 1): the knob is the in-code API; env is the conventional override,
   honoured by upstream and final on collision; documented on
   `with_bearer_token`. Zero spark env code.
3. **Malformed env = upstream silent-drop, not spark fail-fast.** Upstream
   does `HeaderValue::from_str(&value).ok()?` (tonic/mod.rs:335) — a
   malformed env value vanishes silently. The original DD4 spark-owned
   fail-fast is DROPPED. The programmatic token is a plain `String` (no
   parse/decode) so it has no malformed case beyond the existing "bytes
   aren't a valid header value" guard (DELIVER inner-loop).

### The three test reconciliations + classification (BY RUNNING)

| Change | Test | Classification (RUN) | State |
|---|---|---|---|
| ADD env happy-path (the amendment's env-before-init disambiguation probe) | `an_env_authorization_header_set_before_init_is_accepted_by_the_authenticated_gateway` | **GREEN TODAY** with no spark change — upstream attaches+decodes; the authenticated aperture ACCEPTS. This is the whole point: the env half works code-free. | UN-ignored non-regression GUARD |
| INVERT precedence to env-as-override | `the_env_authorization_overrides_the_programmatic_bearer_token_on_collision` (replaces `the_programmatic_bearer_token_wins_over_the_headers_env_var`) | Under `--ignored` it PASSES today, but TRIVIALLY (the scaffold knob is a no-op, so only the env token attaches anyway — the deny is satisfied for the wrong reason, not by a real env-over-knob collision). NOT a falsifiable scaffold-RED. | `#[ignore]`d (DELIVER-completion; un-ignored when the programmatic attach lands and the collision becomes real) |
| REMOVE malformed-fail-fast | (deleted) `a_malformed_headers_authorization_value_fails_init_fast_without_echoing_bytes` | n/a — no spark-owned malformed case exists after the amendment. | DELETED; `SparkError` import removed (no remaining use) |

The **env-happy-path GREEN classification is the load-bearing reconciliation
of Bea Verifier msg-038**: msg-038 was a black-box observation that no bearer
arrived via env. The probe demonstrates the env path DOES authenticate at the
real aperture on spark's construction — so msg-038 was environmental (var not
inherited by the init process, or set after `.build()`, or a mis-encoded
value upstream-silent-dropped), NOT a spark code gap. Earned-trust: the claim
"upstream honours OTLP_HEADERS on our path" is now demonstrated against the
real aperture, not assumed.

### Kept unchanged (still aligned with the amendment)

- **#1 programmatic accept WS** + **#3 metric-only** — the load-bearing
  spark-owned deliverable is the programmatic knob (upstream has NO
  programmatic bearer method); both stay RED (the `build_auth_metadata`
  attach is unbuilt). Confirmed behaviourally RED by RUNNING `--ignored`
  (sink empty, `wait_for` timeout — OUTCOME panic, not a missing symbol).
- **#8 never-log grep** (control), **#9 no-token non-regression** (control),
  **#2 no-token deny** (control), **#4 expired-honest-send** (control),
  **#7 empty-as-absent** (control) — all GREEN, all still correct under the
  amendment.
- The `build_auth_metadata` all-three unit assertion — still handed to
  DELIVER as the inner-loop Gate-5 anchor.

### Run evidence (this reconciliation)

Default (`cargo test -p spark --test slice_08_ingest_auth`):
```
running 10 tests
test a_metric_only_export_is_authenticated_proving_the_token_reaches_the_metric_signal ... ignored, RED until DELIVER: spark-ingest-auth-v0
test an_empty_headers_env_var_is_treated_as_no_credential_and_unauth_collector_accepts ... ok
test an_env_authorization_header_set_before_init_is_accepted_by_the_authenticated_gateway ... ok
test marco_with_a_valid_bearer_token_has_his_export_accepted_by_the_authenticated_gateway ... ignored, RED until DELIVER: spark-ingest-auth-v0
test marco_with_an_expired_token_still_initialises_spark_sends_it_honestly ... ok
test marco_without_a_token_is_denied_by_the_authenticated_gateway_nothing_stored ... ok
test no_token_no_header_against_an_unauthenticated_collector_still_exports ... ok
test red_reason_is_documented ... ok
test the_configured_token_never_appears_in_any_spark_log_event_or_config_debug ... ok
test the_env_authorization_overrides_the_programmatic_bearer_token_on_collision ... ignored, DELIVER-completion: env-over-programmatic precedence is only meaningful once the knob attaches (spark-ingest-auth-v0)
test result: ok. 7 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out
```
Full `cargo test -p spark`: every prior binary GREEN (lib 1; invariants 5+1;
slice_01 7; slice_02 11; slice_03 10; slice_04 7; slice_05 8; slice_06 10;
slice_07 5; slice_08 7 passed / 3 ignored), 0 failed.

`--ignored` (`... -- --ignored --test-threads=1`):
```
running 3 tests
test a_metric_only_export_is_authenticated_proving_the_token_reaches_the_metric_signal ... FAILED
test marco_with_a_valid_bearer_token_has_his_export_accepted_by_the_authenticated_gateway ... FAILED
test the_env_authorization_overrides_the_programmatic_bearer_token_on_collision ... ok
... panicked at crates/spark/tests/common/mod.rs:646:13: wait_for predicate did not become true within 3s (x2)
test result: FAILED. 1 passed; 2 failed; 0 ignored
```
The two genuinely-RED programmatic-attach tests FAIL on the OUTCOME assertion
(sink empty — no metadata attached), NOT on a missing symbol. The precedence
test passes trivially (classified above). `fmt --check` clean; `clippy -p
spark --all-targets -- -D warnings` clean; `pgrep -fl 'target/debug/aperture'`
empty after every run.
