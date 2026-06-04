# ADR-0062 — query_range at v0 returns raw in-window points; `step` is reserved, not a Prometheus stepped grid

- **Status**: Accepted
- **Date**: 2026-06-05
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `claims-honesty-pass-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0042 (the `query-api` contract and PromQL subset; this ADR
  records the SCOPE of the `step` parameter that ADR-0042 pinned into the
  contract, NOT modified). ADR-0050 (read-side caps; the same read endpoint,
  NOT modified). ADR-0044/ADR-0046 (label-matcher grammar; same endpoint, NOT
  modified).

## Context

`GET /api/v1/query_range` (`crates/query-api/src/lib.rs`) accepts the four
Prometheus query_range parameters — `query`, `start`, `end`, `step`. Three are
honoured. The fourth, `step`, is deserialised and then deliberately not acted
on (`lib.rs:143-146`, `#[allow(dead_code)]`): the endpoint returns the raw
native-timestamp points stored in Pulse over `[start, end]`, NOT a grid
re-sampled at `step` resolution.

The in-code field doc is **already honest** about this: `lib.rs:136-137` reads
"`step` is accepted and ignored at v0 (DD5: raw points, no re-stepping)". The
residual overstatement is one word of prose elsewhere — the `README.md:106`
framing "a Prometheus-compatible `/api/v1/query_range` HTTP endpoint" — which a
reader fairly reads as the full Prometheus contract, including `step`-driven
grid re-sampling and staleness semantics. A black-box verifier (two `step`
values against the same window → byte-identical output) exposes the gap.

This is a `claims-honesty-pass-v0` feature: the thesis is structural honesty
against overstatement. The DISCUSS wave flagged this as DESIGN flag #1 — a
genuine document-vs-implement choice — and the resolution is a SCOPE statement
about what `query_range` is at v0. A scope boundary that a future implementer
and the Prism frontend will both rely on deserves an immutable record; hence
this ADR rather than a wave-note alone.

## Decision

**v0 `GET /api/v1/query_range` returns the raw in-window stored points; `step`
is a reserved, accepted-but-not-honoured parameter, not a Prometheus stepped
grid. We DOCUMENT this scope; we do NOT implement grid re-sampling in this
feature.**

Concretely:

1. The `README.md` framing is corrected so it no longer implies a full
   Prometheus stepped-grid contract. The endpoint is described as serving raw
   stored points over the window, with `step` accepted for request-shape
   compatibility but not yet honoured at v0 (no grid re-sampling). The
   already-honest in-code field doc (`lib.rs:136-137`) is the canonical
   wording the README is aligned TO; the README is not invented fresh.
2. No production behaviour changes. `handle_query_range` keeps returning raw
   points; `step` keeps being deserialised-and-ignored.
3. The behavioural contract the verifier (and DISTILL) asserts is the
   **invariance** contract: for a fixed `query`/`start`/`end`, two distinct
   `step` values (e.g. `15s` and `60s`), and the omitted-`step` case, all
   return byte-identical output. This pins the documented "not honoured"
   boundary as an observable, regression-protected fact.

A future stepped-grid implementation is a real feature (re-sampling +
last-value/staleness semantics + grid alignment) and gets its own feature,
ADR, and per-feature mutation obligation. `step` is reserved for it.

## Alternatives Considered

### Alternative A — Implement the Prometheus stepped grid now (the "honour" option)

Make `step` honoured: re-sample the in-window points onto a grid of
`start, start+step, …, end`, applying Prometheus' last-sample-before-step plus
staleness semantics. The verifier's two-`step`-values black-box would then
assert DIFFERENT, correctly-stepped output.

- **Rejected because**: it is a genuine feature, not a cheap honesty fix, and
  not low-risk. Prometheus' stepping carries subtle staleness, alignment, and
  lookback-delta rules that are easy to get half-right; a half-right grid is a
  WORSE honesty outcome than honestly-raw points. It introduces a new code path
  with its own acceptance criteria and the per-feature 100% mutation kill
  obligation (CLAUDE.md / ADR-0005 Gate 5), inflating a documentation sweep into
  a behavioural feature. Smuggling a real capability into a prose-honesty pass
  violates the feature's scope and the project's "one honest claim at a time"
  carpaccio.

### Alternative B — Leave the README as-is and rely on the in-code field doc

The field doc is already honest; argue the README "Prometheus-compatible" is
close enough.

- **Rejected because**: the README is the loudest surface and the first one an
  evaluator reads; the per-crate honest doc is buried. For an honesty-thesis
  project, leaving the loudest claim overstated while the quiet claim is honest
  is precisely the failure mode this feature exists to remove. "Document" is not
  optional polish here — it is the deliverable.

## Consequences

- **Positive**: the loudest read-side claim becomes honest; an evaluator wiring
  Prometheus/Grafana tooling forms a correct expectation of `step` BEFORE
  querying and is never silently handed un-stepped data under a stepped-grid
  promise. The scope of `query_range` at v0 is recorded immutably, so a future
  stepped-grid feature is a deliberate extension, not a surprise. Zero
  production-behaviour risk; nothing to mutate in this slice.
- **Negative**: the endpoint is no longer brandable as fully "Prometheus
  query_range compatible" until the grid lands; this is a true statement we now
  make plainly. The invariance contract (two `step` values → identical output)
  becomes a regression net that a FUTURE stepped-grid feature will intentionally
  break — DISTILL/DELIVER for that future feature must retire this assertion,
  which this ADR flags explicitly so the broken test is understood as planned,
  not a regression.
- **For the verifier / acceptance designer**: the observable is
  invariance-under-`step`, not difference-under-`step`. Assert identical
  response bytes for two `step` values and the omitted case over one fixed
  window, plus a doc-guard that `README.md` no longer implies a stepped grid and
  states `step` is accepted-but-not-honoured at v0.
