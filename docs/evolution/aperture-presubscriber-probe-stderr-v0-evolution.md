# Evolution archive — aperture-presubscriber-probe-stderr-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md`,
`cli-ingest-atomic-v0-evolution.md`,
`cinder-wal-error-surfacing-v0-evolution.md`,
`aperture-serve-loop-error-surfacing-v0-evolution.md`,
`beacon-slo-operator-path-v0-evolution.md`,
`aegis-ingest-auth-v0-evolution.md`,
`spark-ingest-auth-v0-evolution.md` and
`perf-kpi-ci-non-gating-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This is a SMALL net-deletion behaviour fix on the
aperture binary, so the record is deliberately proportionate to that
scope.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0071
  (`docs/product/architecture/adr-0071-aperture-presubscriber-probe-refusal-visibility.md`),
  which surfaces a previously-silent startup refusal WITHOUT changing the
  refusal decision or the probe semantics (ADR-0007 / ADR-0061 stand). It
  belongs to the swallowed-errors family (cinder / sluice / serve-loop
  siblings, ADR-0066 the post-bind counterpart).
- Closes: the verifier-flagged honesty gap (Bea Verifier msg 037, A21
  issue 012). For a `Forwarding` sink the production startup probed the
  downstream BEFORE the tracing subscriber was installed; a
  downstream-not-accepting refusal exited 1 SILENTLY, the operator seeing
  empty stderr and a bare exit code.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `3532459` | a fail-closed refusal that says nothing |
| design | `d736741` | delete the silent probe, keep the visible one |
| devops | `8c419c1` | existing CI covers a net deletion, no semver |
| distill | `0885b1f` | RED test for the now-silent probe refusal |
| deliver | `b4ff12a` | surface silent startup refusal via post-subscriber probe |
| docs | `067afc5` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `b4ff12a`.

## The problem, in Earned-Trust framing

Aperture is the OTLP forwarding gateway. At startup it runs an
Earned-Trust probe (ADR-0007, Principle 12) against the configured sink:
if the downstream is not accepting telemetry the gateway refuses to
start, binds no listener, and exits non-zero. That fail-closed decision
is correct and stays. The defect was the SILENCE, not the decision.

The refusal is emitted through
`tracing::error!(event = HEALTH_STARTUP_REFUSED, reason = %e)` inside
`probe_or_refuse`. For a `Forwarding` sink the production call chain
probed TWICE. The first probe ran inside `wire_sink` (`lib.rs:223`)
BEFORE `install_subscriber` had run, so its event had no subscriber to
flow through and was dropped on the floor. The second probe, the
post-subscriber one in `spawn_with_readiness`, would have been visible,
but the pre-subscriber probe failed fast first and won the race. Worse,
for `Forwarding` the sink object the first probe checked was DISCARDED
and a fresh `ForwardingSink` rebuilt at the second site, so the silent
probe was not only inaudible but redundant: it probed a sink that was
never used. The operator saw an empty stderr and a bare `exit 1`. The
sink was probed twice; the pre-subscriber one won the race and lost the
words.

## The decision lineage

### ADR-0071 mechanism (c): delete the duplicate, keep the visible one

ADR-0071 chose mechanism (c): DROP the redundant pre-subscriber probe
from `wire_sink` so the surviving post-subscriber probe carries the
refusal. The deciding evidence was an ordering finding read in source on
2026-06-07: `install_subscriber` runs at `compose.rs:134`, the
post-subscriber `probe_or_refuse` at `compose.rs:157-167`, and the FIRST
listener bind (`spawn_grpc`) at `compose.rs:196`. The post-subscriber
probe is therefore ALREADY in exactly the right place: strictly AFTER the
subscriber (so a refusal's `event=health.startup.refused` line is visible
on stderr) AND strictly BEFORE any bind (so fail-closed-with-no-bind is
preserved). The fix is a NET DELETION plus a doc update, not added
machinery.

### The alternatives, rejected

- (a) Install the subscriber before `wire_sink`. Rejected: more invasive
  for no benefit, it disturbs the idempotent `install_subscriber`
  ordering and the ADR-0066 seam, AND it keeps the redundant double-probe
  (the `wire_sink` `Forwarding` sink is discarded anyway).
- (b) A direct-stderr bridge for the pre-subscriber window, mirroring
  `emit_config_error`. Rejected: only necessary if a refusal could occur
  before the subscriber AND before a bind with no other emission path,
  and the ordering finding shows it cannot. (b) would also force
  `main.rs` to discriminate pre- vs post-subscriber failures and restate
  the `health.startup.refused` literal in a second place. It is kept on
  the record as the correct choice in the counterfactual where the
  post-subscriber probe ran AFTER a bind, but that counterfactual does
  not hold here.

## The as-built shape

`wire_sink` became pure and infallible. Its signature dropped from
`async fn wire_sink(&Config) -> crate::Result<Arc<dyn OtlpSink>>` to
`fn wire_sink(&Config) -> Arc<dyn OtlpSink>`: both arms (`Stub` and
`Forwarding`) build the configured concrete sink, erase the type, and
return, with both `probe_or_refuse` calls removed. The call site in
`run()` (`lib.rs:223`) dropped its `.await?` accordingly. The doc comment
was rewritten to record that the single probe now runs in
`spawn_with_readiness`, after the subscriber and before any bind.

`probe_or_refuse` is KEPT, now with one caller (the post-subscriber site
at `compose.rs:157-167`), unchanged: it still emits
`event=health.startup.refused reason=%e` (ADR-0009 closed vocabulary,
`observability.rs:49`, no new constant) and returns `Err`. The `Err`
propagates `spawn_with_readiness` to `run()` to `main.rs` to
`ExitCode::FAILURE`. Fail-closed is unchanged: exit non-zero, no listener
bound. No new event constant, no new stderr path, no `main.rs` window
discrimination, no `install_subscriber` ordering churn. Net deletion of
24 insertions / 25 deletions across `compose.rs`, `lib.rs`, and the
un-ignored test file.

## The proof and its boundary

- The acceptance lives in
  `crates/aperture/tests/probe_refusal_visibility.rs`: 3 previously-RED
  `#[ignore]`d tests un-ignored and now green
  (`probe_refusal_emits_health_startup_refused_on_stderr`,
  `probe_refusal_line_names_the_sink_and_the_underlying_error`,
  `probe_refusal_is_fail_closed_and_visible`). They are real-binary
  subprocess tests driving the configured port against a wiremock
  substrate-lie subprocess (the catalogued v0 lie: 200 on OPTIONS
  preflight, 503 on POST, the same fixture pattern as
  `tests/probe_gold_runner.rs`). The 2 green negative controls (healthy
  downstream emits no refusal line; config-error still emits
  `event=config_validation_failed` exit 2) plus the meta test stayed
  green. Zero `#[ignore]` remain on the feature tests.
- The full aperture suite stayed green
  (`probe_gold_runner`, `slice_06_forwarding_sink`,
  `serve_loop_error_surfacing`, `slice_08`, `slice_10_ingest_auth`,
  invariants): the probe semantics did not regress; the gold runner keeps
  guarding that the probe BITES on the lie, while the new subprocess test
  guards that the bite is now VISIBLE at startup.
- Mutation (ADR-0005 Gate 5, CLAUDE.md per-feature 100%) via
  `cargo mutants --in-diff` on aperture: 3 mutants found, 2 CAUGHT, 1
  UNVIABLE, 0 MISSED = 100% kill on viable mutants. The pure-deletion
  lines yield no viable mutant (you cannot mutate a removed line), so the
  new subprocess test is the behaviour guard for the consolidated probe
  path. Carried by the existing `gate-5-mutants-aperture` job; no new CI
  job.
- SemVer (Gate 2 / Gate 3): none. `wire_sink`, `spawn_with_readiness` and
  `probe_or_refuse` are all `pub(crate)`; aperture is NOT in the Gate 2/3
  public-API set (unlike `spark`). No version bump (aperture stays
  `0.1.0`; never 1.0.0, Andrea's call alone, CLAUDE.md / MEMORY).
- The boundary: the refusal decision was NOT changed, the probe semantics
  were NOT changed, fail-closed was NOT weakened (no listener binds on a
  refusal), and the ADR-0066 post-init tracing path and the ADR-0061
  config-error pre-init line are both untouched. ONLY the duplicate,
  silent probe was removed.

## The verifier's transition-proof (A21)

Recorded in the same spirit as the prior archives' honest-finding
sections. Bea Verifier flagged the silence as msg 037 / A21 issue 012.
The three acceptance tests were authored RED at the discuss SHA
(`3532459`): on that code the binary spawned, exited silently, and the
`event=health.startup.refused` line was absent. At the deliver SHA
(`b4ff12a`) the same tests are GREEN: the surviving post-subscriber probe
now carries the refusal to stderr through the installed subscriber. A21
issue 012 is CLOSED, settled by the substrate (a real binary against a
real liar) rather than by assertion.

## Note for the operator

This feature adds no deployment precondition and changes no runtime
behaviour beyond startup VISIBILITY. The only observable change: when a
`Forwarding` aperture is pointed at a downstream that is not accepting
telemetry, the gateway still refuses to start, binds nothing, and exits
non-zero, but now emits `event=health.startup.refused` with a `reason`
naming the downstream and the underlying cause on stderr, through the
normal JSON-stderr subscriber, instead of exiting silently. A fleet-level
alert on "non-zero exit with no refusal/config line" is recorded in
`discuss/outcome-kpis.md` as a future, out-of-this-wave concern.

## The lesson

Most slices added something. This one made the software more honest by
doing LESS. The silence was not a missing emission to be built around: it
was a duplicate doing harm, a redundant pre-subscriber probe that probed
a discarded sink, won the race against the visible probe, and lost the
words. The cleanest repair was to remove it rather than to build
machinery around it. The visible refusal was already in exactly the right
place, after the subscriber and before any bind; the fix was to stop
pre-empting it. A net deletion, and the binary now says what it does.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The aperture
startup-refusal silence is CLOSED by this feature.

1. read-path auth (the next aegis wire). The query / log-query /
   trace-query read APIs are still unauthenticated; aperture-storage-sink
   reaches through `.inner` and read-path tenant authority is deferred.
   Open.

2. ingest role-gating. ingest auth is authentication-only: any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one
   `if ctx.role != Operator { reject }` gate with no re-plumbing. Open.

3. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. Disposition: a `docs:`
   fix-forward or a trivial micro-wave. Open.

4. fleet alert on silent non-zero exit. The outcome-kpis note proposes a
   fleet-level alert on a non-zero aperture exit carrying neither a
   refusal nor a config-error line. It is a separate future feature, not
   this wave. Open only if wanted.

5. aperture Gate 2/3 enrolment. aperture is NOT in the public-API-tracked
   set (only four packages are graduated). Enrolling it is a project
   decision independent of this feature. Open only if wanted.

6. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

7. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. The wiring is a separate,
   still-open slice. Open.

8. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

9. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

10. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
    holds the whole input's records in RAM before commit. A future
    feature lifts it with a temp-WAL staging stage or a max-records
    streaming cap. Open.

11. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage. Open.

12. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is
    not surfaced to clients. Open.

13. The two claims-honesty DOCUMENT items remain future features if
    wanted. The actual Prometheus-stepped grid for `query_range` (a
    query-api feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.

14. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
