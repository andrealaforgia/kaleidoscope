# Peer review — Codex v0 DESIGN

- **Date**: 2026-05-07
- **Reviewer**: `@nw-solution-architect-reviewer` (Atlas)
- **Wave**: DESIGN (Morgan's primary decisions plus Bea's recovery
  work on ADR-0024 / ADR-0025 / C4 / technology-choices / slice-mapping)
- **Artefact set**: `docs/feature/codex/design/` plus ADRs 0022-0025
- **Verdict**: **APPROVED** — handoff to DISTILL
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

Codex v0 DESIGN is rigorous and complete. Six DISCUSS-flagged DESIGN
decisions D1-D6 resolve cleanly with substantive alternatives in each
ADR. Four ADRs follow the Nygard template. Cross-feature integration
(Slice 06 / Spark) is additive-only on `#[non_exhaustive]`. C4
diagrams span L1-L3 with clear responsibility mapping. No
architectural bias, no quality-attribute gaps, no missing evidence.

The Q9-vs-slice-06 alignment that Morgan flagged before stalling has
been verified resolved: Q9 ("single warn per init") matches ADR-0025
§3 ("one warn call per init") matches the amended slice-06 brief
("one warn per misconfigured init"). Bea's slice-06 fix at the
recovery commit lands the consistency.

---

## Per-ADR findings

### ADR-0022 — public API and crate layout — APPROVED

`praise:` Tight scope. Five public types + `forbid(unsafe_code)`,
explicit module split, quality-attribute alignment section cites
implementation constraints. The Earned-Trust three-layer enforcement
(subtype via `forbid(unsafe_code)` + tight re-exports; structural via
`cargo public-api` Gate 2 and `cargo semver-checks` Gate 3;
behavioural via slice 01-05 integration tests) is the right pattern.

Four alternatives genuinely considered: struct-with-MatchKind-field
(rejected, breaks five-type lock), `IntoIterator` validate signature
(rejected, no caller benefit at v0), direct OTel SDK type coupling
(rejected, defeats abstraction), short-circuit-on-first-violation
(rejected, contradicts Q5 collect-all). Each rejection has a
substantive reason.

### ADR-0023 — corpus regeneration ritual — APPROVED

`praise:` Pin-bump procedure explicit. Defensive separation of the
generated `SEMCONV_0_27` slice from the hand-maintained
`HOUSE_ATTRIBUTES` slice means the regeneration ritual touches only
the upstream-mirroring file; the house attributes are stable and
human-edited.

Four alternatives: `build.rs` (rejected, Q7 lock on PR-diff
visibility), shell+jq (rejected, brittle parsing of the upstream Rust
source), `cargo-script` (rejected, onboarding burden), hand-typed
list (rejected, transcription error is the failure mode). Each
rejection grounded.

### ADR-0024 — dependency pinning + in-tree Levenshtein — APPROVED

`praise:` Levenshtein sketch is explicit (two-row DP matrix, `Vec`
allocation justified by corpus size). Verification via slice 02
corpus-drift test and slice 05 typo-fixture test is concrete and
named. The "zero runtime deps" posture composes cleanly with Codex's
position in Spark's runtime closure: nothing transitive, nothing to
audit beyond the BSL-1.0 entry Sieve already added.

Four alternatives: runtime semconv dep (rejected, API coupling
defeats decoupling), `strsim` crate (rejected, AGPL context makes
deps costly), vectorised `triple-accel` (rejected, corpus size too
small), embed-full-semconv (rejected, API surface coupling). Each
substantive.

### ADR-0025 — Codex–Spark integration — APPROVED

`praise:` OnceLock catalogue construction is the right shape;
integration point (after Resource composition, before OTel SDK
construction) is correctly fail-fast for strict mode; non-exhaustive
variant addition is non-breaking by Rust semver rules; Display
contract specified explicitly.

The Q9 alignment is verified: ADR-0025 §3 specifies "one
`tracing::warn!(target = "spark", ...)` event per misconfigured
init", with the Display rendering inline in the message body. The
rejected per-violation alternative (Option A) is correctly cited as
the noisier shape Q9 chose against.

Four alternatives: per-violation warn (rejected, Q9 lock on
"one per init"), lint-after-SDK (rejected, fail-fast posture for
strict mode), Codex-emits-directly (rejected, keeps Codex
emit-free per ADR-0024 §3), env-var knob (rejected, builder method
is the right shape for deployment config). All grounded.

---

## Cross-cutting checks

**DISCUSS contract fidelity**: All nine Q1-Q9 implemented in the
ADRs.

| Q | Implementation | ADR |
|---|---|---|
| Q1 library | Pure Rust crate | ADR-0022 |
| Q2 corpus shape | Rust constants from xtask regenerator | ADR-0023 |
| Q3 single version | `=0.27` exact-minor pin | ADR-0024 §1 |
| Q4 no per-tenant overlays | v1+ deferred | wave-decisions out-of-scope |
| Q5 LintReport | Multi-violation collection, Display impl | ADR-0022 |
| Q6 Spark integration | runtime dep, additive variant, builder | ADR-0025 |
| Q7 regeneration ritual | xtask binary, PR-diff visibility | ADR-0023 |
| Q8 in-tree Levenshtein | pure function, no external dep | ADR-0024 §2 |
| Q9 single warn per init | one `tracing::warn!` carrying Display | ADR-0025 §3 |

**Six implementation decisions D1-D6**: All closed in the ADRs with
explicit rationale.

**Scope discipline**: All four ADRs stay v0-disciplined. Out-of-scope
items (gRPC daemon, FoundationDB, CUE, per-tenant overlays, HTML
rendering, Aperture-side integration) are explicitly deferred to v1+.
No padding.

**ISO 25010 quality attribute coverage**:

| Attribute | v0 strategy | Where |
|---|---|---|
| Functional suitability | Six BDD scenarios, happy + edge + error, slice carpaccio | user-stories.md |
| Performance efficiency | <10ms full-corpus budget; ~30-line Levenshtein | ADR-0024 §2, KPI 6 |
| Compatibility | Single semconv pin; new variant additive on `#[non_exhaustive]` | ADR-0024 §1, ADR-0025 §4 |
| Reliability | No I/O, no subprocess, total `Result` | ADR-0024 §3 |
| Security | AGPL-3.0-or-later; in-process; no exfiltration surface | wave-decisions |
| Maintainability | 5 public types; 4 internal modules; 100% mutation kill rate | ADR-0022, slice-mapping |
| Testability | Free-function (no async, no I/O); synchronous tests | ADR-0022 §4 |
| Portability | Pure Rust, workspace MSRV, no platform-specific code | ADR-0024 §2 |

**Antipattern scan**: None. ADRs avoid implementation-as-architecture;
type decisions are justified; no premature optimisation; no
speculative generality.

---

## Suggestions (non-blocking)

`suggestion (non-blocking):` xtask location name varies slightly
between ADR-0023 (which discusses the alternatives `xtask/regenerate_codex_corpus/`
and `crates/codex-tools/`) and ADR-0024 §1 (which cites only the
first variant). At DELIVER time, document the chosen location
consistently in both ADRs (or in implementation notes that
supersede). Recommendation: pick one name now and amend the loser.
For Codex v0 the recommendation is `xtask/regenerate_codex_corpus/`
because the workspace already has an `xtask/` directory pattern
elsewhere.

`suggestion (non-blocking):` ADR-0025's example code block in §3
shows the warn message body as `"schema validation failed:\n{}"`
with `report` interpolated. This locks the human-readable text. If
operators end up parsing the warn body with a regex (which is a
known anti-pattern but happens), the leading `"schema validation
failed:"` becomes load-bearing. Worth noting in the ADR's
"Consequences > Negative" section that the message body is part of
the operational contract, not just a prose suggestion. Non-blocking;
DELIVER can add the note inline.

---

## Praise

`praise:` Bea's recovery finalisation of ADRs 0024-0025 is
indistinguishable in rigour from Morgan's original work on ADRs
0022-0023. Same Nygard structure, same alternatives discipline, same
verification clauses. The methodology held under the watchdog stall.

`praise:` D1-D6 resolution. Six DESIGN decisions, each with two or
more substantive alternatives and explicit rejection rationale.
Teams typically defer this kind of decision to DELIVER and
discover the cost mid-implementation; locking here saves iteration.

`praise:` Non-exhaustive discipline validation. Codex v0 is the
first real exercise of Spark's `#[non_exhaustive]` SparkError shape
since Spark v0.1.0 graduated. The new variant lands additive,
non-breaking. Confidence-building milestone.

`praise:` Q9 ↔ ADR-0025 ↔ Slice 06 alignment is tight. "One warn
per init" was locked at DISCUSS; implemented precisely in ADR-0025
§3; the slice-06 brief was amended at the recovery commit to
match. Morgan's flagged risk is resolved correctly.

`praise:` C4 L1-L3 diagrams are clear. Component responsibility
table in c4-component.md is explicit; technology-choices.md is
comprehensive; slice-mapping.md traces every slice to story → ADR
→ module → CI invariant → KPI without gaps.

---

## Approval

**APPROVED** for handoff to DISTILL.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.

Architecture artifacts, four ADRs, C4 diagrams, and six slice
briefs are ready. No external integrations. Post-DELIVER
amendments (ADR-0012, ADR-0013) routed via orchestrator at Slice 06
landing per ADR-0025 §4-5.
