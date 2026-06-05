# ADR-0063 — Beacon SIGHUP reload: atomic catalogue swap and in-flight state carryover

**Status**: Accepted
**Date**: 2026-06-05
**Author**: Morgan (autonomous DESIGN dispatch, `nw-solution-architect`)
**Governs**: `beacon-sighup-reload-v0`
**Relates to**: ADR-0034 (reload semantics — the governing contract), ADR-0037 (evaluator/orchestrator seam), ADR-0040 (durable rule-state store), ADR-0035 (sink trait and inhibition resolver).

British English throughout, no em dashes.

## Context

ADR-0034 "Reload semantics" already states the operator-visible
contract: on SIGHUP, the loader re-reads `--rules`; the new catalogue
must validate completely (at least one rule loads); if validation fails
the active catalogue stays as-is and a diagnostic is emitted; the swap
is atomic so the evaluator never sees a half-loaded catalogue. That
contract is the spec for this feature and is NOT re-decided here.

What ADR-0034 does NOT specify, and what the DISCUSS wave explicitly
flagged for DESIGN (`docs/feature/beacon-sighup-reload-v0/discuss/wave-decisions.md`,
"FLAGGED for DESIGN", sub-decisions 1-4), is the **mechanism** by which
the swap preserves live alerting state without re-paging on-call. Those
mechanism choices are architecturally significant: they touch the
concurrency model of `beacon-server`, the identity of a rule across an
edit, the lifecycle of the shared `InhibitionResolver`, and the
lifecycle of the per-rule Tokio tasks. They survive the feature and
constrain every future edit to the reload path, so they earn an
immutable record here rather than living only in the feature wave notes.

The relevant code today (`crates/beacon-server/src/main.rs`):

- The catalogue is loaded once at startup (`:65`).
- One `run_rule` Tokio task is spawned per rule (`:164-175`); each task
  owns its `RuleState` in a local `state` variable, seeded from the
  recovered durable store, and loops on `tokio::time::interval`.
- The `InhibitionResolver` is built once (`:162`) and shared across all
  tasks as `Arc<Mutex<InhibitionResolver>>`. It holds a live `pending`
  suppressed-incident map (`crates/beacon/src/inhibition.rs:54`).
- Only SIGINT (`:177`) and SIGTERM (`:179`) are in the `tokio::select!`
  (`:187`). There is no SIGHUP arm. At shutdown the handles are
  `abort()`ed (`:197-199`).

## Decision

`beacon-server` adopts a **single-orchestrator reload model**: the SIGHUP
handler runs IN the main `tokio::select!` loop (the same loop that owns
shutdown), never in a per-rule task. The per-rule evaluation tasks become
the unit that is stopped and restarted on reload. The orchestrator owns
the catalogue, the resolver, and the set of `JoinHandle`s, and is the
single writer of all three. This removes the possibility of the handler
racing the evaluation loops because the handler and the task-set
mutation live on one logical thread of control.

### Sub-decision 1 — SIGHUP handler concurrency model (build-new, then swap, then abort-old)

Add a third arm to the `tokio::select!`:
`tokio::signal::unix::signal(SignalKind::hangup())`. The select loop
becomes a `loop { select! { ... } }` rather than a single-shot select:
SIGINT/SIGTERM arms `break` (shutdown, unchanged); the SIGHUP arm runs
the reload sequence inline and `continue`s. Because the orchestrator is
the only writer of `handles` and `resolver`, and the per-rule tasks only
ever *read* the resolver under its `Mutex` and *write* the durable store,
there is no data race between a reload and an in-flight tick.

The reload sequence (sub-decision 4 specifies the ordering invariant):

1. `load_rules(&args.rules)` on the rules dir (reused verbatim).
2. Validate (sub-decision: the safety contract below). If invalid,
   emit `beacon.reload.refused` and `continue` — the old handles and
   resolver are untouched, so the previous catalogue keeps running.
3. If valid: snapshot the carry-over state (sub-decisions 2 and 3),
   build the NEW resolver and the NEW task set seeded with carried-over
   state, then atomically replace `handles` and `resolver`, then
   `abort()` the OLD handles, then emit `beacon.reload.succeeded`.

The handler is installed BEFORE the per-rule tasks are spawned so an
early SIGHUP during startup does not hit the OS default disposition
(terminate). This mirrors the existing SIGTERM install order.

This is **build-new-then-swap-then-abort-old**, not stop-then-rebuild.
The old tasks keep evaluating until the instant the new set is ready, so
there is no window where a surviving rule has zero evaluators (missed
transition). The abort of the old set happens only after the new set is
the live set, so there is at most a sub-tick overlap on the *old*
generation, which is harmless because the old tasks no longer hold the
live resolver (see sub-decision 3) and are aborted immediately.

### Sub-decision 2 — matching key for carried-over in-flight state: NAME, definition-change keeps state

A rule in the new catalogue is "the same rule" as one in the old
catalogue **iff it has the same `name`**. The carried-over payload is the
rule's current `RuleState` (`Pending { since }` / `Firing { since }` /
`Inactive`) read from the durable `RuleStateStore`, which is already
keyed by name (ADR-0040). A surviving rule's new task is seeded with that
state, exactly as startup recovery seeds from the store today.

A rule that keeps its `name` but changes its `query` / `for_duration` /
`severity` / `sinks` / `inhibits` **keeps its in-flight `since`**. We do
NOT reset state on a definition change.

Rationale, with the consequence stated for the operator:

- The whole JTBD is "edit a noisy threshold on a live rule without
  re-paging". The most common edit is exactly a `query`/`for_duration`
  change on a *currently-firing* rule. Resetting on definition change
  would re-page on every threshold tune, which is the precise harm
  US-02 forbids.
- Name is the operator's stable identity for a rule and the key the
  durable store and the inhibition graph already use. Introducing a
  second, content-hash identity would fork "what is a rule" across three
  subsystems for a benefit (fresh re-evaluation of a re-defined
  threshold) that the next evaluation tick delivers anyway: the seeded
  `since` is only the *clock anchor*; the very next tick re-runs the NEW
  query against the NEW `for_duration` and transitions on the new truth.
- **Operator consequence (documented, by design):** if an operator
  raises a threshold on a rule that is currently `Firing`, the rule
  stays `Firing` with its original `since` until the next tick
  re-evaluates the new query; if the new query no longer matches, the
  next tick resolves it normally. The edit does not retroactively
  un-fire an already-sent page, and it does not re-page. This is the
  desired "no surprise alert storm on edit" behaviour.

A rule present only in the OLD catalogue (removed) stops: its new task
is never created, and its durable state is dropped-and-logged exactly as
startup recovery drops state for rules no longer in config
(`main.rs:139-144`). A rule present only in the NEW catalogue (added)
starts `Inactive` and earns its state from fresh ticks, exactly as a
fresh process would.

### Sub-decision 3 — InhibitionResolver: rebuild from new rules, carry over still-relevant `pending`

The resolver is rebuilt from the new rule set (so added/removed inhibitor
relations take effect), but its live `pending` suppressed-incident map is
carried forward for every entry whose **inhibited rule still exists in
the new catalogue (by name)**. A naive `InhibitionResolver::new(&new_rules)`
alone would silently drop suppressed-pending incidents: a rule that was
suppressed (firing-but-held because its inhibitor was firing) would lose
its held incident, and when the inhibitor later resolved there would be
nothing to release — a silently dropped alert.

Mechanism: build `InhibitionResolver::new(&new_rules)` for the fresh
relation graph, then re-apply the carried-over live state:
- carry the `firing` flag for every surviving rule (so the new graph
  knows who is currently firing, which drives suppression on the very
  next tick);
- carry each `pending` entry whose inhibited rule survives; drop (and
  log at debug) pending entries whose inhibited rule was removed.

This requires the resolver to expose a constructor or seam that accepts
carried-over `firing` and `pending` maps (the DELIVER crafter owns the
exact Rust surface — e.g. an inherent `InhibitionResolver::rebuild_from(
&new_rules, carried_firing, carried_pending)` beside `new`, mirroring how
the state store exposes `open_with_fsync_backend` beside `open`). The
public `observe` surface is unchanged. The rebuild is done by the
orchestrator under no contention because the OLD resolver `Arc<Mutex<>>`
is replaced wholesale by a NEW `Arc<Mutex<>>`; surviving new tasks get
the new `Arc`. The carry-over is computed by locking the OLD resolver
once, reading its `firing`/`pending` snapshot, and dropping the lock
before the new tasks are spawned.

We rebuild-then-carry rather than mutate-in-place because mutate-in-place
under the live `Mutex` while old tasks still hold the `Arc` would let an
old task observe a half-mutated relation graph (a torn read of "who
inhibits whom"). Wholesale `Arc` replacement is the atomic primitive:
old tasks keep the old `Arc` until aborted; new tasks get the new `Arc`;
no task ever sees a partially-rebuilt resolver.

### Sub-decision 4 — task lifecycle: generation swap with abort-after-replace

The orchestrator holds `handles: Vec<JoinHandle<()>>` and (new)
`resolver: Arc<Mutex<InhibitionResolver>>` as the live generation. On a
valid reload:

1. Lock the old resolver, snapshot `firing`/`pending`, drop the lock.
2. `store.load_all()` to read the current durable state per surviving
   rule (the carried `since`); compute dropped-rule logs.
3. Build `new_resolver` (sub-decision 3) and spawn the `new_handles`,
   each seeded with its rule's carried-over `RuleState`
   (sub-decision 2) and a clone of the `new_resolver` `Arc`.
4. Replace the live generation: `let old = std::mem::replace(&mut
   handles, new_handles); resolver = new_resolver;`.
5. `for h in old { h.abort(); }` — abort the OLD tasks only now.
6. Emit `beacon.reload.succeeded`.

Ordering is the invariant: **new set live before old set aborted.** A
surviving rule therefore has continuous evaluation coverage across the
swap (old task evaluating until step 5, new task evaluating from step 3),
never zero. Double-emission is prevented because the OLD and NEW task for
the same surviving rule never both reach a sink in the same tick window:
the old task is aborted within one scheduler turn of the new task
becoming live, and both seed from the same durable `since`, so even an
overlapping tick produces an idempotent transition (same state in, same
state out) rather than a spurious second `Firing`. The durable store's
latest-wins keying (ADR-0040) absorbs any overlapping `put`.

`abort()` on a `JoinHandle` is cooperative-at-the-next-await; a
`run_rule` task awaits at `ticker.tick()` and at `fetch_query`, so it is
promptly cancellable and holds no lock across an `.await` that matters
(the resolver lock is taken and released synchronously inside one tick,
`main.rs:253-256`). There is no graceful-drain requirement for an aborted
rule task because its only side effect, the durable `put`, is
latest-wins and the surviving rule's new task re-establishes the same
state.

## The safety contract (all-or-nothing, from ADR-0034 and US-02)

The validity bar is **"at least one rule loaded"**, identical to the
startup `has_any_rules()` refusal (`main.rs:77-84`). SIGHUP and startup
share one contract:

- `load_rules` returns `Err(LoaderError)` (directory unreadable) → refuse
  the reload, keep the previous catalogue, emit `beacon.reload.refused`
  naming the directory error. Do NOT crash.
- `outcome.has_any_rules() == false` (zero rules) → refuse, keep
  previous catalogue, emit `beacon.reload.refused` stating no rules were
  found. Do NOT crash, do NOT go dark.
- `outcome.has_any_rules() == true` with one or more per-file
  diagnostics (a partly-broken catalogue) → **apply the valid rules**
  (the swap proceeds) AND surface each `LoaderDiagnostic` via the
  existing `warn!` path, exactly as startup does (`main.rs:73-75`). This
  is the loader's established report-and-skip posture (B01); reload is
  kept consistent with startup. A per-file diagnostic does NOT escalate
  to a full refusal, because the catalogue as a whole validated.

The swap is all-or-nothing: either the new catalogue is fully live (with
carried-over state) or the previous catalogue is fully retained. There is
no partial-apply path, because the new task set and new resolver are
built completely before the generation is replaced (sub-decision 4). A
refusal touches neither `handles` nor `resolver`.

## Observables (named, for DISTILL)

Two structured events on `beacon-server`'s existing `tracing` stream:

- **`beacon.reload.succeeded`** (INFO): carries `rules_loaded` (count of
  rules in the new live catalogue), `added` (count of names new vs old),
  `removed` (count of names old not in new), and `diagnostics` (count of
  skipped files). This is the success event US-01 AC-5 reads and the
  verifier's B03 black-box asserts.
- **`beacon.reload.refused`** (WARN): carries `file` (the offending
  file, when a per-file parse failure or a directory error names one) or
  a no-rules reason, `error` (the `LoaderDiagnostic::display` text incl.
  the "did you mean" suggestion, or the `LoaderError` text), and a fixed
  `previous_catalogue_retained = true`. This is the refusal event US-02
  reads.

Both event names are stable identifiers (the `message` field of the
tracing event). The per-file report-and-skip diagnostics continue to use
the existing `"rule load diagnostic"` `warn!` (`main.rs:74`), unchanged,
so a partly-broken successful reload emits one `beacon.reload.succeeded`
plus N pre-existing `rule load diagnostic` lines.

## Alternatives considered

1. **Per-task self-reload (each `run_rule` watches the rules dir and
   reloads its own rule).** Rejected: it has no atomic catalogue
   boundary (each task reloads independently, so the evaluator CAN see a
   half-loaded catalogue, violating ADR-0034), it forks the loader across
   N tasks, and it makes the all-or-nothing safety property impossible to
   honour. The single-orchestrator model is the only one consistent with
   ADR-0034's "evaluator never sees a half-loaded catalogue".

2. **A shared `ArcSwap<Catalogue>` read by long-lived per-rule tasks that
   re-read the catalogue each tick.** Rejected for v0: it requires every
   per-rule task to re-derive "is my rule still present / changed" each
   tick, pushing reload logic into the hot evaluation path, and it
   complicates the inhibition resolver's lifecycle (the resolver is not a
   per-rule value). The build-new-then-swap model keeps reload logic
   entirely in the orchestrator and the hot path unchanged. `ArcSwap`
   would be the right tool if rules outnumbered the cost of task respawn
   (thousands of rules); the scaling target is 35 rules (ADR-0034), so
   task respawn is cheap and the simpler model wins.

3. **Mutate the existing `InhibitionResolver` in place under its
   `Mutex`** (sub-decision 3 alternative). Rejected: torn reads of the
   relation graph by old tasks still holding the `Arc`. Wholesale `Arc`
   replacement is atomic; in-place mutation is not, for readers that
   already hold the lock-bearing `Arc`.

4. **Reset in-flight state to `Inactive` on every swap** (sub-decision 2
   alternative). Rejected: re-pages the entire on-call rotation a
   `for_duration` after every SIGHUP, turning a routine edit into an
   alert storm. This is the exact anti-behaviour US-02 forbids.

## Consequences

- **Positive:** the evaluator never sees a half-loaded catalogue
  (ADR-0034 honoured); a surviving rule keeps its `since` and does not
  re-page (US-02 honoured); a removed rule stops and its durable state is
  dropped-and-logged (consistent with startup); the hot evaluation path
  (`run_rule`) is unchanged; the loader and durable store are reused
  verbatim; the safety bar is identical to startup, so one contract
  governs both.
- **Negative / accepted:** a definition change keeps the old `since`
  until the next tick re-evaluates (documented operator consequence, by
  design); the `InhibitionResolver` gains a `rebuild_from`-style seam
  (small additive surface beside `new`); a reload respawns all per-rule
  tasks even on a no-op SIGHUP (cheap at the 35-rule scaling target,
  measurably wasteful only at thousands of rules — at which point
  alternative 2's `ArcSwap` becomes the right successor).
- **Enforcement:** the all-or-nothing invariant and the
  new-live-before-old-aborted ordering are behaviour the acceptance suite
  pins black-box (the malformed-reload-keeps-previous negative and the
  surviving-`since` assertion). There is no import-graph rule to add; the
  invariant is a sequencing property of one orchestrator function, best
  guarded by the acceptance test plus the per-feature 100% mutation gate
  on the modified `main.rs` (CLAUDE.md, ADR-0005 Gate 5).

## Review clarifications (nw-solution-architect-reviewer, iteration 1)

The independent review conditionally approved this ADR and asked for three
clarifications, folded in here verbatim as the governing detail; DELIVER
implements to these.

1. Sub-decision 4, the sink-emit overlap window. Cooperative abort means
   an old `run_rule` task keeps running until its next await point
   (ticker tick, query fetch, resolver lock, or `sink.emit`). In the brief
   overlap before the abort takes effect, an old and a new task for the
   same surviving rule may execute concurrently. This is SAFE and does not
   double-page: both tasks are seeded from the same durable state
   (latest-wins from the store, ADR-0040) and evaluate the same rule
   definition, so their transitions are deterministic and idempotent. A
   rule that is Firing with the alert still Active transitions to
   `(Firing, None)` regardless of which task runs first, so neither emits
   a second incident; in steady state both emit nothing. The old task's
   in-flight `sink.emit` completes and emits the same incident the new
   task would, or nothing. There is no duplicate-Firing window.

2. Sub-decision 2, the `for_duration`-decrease pre-fire. Name-only
   matching keeps a Pending rule's `since` across an edit. If an operator
   DECREASES a rule's `for_duration` (for example 5m to 1m), a rule
   currently Pending crosses the new, earlier threshold on the next tick
   and transitions to Firing earlier than the old clock would have. This
   is a deliberate, documented trade-off: the alternative, resetting state
   on any definition change, would re-page on-call on every threshold
   tune and break US-02's no-re-page promise. The common edit raises a
   threshold to reduce noise; an operator decreasing `for_duration` on a
   live Pending rule should coordinate with on-call for the pre-fire.

3. Sub-decision 3, the inhibition carryover both-ends survival check. A
   pending suppressed-incident entry for inhibited rule B is carried over
   only if BOTH B survives in the new catalogue AND at least one rule
   whose `inhibits` names B survives. If the inhibitor is removed, B's
   pending entry is dropped (logged at debug) so a removed inhibitor
   cannot keep suppressing a survivor; if B is removed its entry is
   dropped with B. The rebuilt resolver carries no relation the new rule
   set does not assert, so no inhibition leaks and no live suppression is
   lost.
