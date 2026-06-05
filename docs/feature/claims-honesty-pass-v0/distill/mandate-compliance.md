# Mandate Compliance — claims-honesty-pass-v0 (DISTILL)

Evidence that the four acceptance-test design mandates pass, for handoff to
DELIVER (`nw-software-crafter`).

## CM-A — Hexagonal Boundary Enforcement

Behaviour tests invoke DRIVING PORTS / the crate's PUBLIC surface only; doc
guards are file-reads, not component invocations. Zero internal-component
imports.

**`harness/tests/slice_09` (US-04 / US-06 behaviour) imports:**

```
use otlp_conformance_harness::{validate_logs, validate_traces, Framing,
    OtlpViolation, Rule, WireTypeRule};
```

`validate_logs` / `validate_traces` are the crate's three public free functions
(the locked public surface per ADR-0001). `Framing`, `OtlpViolation`, `Rule`,
`WireTypeRule` are public re-exports. The proto types
(`opentelemetry_proto::...`) are upstream fixture builders, not internal
harness components. NO `crate::validate::*` / `crate::decode::*` internal-module
import.

**`query-api/tests/slice_06` (US-05 behaviour) entry point:**

```
query_api::router(store, Some(tenant), None)   // the single public driving port
```

Driven through `tower::ServiceExt::oneshot` against the axum `Router` — the
exact driving port the slice_01..05 suites use. NO import of `selector`,
`matrix`, or any internal module.

**Doc guards** (`slice_08`, codex `slice_06`, query-http-common `slice_01`,
trace-query-api `slice_04`): plain `std::fs::read_to_string` over workspace
files. They assert document CONTENT, not component behaviour — there is no
boundary to cross because there is no component under test (prose honesty).

Verdict: **CM-A PASS.**

## CM-B — Business Language Abstraction

The corrections speak in HONESTY terms (false claim ABSENT, corrected claim
PRESENT) and the behaviour tests speak in the OTLP / metrics-read domain.

Technical-jargon grep over the guard narratives
(`status_code|http 200|404|database|REST endpoint`, case-insensitive):

```
harness/tests/slice_08 : 0
codex/tests/slice_06   : 0
```

The behaviour tests legitimately use `StatusCode::OK` as the OBSERVABLE outcome
(the same way the existing query-api acceptance suite does — it is the user-
visible result of the read endpoint, the established domain vocabulary for this
crate), and `Rule::WireType(ProtobufDecode)` as the named domain outcome of a
decode failure. These are domain outcomes, not transport mechanics leaking into
the narrative. Scenario names are user/evaluator-centric:
"structurally_valid_semantically_bogus_trace_id_is_accepted",
"step_is_not_honoured_two_step_values_and_omitted_step_return_identical_output".

Verdict: **CM-B PASS.**

## CM-C — User Journey Completeness / Observable Outcomes

Every behaviour-test assertion checks an OBSERVABLE outcome (a returned value or
a user-visible response), never internal state, private fields, or call counts:

| Test | Assertion | Observable? |
|---|---|---|
| US-04 bogus trace_id | `result.is_ok()`; `span.trace_id.len() == 4` (the returned record) | YES — return value of the public function |
| US-06 both framings | both `is_ok()`; `http_record.encode_to_vec() == grpc_record.encode_to_vec()` | YES — returned records |
| US-06 length-prefixed | `violation.rule == Rule::WireType(ProtobufDecode)` | YES — returned violation |
| US-05 invariance | `body15 == body60 == body_none` (the HTTP response bodies) | YES — driving-port response |

The doc guards assert observable document content (a reader reads the corrected
claim). No `assert mock.called`, no internal-DB-state assertion, no private-
field probe.

The journeys map to the JTBD: "When I read what Kaleidoscope claims … the claim
matches what the code actually does." Each guard is one claim Devin (the
evaluator persona) reads; each behaviour test is one boundary Devin probes and
finds the doc predicted.

Verdict: **CM-C PASS.**

## CM-D — Pure Function Extraction Before Fixtures

No fixture-matrix parametrisation exists in this feature, so the mandate is
satisfied trivially-but-correctly:

- The corrections are pure file content, asserted by reading the file (no
  environment setup).
- The harness behaviour is PURE over fixed bytes (`validate_*` are free
  functions `&[u8] -> Result<_, _>`); fixtures are built in-process with
  `prost::encode_to_vec`. No I/O, no mocks, no environment.
- The query-api behaviour drives the existing public `router` over a real
  `FileBackedMetricStore` in a tempdir — the SAME adapter the existing
  acceptance suite uses. It is NOT parametrised across environments because the
  `step`-invariance behaviour is environment-independent.

No business logic needed extraction because the behaviours already live behind
the existing public free functions / driving port. There is no `@pytest.fixture
(params=[...])`-style environment sweep to justify.

Verdict: **CM-D PASS (no fixture-matrix introduced; minimal real-I/O touch is
the established adapter, unparametrised).**

## Summary

| Mandate | Verdict | Evidence |
|---|---|---|
| CM-A Hexagonal boundary | PASS | behaviour tests import public driving ports / public surface only; guards are file-reads |
| CM-B Business language | PASS | 0 jargon in guard narratives; domain outcomes only |
| CM-C Observable outcomes | PASS | every Then asserts a return value / response, never internal state |
| CM-D Pure-function / fixtures | PASS | no environment fixture matrix; pure-over-fixed-input behaviour |
