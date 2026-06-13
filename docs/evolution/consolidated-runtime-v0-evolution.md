# Evolution archive — consolidated-runtime-v0

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
`spark-ingest-auth-v0-evolution.md`,
`perf-kpi-ci-non-gating-v0-evolution.md`,
`aperture-presubscriber-probe-stderr-v0-evolution.md`,
`speed-up-local-precommit-v0-evolution.md`,
`claims-honesty-pass-2-v0-evolution.md`,
`aperture-body-size-cap-v0-evolution.md`,
`read-path-query-api-auth-v0-evolution.md` and
`prism-echarts-paint-e2e-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This is the CONSOLIDATION SPINE feature (item C1 of
the consolidation roadmap, the wiring that finally makes the nineteen
delivered components run together as one live system), so the record is
proportionate to that scope and carries the parts-are-not-a-system lesson
and the named fsync-in-lock watch-item in full.

## Status

- State: DELIVERED and pushed on `main`, and CI-verified green. Delivered
  across one DEVOPS commit, two DELIVER slices, and a docs commit; the
  whole story is below.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent. Autonomous overnight run.
- ADR: ADR-0076
  (`docs/product/architecture/adr-0076-consolidated-runtime.md`), which
  records the single-process shared-`Arc<Store>` decision (the
  recommended model), the new-crate-vs-extend-gateway call (DD1), the
  precise shared-`Arc` flow with the no-concurrency-change confirmation
  (DD2), the runtime and port layout with one tracing install and
  fail-closed startup (DD3), the minimal single-tenant local posture with
  the auth seams preserved (DD4), the additive-binaries constraint (DD5),
  and the three rejected alternatives (A1 extend-the-gateway, A2 the
  distributed-with-WAL-watch shape the Andrea-veto would select, A3 the
  in-process broker). It relates to ADR-0041 (the write side reused),
  ADR-0042 / 0047 / 0048 (the three query-API injected-store seams reused),
  ADR-0049 / 0060 (the fsync-honesty probes the runtime must run before
  serving), ADR-0066 (the fail-on-serve-death posture mirrored across all
  five listeners), ADR-0068 and ADR-0074 (the optional ingest and read
  auth that must not regress), and ADR-0015 / 0009 (the single
  tracing-subscriber invariant). Supersedes nothing.
- Closes: item C1 of `docs/roadmap/consolidation-roadmap.md`, the SPINE
  of Milestone 1 and the gate for everything after it. It fixes the
  single load-bearing consolidation gap named in
  `docs/analysis/consolidation-state-2026-06.md` §4: ingest and query
  were separate OS processes whose file-backed stores load into memory
  once at startup, so a query never saw telemetry that arrived after it
  booted. Until C1 existed, nothing else made the experiment work.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| devops | `ed5c846` | add `gate-5-mutants-kaleidoscope-runtime` (mirrors the sibling composition-root binary job exactly), the environment inventory (one process, five listeners, one root, one tenant), and the freshness-KPI plan; a harmless zero-second pass until the crate lands |
| deliver slice 1 | `fbcacca` | the single-process metrics live loop: `spawn_consolidated` shares one `Arc<pulse store>` into both the ingest sink and the metrics query router so an ingested metric is immediately queryable with no restart; fail-closed wire->probe->use startup; 9 scenarios un-ignored; 100% mutation kill |
| deliver slice 2 | `2a74e4f` | logs and traces live in the same consolidated process (shared lumen / ray stores) plus the three-signal one-process capstone; the slice-1 composition already served all three signals, so slice 2 is the falsifiable logs / traces / capstone proof; 10 scenarios un-ignored; closes C1 |
| docs | `ccb1b99` | narrative + slide for the feature (the day the parts became a system) |

The DISCUSS, DESIGN and DISTILL artefacts (the four `wave-decisions.md`
files, the user stories, the story map, the outcome KPIs, the DISTILL
RED-not-BROKEN scaffold and the new crate skeleton) plus ADR-0076 landed
on `main` inside the slice-1 commit `fbcacca`, each authored by its own
wave agent; the as-built facts below are read from the two DELIVER
commits.

## The problem, in consolidation framing (the crux)

Nineteen well-tested features had landed and the components were
individually solid: durable stores with per-record fsync, OTLP ingest on
both transports for all three signals, a gateway that fans telemetry to
the stores, three query APIs, Prism painting metrics, per-request auth on
both the write and the read door. What did not yet exist was a SYSTEM.

The single load-bearing gap: ingest (`kaleidoscope-gateway`) and the
three query APIs (`query-api`, `log-query-api`, `trace-query-api`) were
SEPARATE OS processes that shared only a filesystem path. Each
file-backed query store loads its snapshot and WAL into an in-memory map
exactly ONCE, at `open()`, and never re-reads. So a query API started
before telemetry arrived returned empty until it was restarted. The
natural experiment loop, the one thing the whole consolidation roadmap
exists to enable, bring up the stack then send a metric then look, FAILED
BY CONSTRUCTION. This was not a bug in any one crate. It was the absence
of shared live state between the writer and the reader: the seam between
the parts was the missing thing. This is C1, the spine of the roadmap
toward an experimentable Kaleidoscope.

## The design decision (ADR-0076)

The decision is the SINGLE-PROCESS consolidation over a shared
`Arc<Store>` per signal: one process builds one durable store per signal,
wraps it in `Arc`, and hands the SAME instance to BOTH the ingest sink
and the query router, so a write is immediately visible to a read. This
was recommended over the distributed / multi-process-with-WAL-watch
alternative (A2), which buys horizontal distribution Kaleidoscope cannot
yet use, at the cost of a new cross-process freshness mechanism
(file-watch, inotify portability, snapshot / WAL race windows, a
cross-process file-lock to make two writers safe) that is strictly more
code and more failure modes than sharing one `Arc`, for a SCALING benefit
that is not an EXPERIMENTATION need today.

### The Andrea-veto, carried through every wave

The single-process-vs-distributed choice is a genuine architecture fork
and it is Andrea's to veto. The veto flag was carried VERBATIM through
DISCUSS (W1), DESIGN, DEVOPS and the ADR: if Andrea prefers the
distributed shape, the mechanism reshapes to a WAL-watch / reload adapter
on the standalone query stores instead of a shared in-process store, the
user-visible outcome and EVERY acceptance scenario stay identical, and
Milestone 1 reshapes around that adapter. Proceeding on single-process
per the roadmap and decide-don't-ask; one word from Andrea flips the
mechanism, and the fork stays open.

### A new wiring-only crate, not an extension of the gateway (DD1)

The decision introduces a NEW, additive composition-root crate
`crates/kaleidoscope-runtime` exposing one binary (bin name
`kaleidoscope`), rather than adding a second `[[bin]]` to the gateway.
Extending the gateway would force the pure-ingest crate to depend on the
three query crates even for the ingest-only binary, coupling the ingest
tier to the read tier and eroding the boundary the distributed future
relies on. The new crate keeps every existing composition root
byte-for-byte, adds exactly one entry point, and contains no new domain
logic, no new store, no new port: only wiring over the existing `pub`
library seams. The four existing binaries all still build and run.

### The feature is a composition root, not a build (DD2)

The work was almost entirely REUSE. The query routers ALREADY accepted an
injected store (`query_api::router_with_auth`,
`log_query_api::router_with_auth`,
`trace_query_api::router_with_auth`), and the sink ALREADY took the
concrete store Arcs (`StorageSink::with_all_stores`). So almost nothing
new was written: the feature is a composition root that hands the writer
and the reader the SAME store handle instead of separate copies. The
load-bearing invariant: `Arc::clone` shares the same heap allocation and
the same interior `Mutex<Inner>`, and coercing
`Arc<FileBackedMetricStore>` to `Arc<dyn MetricStore + Send + Sync>`
attaches a vtable rather than copying the store, so the sink's write and
the router's read go through the same Mutex on the same state. No store
concurrency change was required, confirmed by reading: the trait methods
take `&self`, each adapter serialises behind one `Mutex<Inner>`, and
`ingest` and `query` lock the same Mutex, so a write commits and releases
before any subsequent read acquires it.

## The as-built shape (slices fbcacca + 2a74e4f)

- `spawn_consolidated` (in `crates/kaleidoscope-runtime/src/lib.rs`)
  builds one `Arc<FileBacked*Store>` per signal under the pillar root and
  `Arc::clone`s the SAME instance into both `StorageSink::with_all_stores`
  (the write side, verbatim gateway reuse) and the matching
  `*_query_api::router_with_auth` (the read side, the store coerced to the
  trait object each router accepts).
- One tracing subscriber, installed via the idempotent
  `query_http_common::init_tracing` (`OnceLock` + `try_init`-guarded);
  aperture's own install then observes a default already present and
  no-ops. No double-install.
- Fail-closed wire -> probe -> use startup: the runtime refuses to come up
  if any of the five listeners cannot bind or any store read probe fails.
  The three query listeners bind first (cheap port-conflict detection),
  then `aperture::spawn` binds the two ingest listeners; any bind, probe
  or open failure returns `RuntimeError` with no half-up process. The
  five actual bound `SocketAddr`s are read back into `RunningRuntime`.
- Five listeners on one tokio runtime: ingest gRPC `:4317` and ingest HTTP
  `:4318` (aperture defaults), metrics query `:9090`, logs query `:9091`,
  traces query `:9092` (the three existing query defaults). Each is
  configurable through the existing env knob; tests bind ephemeral
  `127.0.0.1:0` and read the address back.
- A single `KALEIDOSCOPE_TENANT` drives the ingest default tenant and all
  three query tenants for the one-command experiment; the existing
  per-role vars still work and override the unified one when present.
  Optional ingest auth (ADR-0068) and optional per-request read auth
  (ADR-0074) remain available and behave identically when configured;
  both are additive and off by default. A partial read-auth config still
  refuses to start, unchanged.
- Slice 1 (`fbcacca`) is the metrics live loop and the feature walking
  skeleton, derisking the whole single-process bet on the simplest
  signal. Slice 2 (`2a74e4f`) proves the same generic composition for
  logs (lumen, `/api/v1/logs`) and traces (ray, `/api/v1/traces` window +
  `/api/v1/traces/by_id`) and adds the three-signal one-process capstone.
  Slice 2 needed NO library change: the slice-1 composition already served
  all three signals, so slice 2 is the falsifiable logs / traces /
  capstone proof (10 scenarios un-ignored, a five-insertion test diff).
- The minimal run command (the C1 deliverable, not the polished
  one-command product, which is C2):

  ```
  KALEIDOSCOPE_PILLAR_ROOT=... KALEIDOSCOPE_TENANT=... cargo run -p kaleidoscope-runtime
  ```

  (equivalently the built binary `kaleidoscope`). It boots all five ports
  over the shared `Arc<Store>`s; send OTLP to `:4318` / `:4317`, query
  back from `:9090` / `:9091` / `:9092` with no restart.

## The proof and its boundary

- 19 acceptance scenarios across the two slice files
  (`slice_01_live_metrics.rs` 9, `slice_02_live_logs_traces.rs` 10), all
  green by default with zero real `#[ignore]` remaining. The
  load-bearing one is the live-visibility loop: POST a metric, immediately
  GET it back, with NO restart of anything between send and query. It is
  RED by construction on the old separate-process shape (a frozen
  in-memory snapshot cannot return a post-startup append) and GREEN only
  when the sink and router hold the SAME `Arc`. That single-process
  write-then-read is the falsifiable hook; if they ever held different
  instances the write would not be visible and the test reds.
- The suite also covers: tenant isolation positive and negative in one
  process (an owning-tenant read returns its data, a cross-tenant read
  returns empty success with no leak); empty-before-ingest returning
  empty success (HTTP 200, never 500); fail-closed startup on a bind
  conflict; the optional read-auth staying fail-closed when configured;
  all three signals live (logs, traces by window AND by id); the
  all-five-ports one-command capstone; and a `@kpi` freshness scenario
  (the ingest-ack to query-returns interval).
- All scenarios run IN ONE PROCESS on ephemeral `127.0.0.1:0` binds (no
  fixed-port flake, project memory `aperture_fixed_port_4317_flake`): the
  test builds the composition root in the test process, ingests, then
  queries, with no second process and no store drop / reopen.
- Mutation: 100% kill on the composition surface (per-feature strategy,
  CLAUDE.md / ADR-0005 Gate 5). 7 mutants, 1 caught, 6 unviable, 0 missed,
  against the fuller 19-scenario set. `main.rs` (thin env-plumbing) and
  `RunningRuntime::shutdown` (Drop-equivalent teardown) carry justified
  `#[mutants::skip]`.

## CI-verified green (genuinely verified, not pending)

CI is green at `ccb1b99`. The deep workspace gates that auto-cover the new
crate ran and passed: Gate 1 (`cargo test --workspace --all-targets
--locked`, which compiles and runs the 19 acceptance scenarios) success;
the new Gate 5 (`cargo mutants` on `kaleidoscope-runtime`) success; Gate 7
success. The only red is the known non-gating `perf-kpis` p95 wall-clock
flake, which is `continue-on-error` by design (project memory
`p95_wallclock_flakes_overnight`; the fix is nWave CI-gating, never a
threshold raise). This is stated plainly because it is genuinely verified:
the live-visibility loop is proven by a green deep suite in CI, not merely
locally and not "pending".

## The watch-item (named, not hidden)

The per-record fsync runs INSIDE the store write lock (`append_wal` ->
`fsync_file`, `crates/pulse/src/file_backed.rs:325,515`). A query issued
concurrent with a heavy ingest batch therefore blocks until that batch's
fsync completes and the lock releases. This is a LATENCY characteristic,
NOT a correctness one: it does not threaten the live-visibility property,
tenant isolation, or durability, and it needs no store change for C1. The
freshness budget (KPI 2, p95 < 1 s) passed in delivery for the local
single-experimenter workload. The honest disposition, carried verbatim
from DESIGN through DEVOPS, is MEASURE, do not pre-optimise: only if a
realistic local load breaches the budget under concurrent fsync-heavy
ingest does optimisation (fsync outside the lock, batched fsync) become a
separate item. Measured, not assumed, and carried forward.

## Operational note

The push for this archive had to use SSH-over-443 because port 22 is
blocked in the delivery environment. The repository's `origin` is already
configured for SSH-over-443 (`ssh://git@ssh.github.com:443` with
`core.sshCommand` set), so a plain `git push origin main` works. This is a
delivery-environment quirk, not a repository change of substance.

## The lesson

A pile of well-tested components is not a system. Nineteen good
components, each green on its own, did not share a present: the writer
wrote to one frozen-at-startup view and the reader read another, so the
send-then-look loop returned nothing until a restart, and no amount of
per-component quality fixed that, because the defect was in the SEAM, not
the parts. The value was never in the parts; it was in whether they share
a present. And the seam that turned the collection into a system was the
smallest possible thing: one store handle held in common between the
writer and the reader, the same `Arc`, the same `Mutex`, the same state.
The feature wrote almost no new code; its whole substance was making the
sink and the router point at one allocation instead of two. That is the
day the parts became a system you can finally run and experiment with.

## Note for the operator

This feature is additive. The four existing binaries
(`kaleidoscope-gateway`, `query-api`, `log-query-api`,
`trace-query-api`) are untouched and still build and run, and the
on-disk pulse / lumen / ray formats are unchanged, so a `git revert`
needs no data migration and leaves the standalone binaries as the
supported run path exactly as before C1.

The new entry point is the `kaleidoscope` binary (crate
`kaleidoscope-runtime`). Started once over an empty pillar root with
`KALEIDOSCOPE_TENANT` set, it binds all five ports on one process and
serves the live send-then-query loop for all three signals. It is the
SOLE writer of its pillar root: do NOT co-run it against a separate
`kaleidoscope-gateway` on the same root, because two writers would
corrupt the WAL. Auth is off in the local posture; with a complete ingest
or read auth config set, the optional fail-closed paths behave exactly as
their own features specify. The polished one-command compose / run story,
the telemetry generator and the getting-started doc are C2 / C3 / C4 and
are NOT shipped here; C1 ships the binary and the minimal run command
only.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. C1, the
consolidation spine, is CLOSED by this feature; the rest of Milestone 1
and the open architecture fork are the first three items below.

1. C2, the one-command run story. A compose file plus a thin Makefile or
   justfile that brings up the consolidated runtime and Prism with a
   shared volume and the one required tenant variable: `docker compose
   up`, then a working stack. C1 ships only the minimal `cargo run`
   command; the polished launcher is C2. Open, and the next roadmap item.

2. C3, a telemetry generator, and C4, getting started. A small tool (or a
   CLI extension) that pushes sample OTLP metrics, logs and traces plus a
   tiny seed so a fresh stack is not empty (C3), and a README section that
   is honestly the gateway path, one command up, send, see (C4). Together
   with C2 these complete Milestone 1, the first real experiment. Open.

3. the Andrea-veto on single-process-vs-distributed. The architecture
   fork stays open: if Andrea ever wants the distributed shape, the
   mechanism moves from a shared `Arc` to a WAL-watch / reload adapter on
   the standalone query stores, the user-visible outcome and every
   acceptance scenario stay identical, and Milestone 1 reshapes around
   that adapter. No bearer work is wasted by the wait. Open by design.

4. the fsync-in-lock read-latency characteristic. The per-record fsync
   runs inside the write lock, so a query concurrent with a heavy ingest
   batch can see added latency under sustained high-throughput ingest. A
   latency characteristic, not a correctness one; the freshness budget
   passed in delivery. Measure under a realistic local load before any
   optimisation (fsync outside the lock, batched fsync); do not
   pre-optimise. Open as a measure-don't-pre-optimise watch-item.

5. faster-test-fsync-backend-v0. The fsync-bound durability bins remain
   I/O-bound in CI, paying the honest per-record `sync_all` of
   ADR-0049 / 0060. A future feature could speed them with a faster
   test-fsync backend or a batched-fsync test mode behind an env guard.
   Open.

6. read role-gating and ingest role-gating. Both the read and the ingest
   auth are authentication and tenant-scoping only; any valid catalogued
   token (viewer or operator) may read or write. A future role gate is one
   `if ctx.role != … { reject }` with no re-plumbing on each side
   (`TenantContext.role` is already threaded). Open.

7. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. A `docs:` fix-forward or a
   trivial micro-wave. Open.

8. sluice nack-past-cap; sluice wiring; sluice torn-tail migration.
   sluice's behaviour past its cap needs its own slice; sluice remains
   UNWIRED (no `src` path drives `FileBackedQueue`); and its inline
   parse-or-die recovery loop still awaits migration to the shared
   `replay_wal_tolerating_torn_tail` routine (ADR-0059 §5). Open.

9. ingest-dedup-v0 and ingest-bounded-memory. A re-run of a successful,
   fully-valid ingest still doubles the store (lumen has no idempotency
   key, ADR-0064 DD-3), and the buffer-all-then-flush design holds the
   whole input in RAM before commit (ADR-0064). Each earns its own slice.
   Open.

10. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. It is feedback, not a gate, consistent with the pure
    trunk-based, no-required-checks posture; when wired it belongs in the
    local pre-commit stage. Open.

11. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is
    not surfaced to clients. Open.

12. prism dashboarding, logs and traces in Prism (C5), and query
    completeness (C6). Prism paints metrics only; surfacing log and trace
    query results in the UI and honouring `step` in the metrics query are
    Milestone 2. The prism browser-matrix e2e and the firefox / webkit
    matrix remain scaffolded. Open.

13. pulse columnar adapter. The Arrow / Parquet / DataFusion / TSDB
    columnar story was reframed FUTURE-tense rather than built. Open only
    if wanted.

14. body-size-cap rejection counter and genuine per-arm body cap
    (ADR-0073 D4 / D1). Each is a future slice if operators report the
    need. Open only if wanted.

15. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period; other windows would each earn their own slice. Open
    only if wanted.

16. Milestone 3, pull the shelf into the running system. cinder tiering,
    the sluice / sieve buffers, self-observation inside the runtime, and
    beacon SLO / augur / strata / loom / codex as the system grows past a
    first experiment. Demand-driven once the thing actually runs. Open.
