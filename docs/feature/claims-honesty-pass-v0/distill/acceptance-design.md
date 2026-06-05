# Acceptance Design — claims-honesty-pass-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Designer**: Quinn (nw-acceptance-designer)
- **Date**: 2026-06-05
- **Mode**: autonomous overnight; no questions returned to the operator.
- **Inputs**: DISCUSS `user-stories.md` (US-01..US-06, verified overstatement
  inventory), `story-map.md`, `wave-decisions.md`; DESIGN
  `brief.md > claims-honesty-pass-v0` (the per-slice "For Acceptance Designer"
  note), ADR-0062 (query_range raw points / `step` reserved); DEVOPS
  `wave-decisions.md` (README-path portability, decision 3) and
  `environments.yaml`.

## What "acceptance test" means for a prose-honesty feature

This feature changes documentation strings, codenames, and a handful of
stale-over-green doc comments — not behaviour (both DESIGN document-vs-implement
flags resolved DOCUMENT). A prose-honesty correction is not conventionally
acceptance-testable through a driving port, so the testable shape (fixed by
DISCUSS, DESIGN, and DEVOPS) is two kinds of guard:

1. **Doc-lint / grep guards** (US-01, US-02, US-03 stale half, US-04 prose,
   US-05 README, US-06 docs): a plain file-read asserts the specific FALSE
   string is **ABSENT** from the target document AND the CORRECTED string is
   **PRESENT**. Because the corrections do not exist yet (DELIVER makes them),
   these guards FAIL today (the false string is still present). They are marked
   `#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]` so
   `cargo test --workspace` stays GREEN at the DISTILL commit. They COMPILE
   (plain file-reads + string checks, no new symbols), so they are
   **RED-not-BROKEN**. DELIVER removes each `#[ignore]` immediately after it
   applies the matching prose correction.

2. **Behaviour tests** (US-04 boundary, US-05 step-invariance, US-06 framing):
   these assert the REAL current behaviour the corrected docs will describe, so
   they **PASS TODAY** and guard against future regression — NOT `#[ignore]`d.

## The nWave-order invariant (read before judging "missing" corrections)

The nWave order is DISCUSS -> DESIGN -> DEVOPS -> **DISTILL** -> DELIVER. At the
DISTILL commit the prose corrections DO NOT EXIST YET — that is the EXPECTED,
CORRECT state. RED `#[ignore]`d doc guards with no DELIVER corrections behind
them is exactly right at DISTILL. A reviewer must NOT reject on the (correct)
absence of the not-yet-written prose edits. The behaviour tests are GREEN
because they pin behaviour that already exists.

## The US-03 bidirectional guard (load-bearing)

The US-03 honesty pass must remove the stale `__SCAFFOLD__`-over-green doc
comments WITHOUT deleting any marker that describes a TRUE current RED state.
So US-03 is expressed in BOTH directions, split across crates by locus:

- **Stale-over-green half** (RED, `#[ignore]`d): the scaffold claim is GONE in
  `query-http-common/src/lib.rs` (its guard:
  `query-http-common/tests/slice_01_claims_honesty_module_doc.rs`) and in
  `trace-query-api/src/lib.rs` (its guard:
  `trace-query-api/tests/slice_04_claims_honesty_handler_doc.rs`). True only
  after DELIVER edits the prose.
- **In-flight half** (GREEN, NOT ignored): the genuinely-RED in-flight
  `__SCAFFOLD__` / `#[ignore]` markers across the workspace REMAIN PRESENT —
  guard `us03_in_flight_scaffold_markers_remain_present` in
  `otlp-conformance-harness/tests/slice_08_claims_honesty_doc_guards.rs`. True
  today and must STAY true, so the prose pass cannot over-reach and silence an
  honest in-flight marker. Each crate-local stale-half guard ALSO carries a
  GREEN guardrail (the per-fn "DELIVER state: implemented" note / the live
  handler body is present today) proving the marker was stale-over-green.

Both directions are expressed so DELIVER cannot over-reach: the stale-over-green
markers must go; the in-flight markers must stay.

## Test file layout and slice numbers (next-free per crate)

Per DEVOPS decision 3, the workspace-root README greps and the cross-crate
US-03 in-flight half are consolidated into ONE dedicated docs-guard file hosted
in `otlp-conformance-harness/tests/` (it already reads workspace files via
`env!("CARGO_MANIFEST_DIR")` + `../../`, the `slice_07` idiom). The crate-LOCAL
guards live in their home crates' `tests/` next to the file each protects.

| Crate | File | Slice # | Kind | RED-ignored / GREEN |
|-------|------|---------|------|---------------------|
| `otlp-conformance-harness` | `tests/slice_08_claims_honesty_doc_guards.rs` | 08 | doc-lint grep (README US-01/US-05, harness US-04/US-06 prose) + US-03 cross-crate in-flight half | 8 RED-`#[ignore]`d + 1 GREEN (in-flight) |
| `otlp-conformance-harness` | `tests/slice_09_claims_honesty_behaviour.rs` | 09 | behaviour (US-04 boundary, US-06 framing inert) | 3 GREEN |
| `codex` | `tests/slice_06_claims_honesty_stub_headers.rs` | 06 | doc-lint grep (US-02) | 3 RED-`#[ignore]`d + 1 GREEN guardrail |
| `query-http-common` | `tests/slice_01_claims_honesty_module_doc.rs` | 01 | doc-lint grep (US-03 stale half) | 1 RED-`#[ignore]`d + 1 GREEN guardrail |
| `trace-query-api` | `tests/slice_04_claims_honesty_handler_doc.rs` | 04 | doc-lint grep (US-03 stale half) | 1 RED-`#[ignore]`d + 1 GREEN guardrail |
| `query-api` | `tests/slice_06_claims_honesty_step_invariance.rs` | 06 | behaviour (US-05 step invariance) | 1 GREEN |

## Walking skeleton — none (correctly)

Per DISCUSS and DESIGN: this is a brownfield documentation sweep with no
end-to-end flow to thread. There is no walking skeleton and no `@walking_skeleton`
scenario. The behaviour tests drive real entry points (the harness `validate_*`
free functions; the `query_api::router` driving port via `oneshot`), but they
pin existing behaviour, not a new vertical slice.

## Mandate compliance (summary; evidence in mandate-compliance.md)

- **CM-A (hexagonal boundary)**: behaviour tests invoke driving ports only —
  `query_api::router` (the single public driving port) and the harness public
  `validate_*` free functions (the crate's public surface). The doc guards are
  file-reads, not component invocations. No internal-component imports.
- **CM-B (business language)**: doc-guard assertions speak in honesty terms
  (false claim ABSENT, corrected claim PRESENT); behaviour tests speak in the
  domain ("a semantically-bogus trace_id is accepted", "two step values return
  identical output"). No HTTP verbs / status codes leak into the scenario
  narrative (status codes appear only as the observable outcome the existing
  query-api suite already uses).
- **CM-C (complete journeys / observable outcomes)**: each behaviour test
  asserts an observable outcome (the returned record, byte-identical responses,
  a decode failure), not internal state.
- **CM-D (pure function extraction)**: no fixture-matrix parametrisation is
  needed — the corrections are pure file content and the behaviour is pure over
  fixed inputs (bytes / a seeded store). No environment variants to parametrise.

## Error / boundary coverage

For a prose-honesty feature the "error paths" are the boundary behaviours the
corrected docs now name honestly: US-04 accepts a semantically-INVALID body
(the structural-decode boundary); US-06 a length-prefixed body FAILS to decode
(the strip-the-prefix boundary); US-05 the omitted-`step` and two-distinct-`step`
cases all collapse to identical output (the not-honoured boundary). Each
behaviour test pins a boundary, not just a happy path.
