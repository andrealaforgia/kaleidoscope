# Evolution archive — claims-honesty-pass-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md` and
`tls-config-reject-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0062
  (`docs/product/architecture/adr-0062-query-range-v0-raw-points-step-reserved.md`),
  scoping the single document-vs-implement decision that needed a record.
- Closes: the four-quadrants assessment "stale prose" family (backlog
  item 3). The cheapest of the four-quadrants findings, and for a project
  whose thesis is structural honesty against vendor overstatement, the
  sharpest.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `8d040f2` | make the prose match the code |
| design | `50b39dc` | both flags DOCUMENT, ADR-0062 scopes query_range |
| devops | `a812193` | slim wave, existing gates absorb the doc pass |
| distill | `894cc51` | doc-lint guards + behaviour pins, RED-ready (21 across 6 crates) |
| deliver | `be893c5` | make the README, codenames, and doc comments match the code |
| docs | `0490603` | narrative + slide closure |

## The problem, in Earned-Trust framing

The four-quadrants assessment's third backlog item was the stale-prose
family: surfaces that claimed behaviour the shipped code no longer
matched. Unlike the durability and security findings before it, the code
here was overwhelmingly honest already. The lie had migrated to the
prose. The README, the crate codenames, the `Cargo.toml` descriptions and
some doc comments lagged behind code that prior waves had already brought
into line.

The decisive finding, recorded plainly because it shaped the whole
feature, is that Luna verified each claim against the code and found that
most per-crate `lib.rs` had ALREADY been corrected by earlier waves. The
overstatements had not stayed where the assessment first saw them; they
had migrated outward, to the README "Components at a glance" table and to
stale `__SCAFFOLD__`-over-green doc comments left behind when code went
green without its describing prose following it. So the pass was not a
matter of rewriting authoritative-but-wrong module docs; it was a matter
of aligning each lagging outer surface to the crate's own, already-honest
`lib.rs`.

This is the same class of substrate lie the project's Earned-Trust
principle exists to forbid, turned inward on the project's own claims. A
README that says "Auto-instrumentation SDKs" beside a crate that ships a
manual-init wrapper is a vendor-style overstatement, the exact thing the
project's thesis sets out to refuse. The corrections make each surface
true the only honest way: by saying what the code does, not what a
roadmap hopes it will do.

## What was corrected, surface by surface

Each correction aligned a lagging surface to the crate's own already-honest
`lib.rs`. The pattern throughout is alignment to an existing truth, not
invention of a new claim.

- Spark: README "auto-instrumentation" became the manual-init OTel SDK
  wrapper the crate actually ships (auto-instrumentation marked roadmap).
- Strata: README "continuous profiling" became the passive profile
  storage the crate actually provides (continuous scraping marked
  roadmap).
- Cinder: README "cold-tier coordinator (S3)" became the local
  tier-metadata governor it is (object-storage cold tier marked v2).
- Loom: README "dashboards-as-code, CUE" became the TOML rule-catalogue
  change control it implements (dashboards-as-code marked v1+).
- codex: `Cargo.toml` description and the test-module headers "DISTILL
  stub / `unimplemented!()`" became delivered and green, matching
  `codex/src/lib.rs`.
- query-http-common and trace-query-api: stale `__SCAFFOLD__` /
  `unimplemented!` module and handler doc comments became descriptions of
  the live implemented bodies (the four helpers, and the
  resolve-parse-get-cap-serialise orchestration).
- harness: "validates against the wire specification" became
  structural decode-level validation, naming the semantic checks that are
  absent; the README status "implementation intentionally absent" became
  delivered and green.
- README `query_range`: bare "Prometheus-compatible" became raw in-window
  points with a step that is accepted but not honoured at v0 (ADR-0062).
- harness `Framing::GrpcProtobuf`: documented as an inert label at v0; the
  caller strips the five-byte gRPC length prefix but the variant carries
  no behaviour beyond that.

The README "durable / survives restart" claim was deliberately NOT
touched. `store-fsync-durability-v0` had already made that claim true, so
there was nothing left to correct. This is the honest feedback loop in
action: a prior feature retired one of this feature's own backlog items
before this feature ran, and the right move was to leave the now-true
claim alone rather than re-edit it for the sake of churn.

## The two document-vs-implement decisions, both resolved DOCUMENT

Two flagged claims could in principle have been made true either by
correcting the prose or by building the claimed feature. Both were
resolved DOCUMENT, and that resolution is the load-bearing design
principle of a prose-honesty pass.

- `query_range` could have grown a Prometheus-stepped grid; instead
  ADR-0062 records that v0 returns raw in-window points and the `step`
  parameter is reserved (accepted, not honoured). The honest claim is the
  smaller one.
- harness `Framing::GrpcProtobuf` could have grown real gRPC-prefix
  honouring beyond stripping the length prefix; instead the label is
  documented as inert at v0.

The DOCUMENT-over-IMPLEMENT principle for a prose-honesty pass is: say
what is true rather than rush to build the claimed feature under cover of
a tidy-up. A pass whose remit is to stop the prose lying must not become a
back door for unplanned feature work. If the claimed feature is wanted, it
earns its own slice (see the open follow-ups); until then the honest move
is to describe the v0 reality. Both decisions added tests, not production
code.

## The load-bearing bidirectional guard (US-03)

The sharpest design point in the feature is the US-03 bidirectional
guard, because it pins the one way a prose-honesty pass can go wrong.

The pass had to remove stale `__SCAFFOLD__`-over-green doc comments,
markers describing as unimplemented scaffolding code that is in fact
shipped and green. But the codebase ALSO carries genuine `__SCAFFOLD__` /
`#[ignore]` markers over code that is genuinely RED and in flight: the
seven stores' crash-durability work, aperture `slice_09`, the
log-query and gateway tracing scaffolds, and the log-query body-regex and
pagination scaffolds. A careless honesty pass that deleted markers by
pattern would trade one lie for another: it would remove a truthful
RED marker, leaving prose that implies in-flight code is done when it is
not.

So the guard is bidirectional. A GREEN test asserts that the genuine
in-flight markers REMAIN PRESENT
(`us03_in_flight_scaffold_markers_remain_present`), exactly as a separate
set of guards asserts that the stale-over-green markers are GONE. The
honesty pass is constrained from both sides: it must remove the false
markers and it must NOT remove the true ones. RED-not-BROKEN is the
distinction the guard encodes: a marker over genuinely-red in-flight code
is honest and must survive the pass; a marker over green code is a lie and
must not. Without the in-flight guard, the cheapest correct way to make a
stale-marker grep go green would be to delete every marker, which would
replace stale dishonesty with fresh dishonesty.

## ADR-0062 and the step-invariance test

ADR-0062 records that `query_range` v0 returns raw in-window points, not a
Prometheus-stepped grid, and that the `step` parameter is accepted but
reserved. The claim is pinned by a step-invariance test: two different
`step` values against the same window produce identical output, proving
that `step` is genuinely not honoured rather than honoured-by-accident.

This test is deliberately a temporary pin. When a future stepped-grid
feature implements real `step` honouring, the step-invariance test will
FAIL by design, and that failure is the signal to retire it as part of
that feature's work. It is recorded here as a guard that is meant to be
removed deliberately by the feature that makes the claim it pins obsolete,
not as a permanent invariant. A pin that documents the boundary of v0 must
yield gracefully to the feature that moves that boundary.

## The RED-not-BROKEN doc-lint guards

The doc-lint guards (grep-style assertions over the prose, one per
correction) were authored at DISTILL as 13 `#[ignore]`d tests:
RED-ready but not yet asserting, because at DISTILL the prose they check
had not yet been corrected. They were un-ignored per-correction at
DELIVER, each one becoming GREEN as its matching surface was aligned. This
mirrors the project's RED-not-BROKEN discipline: a test that cannot yet
pass because its production change has not landed is `#[ignore]`d (RED, in
flight), not deleted or weakened, and is un-ignored exactly when its
change lands. The guards prove each correction held and prevent a later
edit from silently reintroducing a corrected overstatement.

## Verification

- 21 tests across 6 crates. 13 are the doc-lint grep guards (un-ignored at
  DELIVER, now green), proving each prose correction. 8 are behaviour and
  guardrail tests, including the US-03 in-flight over-reach guard
  (`us03_in_flight_scaffold_markers_remain_present`) and the
  step-invariance test that pins ADR-0062.
- The pure-prose slices have nothing to mutate: a correction that changes
  only a doc comment, a README row or a `Cargo.toml` description introduces
  no executable production line, so there is no mutant to kill. Mutation
  testing is correctly silent on them rather than vacuously green, because
  there is genuinely no production logic in the diff for those slices.
- The two DOCUMENT items added tests (the step-invariance pin and the
  framing-label assertion), not production code. They moved no behaviour;
  they recorded the existing v0 behaviour and pinned it.
- Workspace green: 1449 tests passed at DELIVER. Gate 1 (cargo test
  --workspace) and Gate 4 (cargo deny) auto-cover the change. No new CI job
  was needed; the doc-lint guards run inside the existing per-crate test
  suites.

## The honest finding

The honest finding of this feature is the one that shaped it: the code was
already telling the truth. The overstatements had outlived the code they
once described and had drifted to the outer surfaces, the README table and
the stale scaffold comments, while every per-crate `lib.rs` had already
been corrected by prior waves. The work was therefore not correction of
authoritative documentation but reconciliation of lagging surfaces to an
existing truth, plus the bidirectional guard that stops the reconciliation
from over-reaching into truthful in-flight markers. The value of recording
this is the same as the prior archives': the difficulty was not where a
first glance at the assessment put it. It was in proving that the
corrected prose now matches the code AND that the pass did not trade a
stale lie for a fresh one.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them, except where noted that a prior
feature retired one of this feature's own items.

1. cinder-wal-error-surfacing-v0. cinder's `place()` and `evaluate_at()`
   swallow the result of `append_wal` rather than surfacing it
   (`crates/cinder/src/file_backed.rs`, the `if let Err(_e)` and `let _ =`
   sites). A failed durable append on these two paths is silently dropped,
   itself a residual substrate lie now that the append is fsync-honest.
   Four-quadrants backlog item 4. Open.

2. sluice nack-past-cap. sluice's behaviour when a write is nacked past its
   cap needs its own slice. Open.

3. ADR-0059 Decision 8 layer b, the AST structural check, remains UNWIRED.
   The structural pre-commit check asserting in-scope stores delegate to
   the shared wal-recovery routine and retain no inline replay loop; the
   tool choice was deferred to DELIVER and remains deferred. It is feedback,
   not a gate, consistent with the pure trunk-based, no-required-checks
   posture; when wired it belongs in the local pre-commit stage. Carried
   forward unchanged. Open.

4. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

5. beacon SLO unreachable. The beacon SLO as specified is not reachable by
   the current implementation. Open.

6. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

7. verifier issue 009 (CLI non-atomic ingest), accepted and queued for its
   own slice. Open.

8. The two DOCUMENT items leave future features open, if ever wanted. The
   actual Prometheus-stepped grid for `query_range` (a query-api feature)
   and real gRPC-prefix honouring for `harness` (`Framing::GrpcProtobuf`)
   were deliberately NOT built; they were documented as v0 reality instead.
   Each is a future feature that would retire its respective pin (the
   step-invariance test, and the inert-label assertion). Open only if
   wanted.

## Retired by a prior feature

Recorded for the honest feedback loop. The README "durable / survives
restart" claim was on this feature's original backlog, but
`store-fsync-durability-v0` made the claim TRUE before this pass ran, so
this feature left it untouched. A prior feature retired one of this
feature's items, which is the durability lineage and the honesty pass
reinforcing each other rather than overlapping.
