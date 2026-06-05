# Evolution archive — beacon-sighup-reload-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md` and
`claims-honesty-pass-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0063
  (`docs/product/architecture/adr-0063-beacon-sighup-atomic-hot-reload.md`),
  citing ADR-0034 ('Reload semantics') UNMODIFIED, and observing
  ADR-0037 (pure evaluator) as inviolable.
- Closes: the black-box verifier's issue 010 (B03, RED quadrant).

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `a62b29a` | deliver the SIGHUP reload the docs promised |
| design | `d7dab93` | ADR-0063, single-orchestrator atomic swap |
| devops | `3f7c657` | slim wave, existing beacon gates cover, SIGHUP-test determinism |
| distill | `15533b2` | 9 SIGHUP-reload acceptance scenarios, RED-ready |
| feat | `d9f88ba` | beacon-server SIGHUP hot-reload of the rule catalogue, atomic and all-or-nothing |
| docs | `75e6ac8` | narrative + slide closure |

## The problem, in Earned-Trust framing

beacon-server documented a SIGHUP rule-catalogue hot-reload in five
places: c4-context, c4-container, slice-02, wave-decisions, and ADR-0034's
'Reload semantics'. The binary installed only SIGINT and SIGTERM handlers,
loaded the rule catalogue once at startup, and treated SIGHUP as a silent
no-op. An operator who edited the rules directory and sent SIGHUP, exactly
as the documentation described, saw nothing change and received no signal
that nothing had changed. The documented hot-reload did not exist.

This is the same class of substrate lie the project's Earned-Trust
principle exists to forbid: a documented capability the binary does not
deliver and does not refuse. It is the verifier's issue 010, the B03 RED
finding.

## The deliver-versus-retract decision, and why DELIVER

The decisive decision of this feature, recorded plainly because it shaped
everything after it, is that this finding was resolved by DELIVERING the
designed capability rather than RETRACTING the documentation. This is the
opposite resolution to `tls-config-reject-v0`, which retracted aperture's
never-v0 TLS claim, and to the `claims-honesty-pass` DOCUMENT items, which
chose to make prose match code rather than build the claimed feature.

The discriminator is whether the claim was DESIGNED or merely ASPIRATIONAL.
ADR-0034 'Reload semantics' specified the SIGHUP reload MECHANISM in full:
not a roadmap aspiration but a designed contract with stated semantics. A
designed mechanism that the binary failed to install is an implementation
gap to close, not an overstatement to retract. Contrast aperture's
`tls.enabled`: there the v0 binary implements neither transport encryption
nor SPIFFE, and no ADR designed the mechanism, so the honest move was to
refuse the knob loudly. The line is: retract what was never designed,
deliver what was designed and merely missing. ADR-0034 put SIGHUP reload
firmly on the deliver side.

## The architecture decision

ADR-0063 records four sub-decisions, each load-bearing.

### Single-orchestrator handler

The SIGHUP reload is driven by one orchestrator that owns the whole
sequence end to end, rather than spread across the existing per-task
machinery. A single owner of the load-validate-swap-abort sequence is what
makes the all-or-nothing ordering invariant (below) expressible as one
linear procedure with no interleaving, no partially-applied intermediate
state visible to any other component, and one place to read the safety
argument.

### Name-only matching key, keeping `since`

In-flight evaluation state is carried across a reload by NAME-MATCHING:
a rule that survives the reload (same name) keeps its in-flight state. A
surviving rule that was Firing keeps its `since`, so a reload does not
re-page on a rule that was already alerting. The matching key is the rule
NAME only, deliberately not the rule's full content. The trade-off this
buys, and the one it accepts, is recorded under the review clarifications
below.

### Wholesale resolver rebuild with both-ends carryover

The `InhibitionResolver` is rebuilt WHOLESALE from the new catalogue rather
than mutated in place. Pending suppressions are carried over only when BOTH
ends survive the reload: a suppression whose inhibitor or whose inhibited
rule did not survive is dropped, because a half-surviving suppression has
no coherent meaning in the new generation. The both-ends survival check is
the seam that keeps the rebuilt resolver honest about which suppressions
still apply.

### Build-new, validate, swap, abort-old ordering

The ordering is: load and validate the new rules directory and build the
new generation (new task set plus rebuilt resolver) BEFORE touching
anything currently live; make the new generation live; THEN abort the old.
This ordering is the load-bearing safety property of the whole feature
(see below).

## The all-or-nothing ordering invariant

The single safety property the feature exists to hold is all-or-nothing
atomicity: a reload either applies wholly or applies not at all, and the
previous catalogue stays fully active until the new one is proven loadable.

- On an INVALID reload (zero valid rules, or a parse edit that adds no
  valid rule), the reload is REFUSED. The previous catalogue stays fully
  active. A `beacon.reload.refused` event names the offending file, the
  error, and `previous_catalogue_retained`. No crash, no partial apply.
- On a VALID reload, the new task set and the rebuilt
  `InhibitionResolver` are built and made live, and only THEN is the old
  generation aborted. Because the new generation is live before the old is
  aborted, there is no missed evaluation window; overlapping ticks across
  the swap are idempotent, so no double-fire occurs.
- A PARTLY-BROKEN edit that still adds at least one valid rule SUCCEEDS
  with report-and-skip, matching the startup semantics: the valid rules
  load, the broken entries are reported and skipped, the reload is not
  refused. The refuse line is drawn at zero-valid-rules, not at
  any-broken-entry.

The invariant is structural in the ordering: because the new generation is
fully built and validated before anything live is touched, there is no
control-flow path that applies a partial reload. The refusal case never
reaches the swap; the success case reaches the swap only with a fully built
new generation in hand.

## The three review clarifications folded in

Three points were raised in peer review and folded into ADR-0063, recorded
here because each pins a real trade-off.

- Sink-emit overlap idempotency. Because the new generation goes live
  before the old is aborted, two generations can briefly evaluate
  overlapping ticks. The clarification confirms this is safe: the sink emit
  is idempotent across the overlap, so an overlapping tick cannot
  double-fire. The overlap is the price of no-missed-evaluation, and it is
  paid safely.
- The `for_duration`-decrease pre-fire trade-off. Because the matching key
  is the rule NAME only and a surviving Firing rule keeps its `since`, a
  reload that DECREASES a rule's `for_duration` can cause that rule to be
  considered already-fired sooner than a from-scratch evaluation would
  (the retained `since` is now further past the shortened threshold). This
  is the accepted trade-off of name-only carryover: it is the cost of not
  re-paging surviving rules, and it is documented rather than engineered
  away.
- The inhibition both-ends survival check. The wholesale resolver rebuild
  carries a pending suppression over only when both its ends survive the
  reload. The clarification pins this as the correct rule: a suppression
  with a vanished end has no meaning in the new generation and is dropped,
  not silently half-applied.

## Reuse, not invention

ADR-0034 was CITED and left UNMODIFIED: the feature delivers the mechanism
ADR-0034 already designed, so the ADR needed no edit. ADR-0037's pure
evaluator is inviolable and was not touched; the reload swaps the task set
and resolver around the evaluator without changing the evaluation function.

The feature reuses `load_rules` (the existing startup load-and-validate
path, which is why partly-broken-but-some-valid behaves identically at
reload and at startup), the `RuleStateStore`, and `JoinHandle::abort` for
retiring the old generation. The single new seam is
`InhibitionResolver::rebuild_from` with suppression carryover, the one
piece the wholesale-rebuild decision required that did not already exist.

## Verification

- 9 `sighup_reload` acceptance scenarios, all green and deterministic
  (~5s). The scenarios are EVENT-SYNCHRONISED: they wait on the
  `beacon.reload.succeeded` / `beacon.reload.refused` events rather than on
  wall-clock sleeps, so they carry no p95 or timing-flake dependency (the
  p95 wall-clock flake class recorded in project memory does not touch this
  suite).
- 100% mutation kill (ADR-0005 Gate 5; CLAUDE.md per-feature 100%) on the
  `main.rs` reload diff and on `inhibition.rs`. The existing beacon
  `--in-diff` mutation job picks up the reload diff automatically; no new
  CI job was needed.
- The new `InhibitionResolver::rebuild_from` seam is covered by 6
  port-to-port unit tests exercising the carryover and both-ends-survival
  rules directly, in addition to the acceptance-level coverage.

## Two honest engineering notes for the record

Recorded in the same spirit as the prior archives' honest-finding
sections: two facts about the delivery that an operator or a later
maintainer must know.

### stdout to STDERR move for the reload events

Making the reload events observable required moving beacon-server's
`tracing` subscriber from stdout to STDERR. The acceptance tests and
ADR-0063 anchor on stderr, and this aligns beacon with the rest of the
platform, whose structured tracing already rides stderr (the same stream
that carries `health.startup.refused`, `listener_bound`, and the
`wal.recovery.torn_tail_dropped` warning). This is a beneficial alignment,
but it is a CONSUMER-VISIBLE change: anything consuming beacon-server's log
stream from stdout must now read stderr. It is recorded here so the change
is not a silent surprise.

### ADR-versus-examples reconciliation

The refuse-versus-apply line was looser in ADR-0034 than in the DISCUSS
worked examples that the acceptance tests encode. Where the two differed,
the DISCUSS worked examples were followed as the AUTHORITATIVE contract,
because they are the concrete, test-encoded statement of the intended
behaviour (in particular the zero-valid-rules refusal line and the
partly-broken-but-some-valid success line). ADR-0034 was not edited to
match; the reconciliation is recorded here, and the acceptance tests are
the operative specification of the refuse-versus-apply boundary.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted.

1. verifier issue 009 (CLI non-atomic ingest, K13). Accepted and queued
   for its own slice; this is the NEXT item. Open.

2. cinder-wal-error-surfacing-v0. cinder's `place()` and `evaluate_at()`
   swallow the result of `append_wal` rather than surfacing it
   (`crates/cinder/src/file_backed.rs`, the `if let Err(_e)` and `let _ =`
   sites). A failed durable append on these two paths is silently dropped,
   itself a residual substrate lie now that the append is fsync-honest.
   Open.

3. sluice nack-past-cap. sluice's behaviour when a write is nacked past its
   cap needs its own slice. Open.

4. ADR-0059 Decision 8 layer b, the AST structural check, remains UNWIRED.
   The structural pre-commit check asserting in-scope stores delegate to
   the shared wal-recovery routine and retain no inline replay loop; the
   tool choice was deferred to DELIVER and remains deferred. It is
   feedback, not a gate, consistent with the pure trunk-based,
   no-required-checks posture; when wired it belongs in the local
   pre-commit stage. Open.

5. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

6. beacon SLO unreachable (B06). The beacon SLO as specified is not
   reachable by the current implementation; the SLO MWMBR synthesis the
   verifier left for later is still outstanding. Open.

7. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

8. The two claims-honesty DOCUMENT items remain future features if wanted.
   The actual Prometheus-stepped grid for `query_range` (a query-api
   feature) and real gRPC-prefix honouring for `harness`
   (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
   built; each would retire its respective pin. Open only if wanted.

9. beacon B03 release-half. The verifier flagged that the release half of
   B03 (inhibitor X resolves, so Y's held Firing is delivered) needs a
   query-aware mock to exercise it end to end; this feature delivered the
   reload half of B03 and the release-half verification remains to be
   built with that mock. Open.
