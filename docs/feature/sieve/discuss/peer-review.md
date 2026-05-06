# Peer review — Sieve v0 DISCUSS

- **Date**: 2026-05-06
- **Reviewer**: `@nw-product-owner-reviewer` (Sentinel)
- **Wave**: DISCUSS (Luna's tightened pass + Bea's recovery for the
  remaining seven artefacts; the recovery is documented in commit
  `917c6fd`)
- **Artefact set**: `docs/feature/sieve/discuss/` plus six slice
  briefs at `docs/feature/sieve/slices/`
- **Verdict**: **APPROVED** — handoff to DESIGN after Bea closes the
  two clarifications below
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required for approval
- **Two design clarifications flagged**: locked by Bea inline before
  Morgan starts (see "Bea's resolution" sections below)

---

## Executive summary

The Sieve v0 DISCUSS wave is comprehensively executed and ready for
handoff to DESIGN. All eight scope decisions are locked with rationale
and rejected alternatives. All six user stories carry mandatory
Elevator Pitches whose "After" lines name real entry points
(`cargo test -p sieve --test slice_NN_*`) and concrete observable
output. Domain examples uniformly use realistic data (no
`user123`-style placeholders). Six elephant-carpaccio slices each
≤1 day with named learning hypotheses; carpaccio taste tests pass.
Six outcome KPIs are CI-enforced. DoR validated on all nine items.
Zero LeanUX antipatterns. Zero blocking issues.

---

## High-priority findings (clarifications, not blockers)

### Finding 1 — Q8's periodic summary scope is ambiguous across artefacts

**Issue**: `wave-decisions.md > Q8` says "INFO-level summary every
minute (or on flush)". `journey-sieve.yaml` says "INFO summary every
minute". `user-stories.md > US-SI-06` says "Periodic INFO summary
event with target='sieve' carries kept, dropped, and rate fields".
But `slices/slice-06-observability.md > Out of scope` says "Aggregate
sample-rate metrics (a counter of kept/dropped per minute). Useful for
v1; the tracing events are enough for v0 diagnostics."

Three artefacts say the summary is v0; one slice brief says it is v1.

**Risk**: DESIGN cannot confidently pick the slice 06 implementation
strategy until the contradiction is resolved.

**Bea's resolution** (per the "decide and proceed" rule, applied
inline at this commit): the summary is **v0**. Without it, operators
have no default-verbosity visibility and would have to set
`RUST_LOG=sieve=debug` to see anything, defeating the observability
contract. Lock the tick interval at **60 seconds**. The slice 06 brief
is updated to move the summary from "out of scope" to "in scope" with
the locked interval. Wave-decisions Q8 is tightened to remove the
"or on flush" alternative.

### Finding 2 — KPI 5's tick interval is not locked

**Issue**: `outcome-kpis.md > KPI 5` says "100% of summary windows
emit exactly one INFO event" but does not state the tick interval. A
CI test for KPI 5 needs a deterministic interval to pass non-flakily.

**Risk**: under low test traffic the interval could starve and the
CI assertion times out.

**Bea's resolution** (same commit): the interval is 60 seconds in
production; the test infrastructure parameterises it down to a smaller
value (e.g. 100 ms) so the assertion can fire within a test wall-clock
budget. The KPI vocabulary stays "summary window" (interval-agnostic);
the production default is 60 seconds (locked in Q8); the test override
is implementation detail and lives at DESIGN.

---

## Per-artefact findings

### `wave-decisions.md` — APPROVED

`praise:` Eight decisions locked with rigorous rationale and a clearly
named rejected alternative each. Q7 (xxh3_64) and Q8 (DEBUG/INFO
verbosity) are particularly well-argued — the rejected alternatives
(SipHasher, INFO-per-trace) are not strawmen; they are the choices a
less-disciplined wave would make and the rationale for the rejection
is operationally grounded.

The "Out of scope for v0" trailer is exhaustive and matches the
deferrals named throughout the artefact set. Tail-sampling, PII-
scrubbing, per-tenant rates, dynamic reload — all explicitly v1+.

### `user-stories.md` — APPROVED

`praise:` Every Elevator Pitch's "After" line names a real entry
point (a `cargo test` invocation) and a concrete observable outcome
(a typed Rust enum value, or a tracing event vocabulary). This is
the gold standard. An engineer reading any of the six "After" lines
knows exactly what "done" looks like.

Domain examples are uniformly realistic: `payments-api`,
`checkout-service`, `acme-prod`, `inventory-service`, deterministic
fixture trace IDs. No `user123`-style placeholders. Real `status.code`
+ realistic error messages ("downstream timeout"). This catches
integration bugs that placeholder data hides.

BDD scenarios: 2-4 per story; covering happy + edge + error paths.
Acceptance criteria derive from BDD, are outcome-focused, and never
prescribe implementation (no "Use xxh3_64", no "Implement using
Hasher trait"). Technical Notes correctly flag DESIGN-wave decisions
each story leaves to Morgan (exact `Sampler::sample` signature,
`Signal` enum vs separate methods, summary tick mechanism).

### `story-map.md` — APPROVED

`praise:` Walking skeleton (slice 01) is genuinely the smallest
end-to-end. Slices 02-04 add capability linearly without reshaping
the trait. Slice 05 is correctly placed after the trait stabilises.
Slice 06 depends on the outcomes the prior slices produce. The
dependency chain is acyclic and respects learning leverage.

Carpaccio taste tests (the six checks) all pass with explicit
evidence cited per check. No slice ships 4+ new components, no two
slices are identical-except-for-scale, no slice runs only on
synthetic data.

### `outcome-kpis.md` — APPROVED with Finding 2 clarification

Six KPIs, each numeric, CI-enforced, with measurement mechanism named.
KPI 6 (test wall time + mutation kill rate) ties explicitly to
ADR-0005, showing the wave author understands the project's quality
standards. Finding 2 is a minor clarification; not a blocker.

### `dor-validation.md` — APPROVED

Nine items pass with explicit evidence. Wave-level checks pass (eight
decisions, walking skeleton, carpaccio, antipatterns absent, sizing
sanity).

### `journey-sieve.yaml` — APPROVED

Mental model coherent: Riley's beliefs hang together (volume control,
error retention, rate as the knob, signal passthrough, decision
visibility). Emotional arc rises monotonically (concerned →
calibrated → observing → confident → trustful) with confidence built
on visible evidence. Shared artefacts and error paths are documented.

### `shared-artifacts-registry.md` — APPROVED

All artefacts registered with source-of-truth, consumers, integration
risk, validation. Four high-risk, two medium-risk, four low-risk.
CI invariant table at the bottom names per-invariant owner and
mechanism.

### Six slice briefs — APPROVED with Finding 1 clarification on slice 06

Each brief: outcome added, what it lights up, demo command,
acceptance summary, complexity drivers, out of scope. ≤100 lines
each. Demo commands are reproducible. Slice 06's "out of scope"
section needed Bea's clarification (Finding 1 above).

---

## Antipattern scan

Zero antipatterns detected:

- No "Implement-X" stories. All names are user outcomes.
- No generic placeholder data. Every example uses realistic service
  names and field values.
- No technical AC. Every AC is outcome-focused.
- No giant stories. Every story is 1-2 days, 2-4 BDD scenarios.
- No tests-after-code. BDD UAT is in every story before any
  implementation.
- No vague personas. Riley (SRE) and Sasha (platform engineer) are
  named with role and context.
- No missing edge cases. Every story covers happy + edge + error.

---

## Praise

`praise:` The Elevator Pitch discipline is the single most disciplined
piece of nWave-grade DISCUSS I have reviewed for Kaleidoscope. Every
Pitch ties a real test invocation to a concrete decision the user
makes. This is the standard the prior three features have been
approaching; Sieve v0 hits it across all six stories.

`praise:` The eight scope decisions in `wave-decisions.md` show a
team that has thought through trade-offs honestly. Q7's choice of
xxh3_64 cites the OTel collector's TailSamplingProcessor for interop
expectations; Q8's choice of DEBUG-per-trace + INFO-summary is
operationally argued (operators on default verbosity should not be
flooded; operators investigating should be able to opt in). These are
the kinds of decisions an experienced operator would make.

`praise:` The carpaccio discipline is exemplary. Slice 01 is the
smallest unit of value; slices 02-04 cascade linearly; slice 05 lands
after the trait stabilises. No slice reshapes a prior slice's
contract. This is the discipline that prevents DELIVER from spending
half its time fixing slice-N-1 to accommodate slice-N.

`praise:` The recovery from Luna's stalls is honest and well-scoped.
The commit message at `917c6fd` documents which artefacts came from
Luna and which came from Bea, so the audit trail is intact. The
methodology rewards this kind of escalation; the alternative is a
silent compromise that DESIGN would inherit without context.

---

## Approval

**APPROVED** for handoff to DESIGN once Bea closes the two
clarifications inline (Findings 1 and 2). Both are mechanical edits
applied at the same commit as this review.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.
