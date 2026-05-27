# Story Map — query-http-common-v0

## User

The Kaleidoscope read-side maintainer (Andrea or a crafter agent) whose job
is to change a read-side HTTP scaffolding element (cap value, parser, error
envelope, fail-closed tenancy) and trust the change to propagate to the
three pillar read APIs.

## Goal

Live in a workspace where the read-side HTTP scaffold has a single source of
truth, so future scaffolding changes are one-file edits rather than
three-place lockstep edits.

## Feature type

Cross-cutting refactor (Decision 1). Brownfield, no walking skeleton in the
greenfield sense (Decision 2). Lightweight research depth (Decision 3). No
JTBD analysis (Decision 4); maintainer's job is named inline.

## Backbone

The activities are temporal and dependency-ordered. Each column is one
activity; the rows below are the carpaccio slices.

| Activity 1: Extract            | Activity 2: Rewire                | Activity 3: Verify                  |
|--------------------------------|-----------------------------------|-------------------------------------|
| US-01 Cap constants            | US-02 Time-range parser           | US-05 Workspace integration gate    |
| US-03 Error envelope helper    | US-04 Fail-closed tenant pattern  |                                     |

Reading: Activity 1 establishes the new crate and the simplest possible
shared surface (constants, error envelope helper). Activity 2 moves the two
parser-related and seam-related extractions across, each of which depends on
the new crate already existing. Activity 3 closes the loop with the
integration assertion and the mutation gate.

## Walking skeleton

US-01 is the walking skeleton. It is the thinnest end-to-end slice that
proves the extraction model works: a new workspace crate exists, the three
consumer crates compile against it, the workspace test suite stays green,
and the read-side maintainer can change a constant in one place. Everything
that follows (parser, envelope, tenancy, integration gate) is the same
manoeuvre with a slightly larger surface; if US-01 cannot land cleanly,
none of the others can.

The decision to make US-01 the skeleton (rather than US-03, which is also
trivially scoped) is that the cap constants are PURE data with no behaviour;
verifying byte-identical wire behaviour on US-01 is a non-issue because the
constants do not appear on the wire directly, only as boundary values in
already-tested arms. This makes US-01 the lowest-risk first slice and the
cleanest proof that the new crate compiles and is consumed correctly.

## Carpaccio slices

| # | Story | Slice goal                                                           | IN scope                                                                                                       | OUT scope                                                                  | Learning hypothesis                                                                                       | Effort |
|---|-------|----------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------|--------|
| 1 | US-01 | New crate exists; cap constants extracted; three consumers re-export | New crate + `Cargo.toml` + workspace members entry; constants only; three `pub use ...` lines in consumers     | Parser, envelope, tenancy, ADR-0054, mutation gate setup                   | Disproves the "new workspace crate sets up cleanly with three consumers" assumption if it fails           | ≤ 1 day |
| 2 | US-03 | Error envelope helper extracted; byte-identical responses             | `error_response` in new crate; three consumer crates drop their private copies; acceptance suite stays green   | Parser, tenancy, mutation gate setup                                       | Disproves "the envelope shape is truly identical across the three crates" if any 400/401 diverges on byte | ≤ 1 day |
| 3 | US-02 | Time-range parser extracted; consumer sites wrap as needed             | `parse_time_range_seconds` in new crate with `Option<&str>` shape; consumer adaptations; inline tests migrate  | Tenancy, mutation gate setup, removing `seconds_to_nanos` (stays per-crate) | Disproves "the parser is truly identical across the three" if any inline test diverges                    | ≤ 1 day |
| 4 | US-04 | Fail-closed tenant pattern extracted; four call sites rewired         | `resolve_tenant_or_refuse` in new crate; four handler arms call it; 401 bodies byte-identical                  | Parser, envelope, mutation gate setup                                      | Disproves "the inline `match` block was truly identical up to the pillar label" if any 401 diverges       | ≤ 1 day |
| 5 | US-05 | Integration gate: workspace green, mutation 100%, LOC ≤ 30           | Run full workspace test; run `cargo mutants -p query-http-common`; run LOC counter; write closure note         | Any further extraction; documentation updates beyond ADR-0054              | Disproves "the four extractions compose without regression" if any of the three gates fails               | ≤ 1 day |

Each slice ships end-to-end (the workspace compiles and `cargo test
--workspace` is green after each slice). Each slice has a named learning
hypothesis of the form "disproves X if it fails". The data is real (the
actual `query-api`, `log-query-api`, `trace-query-api` source). There is a
dogfood moment per slice: after each slice, the maintainer can already use
the partial shared surface (e.g. after slice 1, the constants are
single-sourced).

## Slice taste tests

| Test                                                                     | Result | Comment                                                                                                                      |
|--------------------------------------------------------------------------|--------|------------------------------------------------------------------------------------------------------------------------------|
| Does any slice list "ship 4+ new components"?                            | PASS   | Each slice ships at most one shared API surface element (constants, envelope, parser, tenancy) plus the rewirings.           |
| Does every slice depend on a new abstraction (the new crate)?            | NOTED  | Slice 1 SHIPS the new crate (with constants); slices 2-4 then depend on it. This is the correct order.                       |
| Does any slice disprove a pre-commitment?                                | PASS   | Each slice has its own "disproves X if it fails" hypothesis; slice 3 (parser) is the most likely to surface a hidden divergence. |
| Does any slice use only synthetic data?                                  | PASS   | All slices verify against the existing acceptance suite (real data flows through the three crates).                          |
| Are any two slices identical except for scale?                           | PASS   | The four extraction slices are STRUCTURALLY similar but each extracts a different surface; merging would defeat thinness.    |

## Priority rationale

Priority is dependency-driven first, then outcome-impact-driven. Slice 1
ships the new crate; nothing else can land until that exists. Slices 2-4
have NO dependency on each other and can in principle run in parallel; they
are ordered by simplicity-first (envelope is the simplest, parser is the
most error-prone because of the signature mismatch, tenancy is the most
behaviourally subtle because of the pillar-label suffix). Slice 5 must run
last because it gates on the composition of slices 1-4.

| Priority | Slice | Why this order                                                                                          |
|----------|-------|---------------------------------------------------------------------------------------------------------|
| 1        | US-01 | Walking skeleton: ships the new crate. Nothing else can land until this does.                            |
| 2        | US-03 | Simplest surface (`error_response`). Lowest risk after the skeleton. Builds maintainer confidence.       |
| 3        | US-02 | Parser extraction. Hidden divergence risk on the signature (`&str` vs `Option<&str>`); does it second so any rework lands before the more subtle tenancy slice. |
| 4        | US-04 | Tenancy extraction. Pillar-label suffix is the most behaviourally subtle byte-identity property.        |
| 5        | US-05 | Integration gate. Must run after slices 1-4. The mutation gate is the final acceptance.                  |

The walking skeleton tie-breaker (per `nw-user-story-mapping`) favours
US-01 directly: it is the slice that proves the end-to-end manoeuvre
(workspace member + consumer rewiring) works at all.

## Scope assessment

PASS: 5 user stories, 1 bounded context (the read-side HTTP scaffolding
across `query-api`, `log-query-api`, `trace-query-api`), estimated 5 days
(one per slice, no parallelism assumed). Below the elephant-carpaccio
oversized signals (≤ 10 stories, ≤ 3 contexts, ≤ 5 integration points, ≤ 2
weeks). Right-sized.

## Release map

There is one release (the feature itself). All five slices ship together
under the `query-http-common-v0` feature umbrella; nothing in slices 1-4 is
independently releasable as user value because every slice is
`@infrastructure`. The maintainer's job-to-be-done is only fully enabled
after US-05 closes (because the maintainer's mental model "the scaffolding
lives in one place" is only true after all four extractions and the
integration gate have landed).

## Connection to outcome KPIs

| Slice  | Outcome KPI moved                                                                                  |
|--------|----------------------------------------------------------------------------------------------------|
| US-01  | K3 (scaffolding LOC; first reduction, from caps); contributes to K1 (test regressions = zero)      |
| US-03  | K3 (further LOC reduction); K2 (byte-identical bodies on 400/401)                                  |
| US-02  | K3 (further LOC reduction); K1 (no test regressions)                                               |
| US-04  | K3 (further LOC reduction); K2 (byte-identical bodies on 401)                                      |
| US-05  | K1, K2, K3, K4 (mutation kill rate ≥ 100%); the integration assertion                              |

See `outcome-kpis.md` for the KPI definitions.
