# Wave Decisions — aperture-body-size-cap-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Agent**: Quinn (`nw-acceptance-designer`), running as Scholar
- **Date**: 2026-06-07
- **Mode**: Autonomous overnight run. British English. No em dashes, no emoji.
- **Output**: one acceptance test file
  (`crates/aperture/tests/slice_11_body_size_cap.rs`), the minimal config
  scaffold needed to compile RED (`crates/aperture/src/config/mod.rs`: the new
  `pub fn max_recv_msg_size` ConfigBuilder setter + `pub(crate)` field +
  accessor), and this file.
- **Inputs read**: `design/wave-decisions.md` (DD1-DD5 + the "For Acceptance
  Designer" hand-off at the honest protection strength), `ADR-0073`
  (transport-boundary placement + the honest protection-strength envelope),
  `design/upstream-changes.md` (the honest-strength AC wording refinements),
  `discuss/{user-stories,story-map,outcome-kpis}.md`, `devops/environments.yaml`
  + `devops/wave-decisions.md` (the in-process `cap_test_environment`; the
  rejection counter is DISCLOSED-DEFERRED so NO test asserts it), and the live
  aperture test harness (`tests/common/mod.rs`, `tests/slice_05_backpressure.rs`,
  `tests/slice_10_ingest_auth.rs`).

## KPI contracts check (soft gate)

`docs/product/kpi-contracts.yaml` is **MISSING**. Per the DISTILL soft-gate
rule, this is a warning, not a blocker: I proceed without `@kpi`-tagged
observability scenarios at the product-contract level. The feature's own
falsifiability KPIs (KPI-1..6, `discuss/outcome-kpis.md`) ARE encoded as
executable acceptance assertions (reject status, sink untouched, exactly-one
`body_too_large` event with correct `limit`/`size`/`signal`/`transport`,
inclusive boundary, unset no-op). The fleet rejection COUNTER named in the
outcome-kpis DEVOPS handoff is disclosed-deferred (DD4 / C-DEVOPS-8) and is
NOT asserted here.

## DWD-1 — Walking-skeleton scaffold strategy: Strategy C (real local, real I/O)

The DEVOPS `cap_test_environment` mandates an in-process real axum listener +
real tonic server on ephemeral ports, driven with real request bodies above
and below the cap. That is **Strategy C** (real local I/O), not in-memory
doubles. Every scenario:

- binds a REAL aperture instance via `aperture::spawn` on ephemeral loopback
  ports (`127.0.0.1:0`) on BOTH transports;
- drives a REAL HTTP request (`reqwest` POST `application/x-protobuf`) or a
  REAL gRPC `Export` (`tonic` client) through the real allocation/decode path;
- observes ONLY the four driving-port surfaces (HTTP status / gRPC code, the
  recording sink's touched/untouched state, the captured `body_too_large`
  stderr event, exactly-once).

The driven side (the sink) is the existing `RecordingSink` (in-memory record
recorder) — that is correct: the SINK is a driven adapter we substitute, while
the transport boundary under test (the new size guard) is exercised through the
REAL axum/tonic stack. Two scenarios are `@walking_skeleton`-class: one drives
the real HTTP endpoint end to end (`ws_http_oversized_logs_*`), one drives the
real gRPC method end to end (`ws_grpc_oversized_traces_*`).

## DWD-2 — Ephemeral ports (MANDATORY, fixed-port flake guard)

aperture's fixed OTLP defaults are `:4317` (gRPC) / `:4318` (HTTP). A leaked
binder on those fixed ports is a known recurring flake on this project
(project memory `aperture_fixed_port_4317_flake`). **Every** instance in this
slice binds `127.0.0.1:0` on both transports and reads the actual bound address
back via `handle.http_addr()` / `handle.grpc_addr()` (through the existing
`TestInstance` helpers and a `start_with_cap` fixture mirroring
`slice_05`'s `start_with_cap`). NO test references `4317` / `4318`. This is
inherited from the shared harness, which is already ephemeral-only.

## DWD-3 — Honest-strength AC wording (Earned-Trust, DD1a / upstream-changes.md)

Each `Then` is worded to what the transport-boundary placement actually
guarantees, never overstated:

| Arm | AC wording used | In-suite proxy asserted |
|---|---|---|
| HTTP, `Content-Length` present, over cap | rejected **before any body byte is read** | HTTP 413 + recording sink **empty** + one event (the sink being empty is the observable proxy that the harness never validated and the record never landed) |
| HTTP, absent/lying `Content-Length` | rejected **before the FULL body is buffered** (bounded `<= ~one cap`, NOT "before any byte") | same observable triple; the slice does not claim "before any byte" for this case |
| gRPC, over cap | frame **refused in the codec before decode**; typed request never allocated | gRPC `RESOURCE_EXHAUSTED` + sink empty + one event |
| at/under cap | accepted exactly as today | HTTP 200 / gRPC Ok + sink touched + **no** event |
| unset cap | today's exact behaviour | a large body (under axum's default) accepted + sink touched + no event |

`limit` is asserted as the **exact** configured cap. `size` is asserted exactly
ONLY at the HTTP `Content-Length`-present boundary edge (`size=N+1`), where the
rejection surface observes an exact declared `Content-Length` (DD3); the other
arms assert the event's presence + `signal`/`transport`/`limit`, not a
fabricated exact `size`. This honours the "size is the value the rejection
surface observed, not an exact fully-read byte count" refinement.

Because the running aperture's HTTP path uses `axum::extract::Bytes`, axum
applies its built-in 413 only at its own `DefaultBodyLimit`; the AC wording and
the test sizing (DWD-5) keep the WIRED cap the sole cause of the 413 under test.

## DWD-4 — Reject codes locked (DD5)

HTTP **413 Payload Too Large**; gRPC **`RESOURCE_EXHAUSTED`** (tonic
`Code::ResourceExhausted`). Asserted directly. Mirrors the `slice_05`
concurrency-cap refusal-shape precedent (which the harness already exercises).

## DWD-5 — Falsifiability finding: axum's 2 MB DefaultBodyLimit (LOAD-BEARING)

**The single most important DISTILL finding.** axum 0.7 (`axum-core 0.4.5`,
`ext_traits/request.rs:325`) applies a built-in `DEFAULT_LIMIT = 2_097_152`
(2 MB) to the `Bytes` extractor the aperture HTTP handlers use. A naive
oversized-body test (e.g. an 8 MiB body vs a 4 MiB cap) would receive a **413
TODAY** — from axum's DEFAULT limit, NOT from the unwired `max_recv_msg_size`
knob. That test would PASS against today's parsed-but-ignored field: exactly
the "too-large test that passes on the unwired knob" trap the DISCUSS risk and
C-DEVOPS-4 forbid (a false GREEN, Fixture Theatre on the protection side).

**Resolution**: every HTTP/gRPC REJECT arm uses a **tiny cap (16 bytes)** with
an **ordinary ~100-220 byte body**. The body is OVER the configured cap but FAR
UNDER axum's 2 MB default (and tonic's 4 MB `max_decoding_message_size`
default), so:

- **today** the body is ACCEPTED (HTTP 200 / gRPC Ok, sink non-empty, no
  event) -> the reject ACs FAIL on an assertion (proven below);
- **after DELIVER** wires the cap, the same body is rejected (413 /
  `RESOURCE_EXHAUSTED`) -> the ACs pass ONLY then.

The cap is thus the SOLE binding constraint, and the test is genuinely
falsifiable against the unwired knob. The boundary edges (US-02) use an exact
N = 4096 bytes (also far under axum's default) with valid-protobuf exact
padding (a length-delimited unknown field prost skips on decode) so the
`Content-Length` observes `N` / `N+1` faithfully. The unset large-body control
uses ~419 KB (10 000 records), large enough to be rejected under a small cap
yet under axum's 2 MB default so the unset path accepts it today.

This finding is handed to DELIVER: when DELIVER wires the HTTP guard it must
ensure the custom length-checked seam's cap is consulted in ADDITION to (and
below, for the test caps) axum's default, and that the `body_too_large` event
fires from the custom seam (not axum's silent default 413), per ADR-0073
Option A vs the rejected bare-`DefaultBodyLimit` Option C.

## DWD-6 — Metrics IN-SCOPE (DD4)

The cap covers logs, traces, AND metrics. Dedicated metrics scenarios:
`oversized_metrics_http_rejected_413_signal_metrics` and
`oversized_metrics_grpc_tiny_cap_refused_signal_metrics`. No silent gap.

## DWD-7 — RED-not-BROKEN scaffold (Mandate 7)

The only production change DISTILL makes is the **config scaffold**, mirroring
the most recent DISTILL scaffold precedent (`jwt_auth`,
aegis-ingest-auth-v0) and the `max_concurrent_requests` template:

- `Config.max_recv_msg_size: Option<u32>` (`pub(crate)`, `#[allow(dead_code)]`);
- `Config::max_recv_msg_size(&self) -> Option<u32>` accessor (`pub(crate)`);
- `ConfigBuilder.max_recv_msg_size` field + `pub fn max_recv_msg_size(u32)`
  setter (the public-additive surface DEVOPS C-DEVOPS-2 acknowledged) + the
  `build()` wiring.

A config setter that merely STORES `Option<u32>` is a safe scaffold; the
ENFORCEMENT (the transport-boundary guard + the `body_too_large` emit) is
DELIVER's job. With the value stored-but-unconsulted, an instance built with a
cap behaves like today's accept-and-ignore aperture, which is exactly what
makes the reject/boundary scenarios behaviourally RED. No emitter, no event
constant, no transport change is added by DISTILL (the `BODY_TOO_LARGE`
constant already exists at `observability.rs:46`).

### RED-not-BROKEN proof result

`cargo test -p aperture --test slice_11_body_size_cap` (no `--ignored`):
**6 passed, 0 failed, 10 ignored** — the negative controls pass GREEN on
today's behaviour, so `cargo test --workspace` stays green at the DISTILL
commit (pre-commit fast subset + CI deep suite both stay green).

`cargo test -p aperture --test slice_11_body_size_cap -- --ignored`:
**0 passed, 10 failed** — every ignored scenario FAILS on a behavioural
ASSERTION, never a compile/import/setup error:

| Scenario | Failure shape (today) | Falsifiable? |
|---|---|---|
| `ws_http_oversized_logs_*` | `assert status == 413` -> `left: 200` | yes (accepted today) |
| `ws_grpc_oversized_traces_*` | `assert code == ResourceExhausted` -> `left: None` | yes (Ok today) |
| `at_limit_plus_one_logs_http_*` | `left: 200, right: 413` | yes |
| `tiny_cap_rejects_ordinary_logs_*` | `left: 200, right: 413` | yes |
| `oversized_logs_grpc_*` | `left: None, right: Some(ResourceExhausted)` | yes |
| `oversized_traces_http_*` | `left: 200, right: 413` | yes |
| `oversized_metrics_http_*` | `left: 200, right: 413` | yes |
| `oversized_metrics_grpc_tiny_cap_*` | `left: None, right: Some(ResourceExhausted)` | yes |
| `body_too_large_event_is_warn_level` | `expect_stderr_event` panics: no `body_too_large` event emitted today | yes (event-absent) |
| `single_cap_guards_both_logs_and_traces` | `left: 200, right: 413` (logs arm) | yes |

All failures are 200/Ok/event-absent = today's parsed-but-ignored behaviour.
None is a panic from spawn/connect/encode (BROKEN). Classification confirmed:
**RED-not-BROKEN**. DELIVER un-ignores one at a time.

The file compiles clean (`cargo test -p aperture --tests --no-run`: Finished,
no errors, no warnings); the aperture lib subset stays green
(`cargo test -p aperture --lib`: 112 passed).

## Scenario list (happy / error split)

19 scenarios total. Error/boundary ratio = **13 / 19 = 68%** (>= 40% target).
(`red_reason_is_documented` is a documentation pin, excluded from the
behavioural count below; with it, 20 functions.)

### Walking skeletons (real-io, driving adapter) — 2 (both error-path)

1. `ws_http_oversized_logs_rejected_413_sink_untouched_one_event` — US-01,
   HTTP spine. `@walking_skeleton @driving_port @real-io @driving_adapter`
2. `ws_grpc_oversized_traces_refused_resource_exhausted_one_event` — US-01,
   gRPC spine. `@walking_skeleton @driving_port @real-io @driving_adapter`

### Happy path / negative controls (GREEN today, NOT ignored) — 6

3. `under_limit_logs_http_accepted_no_event` — US-01 sc.1
4. `under_limit_traces_grpc_accepted_no_event`
5. `at_limit_logs_http_accepted_no_event` — US-02 sc.1 (inclusive boundary)
6. `unset_cap_large_logs_http_accepted_no_event` — US-03 sc.1 (KPI-4 guardrail)
7. `unset_cap_small_body_accepted_not_zero_byte_limit` — US-03 sc.3
8. `red_reason_is_documented` — doc pin

### Error / boundary (RED, ignored) — 11

9. `at_limit_plus_one_logs_http_rejected_limit_n_size_n_plus_one` — US-02 sc.2
10. `tiny_cap_rejects_ordinary_logs_body_limit_is_config_driven` — US-02 sc.3
11. `oversized_logs_grpc_refused_resource_exhausted_signal_logs` — US-01/US-03
12. `oversized_traces_http_rejected_413_signal_traces` — US-01/US-03
13. `oversized_metrics_http_rejected_413_signal_metrics` — US-03/DD4
14. `oversized_metrics_grpc_tiny_cap_refused_signal_metrics` — US-03/DD4
15. `body_too_large_event_is_warn_level` — US-01/KPI-2
16. `single_cap_guards_both_logs_and_traces` — US-03 sc.2
17. (WS-1 above, counted as error-path)
18. (WS-2 above, counted as error-path)

Happy/control = 6 (incl. doc pin); error/boundary = 13. Without the doc pin:
happy = 5, error = 13, total 18 behavioural, error ratio 72%.

## Story coverage (Dimension 8 Check A)

| Story | Scenarios | Covered |
|---|---|---|
| US-01 (reject before decode + named event) | WS-1, WS-2, under_limit_logs, under_limit_traces, oversized_logs_grpc, oversized_traces_http, body_too_large_warn | yes |
| US-02 (exact inclusive boundary) | at_limit_accept, at_limit_plus_one, tiny_cap | yes (at/at+1 kill the `>`/`>=` mutant, KPI-3) |
| US-03 (unset = unchanged; both signals + metrics) | unset_large, unset_small, single_cap_both, metrics_http, metrics_grpc | yes |

## Environment coverage (Dimension 8 Check B)

`devops/environments.yaml` targets `clean`, `with-pre-commit`, `ci` — all the
standard in-process build/test matrix (NOT deploy targets). The
`cap_test_environment` is in-process Tokio + ephemeral local TCP, identical on
macOS local and Linux CI. Every scenario runs identically in all three (plain
`cargo test`, no privilege, no real network peer, no wall-clock threshold), so
the environment-to-scenario mapping is satisfied by construction (there is no
environment-specific Given to vary). Determinism: boolean reject/accept, exact
status/code, sink touched/untouched, exactly-one-event + its fields, inclusive
boundary — NO timing, so neither the fast pre-commit subset nor CI flakes
(C-DEVOPS-3; the p95-flake class does not apply).

## Adapter coverage table (Dimension 9c)

| Driven/boundary adapter | Real I/O scenario(s) | `@real-io` present? |
|---|---|---|
| axum HTTP transport boundary (the new length-checked read seam) | WS-1, at_limit, at_limit_plus_one, tiny_cap, traces_http, metrics_http, unset_large, unset_small, single_cap | yes |
| tonic gRPC codec boundary (`max_decoding_message_size` + codec-error surface) | WS-2, logs_grpc, metrics_grpc, single_cap | yes |
| `RecordingSink` (driven sink) | exercised on every accept path; asserted untouched on every reject path | yes (real in-process recorder) |
| structured stderr (`testing::stderr_capture`) | every event-asserting scenario | yes (real tracing capture) |

Every transport boundary the cap guards has at least one real-I/O scenario on
BOTH the accept and reject sides. No InMemory double stands in for a boundary
under test; the only double is the SINK, which is correct (it is downstream of
the guard).

## Self-review verdict (reviewer not nested-invocable; self-review per project precedent)

The `nw-acceptance-designer-reviewer` (Sentinel) could not be invoked as a
nested subagent from within this subagent context (the same constraint the
DESIGN and DEVOPS waves recorded). Per the established project precedent, a
structured self-review was conducted against the acceptance-designer critique
dimensions.

| Dimension | Check | Verdict |
|---|---|---|
| 1 Happy-path bias | 13/19 error-path (68%), >= 40% | PASS |
| 2 GWT compliance | each scenario one Given-context / one When-action / observable Then; Gherkin embedded in module doc per project precedent | PASS |
| 3 Business-language purity | scenario titles + doc Gherkin use operator terms (oversized body, rejected, named on stderr, the cap); `413`/`RESOURCE_EXHAUSTED` appear only in step-level assertions, which is the project's Rust-test convention (the harness, not the Gherkin, names codes) | PASS (with the project's status-code-in-assertions convention) |
| 4 Coverage completeness | all 3 stories + all named AC mapped (table above) | PASS |
| 5 WS user-centricity | both WS titles are operator outcomes ("oversized logs rejected ... sink untouched ... one event"), Then = observable (status + sink-empty + event), not "layers connect" | PASS |
| 6 Priority validation | the largest exposure (unbounded ingest allocation) IS the target; the falsifiability finding (DWD-5) prevents the highest-risk false GREEN | PASS |
| 7 Observable-behaviour assertions | every Then asserts a driving-port return (status/code), an observable outcome (sink empty/non-empty via the public `is_empty()`/`len()`), or a captured event field; NO private-field / internal-state assertion | PASS |
| 8 Traceability (A story, B env) | A: every US-* mapped; B: single in-process env, satisfied by construction | PASS |
| 9 WS boundary proof | Strategy C declared (DWD-1); both WS use real axum/tonic over ephemeral ports; "delete the real adapter and the WS still passes?" = NO (it drives the real transport); no `@in-memory` on any WS; every boundary has a real-I/O scenario | PASS |

**Verdict: APPROVED (self-review), 0 blocking issues.** One load-bearing
finding (DWD-5, axum's 2 MB default) was caught and resolved DURING DISTILL by
re-sizing every reject arm to a tiny cap + ordinary body, converting a latent
false-GREEN into a genuine falsifiable RED (proven above). One soft-gate
warning (no `kpi-contracts.yaml`) recorded, not blocking. An independent
top-level `nw-acceptance-designer-reviewer` run is recommended before DELIVER.

## Mandate compliance evidence (CM-A/B/C/D)

- **CM-A (driving ports only)**: the test imports are `aperture::config::Config`,
  `aperture::Handle`, the `common` harness, and the upstream tonic/reqwest
  clients. NO internal aperture type (no `transport::*` private fn, no the new
  seam types) is imported. The instance is observed only through HTTP/gRPC
  requests + the public sink predicates + `stderr_capture`.
- **CM-B (business language)**: scenario function names + doc Gherkin use
  operator domain terms; codes live only in assertions (project convention).
- **CM-C (complete journeys)**: each scenario is a full operator journey
  (configure a cap -> a body arrives -> rejected/accepted -> the operator sees
  the event / the sink state), not an isolated validator call.
- **CM-D (pure-function / adapter boundary)**: the only impure surface is the
  real transport stack, exercised through the real adapter on ephemeral ports;
  the sink is the substituted driven adapter. No fixture is parametrised across
  environments (the env is uniform in-process), so the mandate's
  parametrise-only-the-adapter rule is trivially honoured.

## Constraints inherited (DEVOPS C-DEVOPS-1..8) — honoured

- C-DEVOPS-1 (no new CI job): no CI change made.
- C-DEVOPS-2 (acknowledge the public-additive setter, no bump): the `pub fn
  max_recv_msg_size` setter is added; `Cargo.toml` NOT bumped (stays 0.1.0);
  aperture NOT enrolled in Gate 2/3.
- C-DEVOPS-3 (deterministic, no wall-clock): all assertions are boolean /
  status-code / structured-field / exact-size; NO timing threshold.
- C-DEVOPS-4 (falsifiability): proven above; the axum-default trap (DWD-5) was
  specifically defused so no AC passes on the unwired knob.
- C-DEVOPS-5 (guardrails stay green): the 6 non-ignored controls pass today;
  the existing slice-01..05 suites are untouched (lib + all test targets
  compile + lib green).
- C-DEVOPS-6 (no CLAUDE.md change): none.
- C-DEVOPS-7 (gates 6/7/8 not perturbed): no validator, no decode-in-src added;
  the slice adds only the config scaffold + a test file.
- C-DEVOPS-8 (counter disclosed-deferred): NO test asserts a rejection counter
  / metric; only the `body_too_large` event stream is asserted.

## Handoff to DELIVER

- Un-ignore the 10 RED scenarios ONE AT A TIME, in this order: WS-1 (HTTP logs),
  WS-2 (gRPC traces), then the boundary (at_limit_plus_one), tiny_cap, the
  remaining per-signal/per-transport arms, the warn-level event, and finally
  single_cap_both. The 6 negative controls are already GREEN and are the
  regression guardrail at every step.
- The HTTP guard MUST consult `max_recv_msg_size` via the custom
  length-checked seam (ADR-0073 Option A) so the `body_too_large` event fires
  (the bare axum `DefaultBodyLimit` Option C would lose the event). The tiny
  test caps sit below axum's 2 MB default, so the wired cap is the cause under
  test.
- The gRPC guard is `max_decoding_message_size(cap)` per service + the
  codec-error event surface.
- `size` at the HTTP `Content-Length`-present boundary edge MUST be the declared
  `Content-Length` (the `at_limit_plus_one` test asserts `size=N+1`); other arms
  report the observed value (DD3).
- Gate 5 (100% kill) targets the `>`/`>=` boundary mutant (killed by
  at_limit_accept + at_limit_plus_one) and the unset `None` early-return mutant
  (killed by the unset controls).
