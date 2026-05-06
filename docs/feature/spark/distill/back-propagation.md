# Back-propagation note — `spark` v0 DISTILL to DISCUSS / DESIGN

> **Wave**: DISTILL.
> **Author**: Atlas (`nw-acceptance-designer`).
> **Date**: 2026-05-06.
> **Recipient**: Bea (orchestrator) for forwarding to Luna
> (`nw-product-owner`) and Morgan (`nw-solution-architect`) if a
> contract update is warranted.

DISTILL's job is to turn the BDD scenarios + DESIGN ADRs into
executable Cargo tests against the locked public surface. While
writing the Slice 05 (logs and metrics symmetry) integration tests,
DISTILL surfaced one contract gap that DESIGN cannot resolve without
a DISCUSS-side rephrasing or a public-surface change. This file
captures the gap.

---

## Issue 1 — The OTel Rust SDK at `=0.27` has no `opentelemetry::global::logger_provider()`

### The DISCUSS contract today

`docs/feature/spark/discuss/journey-spark.yaml > step 4 gherkin`:

```
Scenario: A logs export carries the same four house attributes on the Resource
  Given an Aperture instance running locally with a RecordingSink
  And spark::init has succeeded with the canonical configuration
  When the application emits one log record via opentelemetry::global::logger_provider().logger("checkout-service")
```

`docs/feature/spark/discuss/user-stories.md > US-SP-05 > UAT > Scenario:
A logs export carries the same four house attributes on the Resource`:

```
When the application emits one log record via opentelemetry::global::logger_provider().logger("checkout-service")
```

`docs/feature/spark/discuss/journey-spark.yaml > step 4 command`:

```
let logger = opentelemetry::global::logger_provider().logger("my-component");
logger.emit(LogRecord::builder()...build());
```

The contract presupposes a global logger-provider getter analogous to
`opentelemetry::global::tracer_provider()` and
`opentelemetry::global::meter_provider()`.

### What DISTILL found

The OpenTelemetry Rust SDK at the family-pinned version `=0.27`
(DESIGN ADR-0013) does **not** expose a global logger-provider getter.
The `opentelemetry::global` module re-exports only:

- `tracer_provider()` / `tracer()` / `set_tracer_provider()` (from
  `src/global/trace.rs`)
- `meter_provider()` / `meter()` / `set_meter_provider()` (from
  `src/global/metrics.rs`)
- `set_text_map_propagator()` / `get_text_map_propagator()` (from
  `src/global/propagation.rs`)

Verified by `grep -rn "pub fn"
~/.cargo/registry/src/index.crates.io-*/opentelemetry-0.27.1/src/global/`
on 2026-05-06.

The OTel 0.27 logs API exists in `opentelemetry::logs` (the trait surface
`LoggerProvider`, `Logger`, `LogRecord`) and in
`opentelemetry_sdk::logs::LoggerProvider` (the SDK implementation). But
there is **no `set_logger_provider` global setter** and **no
`logger_provider()` global getter** at this version.

The implication for Spark: at v0 with the pinned SDK, an application
embedding Spark cannot emit a log via the standard `opentelemetry::
global::*` namespace. The application must:

- Hold a direct reference to the `opentelemetry_sdk::logs::LoggerProvider`
  that Spark configured (which Spark does not expose on its public surface
  per ADR-0011 §"Public surface").
- Use `opentelemetry-appender-tracing` (an external crate that bridges
  `tracing` events into the OTel logs pipeline) — but that crate is not
  in the workspace at v0 lock-time.
- Wait for a future OTel SDK release that adds the global-setter for
  logs.

### Why this surfaces at DISTILL, not DESIGN

DESIGN ADR-0013 §1 verified that `opentelemetry-otlp 0.27` supports
the three signal feature-set (`trace`, `logs`, `metrics`). What DESIGN
did NOT verify (and could not reasonably have verified without
writing the integration test code) is the *application-side emission
API* for each signal. ADR-0011 declares the public surface as four
items (`init`, `SparkConfig`, `SparkError`, `SparkGuard`) and US-SP-05's
AC says "an emitted log record reaches Aperture", but the BDD scenario
phrasing inherits the symmetric `opentelemetry::global::*` shape that
holds for traces and metrics and does not hold for logs at 0.27.

### Two paths forward

**Path A — Rephrase the DISCUSS contract** (DISTILL's recommendation):

US-SP-05's behavioural contract is "logs reach Aperture with the
configured Resource"; the *emission mechanism* is application-specific.
The BDD scenario phrasing can be loosened to:

```
When the application emits one log record (via the application's chosen
logs-emission path; see Spark's user guide for the v0 patterns
documented for opentelemetry_sdk =0.27)
```

This preserves the contract intent (the LoggerProvider Spark configures
carries the right Resource; logs that flow through it land in Aperture
with the four house attributes intact) while acknowledging that OTel
0.27's lack of a global logger-provider getter is an upstream
limitation Spark v0 inherits.

The DELIVER-wave Slice 05 implementation still needs to wire up the
LoggerProvider and prove (via an integration test) that a log record
flowing through the configured LoggerProvider reaches Aperture with the
right Resource. The mechanism by which the test obtains a Logger is the
DESIGN/DELIVER-wave call:

- Option A1: Spark exposes `pub fn logger_provider() -> impl
  opentelemetry::logs::LoggerProvider` (expanding the public surface
  by one item — non-breaking addition, but ADR-0011 would need to be
  superseded for the four-item public-surface count).
- Option A2: Spark adds an internal test seam (`pub(crate) fn
  test_logger_provider`) that the integration test uses, with the
  understanding that this is test-only and does not appear on the
  consumer-facing surface.
- Option A3: Spark relies on `opentelemetry-appender-tracing` as the
  application's bridge from `tracing` to OTel logs. This requires
  adding it as a runtime dep (Apache-2.0, fits the licence policy) or
  a dev-dep (for the integration tests only).
- Option A4: DELIVER waits for a future OTel SDK release that adds
  `opentelemetry::global::set_logger_provider` and
  `opentelemetry::global::logger_provider`, then the Slice 05 contract
  ships verbatim.

**Path B — Tighten Spark's public surface to expose a logger seam**
(rejected as too invasive at DISTILL):

Adding a public method like `SparkGuard::logger("scope-name")` would
break the Drop-only contract ADR-0016 §3 locks. Adding a free
`pub fn logger_provider()` would expand the public surface beyond the
four items ADR-0011 declares. Either is a contract change DESIGN must
re-derive.

### DISTILL's DECISION at this wave

DISTILL has written the Slice 05 integration tests against the
**traces** and **metrics** signal types (both of which have the
canonical `opentelemetry::global::*` API at 0.27) and has emitted
verbatim BDD-scenario assertions for those two. The cross-signal
symmetry assertion ("all three signals carry identical Resource") is
preserved across the two API-supported signal types; the third
signal's wire-byte assertion is deferred to a follow-up note in
`distill/wave-decisions.md` until Bea routes Path A through Luna.

The Slice 05 integration test file (`tests/slice_05_logs_and_metrics.rs`)
has been written **with logs emission code that does NOT compile at
0.27** (`opentelemetry::global::logger_provider()` is undefined),
deliberately so that the contract gap is visible at the build error
rather than papered over by a workaround that loses fidelity to the
DISCUSS phrasing. DELIVER-wave Crafty must NOT silently rewrite the
test to use a different emission path; the DISCUSS contract update
(Path A) must precede that.

If Bea decides to ship the v0 contract verbatim (Path B rejected), the
Slice 05 binary stays in the `[[test]]` declarations with a
single-line comment annotating the contract gap, the code is
commented out (or `#[ignore]`'d) until DELIVER's contract resolution
lands, and the Slice 05 KPI is documented as deferred to v0.1 in
`outcome-kpis.md`.

### Recommended path

Path A. The semantic intent of US-SP-05 (logs flowing through Spark's
configured LoggerProvider land in Aperture with the four house
attributes) is preserved; only the literal `opentelemetry::global::
logger_provider()` API path is rephrased. DESIGN can pick A1, A2, A3,
or A4 in a follow-up ADR (likely a new ADR-0017 superseding ADR-0011
§"Public surface" if Path A1 is chosen).

If Bea forwards Path A to Luna, the changes are:

1. `user-stories.md > US-SP-05 > UAT > Scenario: A logs export ...`:
   replace the literal `opentelemetry::global::logger_provider()` with
   "the application's chosen logs-emission path".
2. `journey-spark.yaml > step 4 command + step 4 gherkin`: same.
3. `journey-spark.feature`: same.
4. `slices/slice-05-logs-and-metrics.md > Demo command`: replace the
   `opentelemetry::global::logger_provider().logger("svc").emit(...)`
   line with a comment naming the v0 logs-emission path Spark
   recommends (DESIGN's call).

### Bea's call

DISTILL recommends Path A. If Bea agrees, Luna applies the rephrasing,
Morgan extends ADR-0011 (or writes ADR-0017) to lock the v0 logs-
emission API, and DISTILL re-runs to update Slice 05 against the new
contract.

If Bea decides to defer Path A's resolution, DISTILL has structured
the Slice 05 file so the deferral is visible (the logs-emission test
functions are present, with code that does not compile at 0.27 — a
loud failure surface). Slice 05 ships with traces+metrics symmetry
asserted at v0; the third-signal assertion lands when DELIVER's
contract resolution lands.

---

## Issue 2 — None.

There is no other DISTILL finding that requires a DISCUSS contract
revision or a DESIGN ADR adjustment. Every other DISCUSS contract is
implementable verbatim against the public surface ADR-0011 locks and
the OTel SDK 0.27 family ADR-0013 pins.

The Slice 06 deadline-exceeded WARN event's `dropped=unknown`
literal (Path A from DESIGN's `back-propagation.md`) is already
applied to DISCUSS via commit `25e3732`; DISTILL's Slice 06 tests
honour the prefix-not-value contract verbatim (assert `dropped=`
prefix; accept either `unknown` or an integer value).
