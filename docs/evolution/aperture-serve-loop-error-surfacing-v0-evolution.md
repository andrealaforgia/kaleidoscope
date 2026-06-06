# Evolution archive — aperture-serve-loop-error-surfacing-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md`,
`cli-ingest-atomic-v0-evolution.md` and
`cinder-wal-error-surfacing-v0-evolution.md`, which established the
per-file convention: one file per feature, named
`<feature-id>-evolution.md`, with the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0066
  (`docs/product/architecture/adr-0066-aperture-serve-loop-error-surfacing.md`),
  EXTENDS (does NOT supersede) the slice-08 graceful-shutdown contract
  (`shutdown.rs:138-229`, ADR-0010), with ADR-0061 (refuse-to-start
  fail-closed) as the sibling precedent one lifecycle phase earlier.
- Closes: the serving-layer half of the swallowed-errors family, the
  next item after `cinder-wal-error-surfacing-v0` (ADR-0065) closed its
  storage half. The four-quadrants Q3 report flagged aperture's two
  serve-loop swallows, the HTTP arm as the higher-value undisclosed half.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| deliver | `d9f0f83` | surface post-bind serving-loop death instead of swallowing it |
| docs | `0f53f3a` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `d9f0f83`.

## The problem, in Earned-Trust framing

aperture v0 is the LIVE OTLP ingest gateway (`tonic` gRPC on `:4317`,
`axum` HTTP/protobuf on `:4318`, tagged `aperture/v0.1.0`). Each transport
binds its socket synchronously (a bind error already surfaces as
`listener_bind_failed`) and then spawns the serving loop as a Tokio task
that DISCARDS the serve future's `Result`. Two swallow sites, both
verified in code on this branch:

- gRPC serve (`crates/aperture/src/transport.rs:89-94`):
  `tokio::spawn(async move { let _ = server.await; })`. The swallow was
  DISCLOSED by a comment promising slice 08 would surface it; the promise
  was never kept for the error case.
- HTTP serve (`crates/aperture/src/transport.rs:152-158`):
  `tokio::spawn(async move { let _ = axum::serve(...).with_graceful_shutdown(...).await; })`.
  The swallow was SILENT: no disclosing comment, the higher-value
  undisclosed half the Q3 report named.

Slice 08 surfaced DRAIN outcomes (`in_flight_drained` /
`drain_deadline_exceeded`) but never the serve ERROR:
`orchestrate_shutdown` was the sole awaiter of the joins and did
`let _ = join_grpc.await; let _ = join_http.await;` (`shutdown.rs:185-190`),
throwing the serve `Result` away there too.

The operator consequence is the ZOMBIE LISTENER. When a serving loop dies
after the socket is bound (the accept loop errors out post-bind), the
process keeps running and keeps lying about its health: `/healthz` returns
200 unconditionally (liveness stays true), `/readyz` reflects only
`Starting`/`Ready`/`Draining` so a dead loop has no `Failed` phase to flip
to and stays 200 "ready", and the exit code stays 0 because a serve death
has no path into `DrainOutcome::exit_code()`. Process up, `/healthz` 200,
`/readyz` ready, exit 0, listener dead, nothing landing. A k8s orchestrator
keeps the dead instance in rotation, routing telemetry into a socket that
accepts nothing, while the operator's stderr scrape and `/readyz` runbook
both stay green in front of a dead gateway. This is the
acked-but-actually-broken lie the project's Earned-Trust posture forbids,
in the serving layer rather than the storage layer.

## The decision lineage

### ADR-0066 EXTENDS the slice-08 graceful-shutdown contract

ADR-0066 does NOT supersede anything. Serve-loop failure is the SIBLING of
drain-deadline-exceeded: both are non-clean process verdicts named on the
way out, both ride the same `DrainOutcome` -> exit-code seam. The ADR adds
the serve-failure arm and changes NOTHING about the graceful-drain arm.
slice-08 (ADR-0010) stays the contract for the drain narrative; this
feature reuses its readiness CAS, its `DrainOutcome` exit map, and its
exit-code seam rather than building parallel machinery. ADR-0061
(refuse-to-start fail-closed) is the sibling precedent one lifecycle phase
earlier: a requested-but-unimplemented property produces a loud refusal at
startup; this is the same reflex post-bind, a serving death producing a
loud, honest, operator-actionable signal instead of a silent zombie.

### Reuse, not invention: no new public type

The fix is EXTEND-ONLY. It introduces NO new public type, NO public-API
break, NO new crate, NO new always-running task. It reuses the existing
spawn helpers, the `ShutdownBundle`, the readiness machine, the
`DrainOutcome` exit map, and the closed event vocabulary (ADR-0009). The
new internal types (`ServeOutcome`, `ServeError`) are `pub(crate)`,
carried inside the crate-private bundle and join types, never nameable
from outside. aperture stays `0.1.0`: there is no break to bump, and 1.0.0
is a public stability promise that is Andrea's call alone and premature
while these APIs churn.

## The as-built shape

### D1 — typed join result plus serve-task self-reaction

The spawn helpers change their return type from `JoinHandle<()>` to
`JoinHandle<ServeOutcome>` (`ServeOutcome { Graceful, Failed }`,
`pub(crate)`, Copy/Eq). Each task awaits the serve future into a local
`Result`, consults the D3 shutdown flag, and SELF-REACTS at the failure
site: on a non-graceful return it emits the event, flips readiness, and
resolves to `ServeOutcome::Failed`; on a graceful return it resolves to
`Graceful` with no event and no flip. The single `resolve_serve_outcome`
free function is the one place both transports route through, so the
graceful-vs-fatal decision and the self-reaction are written once. The
serve error is rendered to a reason `String` inside the task at the
failure site (tonic/axum serve errors are not `Send + 'static`-uniform and
nothing downstream needs the rich type), so the join carries only the
verdict the orchestrator needs. The typed join is the single exit-code
seam: option D's dedicated `mpsc`/supervisor task was rejected because the
join IS the outcome channel.

### D2 — sticky `ReadinessPhase::Failed`, `/readyz` 503, exit code 3

Readiness gains a fourth phase `Failed`, sticky like `Draining` (a dead
listener never recovers; the process exits). `flip_to_failed()` CAS-flips
`Ready|Starting -> Failed` and emits
`readiness_changed ready=false reason=serve_loop_failed`; `/readyz` maps
`Failed -> (503, "failed\n")`. `/healthz` stays 200 (liveness is not the
lever). The exit code gains `3` via a new `DrainOutcome::ServeFailed`
variant, distinct from clean-drain 0, deadline-exceeded 1, and config-error
2 (ADR-0061), so a supervisor restarts the zombie rather than leaving it in
rotation. `Draining` and `Failed` are both sticky terminal 503 states:
whichever lands first wins, the other's CAS no-ops, and `/readyz` is 503
either way and never flaps back to 200.

### D3 — the per-transport `shutdown_requested` discriminator

The discriminator is "was shutdown requested?", NOT the serve future's
`Ok`/`Err`. A per-transport `Arc<AtomicBool>` `shutdown_requested` (init
`false`) is set `true` inside the existing graceful-shutdown closure the
instant the oneshot resolves, before the serve future drains. After the
serve future returns, the task reads the flag: `true` -> `Graceful`
(byte-for-byte the old behaviour, no event, no flip); `false` -> ANY return
is fatal, including an unexpected early `Ok`. A listener that stops serving
without anyone asking is exactly the zombie the feature kills, so early-Ok
is treated as fatal at v0 rather than tolerated as a quieter zombie. The
one additive closed-vocabulary constant is `SERVE_LOOP_FAILED`
(`observability.rs`), level `error`, fields `transport` (`"grpc"`|`"http"`)
and `error`.

### The 11-site internal ripple, all `pub(crate)`, no public-API break

The change rippled across 11 enumerated internal sites, all `pub(crate)`:
`transport.rs` (the `ServeOutcome`/`ServeError` types, the self-react, and
the two spawn tasks returning `JoinHandle<ServeOutcome>`), `shutdown.rs`
(the `ShutdownBundle` join field types, `DrainOutcome::ServeFailed -> 3`,
and the orchestrator folding a serve death), `readiness.rs` (the sticky
`Failed` phase and `flip_to_failed`), `lib.rs` (the run loop racing the
joins for the no-SIGTERM death path plus the exit-3 fold), `observability.rs`
(the event constant), `compose.rs` (`spawn_with_readiness`), and `main.rs`
(the exit-code 3 doc line). No public re-export changed; downstream
embedders (notably `gateway`) consume only the unchanged public surface.
aperture stays `0.1.0`.

## The proof

- 100% mutation kill on the modified surface (ADR-0005 Gate 5; CLAUDE.md
  per-feature 100%), via `cargo mutants --in-diff` on the five modified
  files: 45 mutants, 26 caught + 19 unviable, 0 missed, 0 timeout. The
  existing `gate-5-mutants-aperture --in-diff` job picked up the diff; no
  new CI job was needed.
- All 10 feature scenarios green, 0 ignored; the 3 negative controls and
  the slice-08 graceful-drain suite stay green (SIGTERM still exits 0, no
  false alarm). No new dependency (the `rustix` dev-dep used by the SIGTERM
  subprocess control was already in the workspace tree).
- DISTILL committed the failure scenarios RED-`#[ignore]`d (proven RED,
  not BROKEN: the suite compiled, the negative controls were green, and
  the 7 ignored scenarios panicked at the `unimplemented!` seam or at
  behavioural subprocess assertions). DELIVER un-ignored them one at a time
  in the outer-loop order: gRPC stderr; HTTP stderr (the previously-silent
  arm); `/readyz`-failed flip; sticky `Failed`; early-Ok-fatal; binary
  exit-3 (real subprocess); binary SIGTERM exit-0 (real subprocess).

### The timeout mutant killed by a reaper, not by weakening

One mutant surfaced as a TIMEOUT rather than a clean miss. It was killed
not by weakening any assertion but by adding a bounded-wait reaper to the
subprocess test, so the test reaps its spawned aperture child within a
bound instead of hanging when the mutant left a serving loop alive. The
mutant is caught; the assertion's strength is untouched. Final tally: 0
missed AND 0 timeout.

### The falsifiability and its honest boundary

Every failure scenario asserts a specific operator-visible observable the
swallow CANNOT produce (the discarded `Result` emits nothing, there is no
`Failed` phase to flip to today, the swallowed serve error has no path into
the exit code), so the tests pass only once the surfacing-and-reaction fix
lands. The trigger is the test-only `spawn_with_injected_serve_failure`
seam (and its binary analogue, the `APERTURE_TEST_INJECT_SERVE_FAILURE`
env var), the aperture analogue of cinder's `FailingFsyncBackend`: the
ONLY thing faked is the trigger (a serve future forced to resolve `Err` or
early `Ok` post-bind with `shutdown_requested=false`); everything
downstream (the event, the `flip_to_failed`, the exit-code fold) is REAL
production code. This is the same honest boundary cinder recorded: there
is NO operator-reachable post-bind serve-failure trigger in production, so
the seam is the only honest way to drive the boundary, and faking the
outcome rather than the trigger would be Fixture Theater. The
previously-SILENT HTTP arm got its OWN scenario, never implied by gRPC, so
its surfacing is proven independently.

## The honest finding: the test-hygiene lesson

Recorded in the same spirit as the prior archives' honest-finding
sections: the binary subprocess tests must REAP their spawned aperture
children. During DISTILL a leak from a manual `--ignored` run had
transiently polluted `slice_09` (an orphaned aperture child held a port).
DELIVER fixed it with a `Drop`-guard reaper on the subprocess fixture, the
same mechanism that killed the timeout mutant above: a subprocess test that
spawns a real binary owns the duty of killing it on every exit path,
including panics and early returns. The production change was conceptually
contained (route the serve `Result` through a typed join, self-react at the
failure site, fold the verdict into the existing exit-code and readiness
seams); the load-bearing care was in the test substrate, in building a
deterministic post-bind death behind a real bound listener and in reaping
the children that death leaves behind.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted.

1. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

2. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. Its `Queue` surface was made
   fail-loud before it is wired (zero live blast radius); the wiring
   itself is a separate, still-open slice. Open.

3. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

4. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

5. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
   holds the whole input's records in RAM before commit. A future feature
   lifts it with a temp-WAL staging stage or a max-records streaming cap.
   Open.

6. ADR-0059 Decision 8 layer b, the AST structural check, remains
   UNWIRED. The structural pre-commit check asserting in-scope stores
   delegate to the shared wal-recovery routine and carry no `let _ =`
   swallow; the tool choice was deferred and remains deferred. It is
   feedback, not a gate, consistent with the pure trunk-based,
   no-required-checks posture; when wired it belongs in the local
   pre-commit stage. Open.

7. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

8. beacon SLO unreachable (B06). The beacon SLO as specified is not
   reachable by the current implementation; the SLO MWMBR synthesis the
   verifier left for later is still outstanding. Open.

9. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

10. The two claims-honesty DOCUMENT items remain future features if
    wanted. The actual Prometheus-stepped grid for `query_range` (a
    query-api feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.

11. aperture early-Ok tolerance. The unexpected-early-`Ok`-without-shutdown
    is treated as FATAL at v0 (surfaced, not tolerated), the honest default
    for a listener that stops unbidden. If a future transport legitimately
    self-stops `Ok` without a shutdown request, that distinction would earn
    its own slice; at v0 there is no such path and the fatal treatment is
    correct. Open only if such a path ever appears.
